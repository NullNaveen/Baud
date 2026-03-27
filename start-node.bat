@echo off
title Baud Node
echo ================================================
echo   Baud Node - M2M Cryptocurrency
echo ================================================
echo.

set "BAUD_EXE=%~dp0target\release\baud-node.exe"

if not exist "%BAUD_EXE%" (
    echo [!] Release binary not found. Building...
    cargo build --release -p baud-node
    if errorlevel 1 (
        echo [ERROR] Build failed!
        pause
        exit /b 1
    )
)

if "%BAUD_SECRET_KEY%"=="" (
    echo [!] BAUD_SECRET_KEY environment variable not set.
    echo     Set it before running, or pass --secret-key as argument.
    echo.
    echo     Example:
    echo       set BAUD_SECRET_KEY=your_hex_key_here
    echo       start-node.bat
    echo.
    echo     Or:
    echo       start-node.bat --secret-key your_hex_key_here
    echo.
    "%BAUD_EXE%" %*
) else (
    echo Starting node...
    "%BAUD_EXE%" --secret-key %BAUD_SECRET_KEY% %*
)

pause
