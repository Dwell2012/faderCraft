# Local stem EQ

Settings > Audio > Stem preparation now offers **Fast** (`htdemucs`, the default) and **Detailed (fine-tuned)** (`htdemucs_ft`). Detailed uses four source-specific models: upstream estimates roughly four times the inference time, with potentially improved separation. Download buttons fetch and verify either model from the official Demucs repository; both remain installed for offline use. The selected model is preloaded on startup and used for newly loaded tracks/jobs. Reload a deck after changing models to compare results. Existing attached stems finish/use their original model. Each model has an independent audio-keyed cache; the Fast cache keeps its previous identity. The persistent worker releases the previous model before loading the selected one. GPU preference and CPU fallback apply to both. Setup also accepts `-Model htdemucs_ft` or `-Model Both`; the check_stems example accepts the model name as its second argument.

The app preloads one persistent stem worker during startup. A splash remains until the model and a real inference warm-up finish. The same Python process and model serve subsequent tracks, retaining GPU weights between jobs. Settings changes apply per job. Failed workers are discarded and restarted on the next request; startup failure is logged and the app remains usable. Close the app to release the worker and GPU memory. Startup diagnostics are in stem-runtime/worker.log; track diagnostics remain in prepare.log.

Choose the EQ menu beside the Stems icon in the Live toolbar. Both decks use
the selected layout; gain trim, channel volume and crossfader keep their roles.

| Mode | Controls below trim |
| --- | --- |
| Classic EQ | High, Mid, Low frequency bands |
| EZRemix | Vocal; Instruments (instruments + bass); Beat (drums) |
| Modern EQ | Drums; Melody/Vox (instruments + vocals); Bass |
| Stems | Vocals, Instruments, Bass, Drums individually |

Stem knobs run from mute through unity at centre to +6 dB. Drag or use the
mousewheel; Shift-wheel is fine adjustment and double-click restores unity.
Switching mode resets stem gains to unity and restores the saved frequency EQ
when returning to Classic. These are DJCraft's four-source layouts, not copies
of VirtualDJ's proprietary five-source processing. Kick and hi-hat are part of
Drums. Separation can leave audible bleed and artifacts.

Click **Prepare stems** below a channel, or the toolbar Stems icon to prepare
both loaded decks. Preparation runs in the background, one track at a time.
The channel shows `Queued` while waiting and `Stems 0%` through `Stems 99%`
as audio chunks are processed and files saved, then `Stems ready` after validation.
Playback continues from the original until the four sources are ready. The
knobs are dimmed while unavailable. Hover the status button for error details;
click again to retry. Loading another track does not cancel an existing job:
that job finishes into cache without changing the newly loaded deck.

Prepared stems automatically load when that audio is loaded again. Cache keys
include decoded audio, sample rate and the preparation version. Incomplete or
wrong-length cache entries are ignored. The cache is under the app data
directory's `stems` folder (`DJCRAFT_DATA_DIR` is honoured). Four uncompressed
stereo float sources need about 92 MB per minute at 48 kHz. Source files are
read only. No music is uploaded and no account or subscription is required.

The Rust mixer applies the cached sources before tempo/key-lock processing and
scratching. Stem changes are smoothed per sample. At unity, playback uses the
original samples exactly. The separation residual is included in Instruments,
so all four sources reconstruct the original rather than changing unity level.

## Optional preparation runtime

The app and realtime mixer remain Rust. Offline AI preparation uses a separate
Python 3.12 worker with Demucs 4.0.1's `htdemucs` model. Settings > Audio >
Stem preparation offers Automatic (GPU preferred, the default) and CPU only.
Automatic uses available CUDA/ROCm or Apple MPS acceleration and retries on CPU
if GPU allocation or inference fails. CPU processing retains the existing cap:
half the logical processors, at most four threads. Changes apply to new jobs;
existing cached stems are reused. Whole-track buffers remain in system RAM,
with only model chunks sent to the GPU. Initial
preparation can take several minutes; subsequent cached loads are much faster.

On Windows, run `tools/setup-stems.ps1 -Python <path-to-python-3.12.exe>`.
It creates `%LOCALAPPDATA%\DJCraft\stem-runtime`, installs the pinned primary
dependencies, checks them and downloads the model from its official source.
Setup selects the pinned CUDA 12.1 PyTorch build when an NVIDIA card is detected,
or the CPU build otherwise. Use `-Device CPU` or `-Device CUDA` to override setup.
Re-running setup upgrades an existing runtime without redownloading its model.
CUDA use requires a compatible NVIDIA driver; unsupported hardware uses CPU.
The runtime remains a separate local install, including its CUDA library notices.
Once installed, separation works offline. Set `DJCRAFT_STEM_RUNTIME` to use
another runtime directory. Per-track `prepare.log` files report the actual
processing device and any GPU-to-CPU retry, alongside diagnostics.
Runtime/model files and caches remain local and can be removed when the app is
closed to reclaim space.

For an end-to-end check without opening a window, load the development shell and
run `cargo run -p djcraft --example check_stems -- <audio file>`. It prepares an
eight-second excerpt through the same worker as the UI, validates all four stems,
and caches the check under `DJCRAFT_DATA_DIR/stem-check`. The source file is read only.
Worker selection/retry tests run with the stem runtime's Python:
`python tools/test_prepare_stems.py` (no model download or GPU required).

## Dependency licences

Demucs code and pretrained weights are MIT (upstream
https://github.com/facebookresearch/demucs). PyTorch and torchaudio are BSD;
NumPy is BSD with bundled numerical-library notices. Demucs support packages
include MIT, BSD, Apache, Unlicense and MPL components. Its installed `lameenc`
dependency is LGPL-3.0-or-later; this PCM worker does not use MP3 encoding.
The Python environment is separately installed, not linked into the Rust app.
Keep the installed distributions' complete licence/notices if redistributing
that environment. Do not assume the Rust `cargo deny` check audits Python
packages. No GPL or AGPL dependency was added to the Rust application.

Live separation, batch preparation from the library, dedicated solo buttons,
job cancellation and automatic cache eviction are not implemented yet.

Loaded/prepared vocal stems now also produce a local vocal-activity map for
automix. The preparation strip displays purple vocal regions and the Automix
preview reports predicted overlaps; see [AUTOMIX.md](AUTOMIX.md).

## AMD Radeon on Windows
Setup now detects supported Radeon GPUs and offers -Device ROCm, using AMD's official Python 3.12 / ROCm 7.2.1 / PyTorch 2.9.1 wheels. The RX 9070 XT is supported by AMD; a compatible Windows 11 Radeon driver is required. The AMD environment lives in DJCraft/stem-runtime-rocm alongside the existing CPU/CUDA environment. Setup shares an existing verified model cache, checks dependencies, and performs real GPU inference without CPU fallback before writing stem-runtime-choice.txt to select it. DJCRAFT_STEM_RUNTIME still overrides the choice. Running setup with -Device CPU selects the original runtime again. Existing cached stems remain valid and are reused.
Official instructions: https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/install/installrad/windows/install-pytorch.html
GPU workers use PyTorch's cuda API on AMD as well as NVIDIA; worker.log must also name the actual adapter. On machines exposing multiple GPU adapters, the worker selects the adapter with the most reported GPU memory. A CPU-only PyTorch install cannot use an AMD card merely by enabling Automatic in settings. The optional Python runtime is separately installed; Rust licence checks do not audit its dependencies.

## Automatic deck preparation and manual track-menu action
Loading a track onto any deck now checks the selected model's cache and automatically separates if needed once decoding completes. The automatic processing budget is 20% of audio duration: a 5-minute song allows 60 seconds. Queue waiting and the initial cache lookup are excluded; cold worker/model startup, input writing and separation share the budget. The existing two-hour upper bound still applies. Cached stems load regardless of this budget.

The bounded receiver waits use the remaining budget even if the worker sends heartbeats. On inference timeout the owned worker process tree is terminated, partial outputs are discarded and no complete-cache marker is published. Ordinary deck playback continues. The deck reports Stopped with the reason in its tooltip; it does not retry continuously. A later job can start a clean worker.

Right-click a track in either Library or the performance browser and choose Separate stems. This prepares a cache without loading or disturbing a deck; completion/failure is reported in the browser. If already loaded, progress appears on that deck. This deliberate manual action (and the existing stem button) is not subject to the automatic 20% limit. Manual jobs retain the existing startup/worker safety limits. The selected Fast/Detailed model and GPU/CPU preference still apply.

-quick skips startup model warm-up, as before; loading an uncached track still triggers the new automatic work. Tests use a harmless subprocess protocol fixture to prove heartbeat/silent-worker deadline handling and process termination without Python, a model, an audio device or a window.

## Capture selected stems into the Sampler
In the Sampler panel, select a destination bank and pad slot, choose the source Deck A–D, then Capture. The default region is that deck's current loop; without a loop it captures four beats from its playhead. The length menu also offers 1, 2, 4, 8, 16 or 32 beats. Capture needs prepared stems and an enabled source; unlock the sampler bank first. The readout names the source stems that will be captured. A valid loop can be captured without a beat grid. Regions must fit the decoded source and be 10 ms to 120 seconds long.

Only the chosen deck's currently enabled stem mix is saved, including its stem gain levels. Other decks and master/sampler output do not enter the recording. The selection is frozen when Capture is pressed: later mutes, deck changes, eject or bank switching do not change the saved audio. Existing deck-pad capture now follows the same stem selection. Source music files remain read-only.

This is source-audio capture before deck faders, EQ and effects; transport tempo/key changes are not rendered into the sample. Saved samples play independently through the sampler bus.
