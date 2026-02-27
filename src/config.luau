--!nocheck

--// Imports

local fs = require("@lune/fs")
local serde = require("@lune/serde")
local util = require("./util")

--// Types

export type OpenFileConfig = {
	command: string,
	args: { string },
}

export type DeployGithubConfig = {
	owner: string,
	repo: string,
	tokenEnvVar: string?,
}

export type DeploymentConfig = {
	github: DeployGithubConfig?,
}

export type BuildVariant = {
	name: string?,
	input: string?,
	output: string?,
	frame: string?,
	darkluaConfig: string?,
}

export type BuildConfig = {
	name: string,
	input: string?,
	inputDir: string?,
	output: string?,
	outputDir: string?,
	frame: string,
	darkluaConfig: string,
	promptVersion: boolean?,
	versionFormat: string?,
	promptDeploy: boolean?,
	prerelease: boolean?,
	openComposedOutput: OpenFileConfig?,
	variants: { BuildVariant }?,
}

export type PcmpConfig = {
	envFile: string?,
	openComposedOutput: OpenFileConfig?,
	releaseNotesEditor: OpenFileConfig?,
	deployment: DeploymentConfig?,
	buildConfigs: { BuildConfig },
}

export type PcmpLocalConfig = {
	envFile: string?,
	openComposedOutput: OpenFileConfig?,
	releaseNotesEditor: OpenFileConfig?,
}

export type BuildTask = {
	name: string,
	input: string,
	output: string,
	frame: string,
	darkluaConfig: string,
	prerelease: boolean?,
	openComposedOutput: OpenFileConfig?,
}

--// Constants

local LOCAL_CONFIG_SUFFIX = ".local.json"

--// Private Methods

local function resolveOpenComposedOutput(local_: OpenFileConfig?, global_: OpenFileConfig?): OpenFileConfig?
	return if local_ ~= nil then local_ else global_
end

local function deriveLocalPath(configPath: string): string
	local stem = configPath:gsub("%.json$", "")
	return stem .. LOCAL_CONFIG_SUFFIX
end

--// Public Methods

local function load(configPath: string): PcmpConfig
	if not fs.isFile(configPath) then
		error(`[pcmp] Config file not found: {configPath}`)
	end

	local raw = serde.decode("json", fs.readFile(configPath))
	return raw :: PcmpConfig
end

local function applyLocalOverrides(cfg: PcmpConfig, configPath: string): PcmpConfig
	local localPath = deriveLocalPath(configPath)
	if not fs.isFile(localPath) then
		return cfg
	end

	local ok, raw = pcall(serde.decode, "json", fs.readFile(localPath))
	if not ok then
		error(`[pcmp] Failed to parse local config at {localPath}: {raw}`)
	end

	local local_: PcmpLocalConfig = raw :: any

	local merged: PcmpConfig = table.clone(cfg :: any) :: any
	if local_.openComposedOutput ~= nil then
		merged.openComposedOutput = local_.openComposedOutput
	end
	if local_.releaseNotesEditor ~= nil then
		merged.releaseNotesEditor = local_.releaseNotesEditor
	end
	if local_.envFile ~= nil then
		merged.envFile = local_.envFile
	end

	return merged
end

local function getToken(cfg: PcmpConfig, envVars: { [string]: string }): string?
	local github = cfg.deployment and cfg.deployment.github
	if not github then
		return nil
	end

	local varName = (github.tokenEnvVar and github.tokenEnvVar ~= "") and github.tokenEnvVar or "GITHUB_TOKEN"
	return envVars[varName]
end

local function expandBuildConfig(bc: BuildConfig, globalOpen: OpenFileConfig?): { BuildTask }
	local tasks: { BuildTask } = {}
	local resolvedOpen = resolveOpenComposedOutput(bc.openComposedOutput, globalOpen)

	if bc.inputDir then
		if not fs.isDir(bc.inputDir) then
			error(`[pcmp] inputDir not found: {bc.inputDir}`)
		end

		local outDir = bc.outputDir or bc.output or ("generated/" .. bc.name:lower())
		util.ensureDir(outDir)

		for _, fileName in ipairs(fs.readDir(bc.inputDir)) do
			if fileName:match("%.luau$") or fileName:match("%.lua$") then
				table.insert(tasks, {
					name = fileName,
					input = bc.inputDir .. "/" .. fileName,
					output = outDir .. "/" .. fileName,
					frame = bc.frame,
					darkluaConfig = bc.darkluaConfig,
					prerelease = bc.prerelease,
					openComposedOutput = resolvedOpen,
				})
			end
		end
	else
		if not bc.input then
			error(`[pcmp] Build config '{bc.name}' must specify 'input' or 'inputDir'`)
		end
		if not bc.output then
			error(`[pcmp] Build config '{bc.name}' must specify 'output'`)
		end

		table.insert(tasks, {
			name = bc.name,
			input = bc.input :: string,
			output = bc.output :: string,
			frame = bc.frame,
			darkluaConfig = bc.darkluaConfig,
			prerelease = bc.prerelease,
			openComposedOutput = resolvedOpen,
		})

		if bc.variants then
			for _, variant in ipairs(bc.variants) do
				table.insert(tasks, {
					name = variant.name or bc.name,
					input = variant.input or bc.input :: string,
					output = variant.output or bc.output :: string,
					frame = variant.frame or bc.frame,
					darkluaConfig = variant.darkluaConfig or bc.darkluaConfig,
					prerelease = bc.prerelease,
					openComposedOutput = resolvedOpen,
				})
			end
		end
	end

	return tasks
end

return {
	load = load,
	applyLocalOverrides = applyLocalOverrides,
	getToken = getToken,
	expandBuildConfig = expandBuildConfig,
}
