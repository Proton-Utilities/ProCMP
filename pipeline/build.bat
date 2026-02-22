:: This is really bad

@echo off
setlocal EnableDelayedExpansion

set outputDir=generated
set exeName=pcmp.exe
set tempPath=%outputDir%\__temp_release

set targets=windows-x86_64 linux-x86_64 linux-aarch64 macos-x86_64 macos-aarch64

:: !!!!!!!!!!!!!!!!!! ::

set version=2.1.1-beta.1

:: !!!!!!!!!!!!!!!!!! ::

rmdir /s /q %outputDir%
mkdir %tempPath%

for %%T in (%targets%) do (
    echo Building for %%T...

    for /f "tokens=1,2 delims=-" %%A in ("%%T") do (
        set platform=%%A
        set arch=%%B
    )

    set zipName=pcmp-!version!-!platform!-!arch!.zip
    set zipPath=%outputDir%\!zipName!

    if exist "!zipPath!" del "!zipPath!"

    lune build src/pcmp.luau -o "%tempPath%\%exeName%" -t !platform!-!arch!

    powershell Compress-Archive -Path "%tempPath%\%exeName%" -DestinationPath "!zipPath!"

    echo Built and zipped: !zipName!
)

rmdir /s /q %tempPath%

echo All builds complete!
pause
