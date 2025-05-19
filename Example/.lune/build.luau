--!nocheck
--!nolint LocalUnused

--[[
    @author Proton-Utilities
	@version 1.1.0
    @name ProCMP
    @description Build composer system using Lune & darklua
]]

--// Imports
local io = require("@lune/stdio")
local fs = require("@lune/fs")
local process = require("@lune/process")
local serde = require("@lune/serde")
local net = require("@lune/net")

--// Variables
local frame
local build
local buildConfig
local buildVers

--// Settings
local config = serde.decode("json", fs.readFile(process.args[1]))

local paths = config.paths
local releaseBodyPath = "RELEASE_BODY.md"
local buildConfigs = config.buildConfigs

local composerMarkers = {
	["__PROCMP.Insert(__PROCMP.Build)"] = function()
		return build or ""
	end,
	["__PROCMP.Insert(__PROCMP.BuildConfig)"] = function()
		return string.format("%q", buildConfig or "")
	end,
	["__PROCMP.Insert(__PROCMP.BuildVersion)"] = function()
		return string.format("%q", buildVers or "")
	end,
}

--// Composition
buildConfig = buildConfigs[io.prompt("select", "Build configuration", buildConfigs)]
buildVers = buildConfigs[table.find(buildConfigs, buildConfig)] ~= "Debug"
		and io.prompt("text", "Build version (vMAJOR.MINOR.PATCH)")
	or "v0.0.0"

local srcResult = process.spawn("darklua", {
	"process",
	paths.input,
	paths.output,
	"-c",
	paths.darkluaConfig,
})

if not srcResult.ok then
	print(srcResult.stderr)
	return
end

build = fs.readFile(paths.output)
frame = fs.readFile(paths.frame)

for marker, replacement in pairs(composerMarkers) do
	local escapedMarker = marker:gsub("([%%%+%-%*%?%[%]%^%$%(%)%%])", "%%%1")
	frame = frame:gsub(escapedMarker, function()
		return replacement()
	end)
end

fs.writeFile(paths.output, frame)

print("Build complete: " .. paths.output)

--// Deployment
if buildConfig == "Release" and config.deployment and config.deployment.enabled then
	local ghCfg = config.deployment.github
	local confirm = io.prompt("confirm", "Do you want to distribute this build publicly?")

	if confirm then
		fs.writeFile(releaseBodyPath, `# Release {buildVers}`)

		if process.os == "windows" then
			process.spawn("powershell", {
				"-ExecutionPolicy",
				"Bypass",
				"-NoProfile",
				"-Command",
				`code '{releaseBodyPath}'`,
			})
		end

		local releaseConfirm = io.prompt("confirm", "Add release notes, and confirm when done")

		if releaseConfirm then
			local response = net.request({
				url = string.format("https://api.github.com/repos/%s/%s/releases", ghCfg.owner, ghCfg.repo),
				method = "POST",
				headers = {
					["Authorization"] = "Bearer " .. ghCfg.apiKey,
					["Content-Type"] = "application/json",
				},
				body = net.jsonEncode({
					tag_name = buildVers,
					name = "Release " .. buildVers,
					body = fs.isFile(releaseBodyPath) and fs.readFile(releaseBodyPath) or "",
					draft = false,
					prerelease = false,
				}),
			})

			if response.ok then
				print("Deployment successful: " .. response.statusMessage)
			else
				print("Deployment failed: " .. response.statusMessage)
				print(response.body)
			end
		end
	else
		print("Deployment skipped.")
	end

	if fs.isFile(releaseBodyPath) then
		fs.removeFile(releaseBodyPath)
	end
end

if process.os == "windows" then
	process.spawn("powershell", {
		"-ExecutionPolicy",
		"Bypass",
		"-NoProfile",
		"-Command",
		`code '{paths.output}'`,
	})
end
