--!nocheck

--// Imports

local fs = require("@lune/fs")
local process = require("@lune/process")
local dateTime = require("@lune/datetime")
local util = require("./util")

--// Types

type BuildTask = {
	name: string,
	input: string,
	output: string,
	frame: string,
	darkluaConfig: string,
	[string]: any,
}

--// State

local currentBuildSource: string? = nil
local currentBuildConfig: string? = nil
local currentVersion: string? = nil

--// Variables

local composerMarkers: { [string]: () -> string } = {
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

--// Public Methods

local function setContext(cfg: string?, vers: string?)
	currentBuildConfig = cfg
	currentVersion = vers
end

local function processFrame(rawFrame: string, builtSource: string): string
	currentBuildSource = builtSource

	local result = rawFrame
	for marker, handler in pairs(composerMarkers) do
		local cleanMarker = util.escapePattern(marker)
		result = result:gsub(cleanMarker, handler)
	end

	return result
end

local function runTask(buildTask: BuildTask)
	util.ensureDir(util.getDirName(buildTask.output))

	local result = process.exec("darklua", {
		"process",
		buildTask.input,
		buildTask.output,
		"-c",
		buildTask.darkluaConfig,
	})

	if not result.ok then
		error(result.stderr or "darklua returned a non-zero exit code")
	end

	local builtSource = fs.readFile(buildTask.output)
	local rawFrame = fs.readFile(buildTask.frame)
	local composed = processFrame(rawFrame, builtSource)

	fs.writeFile(buildTask.output, composed)
end

return {
	setContext = setContext,
	runTask = runTask,
}
