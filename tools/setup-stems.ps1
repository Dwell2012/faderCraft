param([string]$Python = 'python', [ValidateSet('Auto', 'CUDA', 'CPU')][string]$Device = 'Auto', [ValidateSet('htdemucs', 'htdemucs_ft', 'Both')][string]$Model = 'htdemucs')
$ErrorActionPreference = 'Stop'
$stemRuntime = Join-Path $env:LOCALAPPDATA 'DJCraft\stem-runtime'
$stemPython = Join-Path $stemRuntime 'Scripts\python.exe'
if (-not (Test-Path -LiteralPath $stemPython)) {
    & $Python -m venv $stemRuntime
    if ($LASTEXITCODE -ne 0) { throw 'Python 3.12 is required for the tested Windows runtime.' }
}
$useCuda = $Device -eq 'CUDA' -or ($Device -eq 'Auto' -and
    @(Get-CimInstance Win32_VideoController | Where-Object Name -Match 'NVIDIA').Count -gt 0)
$wheel = if ($useCuda) { 'cu121' } else { 'cpu' }
& $stemPython -m pip install --upgrade "torch==2.5.1+$wheel" "torchaudio==2.5.1+$wheel" --index-url "https://download.pytorch.org/whl/$wheel"
if ($LASTEXITCODE -ne 0) { throw 'Could not install PyTorch.' }
& $stemPython -m pip install -r (Join-Path $PSScriptRoot 'stem-requirements.txt')
if ($LASTEXITCODE -ne 0) { throw 'Could not install stem preparation dependencies.' }
& $stemPython -m pip check
if ($LASTEXITCODE -ne 0) { throw 'Stem runtime dependencies are inconsistent.' }
$env:TORCH_HOME = Join-Path $stemRuntime 'models'
$stemModels = if ($Model -eq 'Both') { @('htdemucs', 'htdemucs_ft') } else { @($Model) }
foreach ($stemModel in $stemModels) {
    & $stemPython (Join-Path $PSScriptRoot 'prepare_stems.py') --download $stemModel
    if ($LASTEXITCODE -ne 0) { throw "Could not download $stemModel." }
}
if ($LASTEXITCODE -ne 0) { throw 'Could not download or load the model.' }
Write-Host "Stem preparation ready: $stemRuntime"
& $stemPython -c "import torch; print('GPU: ' + torch.cuda.get_device_name(0) if torch.cuda.is_available() else 'CPU fallback ready')"
