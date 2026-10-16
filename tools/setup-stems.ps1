param([string]$Python = 'python')
$ErrorActionPreference = 'Stop'
$stemRuntime = Join-Path $env:LOCALAPPDATA 'DJCraft\stem-runtime'
& $Python -m venv $stemRuntime
if ($LASTEXITCODE -ne 0) { throw 'Python 3.12 is required for the tested Windows runtime.' }
$stemPython = Join-Path $stemRuntime 'Scripts\python.exe'
& $stemPython -m pip install torch==2.5.1 torchaudio==2.5.1 --index-url https://download.pytorch.org/whl/cpu
if ($LASTEXITCODE -ne 0) { throw 'Could not install PyTorch.' }
& $stemPython -m pip install -r (Join-Path $PSScriptRoot 'stem-requirements.txt')
if ($LASTEXITCODE -ne 0) { throw 'Could not install stem preparation dependencies.' }
& $stemPython -m pip check
if ($LASTEXITCODE -ne 0) { throw 'Stem runtime dependencies are inconsistent.' }
$env:TORCH_HOME = Join-Path $stemRuntime 'models'
& $stemPython -c "from demucs.pretrained import get_model; print(get_model('htdemucs').sources)"
if ($LASTEXITCODE -ne 0) { throw 'Could not download or load the model.' }
Write-Host "Stem preparation ready: $stemRuntime"
