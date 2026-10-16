# Codex handover

Updated 16 October 2026. Read this and `AGENTS.md` before changing code.

## Repository

Personal private repository: https://github.com/Dwell2012/djcraft. Shared branch:
`origin/main`. Original project is https://github.com/danielchim/djcraft (`upstream`);
retain its MIT licence and attribution. Custom implementation checkpoint:
`bd5ed92888faab1f800943dd1692681ce33ace6f`.

The active original-machine checkout is `C:\Users\Ian Llewellyn\repos\DJ\djcraft`.
The old OneDrive DJ directory is stale; some sessions still start there. Verify the
working directory before editing. The original-machine working branch is
`codex/windows-controller-foundation`, tracking `origin/main`; its local `main`
still points at the old upstream baseline. Fresh clones use the personal main.

This is a custom Windows Rust/GPUI DJ workstation. Historical README sections and
early `CUSTOM-DEVELOPMENT.md` sections describe the original baseline. Use current
code, `FEATURES.md` and specialist documents for current capabilities.

## Another Windows machine

Authenticate Git/GitHub for the private repository and clone outside OneDrive:

```powershell
git clone https://github.com/Dwell2012/djcraft.git "$env:USERPROFILE\repos\djcraft"
Set-Location "$env:USERPROFILE\repos\djcraft"
git remote add upstream https://github.com/danielchim/djcraft.git
```

Install Rust through rustup (repository pin: **1.99.0**), Visual Studio Desktop
Development with C++ including x64 MSVC libraries and a Windows SDK, and libclang
for bindgen. A linker alone is insufficient: missing `msvcrt.lib` previously
blocked compilation. Preserve pinned versions unless updating them is the task.

Windows builds currently enable CPAL ASIO even when runtime output uses WASAPI.
Read [ASIO.md](ASIO.md) for SDK setup and terms. If automatic extraction fails,
set `CPAL_ASIO_DIR` to the SDK directory containing `common` and `host`.
SDK and libclang binaries are not committed.

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'DJCraft\target'
# Replace with the directory containing libclang.dll on this machine:
$env:LIBCLANG_PATH = 'C:\path\to\clang\bin'
$env:DJCRAFT_DATA_DIR = Join-Path $env:LOCALAPPDATA 'DJCraft-Development'
cargo build --workspace --locked
cargo run --locked
# Optional local tracks:
cargo run --locked -- 'C:\Music\track-a.mp3' 'C:\Music\track-b.mp3'
```

Original-machine libclang location:
`$env:LOCALAPPDATA\DJCraft\build-tools\clang\native`. Do not assume it exists on
another machine. Resolve LOCALAPPDATA from the environment, not a hardcoded path.
Windows locks running executables: close DJCraft before replacing its build.
Coordinate closing with the user if it is playing. Always isolate development data.

## Checks

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
# Install once if needed: cargo install cargo-deny --locked
cargo deny check licenses sources
git diff --check
cargo build --workspace --locked
```

At the implementation checkpoint: **191 tests passed, 0 failed, 1 ignored**;
formatting and licence/source checks passed. Clippy was unavailable for the installed
pinned toolchain and was not a passing check. These are historical results; verify
subsequent code changes. Inspect UI changes in the running app and test hardware
claims on the actual device. Other platforms need their own validation.

## State that is separate from Git

Git carries source, Cargo.lock, documentation and stem setup tools. It does not carry
music, library/settings, imported samples, waveform/stem caches, Python runtime/model,
drivers, SDKs or compiled binaries.

To preserve personal state, close the app and separately back up the complete
`DJCRAFT_DATA_DIR` directory. Original development setup uses
`$env:LOCALAPPDATA\DJCraft-Development`. Without the override, `settings::data_dir()`
uses the OS application-data directory's `djcraft` folder. Preserve settings,
library, saved cue/loop/preparation/playlist state and sampler assets together.
Settings can contain lookup-service credentials: do not commit them.
Music and some per-track metadata are path based; changed paths/drives require
relinking and checking saved cues. Copy music separately or start a fresh library.

User-supplied Kick, Clap, Snare, Siren and two Airhorn WAVs replaced synth sampler
defaults locally. These imported files are not bundled in Git; migrate sampler data.

Optional local stem preparation uses a separate Python 3.12 CPU Demucs worker:

```powershell
./tools/setup-stems.ps1 -Python 'C:\path\to\python312\python.exe'
$env:DJCRAFT_STEM_RUNTIME = Join-Path $env:LOCALAPPDATA 'DJCraft\stem-runtime'
```

See [STEMS.md](STEMS.md) for model/runtime downloads, cache size and dependency
notices. Four real sources: vocals, instruments, bass, drums. Kick/hi-hat are not
independently separated. Preparation is local and cached, with percentage progress.

## Architecture

- `crates/protocol`: actions and shared control data. Route actions through
  `Engine::apply`; UI is a client of the protocol.
- `crates/engine`: realtime mixing, effects, EQ, loops, sampler, scratching,
  tempo/key lock and output. No UI dependencies. No allocations, locks, file I/O
  or logging inside the realtime mixer callback.
- `crates/analysis`: decoded-sample beat/downbeat, tempo, key, energy and cue analysis.
- `crates/library`: SQLite, scanning, playlists and tags. New schema changes require
  migrations. Preserve the existing reviewed/undoable music-tag write flow.
- `crates/app/src/shell.rs`: deck layout and coordination.
- `crates/app/src/effect_controls.rs`: effects UI.
- `crates/app/src/settings_view.rs`: settings UI; `settings.rs` owns data paths.
- `crates/engine/src/mixer.rs`: audio behavior and headless regression tests.
- `crates/engine/src/wasapi_output.rs`: direct exclusive WASAPI output.

Feature details: [FEATURES.md](FEATURES.md), [EFFECTS.md](EFFECTS.md),
[LOOPS.md](LOOPS.md), [PADS.md](PADS.md), [PREPARATION.md](PREPARATION.md),
[AUTOMIX.md](AUTOMIX.md), [STEMS.md](STEMS.md), [WASAPI.md](WASAPI.md),
[ASIO.md](ASIO.md), [CONTROLLER-REFERENCES.md](CONTROLLER-REFERENCES.md).

## Product conventions to preserve

Locally owned DJ software inspired by professional VirtualDJ/Serato workflows,
without subscriptions. Clean dark theme, neon yellow-green A and sapphire-blue B.
Clean buttons with hover explanations; mousewheel controls wherever useful.

- Mirror group locations, not internal control order. Tabs remain Loops / Effects /
  Pads / Prepare. A/B selection buttons are separate, larger, on the inside edge.
  Active deck border and selection button use its colour.
- Loading selects the deck; Space controls the active deck. Library/Explorer drops
  load tracks and warn before replacing an audible playing deck.
- Centred platter, info beside vertical pitch fader with breathing room. Key lock
  defaults on. Clicking pitch percentage smoothly returns adjustment to zero.
- Solid low/mid/high waveform layers. Main waveform has named coloured cue flags;
  small overview has coloured flags only. Playing-waveform wheel nudges are subtle.
- Hot cues have editable names/colours. Ctrl-click seeks without playing. Hold Cue
  previews from an anchored point; release stops silently and returns exactly.
- Seeking/hot cues inside an active loop retain it even while paused. Loop end is
  exclusive; jumping outside can exit. Sync matches tempo and aligns smoothly to
  the nearest beat on the other deck. Times/waveform scaling account for tempo.
- Auto cues snap to bar starts using first-beat/downbeat analysis; preserve user edits.
- Trims, EQ/stem modes, channel/master faders, stereo master meters and crossfader
  curve icons are implemented. Loudness matching adjusts trim.
- Filter is neutral at centre, low-pass below and high-pass above, 20 Hz to 20 kHz.
  Timed effects default to beat sync and follow live effective tempo.
- Inline effects rows split width equally between effect/selector and controls;
  no redundant options button. Compact rows without knobs retain their details button.
- Sampler one-shots overlap on retrigger. Automix permits manual interruption and
  resumes using the non-playing deck, with tempo and EQ transitions.

## Recent fixes and limitations

Sync now compensates for key-lock latency changes across unity/non-unity speed.
Headless tests cover forward/backward corrections. More real-track listening tests
remain useful; do not claim universally perfect sync. An attempted always-stretch
at unity change broke scratch resume and was reverted.

Settings gear clicks were swallowed by the titlebar drag hitbox. Its wrapper now
occludes that hitbox and stops mouse-down propagation. Ctrl+, also opens settings.
Preserve this behavior when rearranging the toolbar.

Realtek WASAPI exclusive tests at 48 kHz: requested 256 frames gave 256 (5.33 ms);
64 aligned to 144 (3 ms); default gave 480 (10 ms). These are buffer durations,
not round-trip latency. Audio configuration changes require restart. Independent
headphone routing and live device switching are not implemented. ASIO probing uses
a child process because the local Realtek ASIO driver crashed natively.

Numark Mixtrack Pro (generation needs confirmation; a Pro II mapping was supplied)
and Pioneer XDJ-RX3 remain target integrations, not verified supported controllers.
Pioneer screen waveforms require more than ordinary MIDI mapping. Hardware acceptance
requires actual device tests. Spotify/Tidal integration was deferred.

## Continuing on another machine

Suggested first prompt:

> Read AGENTS.md and docs/HANDOVER.md. Confirm the branch/remotes, configure isolated
> development data, and verify the Windows build before continuing. Preserve the
> deck conventions and realtime audio constraints.

Commit and push before switching machines, then fetch/pull on the destination before
editing. On clean main use `git pull --ff-only`. Do not reset or force-push to resolve
local work. Keep origin pointed at Dwell2012 and upstream at the original project.
Update this handover when important setup or product decisions change.
