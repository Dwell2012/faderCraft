# Launch the development app with the bundled Numark MIDI-address profile.
# Copy it to the development data directory once so personal edits survive pulls.
param([string]$MappingPath)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'dev-shell.ps1')
if (-not $MappingPath) {
    $profileDir = Join-Path $env:DJCRAFT_DATA_DIR 'controllers'
    New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
    $MappingPath = Join-Path $profileDir 'numark-mixtrack-pro-ii.json'
    if (-not (Test-Path -LiteralPath $MappingPath)) {
        Copy-Item -LiteralPath (Join-Path $repoRoot 'profiles\numark-mixtrack-pro-ii.json') -Destination $MappingPath
    }
}
$env:DJCRAFT_CONTROLLER_MAP = (Resolve-Path -LiteralPath $MappingPath).Path
$appPath = Join-Path $env:CARGO_TARGET_DIR 'debug\djcraft.exe'
if (-not (Test-Path -LiteralPath $appPath)) {
    throw 'Build first with cargo build --workspace --locked.'
}
Write-Host "Controller profile: $env:DJCRAFT_CONTROLLER_MAP"
$logDir = Join-Path $env:DJCRAFT_DATA_DIR 'logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$logStamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$appProcess = Start-Process -FilePath $appPath -WorkingDirectory $repoRoot -RedirectStandardOutput (Join-Path $logDir "mixtrack-$logStamp.out.log") -RedirectStandardError (Join-Path $logDir "mixtrack-$logStamp.err.log") -PassThru
Write-Host "faderCraft started (PID $($appProcess.Id)). Logs: $logDir"
