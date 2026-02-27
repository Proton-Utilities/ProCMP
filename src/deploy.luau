--!nocheck

--// Imports

local fs = require("@lune/fs")
local net = require("@lune/net")
local serde = require("@lune/serde")
local util = require("./util")
local ui = require("./ui")

--// Constants

local MAX_REDIRECTS = 5
local UPLOAD_HOST = "uploads.github.com"
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

			local location = resp.headers["location"] or resp.headers["Location"]
			if not location or location == "" then
				return resp
			end

			local newHost = getHost(location)
			local origHost = getHost(currentUrl)

			if newHost ~= origHost then
				currentHeaders = table.clone(currentHeaders)
				currentHeaders["Authorization"] = nil :: any
			end

			if status == 307 or status == 308 then
				currentUrl = location
			else
				currentUrl = location
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
): (boolean, number?, string?)
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

	return true, releaseId, nil
end

local function uploadAsset(
	owner: string,
	repo: string,
	releaseId: number,
	token: string,
	filePath: string,
	assetName: string
): (boolean, string?)
	local uploadResp = followRedirects(
		`https://{UPLOAD_HOST}/repos/{owner}/{repo}/releases/{releaseId}/assets?name={assetName}`,
		"POST",
		{
			["Authorization"] = `Bearer {token}`,
			["Content-Type"] = "application/octet-stream",
			["Accept"] = "application/vnd.github+json",
			["X-GitHub-Api-Version"] = "2022-11-28",
		},
		fs.readFile(filePath)
	)

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
		ui.openFile(opts.releaseBodyPath, opts.releaseNotesEditor)
	end

	local releaseBody = fs.isFile(opts.releaseBodyPath) and fs.readFile(opts.releaseBodyPath) or ""

	-- Release
	local ok, releaseId, err = createRelease(
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
		local uploadOk, uploadErr =
			uploadAsset(opts.owner, opts.repo, releaseId :: number, opts.token, filePath, assetName)
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
