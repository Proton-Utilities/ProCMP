--!nocheck
--!nolint LocalUnused

--[[
    @author Proton-Utilities
	@version 1.0.0
    @name ProCMP
    @description Build composer system using Lune & darklua
]]

--// Imports
local io = require("@lune/stdio")
local fs = require("@lune/fs")
local process = require("@lune/process")

--// Variables
local build
local buildConfig
local buildVers

--// Settings
local paths = {
	frame = "build/frame.luau",
	input = "src/init.luau",
	output = "dist.luau",
	darkluaConfig = "build/.darklua.json",
}

local buildConfigs = {
	"Debug",
	"Release",
}

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

--// Initialize
buildConfig = buildConfigs[io.prompt("select", "Build configuration", buildConfigs)]
buildVers = buildConfigs[table.find(buildConfigs, buildConfig)] ~= "Debug" and io.prompt("text", "Build version (vMAJOR.MINOR.PATCH)") or "v0.0.0"

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
header = fs.readFile(paths.frame)

for marker, replacement in pairs(composerMarkers) do
	local escapedMarker = marker:gsub("([%%%+%-%*%?%[%]%^%$%(%)%%])", "%%%1")
	header = header:gsub(escapedMarker, function()
		return replacement()
	end)
end

fs.writeFile(paths.output, header)

print("Build complete: " .. paths.output)
