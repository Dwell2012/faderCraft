param([string]$Python = 'python', [ValidateSet('Auto', 'CUDA', 'ROCm', 'CPU')][string]$Device = 'Auto', [ValidateSet('htdemucs', 'htdemucs_ft', 'Both')][string]$Model = 'htdemucs')
$ErrorActionPreference = 'Stop'
$cards = @(Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name)
if ($Device -eq 'Auto') {
    $Device = if (@($cards | Where-Object { $_ -match 'NVIDIA' }).Count) { 'CUDA' }
        elseif (@($cards | Where-Object { $_ -match 'Radeon (RX 9070(?: XT)?|RX 9060 XT|RX 7900 XTX|PRO W7900(?: Dual Slot)?|AI PRO R9700|RX 7700)$' }).Count) { 'ROCm' }
        else { 'CPU' }
}
$runtimeName = if ($Device -eq 'ROCm') { 'stem-runtime-rocm' } else { 'stem-runtime' }
$stemRoot = Join-Path $env:LOCALAPPDATA 'DJCraft'
$stemRuntime = Join-Path $stemRoot $runtimeName
$stemPython = Join-Path $stemRuntime 'Scripts\python.exe'
if (-not (Test-Path -LiteralPath $stemPython)) {
    & $Python -m venv $stemRuntime
    if ($LASTEXITCODE -ne 0) { throw 'Python 3.12 is required for the tested Windows runtime.' }
}
if ($Device -eq 'ROCm') {
    # Official AMD Windows/Python 3.12 wheels; separate from the CPU runtime.
    $base = 'https://repo.radeon.com/rocm/windows/rocm-rel-7.2.1'
    & $stemPython -m pip install --upgrade "$base/rocm_sdk_core-7.2.1-py3-none-win_amd64.whl" "$base/rocm_sdk_devel-7.2.1-py3-none-win_amd64.whl" "$base/rocm_sdk_libraries_custom-7.2.1-py3-none-win_amd64.whl" "$base/rocm-7.2.1.tar.gz" "$base/torch-2.9.1%2Brocm7.2.1-cp312-cp312-win_amd64.whl" "$base/torchaudio-2.9.1%2Brocm7.2.1-cp312-cp312-win_amd64.whl"
} else {
    $wheel = if ($Device -eq 'CUDA') { 'cu121' } else { 'cpu' }
    & $stemPython -m pip install --upgrade "torch==2.5.1+$wheel" "torchaudio==2.5.1+$wheel" --index-url "https://download.pytorch.org/whl/$wheel"
}
if ($LASTEXITCODE -ne 0) { throw 'Could not install PyTorch.' }
& $stemPython -m pip install -r (Join-Path $PSScriptRoot 'stem-requirements.txt')
if ($LASTEXITCODE -ne 0) { throw 'Could not install stem preparation dependencies.' }
& $stemPython -m pip check
if ($LASTEXITCODE -ne 0) { throw 'Stem runtime dependencies are inconsistent.' }
# Share verified model downloads with an existing CPU runtime without copying.
$cpuModels = Join-Path $stemRoot 'stem-runtime\models'
$ownModels = Join-Path $stemRuntime 'models'
$env:TORCH_HOME = if ($Device -eq 'ROCm' -and (Test-Path -LiteralPath $cpuModels)) { $cpuModels } else { $ownModels }
[System.IO.File]::WriteAllText((Join-Path $stemRuntime 'model-cache-path.txt'), $env:TORCH_HOME)
$stemModels = if ($Model -eq 'Both') { @('htdemucs', 'htdemucs_ft') } else { @($Model) }
foreach ($stemModel in $stemModels) {
    & $stemPython (Join-Path $PSScriptRoot 'prepare_stems.py') --download $stemModel
    if ($LASTEXITCODE -ne 0) { throw "Could not download $stemModel." }
    if ($Device -eq 'ROCm') {
        & $stemPython (Join-Path $PSScriptRoot 'prepare_stems.py') --verify-gpu $stemModel
        if ($LASTEXITCODE -ne 0) { throw 'AMD GPU inference failed; previous runtime selection is unchanged.' }
    }
}
# Activate only after installation and real GPU verification have succeeded.
[System.IO.File]::WriteAllText((Join-Path $stemRoot 'stem-runtime-choice.txt'), $runtimeName)
Write-Host "Stem preparation ready: $stemRuntime ($Device)"
& $stemPython -c "import torch; print('GPU: ' + torch.cuda.get_device_name(0) if torch.cuda.is_available() else 'CPU fallback ready')"
