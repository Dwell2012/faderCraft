# Dot-source from PowerShell: . ./tools/dev-shell.ps1
# Environment changes apply only to the current shell and its child processes.
$ErrorActionPreference = 'Stop'
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (-not (Test-Path (Join-Path $cargoBin 'cargo.exe'))) {
    throw 'Install Rust with rustup before opening the development shell.'
}
$env:Path = "$cargoBin;$env:Path"
$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'DJCraft\target'
$env:DJCRAFT_DATA_DIR = Join-Path $env:LOCALAPPDATA 'DJCraft-Development'
if (-not $env:CPAL_ASIO_DIR) {
    $localAsioSdk = Join-Path $env:LOCALAPPDATA 'DJCraft\build-tools\asio\ASIOSDK'
    if ((Test-Path (Join-Path $localAsioSdk 'common\asio.h')) -and
        (Test-Path (Join-Path $localAsioSdk 'host'))) {
        $env:CPAL_ASIO_DIR = $localAsioSdk
    }
}

if (-not $env:LIBCLANG_PATH) {
    $clangCandidates = @(
        (Join-Path $env:ProgramFiles 'LLVM\bin'),
        (Join-Path $env:LOCALAPPDATA 'DJCraft\build-tools\clang\native')
    )
    foreach ($candidate in $clangCandidates) {
        if (Test-Path (Join-Path $candidate 'libclang.dll')) {
            $env:LIBCLANG_PATH = $candidate
            break
        }
    }
}
if (-not $env:LIBCLANG_PATH -or -not (Test-Path (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) {
    throw 'Install LLVM or set LIBCLANG_PATH to the directory containing libclang.dll.'
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path $vswhere)) {
    throw 'Install Visual Studio Desktop development with C++, including x64 MSVC and a Windows SDK.'
}
$vsInstall = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsInstall) {
    throw 'Visual Studio needs the Desktop development with C++ workload, including x64 MSVC and a Windows SDK.'
}
& (Join-Path $vsInstall 'Common7\Tools\Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    throw 'The Visual Studio development shell could not locate the C++ compiler.'
}
Write-Host "Development data: $env:DJCRAFT_DATA_DIR"
Write-Host "Build output: $env:CARGO_TARGET_DIR"
Write-Host 'Ready: cargo build --workspace --locked; cargo run --locked'
