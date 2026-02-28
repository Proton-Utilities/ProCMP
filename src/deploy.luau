--!nocheck

--// Imports

local fs = require("@lune/fs")
local net = require("@lune/net")
local serde = require("@lune/serde")
local util = require("./util")
local ui = require("./ui")
local io = require("@lune/stdio")

--// Constants

local MAX_REDIRECTS = 5
local _UPLOAD_HOST = "uploads.github.com"
local API_HOST = "api.github.com"

--// Private Methods

local function getHost(url: string): string
	return url:match("^https?://([^/]+)") or ""
end

local function followRedirects(
	url: string,
	method: string,
	headers: { [string]: string },
	body: string?
): net.FetchResponse
	local currentUrl = url
	local currentMethod = method
	local currentHeaders = table.clone(headers)
	local redirectCount = 0

	while true do
		local resp = net.request({
			url = currentUrl,
			method = currentMethod,
			headers = currentHeaders,
			body = body,
		})

		local status = resp.statusCode
		if status >= 300 and status < 400 then
			if redirectCount >= MAX_REDIRECTS then
				error(`[pcmp] Too many redirects (>{MAX_REDIRECTS}) from {currentUrl}`)
			end
			redirectCount += 1

			local location: string? = nil
			for k, v in pairs(resp.headers) do
				if k:lower() == "location" then
					location = v
					break
				end
			end

			if not location or location == "" then
				print(`[pcmp] Redirect status {status} from {currentUrl} but no location header found!`)
				print(`[pcmp] Response body: {resp.body}`)
				print(`[pcmp] Response headers:`)
				for k, v in pairs(resp.headers) do
					print(`  {k}: {v}`)
				end
				return resp
			end

			if not location:match("^https?://") then
				local base = currentUrl:match("^(https?://[^/]+)")
				if base then
					location = base .. (location:match("^/") and "" or "/") .. location
				end
			end

			print(`[pcmp] Redirecting ({status}) to: {location}`)

			local newHost = getHost(location)
			local origHost = getHost(currentUrl)

			local isGitHub = newHost:match("%.github%.com$")
				or newHost:match("%.githubusercontent%.com$")
				or newHost:match("%.githublabs%.com$")

			if newHost ~= origHost and not isGitHub then
				currentHeaders = table.clone(currentHeaders)
				currentHeaders["Authorization"] = nil :: any
			end

			currentUrl = location
			if status ~= 307 and status ~= 308 then
				currentMethod = "GET"
				body = nil
				currentHeaders["Content-Type"] = nil :: any
				currentHeaders["Content-Length"] = nil :: any
			end
		else
			return resp
		end
	end
end

local function createRelease(
	owner: string,
	repo: string,
	token: string,
	tagName: string,
	releaseName: string,
	body: string,
	prerelease: boolean
): (boolean, number?, string?, string?)
	local resp = followRedirects(
		`https://{API_HOST}/repos/{owner}/{repo}/releases`,
		"POST",
		{
			["Authorization"] = `Bearer {token}`,
			["Content-Type"] = "application/json",
			["Accept"] = "application/vnd.github+json",
			["X-GitHub-Api-Version"] = "2022-11-28",
		},
		serde.encode("json", {
			tag_name = tagName,
			name = releaseName,
			body = body,
			draft = false,
			prerelease = prerelease,
		})
	)

	if not resp.ok then
		return false, nil, `HTTP {resp.statusCode}: {resp.body}`
	end

	local data = serde.decode("json", resp.body)
	local releaseId = data.id
	if not releaseId then
		return false, nil, "GitHub response missing release ID"
	end

	return true, releaseId, data.upload_url, nil
end

local function uploadAsset(
	owner: string,
	repo: string,
	releaseId: number,
	token: string,
	filePath: string,
	assetName: string,
	uploadUrl: string
): (boolean, string?)
	local cleanUrl = uploadUrl:gsub("{%?name,label}", "")
	local uploadUrlResolved = `{cleanUrl}?name={assetName}`

	local uploadResp = followRedirects(uploadUrlResolved, "POST", {
		["Authorization"] = `Bearer {token}`,
		["Content-Type"] = "application/octet-stream",
		["Accept"] = "application/vnd.github+json",
		["X-GitHub-Api-Version"] = "2022-11-28",
	}, fs.readFile(filePath))

	if uploadResp.ok or uploadResp.statusCode == 201 then
		return true, nil
	end

	return false, `HTTP {uploadResp.statusCode}: {uploadResp.body}`
end

--// Public Methods

export type DeployOptions = {
	owner: string,
	repo: string,
	token: string,
	version: string,
	configName: string,
	prerelease: boolean,
	artifacts: { string },
	releaseBodyPath: string,
	releaseNotesEditor: { command: string, args: { string } }?,
}

local function run(opts: DeployOptions): boolean
	-- Notes
	fs.writeFile(opts.releaseBodyPath, `# {opts.repo} {opts.version}\n\n`)

	if opts.releaseNotesEditor then
		ui.printInfo("Opening release notes editor — save and close when done.")
		ui.openFile(opts.releaseBodyPath, opts.releaseNotesEditor, true)
		io.prompt("text", "Press Enter when you are finished editing to continue...")
	end

	local releaseBody = fs.isFile(opts.releaseBodyPath) and fs.readFile(opts.releaseBodyPath) or ""

	-- Release
	local ok, releaseId, uploadUrl, err = createRelease(
		opts.owner,
		opts.repo,
		opts.token,
		opts.version,
		`{opts.configName} {opts.version}`,
		releaseBody,
		opts.prerelease
	)

	if opts.releaseBodyPath and fs.isFile(opts.releaseBodyPath) then
		fs.removeFile(opts.releaseBodyPath)
	end

	if not ok or not releaseId then
		ui.printError(`Release creation failed: {err or "unknown error"}`)
		return false
	end

	ui.printSuccess(`Release created (id={releaseId})`)

	-- Artifacts
	local allOk = true
	for _, filePath in ipairs(opts.artifacts) do
		local assetName = util.getBaseName(filePath)
		local uploadOk, uploadErr = uploadAsset(
			opts.owner,
			opts.repo,
			releaseId :: number,
			opts.token,
			filePath,
			assetName,
			uploadUrl :: string
		)
		if uploadOk then
			ui.printSuccess(`Uploaded: {assetName}`)
		else
			ui.printError(`Upload failed: {assetName} — {uploadErr or "unknown"}`)
			allOk = false
		end
	end

	return allOk
end

return {
	run = run,
}
