@echo off
setlocal EnableDelayedExpansion

set outputDir=generated
set exeName=pcmp.exe
set tempPath=%outputDir%\__temp_release
set bundlePath=%outputDir%\__bundle.luau
set darkluaConfig=pipeline\darklua.json

set targets=windows-x86_64 linux-x86_64 linux-aarch64 macos-x86_64 macos-aarch64

:: ── Version ──────────────────────────────────────────────────
set version=3.0.1
:: ─────────────────────────────────────────────────────────────

rmdir /s /q %outputDir%
mkdir %tempPath%

:: ── Bundle src/ into a single file ───────────────────────────
echo Bundling source with darklua...
darklua process src/init.luau %bundlePath% -c %darkluaConfig%
if %errorlevel% neq 0 (
    echo Darklua bundling failed.
    exit /b 1
)
echo Bundle complete: %bundlePath%
:: ─────────────────────────────────────────────────────────────

for %%T in (%targets%) do (
    echo Building for %%T...

    for /f "tokens=1,2 delims=-" %%A in ("%%T") do (
        set platform=%%A
        set arch=%%B
    )

    set zipName=pcmp-!version!-!platform!-!arch!.zip
    set zipPath=%outputDir%\!zipName!

    if exist "!zipPath!" del "!zipPath!"

    lune build %bundlePath% -o "%tempPath%\%exeName%" -t !platform!-!arch!

    powershell Compress-Archive -Path "%tempPath%\%exeName%" -DestinationPath "!zipPath!"

    echo Built: !zipName!
)

del %bundlePath%
rmdir /s /q %tempPath%

echo.
echo All builds complete.
pause
