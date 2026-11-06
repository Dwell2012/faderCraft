# Local stem separation

faderCraft separates four stems (vocals, other instruments, bass and drums). Settings > Audio > Stem preparation offers **Fast** (`htdemucs`) and **Detailed** (`htdemucs_ft`) for both ONNX CPU and ROCm GPU. The native ONNX path runs without Python or PyTorch. Fast is one 316 MB ONNX graph. Detailed consists of four 316 MB specialist graphs (drums, bass, other and vocals), about 1.27 GB total. The app loads the Detailed specialists one at a time to limit model memory. ONNX inference uses the CPU execution provider with up to four threads.

ONNX models are downloaded into `%LOCALAPPDATA%\DJCraft\onnx-models`, never the music library or OneDrive folder. `DJCRAFT_ONNX_MODEL_DIR` can override this directory for testing; `DJCRAFT_ONNX_MODEL` still overrides the Fast graph path. Detailed downloads use pinned files from [StemSplitio's ONNX fine-tuned bag](https://huggingface.co/StemSplitio/htdemucs-ft-onnx), with exact file-size and SHA-256 verification. Existing downloaded files are retained. The Fast graph comes from [StemSplitio's baseline export](https://huggingface.co/StemSplitio/htdemucs-onnx). The ONNX graphs are third-party exports of the [Demucs](https://github.com/facebookresearch/demucs) models. Neither model is bundled with the app.

**ROCm GPU** uses an optional local Python environment with ROCm-enabled PyTorch, torchaudio and Demucs 4.0.1. On Windows the default environment is `%LOCALAPPDATA%\DJCraft\stem-runtime-rocm`. Set `DJCRAFT_ROCM_RUNTIME` to use a different environment. The app uses that environment's Python and writes its own worker script there. The ROCm download buttons fetch the original Demucs checkpoints. The worker shares an existing checkpoint cache when `model-cache-path.txt` in that environment points to its Torch home. The worker verifies GPU inference during startup and reports errors if it cannot use a compatible AMD GPU. Its diagnostics go to `rocm-worker.log` and each job's `prepare.log`.

The selected model is opened during startup unless `-quick` is used. Changing the selection affects newly started separation jobs; reload a deck to compare a different cached result. An unavailable model does not prevent ordinary DJ playback, but its separation job reports an error. Closing the app releases the model session.

Choose the EQ menu beside the Stems icon in the Live toolbar. Both decks use the selected layout; gain trim, channel volume and crossfader retain their roles.

| Mode | Controls below trim |
| --- | --- |
| Classic EQ | High, Mid, Low frequency bands |
| EZRemix | Vocal; Instruments (instruments + bass); Beat (drums) |
| Modern EQ | Drums; Melody/Vox (instruments + vocals); Bass |
| Stems | Vocals, Instruments, Bass, Drums individually |

Stem knobs run from mute through unity at centre to +6 dB. Drag or use the mousewheel; Shift-wheel is fine adjustment and double-click restores unity. Separation can leave audible bleed and artifacts. At unity, playback uses the original samples exactly. The separation residual is included in Instruments so the four sources reconstruct the original.

Loading a track checks the selected model's cache and automatically prepares stems if needed once decoding finishes. Automatic preparation has a processing budget of 20% of track duration. A completed cache bypasses that budget. Automatic timeout or failure leaves ordinary playback working and does not publish a complete-cache marker. The deck shows Queued and percentage progress, then Ready or an error.

Right-click a track in either Library or the performance browser and choose **Separate stems** for an unbounded manual preparation without loading a deck. The library's Stems column shows Queued, live progress or Ready. The deck's Prepare stems button also runs an unbounded manual job. Jobs keep their own model selection even if Settings changes while they run.

Cache keys include decoded audio, sample rate, model and backend. Each model and backend keeps its own cache; incomplete or wrong-length entries are ignored. Cached stems live under the app data directory's `stems` folder (`DJCRAFT_DATA_DIR` is honoured). Four uncompressed stereo float sources need about 92 MB per minute at 48 kHz. Source music files are read-only, and no music is uploaded.

The Rust mixer applies cached stems before tempo/key-lock processing and scratching, with smoothed gain changes. In the Sampler panel, select a destination bank and pad slot, choose Deck A–D, then Capture. The default region is the deck's loop or four beats at its playhead. The length menu also offers 1, 2, 4, 8, 16 and 32 beats. Capture needs prepared stems and an enabled source; unlock the sampler bank first. It saves only the selected deck's enabled stem mix and gain levels, before deck faders, EQ, effects, tempo and key changes. Regions must fit the decoded source and last 10 ms to 120 seconds.

For a short local pipeline test, set `DJCRAFT_DATA_DIR` to a local folder and run `cargo run -p djcraft --example check_stems -- <audio-file> [htdemucs|htdemucs_ft|rocm-htdemucs|rocm-htdemucs_ft]`. The example validates four attached stems from an eight-second excerpt. `cargo run -p djcraft --example check_stems -- --download htdemucs_ft` downloads the Detailed models to the machine-local model directory.
