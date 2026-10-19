# Codex handover

Updated 19 October 2026. Read this and `AGENTS.md` before changing code.

## Controller bridge checkpoint (current-machine work)

FX rotary correction: user reported only Filter responded. Raw secondary parameter can be bypassed by beat sync; mapped EffectParameter now follows the first UI dial: Filter cutoff (and Mix=1, matching set_value), otherwise wet/dry Mix. Type/enabled/timing/feedback are preserved. Existing Echo regression first reproduced unchanged Mix; corrected target fixes it. Added all-20-effect-kind synced-state regression. Same 84 mappings; no profile rewrite needed.

User confirms scratch now sounds much better. Lower pad functions and FX buttons also confirmed working; initial concern was trying auto loops with manual-loop controls. Actual outstanding issue was unmapped FX knobs. Added EffectParameter scalar target preserving full EffectState, and six relative CC bindings (0x1B–0x20, 1% per tick). Current profile has 84 mappings; no pad changes or auto-loop shortcuts added. Shift effect selection/fourth Beats encoder remain pending. Regression covers both decks/all three slots, direction, limits, neutral packets, port isolation and invalid slots.

Latest scratch gain fix: user still heard quick volume cutting. Reproduced headlessly with constant 0.4 audio and one jog count every 40 ms: speed-dependent gain dipped to 0.294 during steady motion. Removed speed-dependent gain; scratch stays at unity while travelling toward the target and fades only after reaching it. Added regression requiring steady-motion output above 0.38, plus stationary silence. This supersedes the earlier below-0.02 speed fade. Sensitivity, velocity smoothing and contact release grace are unchanged. Hardware listening retest pending.

Contact-pulse verification: 269 workspace tests pass (one ignored); formatting and licence/source checks pass.

User clarified residual sound is extremely rapid fading during movement. App trace contains many alternating 0x4E contact on/off messages. Previous immediate release issued Scratch(None)+Seek and the next press re-grabbed the read head. Contact release now waits 30 ms and a re-press cancels it without re-grabbing, preserving accumulated wheel distance. Disconnect/manual takeover remain immediate. Regression simulates 40 contact pulses at 5 ms intervals on each deck and verifies no release/re-anchor until the final grace interval, then exactly one final settle. Earlier audio smoothing remains. Hardware retest pending.

Scratch-click verification: 268 workspace tests pass, one ignored; formatting and licence/source checks pass.

Scratch-click fix: reproduced a 0.4-amplitude discontinuity with constant audio and spaced forward/reverse ScratchTo targets. The engine previously emitted audio at full level until it reached a one-frame position threshold, then hard-zeroed it. It now eases velocity using the existing 4 ms smoothing and fades speed below 0.02 to silence, including the final held sample; no audio-thread allocation/locks/I/O. Travel stays 0.003 seconds/count and rim bend is unchanged. Headless regression verifies audible movement, silent settling, exact final position and adjacent-sample jump below 0.01. Hardware listening retest pending.

Latest verification: 267 workspace tests pass (one ignored); formatting and licence/source checks pass.

Latest: 78-binding profile adds lower-row Loop In/Out, Reloop/Exit, half length (Shift doubles), Sample slots 1–4 per deck (Shift stops), and three upper FX slot toggles. Tap stays unmapped per user. Sample slots must already be loaded in the UI; level/repeat follow the UI. Loop handling reuses Smart/Quantize helpers and retained regions; FX toggles preserve all slot parameters through protocol actions. Scratch sensitivity restored to 0.003 seconds/count at user request (supersedes the reduced sensitivity below); bend unchanged.

Headphone Cue LEDs now follow monitor selection from hardware or UI using optional HostButton LED addresses. Mode-cycle capture received only Shift release; no separate mode-identifying Note/CC was observed. Pad notes distinguish Loop/Sample/Cue.

Current bend fix: user reported rim bending did not return to the set tempo.
Reproduced in headless audio: after fast forward jogs, ratio 0.92 advanced a
full 1.0 source seconds per output second because queued +8% nudges remained.
Native rim/buttons now emit PitchBend, whose unapplied phase correction expires
after 40 ms of rendered audio, independently of the UI. Ordinary Nudge and Sync
retain completion semantics; Sync clears the transient timeout. EndNudge
cleans up idle/released/disconnected bends and does not itself take over Automix.
Manual Sync cancels controller bend holds before applying fresh alignment.
Audio tests verify prompt return at ratios 0.92/1/1.08, both key-lock states,
and that a later Sync is not canceled by an old bend timeout. No new locks,
allocation or logging were added to the audio thread.

User also clarified: Scratch OFF + stopped jog should scrub audibly; Scratch OFF
while playing should pitch bend. Restored the stopped fallback with 80 ms idle
release, keeping Scratch ON top/rim differentiation and latching unchanged.
Pitch fader changed from fixed +/-8% to match the full app ratio range 0.5-2.0
(-50%/+100%): inverted raw endpoints 0=>2, 127=>0.5, exact centre 64=>1, pickup
retained. The bundled, generated and installed profiles all use this range.

Verification: 264 workspace tests passed, one ignored; formatting, licence/source,
whitespace and Windows build checks passed. The reproduced sustained-rim-bend
failure is now a passing regression. App restarted gracefully with the new
binary/profile; window responsive and MixTrack Pro II connected with 40 mappings.
Hardware/listening retest of bend recovery and the larger pitch range is pending.

Latest expansion: user confirms the top/rim and latching behavior works, but
scratching is too fast. Travel reduced from 0.003 to 0.00075 seconds/count (4x
slower); bend sensitivity unchanged. Profile now has 40 bindings: new Sync,
transport Cue hold/release, headphone Cue deck selection, Stutter, held pitch
bend +/- and Shift + Pitch- key lock for both decks. Existing personal mappings
were preserved; installed/bundled profiles had only missing bindings added and
jog sensitivity adjusted. Before-buttons backups preserve earlier profiles.

HostButton mappings queue HostCommand intents; the desktop resolves them via
existing UI logic and applies concrete protocol actions under Host role. API
consumers must drain take_commands after process/disconnect/replace so held Cue
releases are not lost. Headphone Cue selects one active monitor deck. Stutter
uses a retained Cue anchor, or zero if none. Bend mappings emit small 3% Nudge
corrections while held, stop on release/disconnect, and never rewrite tempo or
key lock. Paused holds do not build up a phase jump. Shift + Pitch+ range remains
unmapped, along with browser navigation, load-selected-track, FX and loop pads.

Expansion verification: 261 workspace tests passed, one ignored; formatting,
licence/source, whitespace and Windows build checks passed. The app was closed
gracefully and relaunched with the updated local profile; the replacement window
is responsive and logs `Controller connected: MixTrack Pro II (40 mappings)`.
Bounded 96-note diagnostics are enabled for button testing. Hardware confirmation
of the new buttons and reduced scratch sensitivity is pending.

Current jog behavior supersedes the touch:null experiment below: user confirmed
that the rim should still pitch bend in Scratch mode. Both Numark jog mappings
again use top-touch notes 4D/4E, while the Scratch buttons and LEDs remain latched.
Scratch on + top touched = scratch; no top contact = rim bend while playing.
Releasing the top resumes playback without disabling Scratch mode or its lamp.
Scratch off = bend regardless of top contact. This uses the existing engine/API;
the bundled profile, generator and installed local profile were corrected.

Rim/top correction verified by 257 passing workspace tests (one ignored),
formatting and licence/source checks. The app was restarted with the corrected
profile using the already-built binary (runtime source unchanged). Its window
is responsive and the 26-mapping controller connection is confirmed.

Latest jog correction: user wants the Scratch button latched, not held. The
Numark profile now starts in bend mode (`scratch_enabled:false`, `touch:null`).
Press once enables scratch until the next press; note release never changes mode.
Scratch applies to moving jogs during both playback and pause, releasing the audio
override after 80 ms idle while retaining the mode. Bend mode leaves paused decks
still and sends Nudge only while playing. The optional touch-based API remains
available for other profiles. Captured notes show normal Scratch down/up at 72
(A); touch notes at 77 were also observed, but do not gate this profile's mode.

Profile-owned `scratch_led` sources now drive corresponding MIDI output notes
(100 on/0 off) on connection and mode changes, and resend on button release to
override a local momentary lamp. Only named, unambiguous paired output ports are
opened for explicit LED mappings. Output failure leaves input available and logs
an error. Physical mode/LED behavior still needs user confirmation.

This correction passed 256 workspace tests (one ignored), formatting,
licence/source, whitespace and Windows build checks. The app was restarted
gracefully with the updated profile; the replacement window is responsive and
the controller connection is confirmed. No LED-output errors were logged at
startup. Bounded 64-note tracing is enabled for the user's latch retest.

Latest verification: 255 workspace tests passed, one ignored; formatting,
licence/source, whitespace checks and the Windows build passed. At the user's
explicit request the old window was closed gracefully and the new build started
with the updated local 26-binding profile. The window is responsive and its log
confirms `Controller connected: MixTrack Pro II (26 mappings)`. Bounded tracing
of 128 note events was enabled for the hot-cue retest. Do not assume physical
jog/hot-cue acceptance until the user confirms it.

Hot-cue follow-up: the user reports Cue mode lights correctly but pads do nothing.
The initial profile only emitted TriggerHotCue, which rejects an empty slot; the
running log contained InvalidValue rejections. Native hot_cue bindings now emit
SetHotCues on first press, then TriggerHotCue on subsequent presses. The desktop
persists those edits through the existing cue store, respecting editing lock and
near-beat quantization. Original protocol TriggerHotCue semantics remain unchanged.
No undocumented device initialization was added. Bounded diagnostic note tracing
is available via DJCRAFT_MIDI_TRACE_NOTES. Physical retest remains necessary.

Jog update: the user confirms the original 24 MixTrack bindings work. The profile
now has 26 mappings, with compound jog bindings for movement, top touch and Scratch
mode on each deck. Touch scratching defaults on; the physical Scratch buttons
toggle it. Rim movement while playing emits bounded Nudge actions; while paused
it scrubs audibly, releasing after 80 ms idle. Touch holds do not expire. Targets
accumulate between audio callbacks, clamp to decoded file bounds and settle on
release, even for a whole gesture delivered before one callback. Playback and
key-lock state are preserved; the bridge never toggles key lock to scratch.

The default calibration is 600 counts per 1.8 source seconds, with 0.00025-second
bend nudges per count. Direction can be inverted in the profile. Physical jog
direction/sensitivity and listening remain to be checked. Negative waveform time
is not exposed by these jog bindings. LED mode feedback is still pending.

The desktop calls Bridge::tick between events and releases grabs before manual
transport/load actions. The active local profile was extended by adding only
the two absent jog mappings; a timestamped .before-jog backup preserves the
original. The running app was preserved while building its replacement; no
playback was interrupted. Restart is needed to use the new binary. Headless jog
tests cover direction/bursts, holds, idle release, mode changes, bounds, isolation,
disconnect/replacement/manual takeover, and audible resume for playing/paused
with key lock on/off. Existing engine realtime code was not changed.

The user confirmed and connected a Numark MixTrack Pro II; Windows USB IDs are
`15E4:004B` and native MIDI discovery lists `MixTrack Pro II`. Official Numark
VirtualDJ/Traktor mappings and the manual were supplied and inspected as references;
see CONTROLLER-REFERENCES.md for hashes, addresses and limits.

New `crates/controllers` provides a UI-independent mapping API with versioned JSON,
Learn/replacement hooks, press/release actions, play/key-lock toggles, Shift
conditions, absolute/relative parameter controls and soft takeover. Pickup is a
user requirement: software volume at 100% must remain untouched while hardware
rises from 0; only reaching/crossing the software value acquires control. Software
changes re-arm pickup; exact MIDI 64/8192 centres are supported.

The optional native `midir` transport is enabled by the desktop app. Set
`DJCRAFT_CONTROLLER_MAP` to an explicit JSON file to enable input; otherwise no
ports open. Protocol Actions still use the existing host-role engine route,
including the app's audible-track replacement prompt. Callback work is bounded
enqueueing, with mapping on the control thread. The host scans named inputs once
per second and resets held actions/pickup when disconnects are observed.

See CONTROLLERS.md for monitor/profile-generator commands and the public API.
The Numark starter is deliberately partial and unverified on physical controls.
Graphical mapping UI, LED output/initialization, jog semantics, dynamic effect
targets, browser commands and paired CC assembly remain pending. The app was not
launched or its audio configuration changed as part of this implementation.

Verification: 240 workspace tests passed, one ignored; formatting,
licence/source checks and Windows development build passed. The native monitor
opened `MixTrack Pro II` successfully for a two-second read-only connection check;
no physical control events were captured. An editable 24-binding starter was
created at `$env:LOCALAPPDATA\DJCraft-Development\mixtrack-pro-ii.json`. No
mapping was enabled in a running app and physical acceptance remains pending.

Subsequent launch: bundled `profiles/numark-mixtrack-pro-ii.json` adds the
24 official-address bindings to Git. `tools/run-mixtrack.ps1` copies it once to
the development data `controllers` directory, preserving edits on subsequent
launches, enables that profile and starts the existing build with timestamped
logs. The app launched successfully with a responsive faderCraft window and no
startup errors in its stderr log. 241 tests passed, one ignored; formatting,
licence/source and whitespace checks passed. Physical controls remain unverified;
this is an independently authored supported-control profile, not a complete
VirtualDJ import. Jog/LED and remaining buttons are still pending.

## Latest checkpoint and verification

Windows GitHub CI explicitly adds the installed Windows SDK x64 resource compiler
directory to PATH before building. The initial branding push failed only on
Windows because rc.exe was missing from the runner PATH; Linux, macOS and licence
checks passed. Local dev-shell already provides this SDK setup.

This checkpoint includes faderCraft branding/icon, current-set Played history,
remove-from-library without deleting audio, reviewed year-tag edits, beat-anchor
fixes, the 16-beat negative timeline, and the intro-aware EQ Automix changes below.
The latest user feedback was that incoming bass arrived after the outgoing kick
ended. Planning now finishes the bass exchange by the available Outro marker;
it does not yet measure the actual last kick directly. Audition that boundary
on more tracks before treating the section estimate as reliable kick detection.

Verification: 226 workspace tests passed (one ignored) before the final outro
timing adjustment; all 109 engine/unit/integration/decode tests passed (one
ignored) after it. Windows debug build, formatting, diff whitespace checks and
`cargo deny check licenses sources` passed. The app was rebuilt and relaunched.
No new dependencies were added. Music files, tag-review artifacts, local app
databases, caches and binaries remain outside this Git checkpoint.

## Latest Automix behaviour

Smart Automix measures 4–64 incoming intro bars through the next section or
known vocal entrance, then back-times an outgoing section end. Overlaps can span
outgoing builds/breakdowns: incoming EQ remains at zero until the final safe
section, with at most eight bars of staged high/mid/bass blending. `eq_start`
is now timed before an available outgoing Outro marker rather than EOF; the
bass envelope finishes by that marker to avoid waiting through a kickless tail.
This is a section-based estimate, not a separate kick-end detector. `eq_start`
is a serde-defaulted AutomixPlan fraction passed through audio-thread scheduling.
Smart mixes keep the crossfader at 50% during and after the handover; the outgoing
deck stops with EQ cut, and preparation restores EQ when that deck is reused.
Ordinary non-smart fades still move the crossfader. Takeover preserves mix state.
Grid confidence uses the library's 0.10 threshold, with specific fallback reasons.
The Lizard Picotto / Motorcycle saved-section regression now uses 39 intro bars.
Saved IN/OUT and length overrides still win; use Prepare > Clear IN/OUT and
reset the length override for adaptive planning. New automatic preparation leaves
IN/OUT unset. Existing saved preparation is not rewritten. Structure recognition
remains energy/marker-based; unavailable structure retains the previous fallback.

Original-machine metadata maintenance: `TrackEdit.date` now supports reviewed
recording-date/year writes and undo. `None` preserves an existing date, while
an empty string clears it. The library tests cover full-date restoration and
date round trips across supported formats. Music review artifacts, original-file
backups, and the separate `tag-undo.db` are under
`C:\Users\Ian Llewellyn\repos\DJ\tag-reviews`, outside Git. Consult
`approved-review.json` and `applied-results.json` for the actual batch; do not
rerun the original proposal builder or apply manifest against already edited files.

## Repository

Personal private repository: https://github.com/Dwell2012/djcraft. Shared branch:
`origin/main`. Original project is https://github.com/danielchim/djcraft (`upstream`);
retain its MIT licence and attribution. Custom implementation checkpoint:
`bd5ed92888faab1f800943dd1692681ce33ace6f`.

The original-machine checkout used for this checkpoint is
`C:\Users\Ian Llewellyn\repos\DJ\djcraft`. The other-machine checkout is
`C:\Users\ianll\source\repos\DJCraft`, on `main` tracking `origin/main`.
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

For subsequent PowerShell sessions, the repository helper configures these paths,
finds libclang in the standard LLVM installation, and loads the installed x64
Visual Studio C++ development environment:

```powershell
. ./tools/dev-shell.ps1
cargo build --workspace --locked
cargo run --locked
```

Install the prerequisites above first. If LLVM is installed elsewhere, set
`LIBCLANG_PATH` before loading the helper. The helper changes only the current
shell environment; it keeps development data separate from the normal library.
If ASIO's automatic extraction fails, extract the downloaded SDK into
`$env:LOCALAPPDATA\DJCraft\build-tools\asio` so that `ASIOSDK\common\asio.h`
exists beneath it. The helper detects this copy, or respects an existing
`CPAL_ASIO_DIR` for another SDK location. Keep the SDK outside Git.

## Checks

19 October checkpoint: **213 tests passed, 0 failed, 1 ignored** with
`cargo test --workspace --locked`; formatting, licence/source and whitespace
checks passed. The development build completed and the app launched successfully.

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

The 19 October development build includes the following changes:

- Settings exposes the headphone output pair; cue follows the active deck.
  The current setup uses a LEWITT CONNECT 6, with speakers for master and the
  6.35 mm headphones for cue. Output channel assignments remain user configured.
  Cue volume/mix sit to the right of the deck EQ controls; sampler volume is on
  the left. Cue mix at centre gives both cue and master unity gain.
- New windows default to 1920 x 1080. Enter saves the hot-cue editor.
- Hot-cue additions snap only within 5% of a beat, allowing mid-beat cues.
- Auto-Trim measures the full source, including interpolated peaks, targeting
  -0.1 dBFS with a maximum boost of 12 dB. Key-lock processing and playback
  startup details appear below. Summed master clipping is still possible.
- Sampler capture begins at the playhead unless a loop supplies its bounds.
  Playback uses a short 32-frame attack; old captures must be recaptured if
  they contain the former beat-aligned lead-in.
- Refresh rescans library folders. Changes in the compact browser now trigger
  pending analysis in the main Library view, so new imports get BPM/key analysis.
  Low-confidence BPM remains blank rather than being guessed.
- Ordinary playlist tracks can be reordered by dragging onto another row in
  either library table. Clear column sorting first. Search-filtered views resolve
  positions against the full playlist; smart playlists keep their rule-based order.

The current machine has Visual Studio 2026 with the C++ workload, LLVM and the
external ASIO SDK configured by `tools/dev-shell.ps1`. The local Python 3.12
Demucs CPU runtime/model were installed and passed an eight-second four-stem
functional check. They are outside Git; see STEMS.md to recreate them.

Diagnostic examples: `cargo run -p djcraft --example analyse_library` processes
pending library analysis using `DJCRAFT_DATA_DIR`; `check_levels` inspects levels
and `check_stems` checks the stem worker (read their arguments before running).
Library examples `audit_tags` and `apply_reviewed_tags` support the reviewed tag
workflow; never run the latter without an approved manifest. Metadata cleanup and
analysis were performed on the local DJ Mixing Practice and 70's 80's 90's Extended
Mixes folders. Review manifests, undo database, reports and music remain local,
outside Git, under the development data directory and
`$env:LOCALAPPDATA\DJCraft\tag-reviews`. Preserve them separately.

Do not interrupt a playing app just to verify a build. Windows can keep the
running executable locked; rename it before linking a replacement if needed,
then restart only when requested. The latest playlist-order build was restarted
successfully on 19 October.

Sync now compensates for key-lock latency changes across unity/non-unity speed.
Headless tests cover forward/backward corrections. More real-track listening tests
remain useful; do not claim universally perfect sync. An attempted always-stretch
at unity change broke scratch resume and was reverted.

Settings gear and Library/Perform navigation clicks were swallowed by the titlebar
drag hitbox. Both toolbar control groups now occlude that hitbox and stop mouse-down
propagation. Ctrl+, also opens settings.
Preserve this behavior when rearranging the toolbar.

Realtek WASAPI exclusive tests at 48 kHz: requested 256 frames gave 256 (5.33 ms);
64 aligned to 144 (3 ms); default gave 480 (10 ms). These are buffer durations,
not round-trip latency. Audio configuration changes require restart. Independent
headphone pairs are now selectable for ASIO/shared multichannel output, following
the active deck; exclusive WASAPI remains stereo. Live device switching is not
implemented. CONNECT 6 hardware listening verification is pending. ASIO probing uses
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

Key-locked tempo changes (including Sync) now trigger a debounced background pass
through the full-track time stretcher. Auto-Trim applies the resulting peak while
playing; manual trim overrides remain respected, and stale tempo results are
ignored. A stereo-linked 64-frame lookahead peak guard protects each stretched
deck after EQ/effects while analysis is pending and during transport changes.
This does not limit the summed master or guarantee DAC true-peak headroom.

Playback starts at full transport speed immediately (32-frame anti-click gain
attack). Pause retains the existing vinyl-style speed brake. Key-locked starts
pre-roll the stretcher at the cue position using preallocated buffers, discarding
its startup latency; paused decks prepare this ahead of Play. Starts immediately
after a seek/load prepare on the first callback. The 64-frame peak-protection
lookahead and device/output buffering still apply. No device latency is removed.

Current-set history: schema 17 adds play_sets/play_history; no music tags are changed. Shell HistoryTracker accumulates 10 seconds of audible playback per loaded deck track, excluding Cue and scrubbing. Library > History starts fresh sets and browses retained sets. Both track browsers display [Played] beside the title. Flags persist across restarts; explicitly start New set to clear them.

## faderCraft branding (19 October 2026)
The user-facing name is now faderCraft. Windows MSVC builds embed the lime/sapphire fader icon as resource ID 1 (GPUI loads this for the taskbar), plus product metadata. Build requires Windows SDK rc.exe, provided by tools/dev-shell.ps1. Cargo package/executable and existing data paths remain djcraft/DJCraft for compatibility with libraries, caches and launch scripts.

## Beat grid and silent pre-roll (19 October 2026)
Analysis version 7 repairs the half-beat fallback found on Sunburst Eyeball and
Delerium Silence (Airscape): low log-bass confidence must not discard reliable
uncompressed bass timing in favour of offbeat broadband percussion. Opening
attack refinement now inspects 128 beats, reaching Gouryella's kick entrance.
The usual background pass refreshes old library analysis on launch; saved manual
hot cues are not moved. Regenerate suggested cues if an old First Beat cue remains.
The user clarified this is timeline space, not a count-in menu. The main waveform
now extends 16 beats before file start, including grid lines at nonnegative
playhead positions. Drag and paused wheel scrubbing can reach that negative time;
the small overview remains a whole-file navigator. Seek/ScratchTo clamp to the
16-beat bound from the grid, cue preview supports that range, and playback is
silent until zero. The separate Pre-roll action/menu was removed. Unknown-tempo
tracks retain zero as the lower bound. Negative positions display signed time.

Analysis version 8: once a strict opening-attack consensus establishes phase,
walk back through immediately adjacent strong attacks within 25 ms of that phase.
This preserves an opening kick with a different envelope instead of shifting the
bar origin by one beat (Motorcycle, As the Rush Comes). The phase consensus still
uses the strict 12 ms threshold, and recovery cannot cross a missing attack.
