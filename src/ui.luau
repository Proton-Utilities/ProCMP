--!nocheck

--// Imports

local io = require("@lune/stdio")
local task = require("@lune/task")
local process = require("@lune/process")

--// Constants

local SPINNER_FRAMES = { "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏" }

local BOX_H = "─"
local BOX_TL = "┌"
local BOX_TR = "┐"
local BOX_BL = "└"
local BOX_BR = "┘"
local BOX_V = "│"

--// Private Methods

local function colorWrap(text: string, color: io.Color): string
	return io.color(color) .. text .. io.color("reset")
end

local function padRight(s: string, width: number): string
	local len = #s
	if len >= width then
		return s
	end
	return s .. string.rep(" ", width - len)
end

--// Public Methods

local function withSpinner(msg: string, fn: () -> any, colorName: io.Color?): (boolean, any)
	local done = false
	local idx = 1

	task.spawn(function()
		while not done do
			local frame = SPINNER_FRAMES[idx]
			if colorName then
				io.write(io.color(colorName))
			end
			io.write("\r" .. frame .. " " .. msg)
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

local function printSuccess(msg: string)
	io.write(io.color("green"))
	print("✔ " .. msg)
	io.write(io.color("reset"))
end

local function printError(msg: string)
	io.write(io.color("red"))
	print("✘ " .. msg)
	io.write(io.color("reset"))
end

local function printWarn(msg: string)
	io.write(io.color("yellow"))
	print("⚠ " .. msg)
	io.write(io.color("reset"))
end

local function printInfo(msg: string)
	io.write(io.color("cyan"))
	print("ℹ " .. msg)
	io.write(io.color("reset"))
end

type SummaryRow = { label: string, value: string }

local function printSummaryTable(title: string, rows: { SummaryRow })
	local maxLabel = #title
	local maxValue = 0

	for _, row in ipairs(rows) do
		if #row.label > maxLabel then
			maxLabel = #row.label
		end
		if #row.value > maxValue then
			maxValue = #row.value
		end
	end

	local innerWidth = maxLabel + 2 + maxValue
	local titlePadded = " " .. title .. " "
	local hLine = string.rep(BOX_H, innerWidth + 2)

	local topFill = string.rep(BOX_H, math.floor((innerWidth + 2 - #titlePadded) / 2))
	local topFillR = string.rep(BOX_H, (innerWidth + 2) - #titlePadded - #topFill)
	io.write(io.color("cyan"))
	print(BOX_TL .. topFill .. titlePadded .. topFillR .. BOX_TR)

	for _, row in ipairs(rows) do
		local labelPad = padRight(row.label, maxLabel)
		print(BOX_V .. " " .. colorWrap(labelPad, "white") .. "  " .. row.value .. " " .. BOX_V)
	end

	print(BOX_BL .. hLine .. BOX_BR)
	io.write(io.color("reset"))
end

type OpenFileConfig = {
	command: string,
	args: { string },
}

local function openFile(filePath: string, cfg: OpenFileConfig?)
	if not cfg then
		return
	end

	local resolvedArgs: { string } = {}
	for _, arg in ipairs(cfg.args) do
		table.insert(resolvedArgs, (arg:gsub("{file}", filePath)))
	end

	process.exec(cfg.command, resolvedArgs)
end

return {
	withSpinner = withSpinner,
	printSuccess = printSuccess,
	printError = printError,
	printWarn = printWarn,
	printInfo = printInfo,
	printSummaryTable = printSummaryTable,
	openFile = openFile,
}
