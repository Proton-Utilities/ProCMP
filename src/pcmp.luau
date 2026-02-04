--!nocheck
--!nolint LocalUnused

--// Imports
local io = require("@lune/stdio")
local task = require("@lune/task")
local fs = require("@lune/fs")
local process = require("@lune/process")
local serde = require("@lune/serde")
local net = require("@lune/net")
local dateTime = require("@lune/datetime")

-- State
local currentBuildSource
local currentBuildConfig
local currentVersion

-- Variables
local releaseBody = `{process.env.TEMP or process.env.TMP}/RELEASE_BODY.md`
local config = serde.decode("json", fs.readFile(process.args[1]))
local composerMarkers = {
	["__COMPOSER.Insert(__COMPOSER.build)"] = function()
		return currentBuildSource or ""
	end,
	["__COMPOSER.Insert(__COMPOSER.genDate)"] = function()
		return string.format("%q", dateTime.now():toIsoDate())
	end,
	["__COMPOSER.Insert(__COMPOSER.cfg)"] = function()
		return string.format("%q", currentBuildConfig or "")
	end,
	["__COMPOSER.Insert(__COMPOSER.vers)"] = function()
		return string.format("%q", currentVersion or "")
	end,
}

--// Constants
local SPINNER_FRAMES = { "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏" }

local function withSpinner(msg, fn, colorName)
    local done = false
    local idx = 1

    task.spawn(function()
        while not done do
            if colorName then
                io.write(io.color(colorName))
            end
            io.write("\r" .. SPINNER_FRAMES[idx] .. " " .. msg)
            if colorName then
                io.write(io.color("reset"))
            end
            idx = (idx % #SPINNER_FRAMES) + 1
            task.wait(0.08)
        end
    end)

    local ok, result = pcall(fn)
    done = true

    io.write("\r\27[2K")
    return ok, result
end

local function getBaseName(path)
	return path:match("^.+[\\/](.+)$") or path
end

local function getGhKey()
	if not fs.isFile(".env") then
		return nil
	end

	for line in fs.readFile(".env"):gmatch("[^\r\n]+") do
		local key, val = line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
		if key == "GITHUB_API_KEY" and val then
			return val:gsub("^[\"']", ""):gsub("[\"']$", "")
		end
	end
	
	return nil
end

local function mergeOptions(base, override)
	local new = table.clone(base)
	if override then
		for k, v in pairs(override) do
			new[k] = v
		end
	end
	return new
end

local mode = config.testing
	and config.testing.enabled
	and io.prompt("select", "Select pipeline mode", { "Build Tests", "Build Release" })
local tasks = {}
local isDeployable = false

if mode == 1 then
	local testCfg = config.testing
	currentBuildConfig = "Testing"
	currentVersion = "TEST-BUILD"

	if not fs.isDir(testCfg.inputDir) then
		print(`Test directory not found: {testCfg.inputDir}`)
		return
	end

	for _, fileName in ipairs(fs.readDir(testCfg.inputDir)) do
		if fileName:match("%.luau$") or fileName:match("%.lua$") then
			table.insert(tasks, {
				name = fileName,
				input = `{testCfg.inputDir}/{fileName}`,
				output = `{testCfg.outputDir}/{fileName}`,
				frame = testCfg.frame,
				darkluaConfig = testCfg.darkluaConfig,
			})
		end
	end

	if #tasks == 0 then
		print("No test files found.")
		return
	end

	if not fs.isDir(testCfg.outputDir) then
		fs.writeDir(testCfg.outputDir)
	end
elseif not mode or mode == 2 then
	local configNames = {}
	for _, v in pairs(config.buildConfigs) do
		table.insert(configNames, v.name)
	end

	local selectedIdx = io.prompt("select", "Build configuration", configNames)
	local baseOptions = config.buildConfigs[selectedIdx]

	currentBuildConfig = baseOptions.name
	currentVersion = baseOptions.promptVersion and io.prompt("text", "Build version (vMAJOR.MINOR.PATCH)") or "0.0.0"
	isDeployable = baseOptions.promptDeploy

	table.insert(tasks, baseOptions)
	if baseOptions.variants then
		for _, variant in pairs(baseOptions.variants) do
			table.insert(tasks, mergeOptions(baseOptions, variant))
		end
	end
end

local artifacts = {}

-- Loop
for i, buildTask in ipairs(tasks) do
	local ok, result = withSpinner(`Building {buildTask.name or currentBuildConfig} [{buildTask.output}]...`, function()
        return process.exec("darklua", {
			"process",
			buildTask.input,
			buildTask.output,
			"-c",
			buildTask.darkluaConfig,
		})
	end, "yellow")

	if not ok or not result.ok then
		io.write(io.color("red"))
		print(`Error building {buildTask.output}\n{result and result.stderr or "Unknown error"}`)
		io.write(io.color("reset"))
		return
	end

	currentBuildSource = fs.readFile(buildTask.output)
	local frame = fs.readFile(buildTask.frame)

	for marker, handler in pairs(composerMarkers) do
		local cleanMarker = marker:gsub("([%%%+%-%*%?%[%]%^%$%(%)%%])", "%%%1")
		frame = frame:gsub(cleanMarker, handler)
	end

	fs.writeFile(buildTask.output, frame)
	io.write(io.color("green"))
	print(`✔ Build complete: {buildTask.output}`)
	io.write(io.color("reset"))
	table.insert(artifacts, buildTask.output)
end

-- Output
if config.openComposedOutput and process.os == "windows" and #artifacts > 0 then
	process.exec("powershell", {
		"-ExecutionPolicy",
		"Bypass",
		"-NoProfile",
		"-Command",
		`code '{artifacts[1]}'`,
	})
end

-- Deploy
if isDeployable and config.deployment.enabled then
	local ghCfg = config.deployment.github
	local apiKey = getGhKey() or ghCfg.apiKey

	print("\nFiles prepared for upload:")
	for _, path in ipairs(artifacts) do
		print(` - {getBaseName(path)}`)
	end

	if not io.prompt("confirm", "Do you want to distribute this build publicly?") then
		return
	end

	if not (ghCfg and ghCfg.enabled) then
		return
	end

	fs.writeFile(releaseBody, `# {ghCfg.repo} {currentVersion}`)

	if process.os == "windows" then
		process.exec("powershell", {
			"-ExecutionPolicy",
			"Bypass",
			"-NoProfile",
			"-Command",
			`code '{releaseBody}'`,
		})
	end

	if not io.prompt("confirm", "Add release notes, and confirm when done") then
		return
	end

	local response = net.request({
		url = `https://api.github.com/repos/{ghCfg.owner}/{ghCfg.repo}/releases`,
		method = "POST",
		headers = {
			["Authorization"] = `Bearer {apiKey}`,
			["Content-Type"] = "application/json",
		},
		body = serde.encode("json", {
			tag_name = currentVersion,
			name = `{currentBuildConfig} {currentVersion}`,
			body = fs.isFile(releaseBody) and fs.readFile(releaseBody) or "",
			draft = false,
			prerelease = tasks[1].prerelease,
		}),
	})

	if fs.isFile(releaseBody) then
		fs.removeFile(releaseBody)
	end

	if not response.ok then
		print(`Deployment failed: {response.statusMessage}\n{response.body}`)
		return
	end

	print(`Release created: {response.statusMessage}`)

	local releaseId = serde.decode("json", response.body).id
	if not releaseId then
		print("Error: Could not retrieve release ID.")
		return
	end

	-- Artifacts
	for _, filePath in ipairs(artifacts) do
		local assetName = getBaseName(filePath)
		print(`Uploading {assetName}...`)

		local uploadResp = net.request({
			url = `https://uploads.github.com/repos/{ghCfg.owner}/{ghCfg.repo}/releases/{releaseId}/assets?name={assetName}`,
			method = "POST",
			headers = {
				["Authorization"] = `Bearer {apiKey}`,
				["Content-Type"] = "application/octet-stream",
			},
			body = fs.readFile(filePath),
		})

		if uploadResp.ok then
			print(`✅ Uploaded: {assetName}`)
		else
			print(`❌ Failed: {assetName} - {uploadResp.statusMessage}`)
		end
	end
end
