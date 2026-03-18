# Baud — One-Command Windows Installer
# Run: irm https://nullnaveen.github.io/Baud/install.ps1 | iex
#
# This script:
#   1. Checks for Git and Rust (installs Rust if missing)
#   2. Clones the Baud repository
#   3. Builds baud-node from source
#   4. Generates a unique validator keypair
#   5. Creates Start Menu shortcuts
#   6. Prints a summary with your address and secret key

$ErrorActionPreference = "Stop"
$BaudDir = "$env:USERPROFILE\Baud"
$StartMenu = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Baud"

Write-Host ""
Write-Host "  ========================================" -ForegroundColor Cyan
Write-Host "    BAUD — M2M Agent Ledger Installer" -ForegroundColor Cyan
Write-Host "  ========================================" -ForegroundColor Cyan
Write-Host ""

# --- Check Git ---
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Host "[ERROR] Git is not installed." -ForegroundColor Red
    Write-Host "  Install from: https://git-scm.com/download/win" -ForegroundColor Yellow
    Write-Host "  Then re-run this installer." -ForegroundColor Yellow
    return
}
Write-Host "[OK] Git found: $(git --version)" -ForegroundColor Green

# --- Check / Install Rust ---
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-Host "[...] Rust not found. Installing via rustup..." -ForegroundColor Yellow
    $rustupInit = "$env:TEMP\rustup-init.exe"
    Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $rustupInit -UseBasicParsing
    & $rustupInit -y --default-toolchain stable
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
        Write-Host "[ERROR] Rust installation failed. Visit https://rustup.rs" -ForegroundColor Red
        return
    }
}
Write-Host "[OK] Rust found: $(rustc --version)" -ForegroundColor Green

# --- Clone or update repo ---
if (Test-Path "$BaudDir\.git") {
    Write-Host "[...] Updating existing Baud installation..." -ForegroundColor Yellow
    Push-Location $BaudDir
    git pull origin main --ff-only 2>&1 | Out-Null
    Pop-Location
} else {
    Write-Host "[...] Cloning Baud repository..." -ForegroundColor Yellow
    git clone https://github.com/NullNaveen/Baud.git $BaudDir 2>&1 | Out-Null
}
Write-Host "[OK] Source code ready at $BaudDir" -ForegroundColor Green

# --- Build ---
Write-Host "[...] Building baud-node (this takes 1-3 minutes)..." -ForegroundColor Yellow
Push-Location $BaudDir
cargo build --bin baud-node --release 2>&1 | Out-Null
if (-not (Test-Path "target\release\baud-node.exe")) {
    Write-Host "[ERROR] Build failed. Check that Rust and a C linker are installed." -ForegroundColor Red
    Pop-Location
    return
}
Pop-Location
Write-Host "[OK] baud-node.exe built successfully" -ForegroundColor Green

# --- Generate keypair ---
Write-Host "[...] Generating your validator keypair..." -ForegroundColor Yellow
$keyOutput = & "$BaudDir\target\release\baud-node" keygen 2>&1
# Parse the key output - try to extract address and secret
# If keygen subcommand doesn't exist, generate keys via the API approach
$secretKey = ""
$address = ""

# Try running baud CLI keygen if available
$baudCli = "$BaudDir\target\release\baud.exe"
if (Test-Path $baudCli) {
    $keyOutput = & $baudCli keygen 2>&1 | Out-String
    if ($keyOutput -match "secret[_\s]*key[:\s]+([0-9a-f]{64})" ) {
        $secretKey = $Matches[1]
    }
    if ($keyOutput -match "address[:\s]+([0-9a-f]{64})") {
        $address = $Matches[1]
    }
}

# Fallback: generate via PowerShell using random bytes
if (-not $secretKey) {
    # Generate a 32-byte random secret key
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $bytes = New-Object byte[] 32
    $rng.GetBytes($bytes)
    $secretKey = ($bytes | ForEach-Object { $_.ToString("x2") }) -join ""
    Write-Host "[INFO] Generated random secret key (address will be shown when node starts)" -ForegroundColor Yellow
}

# Save the key securely
$keyFile = "$BaudDir\my-secret-key.txt"
Set-Content -Path $keyFile -Value "# BAUD VALIDATOR SECRET KEY — KEEP THIS SAFE! NEVER SHARE IT!`n# Generated: $(Get-Date -Format o)`n$secretKey"
Write-Host "[OK] Secret key saved to $keyFile" -ForegroundColor Green

# --- Create start-node.bat with the user's key ---
$startBat = @"
@echo off
title Baud Node
echo ========================================
echo   BAUD - M2M Agent Ledger Node
echo ========================================
echo.
cd /d "$BaudDir"
set NODE_EXE=target\release\baud-node.exe
if not exist "%NODE_EXE%" (
    echo [INFO] Building baud-node...
    cargo build --bin baud-node --release
    if errorlevel 1 (
        echo [ERROR] Build failed.
        pause
        exit /b 1
    )
)
echo Starting node...
echo Dashboard: http://localhost:8080
echo.
start "" "http://localhost:8080"
%NODE_EXE% --secret-key $secretKey
"@
Set-Content -Path "$BaudDir\start-node.bat" -Value $startBat

# --- Create stop-node.bat ---
$stopBat = @"
@echo off
title Stop Baud Node
taskkill /IM baud-node.exe /F >nul 2>&1
echo Baud node stopped.
timeout /t 2 >nul
"@
Set-Content -Path "$BaudDir\stop-node.bat" -Value $stopBat

# --- Create dashboard.bat ---
$dashBat = @"
@echo off
start "" "http://localhost:8080"
"@
Set-Content -Path "$BaudDir\dashboard.bat" -Value $dashBat

# --- Create Start Menu shortcuts ---
if (-not (Test-Path $StartMenu)) { New-Item -ItemType Directory -Path $StartMenu -Force | Out-Null }

$icoPath = "$BaudDir\docs\assets\baud.ico"
$WshShell = New-Object -ComObject WScript.Shell

# Baud Node shortcut
$sc = $WshShell.CreateShortcut("$StartMenu\Baud Node.lnk")
$sc.TargetPath = "$BaudDir\start-node.bat"
$sc.WorkingDirectory = $BaudDir
$sc.Description = "Start mining BAUD and open dashboard"
if (Test-Path $icoPath) { $sc.IconLocation = $icoPath }
$sc.Save()

# Dashboard shortcut
$sc = $WshShell.CreateShortcut("$StartMenu\Baud Dashboard.lnk")
$sc.TargetPath = "$BaudDir\dashboard.bat"
$sc.WorkingDirectory = $BaudDir
$sc.Description = "Open Baud dashboard in browser"
if (Test-Path $icoPath) { $sc.IconLocation = $icoPath }
$sc.Save()

# Stop shortcut
$sc = $WshShell.CreateShortcut("$StartMenu\Stop Baud Node.lnk")
$sc.TargetPath = "$BaudDir\stop-node.bat"
$sc.WorkingDirectory = $BaudDir
$sc.Description = "Stop the Baud node"
if (Test-Path $icoPath) { $sc.IconLocation = $icoPath }
$sc.Save()

# Refresh icon cache
ie4uinit.exe -show 2>$null

# --- Done! ---
Write-Host ""
Write-Host "  ========================================" -ForegroundColor Green
Write-Host "    BAUD INSTALLED SUCCESSFULLY!" -ForegroundColor Green
Write-Host "  ========================================" -ForegroundColor Green
Write-Host ""
Write-Host "  Install location:  $BaudDir" -ForegroundColor White
Write-Host "  Secret key file:   $keyFile" -ForegroundColor White
Write-Host "  Secret key:        $secretKey" -ForegroundColor Yellow
if ($address) {
    Write-Host "  Node address:      $address" -ForegroundColor White
}
Write-Host ""
Write-Host "  START MENU SHORTCUTS (press Win key, type 'Baud'):" -ForegroundColor Cyan
Write-Host "    Baud Node       — Start mining + open dashboard" -ForegroundColor White
Write-Host "    Baud Dashboard  — Open dashboard in browser" -ForegroundColor White
Write-Host "    Stop Baud Node  — Stop mining" -ForegroundColor White
Write-Host ""
Write-Host "  TO START MINING:" -ForegroundColor Cyan
Write-Host "    1. Press the Windows key" -ForegroundColor White
Write-Host "    2. Type 'Baud Node'" -ForegroundColor White
Write-Host "    3. Hit Enter" -ForegroundColor White
Write-Host "    4. Dashboard opens at http://localhost:8080" -ForegroundColor White
Write-Host ""
Write-Host "  IMPORTANT: Back up your secret key! If lost, your BAUD is gone forever." -ForegroundColor Red
Write-Host ""
