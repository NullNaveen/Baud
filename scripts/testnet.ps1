#!/usr/bin/env pwsh
#
# testnet.ps1 — Launch a local Baud testnet with 2 validators.
#
# Usage:
#   .\scripts\testnet.ps1          # Start testnet
#   .\scripts\testnet.ps1 -Stop    # Stop all nodes
#
# Requires: cargo build --release -p baud-node -p baud-cli
#

param(
    [switch]$Stop,
    [int]$Nodes = 2,
    [string]$BaseDir = "testnet"
)

$ErrorActionPreference = "Stop"
$BaudCli = "$PSScriptRoot\..\target\release\baud.exe"
$BaudNode = "$PSScriptRoot\..\target\release\baud-node.exe"

if ($Stop) {
    Write-Host "Stopping testnet nodes..." -ForegroundColor Yellow
    Get-Process -Name "baud-node" -ErrorAction SilentlyContinue | Stop-Process -Force
    Write-Host "Done." -ForegroundColor Green
    exit 0
}

# Verify binaries exist
if (-not (Test-Path $BaudCli)) { Write-Error "baud CLI not found at $BaudCli. Run: cargo build --release -p baud-cli"; exit 1 }
if (-not (Test-Path $BaudNode)) { Write-Error "baud-node not found at $BaudNode. Run: cargo build --release -p baud-node"; exit 1 }

Write-Host "`n  Baud Testnet Launcher" -ForegroundColor Cyan
Write-Host "  =====================`n"

# Create base directory
$BaseDir = Join-Path (Get-Location) $BaseDir
New-Item -ItemType Directory -Path $BaseDir -Force | Out-Null

# Generate validator keys
$validators = @()
for ($i = 0; $i -lt $Nodes; $i++) {
    $keyJson = & $BaudCli keygen | ConvertFrom-Json
    $validators += @{
        index = $i
        address = $keyJson.address
        secret_key = $keyJson.secret_key
    }
    Write-Host "  Validator $i : $($keyJson.address.Substring(0,16))..." -ForegroundColor Green
}

# Generate genesis
$secrets = ($validators | ForEach-Object { $_.secret_key }) -join ","
$genesisPath = Join-Path $BaseDir "genesis.json"
& $BaudCli genesis `
    --chain-id "baud-testnet" `
    --validators $secrets `
    --initial-balance 10000000 `
    --output $genesisPath

Write-Host "`n  Genesis written to $genesisPath" -ForegroundColor Cyan

# Build peer list
$basePeerPort = 9944
$baseApiPort = 8080
$peers = @()
for ($i = 0; $i -lt $Nodes; $i++) {
    $peers += "ws://127.0.0.1:$($basePeerPort + $i)"
}
$peerList = $peers -join ","

# Launch nodes
$jobs = @()
for ($i = 0; $i -lt $Nodes; $i++) {
    $v = $validators[$i]
    $dataDir = Join-Path $BaseDir "node-$i"
    New-Item -ItemType Directory -Path $dataDir -Force | Out-Null

    $apiAddr = "0.0.0.0:$($baseApiPort + $i)"
    $p2pAddr = "0.0.0.0:$($basePeerPort + $i)"

    Write-Host "`n  Starting node $i (API=$apiAddr, P2P=$p2pAddr)..." -ForegroundColor Yellow

    $args = @(
        "--genesis", $genesisPath,
        "--secret-key", $v.secret_key,
        "--api-addr", $apiAddr,
        "--p2p-addr", $p2pAddr,
        "--peers", $peerList,
        "--data-dir", $dataDir
    )

    $job = Start-Process -FilePath $BaudNode -ArgumentList $args `
        -RedirectStandardOutput (Join-Path $dataDir "stdout.log") `
        -RedirectStandardError (Join-Path $dataDir "stderr.log") `
        -PassThru -WindowStyle Hidden

    $jobs += $job
    Write-Host "  PID: $($job.Id)" -ForegroundColor DarkGray
}

Write-Host "`n  Testnet is running!" -ForegroundColor Green
Write-Host "  Nodes: $Nodes"
Write-Host "  API endpoints:"
for ($i = 0; $i -lt $Nodes; $i++) {
    Write-Host "    Node $i : http://localhost:$($baseApiPort + $i)"
}
Write-Host "`n  Explorer: Open docs/explorer.html and point to http://localhost:$baseApiPort"
Write-Host "  Stop with: .\scripts\testnet.ps1 -Stop`n"

# Save PIDs for stop script
$jobs | ForEach-Object { $_.Id } | Out-File (Join-Path $BaseDir "pids.txt")

# Wait for user
Write-Host "Press Ctrl+C to stop..." -ForegroundColor DarkGray
try { while ($true) { Start-Sleep -Seconds 5 } }
catch { }
finally {
    Write-Host "`nStopping nodes..." -ForegroundColor Yellow
    $jobs | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
    Write-Host "Testnet stopped." -ForegroundColor Green
}
