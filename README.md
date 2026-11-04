# faderCraft

faderCraft is a custom DJ workstation written in Rust with a GPUI desktop
interface. It combines music-library management, two- or four-deck performance,
and local stem preparation, with Windows as the primary development platform.

Repository: [Dwell2012/faderCraft](https://github.com/Dwell2012/faderCraft).
Based on [danielchim/djcraft](https://github.com/danielchim/djcraft), with the
original Git history and MIT attribution retained.

The app is under active development. See [feature status](docs/FEATURES.md)
for implemented features, limitations, and the broader roadmap.

## What works today

- **Performance:** switch between two and four decks, with layered waveforms,
  beat grids, tempo control, key lock, scratching, channel EQ, crossfader and
  headphone cueing. Both layouts offer a tempo-range menu and per-deck quantize
  control; the four-deck layout has its own compact mixer and deck controls.
  Windows output includes WASAPI exclusive and installed ASIO drivers; see
  [WASAPI](docs/WASAPI.md) and [ASIO setup](docs/ASIO.md).
- **Performance tools:** hot cues, loops, effects, key-shift pads and a polyphonic
  sampler with named banks and bundled CC0 drum/FX sounds. See
  [pads](docs/PADS.md), [loops](docs/LOOPS.md) and [effects](docs/EFFECTS.md).
  Rhythm Shift can replace a prepared drum stem with a beat-synced groove and
  trigger rolls; see [feature status](docs/FEATURES.md).
- **Automix:** playlist playback, loudness matching and intro-aware EQ
  transitions, with saved preparation overrides. See [Automix](docs/AUTOMIX.md)
  and [track preparation](docs/PREPARATION.md). Automatic musical analysis remains
  an estimate and needs checking against the tracks you play.
- **Stems:** local separation into vocals, instruments, bass and drums, with
  cached results, stem controls and waveform overlays. Fast and Detailed models
  can be downloaded in Audio settings. Native ONNX Runtime performs separation
  without Python or PyTorch; see [stem setup](docs/STEMS.md).
- **Library:** searchable, sortable tracks; background scanning and folder
  watching; duplicate detection, missing-file relinking, artwork, metadata and
  played history. Supported audio formats include MP3, FLAC, WAV, AIFF, M4A,
  OGG and Opus.
- **Playlists and crates:** ordered playlists allow a track more than once;
  crates contain each track once and can have nested sub-crates. Smart playlists
  and imports from rekordbox XML, VirtualDJ folders and M3U files are available.
- **Metadata:** online lookup through MusicBrainz, iTunes, Discogs and AcoustID,
  plus reviewed tag editing, file renaming and undo. Online lookups contact the
  selected provider; stem separation processes audio locally.
- **Controllers:** a MIDI mapping bridge with bundled Numark Mixtrack Pro II
  and AlphaTheta DDJ-GRV6 starter profiles. The GRV6 profile includes four-deck
  Hot Cue pads and quantize buttons, but still needs hardware verification and
  more controls mapped. See [controller setup and limitations](docs/CONTROLLERS.md).

Radio broadcasting, remote back-to-back sessions and the plugin system remain
roadmap features. Hardware support and analysis accuracy are still being refined.

## Building and running

Clone the repository:

```powershell
git clone https://github.com/Dwell2012/faderCraft.git
Set-Location faderCraft
```

Install Rust through rustup; `rust-toolchain.toml` pins the toolchain. Windows
also needs Visual Studio C++ build tools, a Windows SDK and libclang. Read
[the development handover](docs/HANDOVER.md) and [ASIO setup](docs/ASIO.md) for
prerequisites and machine-specific configuration.

Once those prerequisites are installed:

```powershell
. ./tools/dev-shell.ps1
cargo run --locked
# Or load tracks onto the first two decks:
cargo run --locked -- one.mp3 two.mp3
# Skip stem model preloading for a quick UI check:
cargo run --locked -- -quick
```

The development shell sets `DJCRAFT_DATA_DIR` to an isolated development library.
For the Numark starter profile, use `./tools/run-mixtrack.ps1` as described in
[controller setup](docs/CONTROLLERS.md).

The fcScript parameter and action commands and their current limits are
documented in [fcScript](docs/SCRIPTING.md).

The CI workflow builds on Windows, Linux and macOS. Linux development packages
are listed in [.github/workflows/ci.yml](.github/workflows/ci.yml); this does not
imply equal hardware or runtime validation on all three platforms.

Validation commands:

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
cargo deny check licenses sources
```

## Library storage

By default, machine settings and library data use the operating system's
application-data folder under `djcraft` (`%APPDATA%\djcraft` on Windows).
`DJCRAFT_DATA_DIR` overrides the location for development and testing.
The existing crate names, executable name and data-directory identifiers still
use `djcraft`.

Settings > Library can select a portable library folder.
The app uses a local working database and exchanges closed snapshots. Use one
computer at a time, close the app and allow synchronisation to finish before
switching machines. See [the handover](docs/HANDOVER.md) for relinking music and
which device settings remain local.

Scanning, analysis and stem preparation read your music. Tag editing and file
renaming are separate operations that present changes for review.

## Project layout

| Crate | Purpose |
|---|---|
| `crates/protocol` | Control actions and shared session types |
| `crates/engine` | Audio decoding, decks, mixing and time-stretching |
| `crates/analysis` | Tempo, key, energy, structure and cue analysis |
| `crates/library` | Database, scanning, tags, playlists, imports and lookups |
| `crates/controllers` | MIDI transport and controller mapping |
| `crates/app` | Desktop interface and application coordination |

## Licence and attribution

MIT; see [LICENSE](LICENSE). Ian Llewellyn holds copyright in faderCraft's
additions and modifications. Daniel Chim's copyright notice remains for code
retained from the original [DJCraft project](https://github.com/danielchim/djcraft),
including portions of the audio engine, desktop interface, library, analysis
and control protocol. The original Git history records that foundation and
subsequent contributions.

Dependency licences and sources are checked with `cargo deny check licenses sources`.
See the ASIO and stem setup documents for the separate SDK and model requirements.

Bundled starter sampler recordings are CC0-1.0, separately from the MIT code.
See [starter sound sources and licence](assets/sampler/frequency303/README.md).
