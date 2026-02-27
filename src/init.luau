--!nocheck

--// Imports

local io = require("@lune/stdio")
local process = require("@lune/process")

local util = require("@self/util")
local ui = require("@self/ui")
local config = require("@self/config")
local build = require("@self/build")
local deploy = require("@self/deploy")
local scaffold = require("@self/scaffold")

--// Constants

local VERSION = "3.0.0"

local RELEASE_BODY_PATH = `{process.env.TEMP or process.env.TMP or "/tmp"}/PCMP_RELEASE_BODY.md`

local HELP_TEXT = "ProCMP v"
	.. VERSION
	.. [[ -- Build composition and release pipeline

Usage:
  pcmp <config>            Run the pipeline with the given config file
  pcmp init                Initialise a new ProCMP project interactively
  pcmp --help              Show this help message
  pcmp --version           Print version and exit

Arguments:
  <config>                 Path to a .pcmp.json file

Subcommands:
  init                     Scaffold pipeline files and personal local config

Options:
  -h, --help               Show this help message
  -V, --version            Print version string

Local config:
  Personal settings (editor, output opener) live in .pcmp.local.json next to
  the shared config — this file is gitignored so each team member sets their own.

Examples:
  pcmp pipeline/.pcmp.json
  pcmp init
]]

--// State

local currentVersion: string = ""
local currentBuildConfig: string = ""

--// Private Methods

type ParsedArgs = {
	configPath: string?,
	subcommand: string?,
	showHelp: boolean,
	showVersion: boolean,
}

local function parseArgs(): ParsedArgs
	local result: ParsedArgs = {
		configPath = nil,
		subcommand = nil,
		showHelp = false,
		showVersion = false,
	}

	for _, arg in ipairs(process.args) do
		if arg == "--help" or arg == "-h" then
			result.showHelp = true
		elseif arg == "--version" or arg == "-V" then
			result.showVersion = true
		elseif arg == "init" then
			result.subcommand = "init"
		elseif not arg:match("^%-") then
			result.configPath = arg
		end
	end

	return result
end

local function promptVersion(pattern: string?): string
	local fmt = pattern or "^v%d+%.%d+%.%d+"
	while true do
		local v = io.prompt("text", "Build version")
		if v:match(fmt) then
			return v
		end
		ui.printWarn(`Version '{v}' does not match the required format. Try again.`)
	end
end

local function runBuilds(
	tasks: { config.BuildTask },
	configName: string,
	version: string,
	globalOpen: config.OpenFileConfig?
): { string }
	build.setContext(configName, version)

	local artifacts: { string } = {}

	for _, task in ipairs(tasks) do
		local label = `{task.name} [{task.output}]`
		local ok, err = ui.withSpinner(`Building {label}...`, function()
			build.runTask(task)
		end, "yellow")

		if not ok then
			ui.printError(`Build error: {task.output}\n{tostring(err)}`)
			process.exit(1)
		end

		ui.printSuccess(`Build complete: {task.output}`)
		table.insert(artifacts, task.output)

		local opener = task.openComposedOutput or globalOpen
		if opener then
			ui.openFile(task.output, opener)
		end
	end

	return artifacts
end

--// Main

local args = parseArgs()

if args.showHelp then
	print(HELP_TEXT)
	process.exit(0)
end

if args.showVersion then
	print(`pcmp v{VERSION}`)
	process.exit(0)
end

if args.subcommand == "init" then
	scaffold.run()
	process.exit(0)
end

if not args.configPath or args.configPath == "" then
	print(HELP_TEXT)
	process.exit(1)
end

local cfg = config.load(args.configPath)
cfg = config.applyLocalOverrides(cfg, args.configPath)

local envFile = cfg.envFile or ".env"
local envVars = util.readEnvFile(envFile)

local buildConfigs = cfg.buildConfigs
if not buildConfigs or #buildConfigs == 0 then
	ui.printError("No buildConfigs defined in config.")
	process.exit(1)
end

local configNames: { string } = {}
for _, bc in ipairs(buildConfigs) do
	table.insert(configNames, bc.name)
end

local selectedIdx = io.prompt("select", "Select build configuration", configNames)
local selectedConfig = buildConfigs[selectedIdx]

currentBuildConfig = selectedConfig.name

if selectedConfig.promptVersion then
	currentVersion = promptVersion(selectedConfig.versionFormat)
else
	currentVersion = "0.0.0"
end

local tasks = config.expandBuildConfig(selectedConfig, cfg.openComposedOutput)
if #tasks == 0 then
	ui.printWarn("No build tasks found (empty inputDir?).")
	process.exit(0)
end

local artifacts = runBuilds(tasks, currentBuildConfig, currentVersion, cfg.openComposedOutput)
local isDeployable = selectedConfig.promptDeploy == true

if isDeployable and cfg.deployment then
	local github = cfg.deployment.github
	if not github then
		ui.printWarn("Deployment enabled but no GitHub config found.")
		process.exit(0)
	end

	local token = config.getToken(cfg, envVars)
	if not token or token == "" then
		local varName = github.tokenEnvVar or "GITHUB_TOKEN"
		ui.printError(`GitHub token not found. Set '{varName}' in your env file ({envFile}).`)
		process.exit(1)
	end

	local variantCount = #tasks - 1
	local filesSummary = util.getBaseName(artifacts[1])
	if variantCount > 0 then
		filesSummary ..= ` (+{variantCount} variant{variantCount == 1 and "" or "s"})`
	end

	ui.printSummaryTable("Release Summary", {
		{ label = "Config", value = currentBuildConfig },
		{ label = "Version", value = currentVersion },
		{ label = "Repo", value = `{github.owner}/{github.repo}` },
		{ label = "Files", value = filesSummary },
		{ label = "Type", value = selectedConfig.prerelease and "Pre-release" or "Stable" },
	})

	if not io.prompt("confirm", "Distribute this build publicly?") then
		ui.printInfo("Deploy cancelled.")
		process.exit(0)
	end

	local deployOk = deploy.run({
		owner = github.owner,
		repo = github.repo,
		token = token,
		version = currentVersion,
		configName = currentBuildConfig,
		prerelease = selectedConfig.prerelease == true,
		artifacts = artifacts,
		releaseBodyPath = RELEASE_BODY_PATH,
		releaseNotesEditor = cfg.releaseNotesEditor,
	})

	if not deployOk then
		process.exit(1)
	end
end
