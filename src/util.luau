--!nocheck

--// Imports

local fs = require("@lune/fs")

--// Public Methods

local function getBaseName(path: string): string
	return path:match("^.+[\\/](.+)$") or path
end

local function getDirName(path: string): string
	return path:match("^(.+)[\\/][^\\/]+$") or "."
end

local function mergeOptions(base: { [string]: any }, override: { [string]: any }?): { [string]: any }
	local new = table.clone(base)
	if override then
		for k, v in pairs(override) do
			new[k] = v
		end
	end
	return new
end

local function readEnvFile(path: string): { [string]: string }
	local env: { [string]: string } = {}

	if not fs.isFile(path) then
		return env
	end

	for line in fs.readFile(path):gmatch("[^\r\n]+") do
		local key, val = line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
		if key and val then
			val = val:gsub("^[\"']", ""):gsub("[\"']$", "")
			env[key] = val
		end
	end

	return env
end

local function ensureDir(path: string)
	if not fs.isDir(path) then
		fs.writeDir(path)
	end
end

local function escapePattern(s: string): string
	return s:gsub("([%%%+%-%*%?%[%]%^%$%(%)%%])", "%%%1")
end

return {
	getBaseName = getBaseName,
	getDirName = getDirName,
	mergeOptions = mergeOptions,
	readEnvFile = readEnvFile,
	ensureDir = ensureDir,
	escapePattern = escapePattern,
}
