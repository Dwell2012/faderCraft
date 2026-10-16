# Windows and controller development

Starting point: https://github.com/danielchim/djcraft at
`d16e3918c36ec91b9fd10d1c2ef497e488c3e8ba`.
Retain the upstream MIT licence and attribution when distributing this work.

## Existing foundation

The source already separates the desktop GPUI interface, audio engine, signal
analysis, library and control protocol. The protocol supports deck transport,
seek, tempo, key lock, volume and external scratch speed. MIDI controls should
translate into these actions and call `Engine::apply` under `Role::Host`.
Never perform MIDI discovery or mapping-file I/O inside the audio callback.

The upstream README reports macOS runtime testing only. Its CI configuration
includes Windows; that is not evidence of a successful build on this machine.

## Hardware targets

- Numark Mixtrack Pro: confirm the exact generation. Use USB MIDI discovery
  and capture actual messages before assigning a factory mapping.
- Pioneer DJ XDJ-RX3: first target its documented MIDI/software-control mode.
  Screen integration, proprietary HID behaviour and LED feedback require
  separate investigation and must not be implied by basic MIDI support.

Neither controller is currently supported by this checkout. Upstream lists
MIDI input, output, mappings and MIDI Learn as unimplemented.

Audio and control are separate integrations. `crates/engine/src/output.rs`
currently opens the default output device, emits stereo on its first two
channels and silences extra channels. Independent headphone cueing therefore
requires a new cue bus, explicit output selection and channel assignment.
There is no ASIO selection implemented in this baseline.

## Implementation sequence

The custom product scope and milestone order are in [PRODUCT-PLAN.md](PRODUCT-PLAN.md).
That plan prioritises the requested mixer before controller mappings. The
upstream feature list is a reference, not a commitment to implement every item.

Hardware acceptance requires testing both controllers individually: transport,
fader endpoints, jog direction/resolution, disconnect during playback, and
master/headphone separation. Do not advertise compatibility until these pass.

## Initial baseline checks (15 October 2026, superseded below)

- Installed Rust 1.99.0 and rustfmt, matching the upstream toolchain pin.
- `cargo fmt --all -- --check`: passed.
- `cargo build --workspace --locked`: blocked by linker error LNK1104,
  missing `msvcrt.lib`, while building dependencies.
- `cargo test --workspace --locked`: blocked by the same missing library;
  no tests executed.
- `cargo deny check licenses sources`: unavailable. Installing cargo-deny
  also failed at the same linker prerequisite; no licence audit completed.

The detected Visual Studio 2026 installation has a linker but its selected
MSVC toolchain lacks the x64 C runtime import library. Install or repair the
Desktop development with C++ workload, including x64/x86 MSVC build tools and
a Windows SDK, using Visual Studio Installer. Then rerun the checks above
and `cargo install cargo-deny --locked` before the licence check.
No application code or dependency versions have been changed from upstream.

## Successful Windows baseline (15 October 2026)

Active checkout: `C:\Users\Ian Llewellyn\repos\DJ\djcraft`.
Moved active development outside OneDrive. Verified 682 remaining source
files against the destination by SHA-256 after a partial move; Git HEAD,
development branch and uncommitted planning documents are preserved.
Cleanup of the old source and nested staging copy was blocked by automatic
approval review. Those residual copies are not the active checkout.

The C++ installation fixed the missing runtime library. OneDrive directory
attributes then caused autocfg to reject its output directory; using a local
build cache fixed that. Installed libclang 18.1.1 locally for bindgen.

```powershell
Set-Location 'C:\Users\Ian Llewellyn\repos\DJ\djcraft'
$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'DJCraft\target'
$env:LIBCLANG_PATH = Join-Path $env:LOCALAPPDATA 'DJCraft\build-tools\clang\native'
cargo build --workspace --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo deny check licenses sources
$env:DJCRAFT_DATA_DIR = Join-Path $env:LOCALAPPDATA 'DJCraft-Development'
cargo run --locked
```

Results: Windows development build passed; 88 tests passed, none failed;
formatting passed; licence and source checks passed. The executable launched
and its `djcraft` window reported responding. This is a process/window startup
check, not visual UI, playback, latency or controller validation.

Executable: `%LOCALAPPDATA%\DJCraft\target\debug\djcraft.exe`.
No application source or dependency-version changes were needed.
