# Codex handover

DDJ-GRV6 Beat FX (5 November 2026): the shared protocol state controls the
selected deck's FX1 or an independent processor for master/booth and sampler.
On/Off can use the visible FX1 before the selector sends its position.
The bundled profile maps the documented selector notes, six target buttons,
On/Off, Beat −/+ and Level/Depth. Helix uses Beat Repeat, Mobius bypasses.
Sampler pads use slots 1–8 on the left and 9–16 on the right, with Shift stop.
See CONTROLLERS.md for the performance workflow and limits.

Analysis progress/priority (4 November 2026): a shared queue promotes loaded library tracks, including in-flight work. A reserved priority worker prevents all background workers parking without a worker available for the deck. Background workers pause at decode/DSP stage boundaries and retain intermediate work; results are saved serially before priority is cleared. Both views show the analysis status strip with Check pending. Compact playlist import now emits Changed so the full Library starts analysis. Existing files and completed analysis are preserved. Restart is needed to use the change.


Hot-cue input backlog (4 November 2026): Controllers::process used to synchronously
force the full LED bank on each mapped note edge before returning its action.
Rapid pad presses/releases amplified output traffic and could delay later input.
Processing now only invalidates the pressed lamp and any mapped HostButton/Jog
lamp; tick refreshes feedback after all drained input actions, once per lamp.
Unchanged lamps stay cached; reconnect still forces initialization. No audio
thread changes or dropped/debounced cue presses. Regression tests cover a 32-hit
burst with one repaired LED, channel isolation, and actual key-locked headless
cue playback at 1.08 tempo reaching its final cue on the next callback and then
advancing without queued retriggers. Mixtrack hardware listening retest pending.

fcScript follow-up (3 November 2026): the former windows-controller-foundation
branch was merged into main and removed remotely. Continue from main's scripting
and DDJ-GRV6 foundation. Key-lock, both tempo-range buttons and eject now use
the common skin-button dispatcher, also reused by Play. New do_action commands:
key_lock_on/off, toggle_key_lock and cycle_tempo_range. Toggles/cycles use current
session state; tempo range cycling preserves speed. Headless engine tests cover
all decks, range wrap, role rejection and isolation; offline controller tests
cover press-edge deduplication, release and fresh state on the next press.
Sync, cue gestures, tempo-return ramp and remaining controls still need conversion.

fcScript follow-up (2 November 2026): continued on codex/windows-controller-foundation,
which holds the scripting foundation ahead of main. Two-deck pitch dragging,
four-deck tempo-knob dragging and tempo scrolling now use Program::set_param
with Param::Tempo and a current session snapshot. They still dispatch through
Shell::send, retaining cue-anchor handling, Automix/manual-adjustment behaviour
and tempo-return cancellation. Sync, range selection and the gradual return
ramp retain their existing protocol paths. See SCRIPTING.md for the current
conversion boundary; controller scripts and the remaining controls are pending.

Stem preparation progress overlays the top edge of each deck overview waveform: 3px, deck colour at 50% opacity, no added layout height. Uses existing per-deck progress and refresh; queued/loading jobs show a short marker, and completion/failure/timeout clears it.

Right-deck performance boxes mirror the left-deck arrangement: Effects/Pads outside, Loops inside, label strips on the right. Controls inside each box retain their original order.

Serial FX order: deck rack → Pink/White Noise generators → Bitcrusher → Flanger → Reverb → Echo → Filter. CFX processes in reverse menu order, with Filter affecting all preceding audio and tails.

Deck FX names show a small muted, baseline-aligned beat duration (for example Echo (1/2 beat)) when beat sync and a track beat grid are available; free-running effects omit it.

Tempo-neutral gain fix: auto-trim remains based on source loudness across tempo/key-lock changes. Processed peak reports no longer lower track trim; the existing audio-side peak guard handles timestretch overshoots.

Main mixer knobs use antialiased SVG rings, arcs and pointers in two- and four-deck layouts, preserving each control’s neutral position and existing interactions.

Stopped-rim/FX AA follow-up: jog movement scrubs whenever the deck is stopped, regardless of Scratch latch or top-contact sensor; playing touch/rim behaviour is unchanged. Paused input clears the previous motor-inertia suppression so a fresh stopped-rim turn is not swallowed. Regression checks both directions with mode on/off, audio output, idle release and stopped position retention. FX knobs now rasterise the same smooth design through SVG geometric-precision rendering at 24/30px, with rounded strokes and alpha edges instead of directly painted small paths. SVG angle values quantise to 1/1024 solely to bound the icon cache (subpixel angular steps).

FX panel routing/knob follow-up: effect_slot_disabled checks actual controller mapping ownership per deck, so Mixtrack CFX routing disables only A/B FX slot 1, not unrelated C/D slots. Toggle/selector/details/focus buttons disable, the row dims, detail popout hides and update/scroll/drag paths reject edits. Other clients and audio state are unchanged. FX dials now call the shared paint_smooth_knob renderer used by full-size mixer EQ/CFX, scaling its continuous arcs, fill and pointer to 24/30px controls. No dotted-circle renderer remains.

Colour FX: Filter is no longer reserved in deck FX slot 1. All six rack slots are assignable; new defaults begin Echo/Reverb/Flanger. Independent per-deck ColourFxState uses position, selected bitmask and enabled flag; mixer CFX arrow toggles Filter/Echo/Reverb/Flanger/Bitcrusher/White Noise/Pink Noise selections. Any combination shares the knob, centre dry/silent. CFX processing follows reverse menu order after the deck rack, with Filter last. Non-filter wet amounts and noise levels grow with distance from centre; Filter retains bipolar cutoff. White noise uses per-deck xorshift stereo values; Pink uses twelve staggered octave rows. Generators can run without a track, through channel/master/crossfader/cue paths. Levels smooth in the callback without allocation or I/O. Effect buffers allocate at engine construction. Saved old slot-1 Filter position migrates to CFX, freeing its rack slot. Saved CFX combination/bypass/position restore, except noise-containing combinations start at neutral until a fresh gesture. Controller Settings > FX 1 controls switches button/relative encoder/LED between rack slot 1 and CFX, leaving FX 2/3 and Shift auto-loop mappings intact; mapping JSON stays untouched. Protocol, headless audio/noise spectrum, migration, settings and controller routing tests cover this change.

FX visual follow-up: compact rows now use square cells like Pads/Loops, with consistent dark fills and thin borders instead of nested rounded button groups. Active ON cells use the solid deck colour with dark text, matching Loop activation. Effect names fill their own cell; details and amount occupy fixed cells. The default view has no visible rounded outer frame. Vertical inset reduced to fit all three 28px rows in the small Effects area. Existing layouts and timing cog remain intact.

Compact deck Effects layout: default FX x3 now uses three 26px rows (on/off, named effect selector, detail button and one compact primary/amount dial). Other layout choices keep their existing expanded controls. In two-deck Pro layout the upper spacer now matches the lower 166px Loops area so Effects aligns with Pads. Removed redundant beat selector from the box header; side-rail Effects cog retains the checked beat popout, and four-deck layouts use a compact timing cog. Shared timing, filter reset behaviour, detail controls and saved effect settings are unchanged.

Controller pad-page following: Bridge reports PadMode on rising mapped HotCue/HotCueDeleteMode, LoopPad and SamplePad bindings, after modifier matching and before any loaded-track requirement. Separate take_pad_modes preserves existing action/command semantics. Shell switches the corresponding deck before applying the pad action, releases held UI pads only when changing page and clears split mode. Loop selects Manual Loop to match physical In/Out/Exit/Reloop; sampler/cues select their pages. Shift alone and button release do not guess a mode. First press is required because Mixtrack mode switching itself has no distinct observed MIDI message. Tests cover both decks, empty state and no spurious Shift/release transitions.

31 October startup controller change: a single unambiguous MIDI input matching the loaded mapping now auto-connects, including reconnects. No Use controller prompt for this case. Unrelated MIDI inputs are ignored; multiple matching inputs or duplicate names are not auto-selected. Explicit Not now remains honoured until disconnect. Disabled stays disabled. Splash shows mapping name and connected input names from actual open ports, updating during stem warmup. This supersedes earlier mandatory-consent notes below. Saved mapping choice, Scratch modes, soft takeover, output LEDs and startup hardware snapshot remain in use. Tests cover single/reconnection, multi/duplicate/unknown inputs and explicit decline.

Stem FX pads: appended Page::StemFx preserves existing page indices. Top four pads choose source, bottom four toggle one latched EffectState per source; four banks cover PAD_FX's 16 types, second parameter changes wet in 5% steps. New SetStemEffect action/state is validated on the control side and persisted with performance settings. Four allocation-free Effect instances per deck are allocated at construction. Selected sources are processed independently before summing and before transport stretch, EQ and deck FX; source tempo/pitch therefore transforms these inserts. Unprepared tracks bypass stem inserts entirely, never treating the full mix as a stem. Muted source gains apply after its insert. Clear/reset paths include insert history; shared beat controls update both racks. Stopped-deck tails feed zero into stem inserts and join the existing output/cue path. Tests cover stereo source isolation with normal/keylocked tempo, unprepared bypass, permissions/validation, timing and saved setup restoration. Sampler capture still bakes selected raw stems, not FX. Adds four delay/reverb allocations per deck at engine construction, with none in the callback.

Pitch-zero lamp check: existing LED_PITCH outputs 0x28/0x29 match Numark's official VirtualDJ definition. Both use neutral-tempo feedback (<0.1% tolerance), steady rather than blinking. Fixed RangedTempo's opposite-BPM snapping stealing raw 64 at wide ranges: centre now always remains ratio 1.0. Tests cover both physical faders with near-centre BPM targets and both lamps independently. Source: https://www.numark.com/images/product_downloads/Mixtrack_Pro_II_-_VirtualDJ_Pro_Mapping.zip (definition facts only; no third-party code/assets added).

Hot-cue quantize is included in performance-settings.json: per-deck cue_quantize and cue_quantize_window restore Pads at construction, and changes use the same debounced save/close flush as effects and Scratch. Existing files default to enabled with a 5% beat window. Cue positions remain track-specific; this stores the setting, not cue markers.

Deck setup persistence: performance-settings.json in machine_dir stores per-deck optional Scratch modes, full effect slots and shared effect beat timing. Old/missing files keep defaults; absent Scratch overrides honour the mapping default. Shell captures authoritative engine/controller state every 50ms, saves after 250ms quiet and flushes on Drop. Separate file prevents an open Settings dialog overwriting current performance preferences. Startup restores effects via existing protocol actions before track loads; controller load overlays saved modes without editing mapping JSON. Bridge preserves its last latch into the in-memory mapping on disconnect, so reconnect feedback is correct. All effect parameters and enabled states restore; playing state, held platter contact and effect audio buffers do not. Tests cover saved setup/reload/headless restoration, missing fields and Scratch toggle/disconnect/reconnect LEDs.

Direct-drive jog pickup: while playing, confirmed contact release (30ms grace for contact pulses) now relinquishes scratch ownership at the audible head when wheel speed falls to playback speed or below, in either direction; fast forward/backward spins retain scratch ownership until they slow or go idle. Existing playback smoothing resumes the selected tempo without a Seek. Residual movement is ignored until 80ms wheel silence or a fresh touch, preventing inertia from turning into pitch bends against the motor. Paused free spin remains unchanged. Contactless jog profiles retain packet/idle handling because they cannot distinguish a hand from inertia. Headless tests cover slow/fast forward/reverse, key-lock on/off, tempo retention, no release seek, inertia rejection and immediate re-grab. No engine/audio-thread changes.

Crossfader Settings: Settings > Mixer follows Serato's control structure (Slow–Fast, Linear/Power, Reverse, Disable), with deck assignments below. Settings persist curve/routing and apply via existing protocol actions at startup and on edits. Adjustable is a new CrossfaderCurve variant; old presets remain readable and sound unchanged until edited. Linear uses a bounded slope from 1 to 100; Power squares those gains, giving a deeper gradual fade at Slow and a tight cut at Fast. This is our response design, not Serato's unpublished DSP formula. Reverse swaps the gain response while preserving physical/MIDI position; Disable returns unity for both sides and retains assignments. Existing mixer preset/assignment buttons also save. Engine tests cover response endpoints, monotonicity, reversal, bypass and live four-deck audio; settings round-trip includes all new fields.

## Reminder for tomorrow morning on the other PC (31 October 2026)

**Enable the Mixtrack controller profile when launching.** A direct launch of
`djcraft.exe` without `DJCRAFT_CONTROLLER_MAP` opens no MIDI ports, even when
Windows detects the controller. Recent RGB-test launches omitted that variable;
the hardware and saved mapping were still present.

From the repository on the other PC, build and launch with:

```powershell
. ./tools/dev-shell.ps1
cargo build --workspace --locked
./tools/run-mixtrack.ps1
```

The launcher creates a local editable copy of
`profiles/numark-mixtrack-pro-ii.json` in the development data directory's
`controllers` folder if missing, then sets `DJCRAFT_CONTROLLER_MAP`.
Personal edits to an existing local copy are preserved. Local edited profiles
are machine-specific and are not included in Git: transfer/review them separately
if the other PC needs those exact edits, or use `-MappingPath` explicitly.
Confirm the exact MIDI port name on that PC (expected `MixTrack Pro II`);
logs should show `Controller connected: MixTrack Pro II (144 mappings)`
and a startup snapshot request. Audio-device settings are separate and must
match that PC. Keep launches visible and retain the isolated development library.

RGB update (30 October): the performance header's persistent RGB toggle colours
scrolling and overview waveforms from bass/mid/high amplitudes, with yellow,
magenta/pink, cyan and white mixtures. The final visual adjustment uses a gentle
1.2 contrast curve and 85% opacity. The main debug build was rebuilt and launched;
Mixtrack connected with 144 mappings. See [waveform details](WAVEFORMS.md).


Controller mapping Settings follow-up: Settings > Controllers now selects Automatic, bundled MixTrack, installed JSON profiles, Disabled or an external JSON via Load mapping. Selection persists as controller_mapping (empty for legacy/default behavior). Changes validate then release held controls, drop old ports and enable currently matched devices. Future reconnections retain the consent prompt. JSON remains at its original path. No port renaming or broad unknown-device matching. Hardware behavior still needs user testing.

30 October controller discovery: normal launches load an installed or bundled MixTrack mapping and scan once per second. Matching unique MIDI inputs prompt Use controller / Not now before opening input/output. Disconnect resets the decision and releases held actions; reconnect prompts again. Explicit DJCRAFT_CONTROLLER_MAP remains supported but also requires approval. Unknown controllers and a general profile chooser/catalogue remain pending. Hardware acceptance testing still required.

Latest session: [29 October UI, waveform, key investigation and ASIO handover](SESSION-2026-10-29.md).

Stem model choice (26 October): stem_model defaults Fast for old settings; Detailed selects htdemucs_ft. Settings has background download/verify buttons and local availability markers. Both models stay on disk; the worker keeps only the current model loaded, switches on a queued request, and preloads the selected model at startup. Cache keys include model name, preserving existing htdemucs-v1 entries. Already attached stems require a deck reload to change model. Worker heartbeats cover first-use downloads, with absolute startup/job limits. Lost-GPU CPU fallback reloads the correct model. No library schema or engine/audio change.

Persistent stem worker (26 October): stdin JSON requests and stdout ready/progress/done/error messages replace per-track Python launches. Shell warms the worker on a background executor and shows a startup splash until ready (or failure). One model and device weights remain loaded; CPU/GPU preference is supplied per job. Rust serializes requests, bounds unresponsive waits, discards failed workers, and shuts down the process at app exit. Cache identity/output are unchanged.

Updated 27 October 2026. Read this and `AGENTS.md` before changing code.

## Measured jog speed (27 October 2026)

User clarified audio must match physical platter speed, not merely retain the gesture. Added JogScratch protocol action with seconds/speed/interval, validated on control side. Native Input has drain_timed with driver-clock reconstruction (clock reset handled), while drain preserves raw-event API for monitor. App passes timestamps to Bridge::process_at. Jog velocity is counts×calibration/interval, bounded ±4×; first/unresolved interval uses prior 10 ms default. Audio ScratchTo command carries optional measured velocity and packet-following frames; a bounded 10% phase correction accompanies feedforward velocity and existing 4 ms easing. Normal waveform ScratchTo still uses position-only following. No allocations/locks/I/O/logging added to the audio thread. Paused Seek after scratch release resets residual speed to avoid drift. Added regressions for queued timestamp preservation/reset, controller rate deceleration/reversal, steady audio speed in both directions, volume stability, validation/stationary stopping and stepped free-spin deceleration. Existing release/idle behavior retained. Hardware listening retest pending.

## Platter free spin (27 October 2026)

User requested physical inertia after lifting from the scratch surface. Jog contact release now waits for both the existing 30 ms contact grace and 80 ms motion idle. Incoming wheel packets keep the last surface gesture active in either direction instead of converting to rim bend; packets continue accumulating the original target. No simulated motion, sensitivity or audio-thread changes. Stationary releases remain 30 ms, normal rim bend returns after idle, and disabling Scratch during released free spin clears the remembered contact. Existing cleanup/cancellation paths remain immediate. Three new regressions cover both directions/decks/transport states, explicit mode-off cleanup, and headless audible free spin with key lock on/off plus playing/paused restoration. Hardware retest pending.

## Current controller range / Cue changes (27 October 2026)

142-entry starter: Shift + Pitch+ cycles Low ±8%, Medium ±16%, High ±32%, Wide ±48%, default Low per deck. Added protocol TempoFaderRange/SetTempoRange and defaulted DeckState field; DeckControl stores scaling with no realtime mixer change. Range changes preserve tempo and Automix. RangedTempo mapping derives endpoints from state and re-arms pickup on range changes. Generic Tempo mappings are unchanged. App fader, thumb and wheel use selected range; UI range button cycles the same protocol state. Profile generator/bundled/installed copies updated. Tests cover four ranges/decks/endpoints/centre, cycle debounce/wrap, pickup, role gating, no tempo change and backward state deserialization.

User also requested transport Cue return to where playback started. Shell/pad send paths now retain the anchor through unrelated actions and capture it on paused-to-Play (including pending seek); paused seek prepares a new start and track load clears it. Playing Cue returns to the anchor and pauses, rather than starting a preview at the currently playing position. Paused Cue hold/release previews are kept. UI regression covers anchor preservation and new playback starts. Hardware retest pending.

## Four-deck performance foundation (24 October 2026)

Stem acceleration (26 October): Settings > Audio > Stem preparation persists
stem_gpu (default true, including older settings). Automatic prefers CUDA/ROCm
or MPS; CPU only bypasses GPU probing. New jobs capture the setting when queued.
The worker keeps whole-track buffers on CPU and transfers chunks for inference,
uses TF32-capable high matmul precision on CUDA, and retries GPU runtime errors
on CPU. Both paths retain half the logical cores capped at four CPU threads.
The same model, overlap and shifts are retained; device does not invalidate
existing stem caches. prepare.log records the device and fallback reason.
Setup detects NVIDIA hardware and installs pinned torch/torchaudio CUDA 12.1
wheels (or CPU wheels), with -Device CUDA/CPU overrides. Requirements accept
either build. Python worker tests cover selection, override and GPU-error retry;
Rust settings tests cover default/persistence. Rust audio processing is unchanged.
Local CUDA validation: torch/torchaudio 2.5.1+cu121 on the RTX 3070 Laptop GPU
(8GB VRAM, driver 536.67). A 32-second synthetic stereo sample produced four
finite, correctly sized stems on both devices: inference 23.177s CPU (4 threads)
versus 5.652s CUDA. These exclude Python/model startup and do not promise a
fixed speedup on other tracks. Six worker unit tests and Rust workspace checks pass.

Playlist sublists (25 October): schema 20 adds nullable parent_playlist_id and
an index. A playlist menu offers New sublist; children can nest further and
each retains its own tracks/order/count. Both sidebar trees share rendering,
with independent expand/collapse state for folders and playlists. Moving a
playlist to a folder detaches it from its parent and moves its descendants to
that folder; deleting a playlist promotes its immediate children one level
without deleting tracks or child playlists. Version-19 migration, reopening,
independent memberships and parent/folder operations have headless coverage.

Analysis version 10 corrects version 9's whole-track kick-phase selection.
It scans kick-transient candidates chronologically and requires eight of the
next nine beats to support a run. Descending bass centroid evidence is limited
to the 90–170Hz attack band, avoiding flat bass tones. The first confirmed run
wins over louder later sections, and time-domain refinement uses only that
local run. The grid projects backwards through musical intros but not leading
silence. It retains the previous bass/broadband fallback when no run is found.
For local ATB 9PM, the first supported kick is around 30.48s; version 9 had
incorrectly chosen a later phase. BPM remains 137.53, with corrected phase
around 0.369s. Cached analyses refresh automatically; music tags are not written.
Regressions cover a late kick entrance, louder later half-beat evidence,
isolated hits, leading silence and bar numbering.

Deck control groups mirror positions across the mixer without reversing the
buttons inside them. Four-deck panels have three mirrored columns: key/pitch/
tempo, transport controls, activation/headphone cue. Pitch/key lock sits between
the disc/information group and transport (mirrored on B/D), rather than at the
outer edge. Transport stacks Cue, Sync, Stems,
then Play/Load, with Stems in its own row immediately above Play/Load.
Two-deck mode keeps Cue, Sync, Play, Load
and Stems together in one row. Per-deck stem buttons retain ready/loading/
progress states and replace the old mixer buttons. Platters no longer repeat
deck letters. B/D key information sits beside the platter and aligns right.
Four-deck platter/BPM/key groups anchor to the outer edges. Their key details
use one line with muted vertical dividers; two-deck key details stay stacked.

Waveform colours now follow deck palettes: low is the darker shade, mid the
deck primary, high the lighter shade (lime, sapphire, amber, violet). Track
overviews share these palettes. Each scrolling row has its own Wave / Stems
button, enabled once stems are attached. Stem mode overlays pink vocals, cyan
instruments, amber bass and pale drums, with a matching legend. It uses the
same row, height, playhead and grid: exactly two rows or four rows, never
extra stem lanes. Overviews retain the mixed deck palette. Mode selection is
per deck for the session and defaults to Wave. Stereo-safe RMS stem envelopes
follow live stem gains, including stem EQ groups and pad mutes: a zero gain
skips its overlay entirely. Mixed deck waveforms and overviews scale low/mid/high
amplitudes by active Classic EQ gains, hiding killed bands. Non-Classic modes
use neutral frequency gains because their retained Classic settings are bypassed
in audio. Gains apply at paint time; cached source summaries remain unchanged.
Flow's independent preview retains neutral waveform gains. Stem envelopes
are calculated on the stem worker after residual reconstruction; drawing reads
only the immutable summaries, not full stem audio. Existing stem caches work.

Protocol DECKS is now four, shared with the engine. Deck IDs 0–3 are A–D.
The performance toolbar saves a 2 Decks / 4 Decks preference in local settings;
older settings retain two-deck view. Four-deck view stacks A/C left and B/D
right around four mixer channels, with four aligned scrolling waveforms and
the library below. Compact decks keep the same 480px performance height as
two-deck mode. Pads, loops, effects and preparation open over the other deck
on the same side, preserving library space. A/B expand down over C/D; C/D
slide up over A/B and reveal tools beneath their transport. Clicking the same
tool tab closes the overlay, reversing the 220ms eased animation. Switching
tools keeps the deck raised; overlay ownership is separate from keyboard
activation. The tools area scrolls within the existing 480px deck column.
Four-deck slots remain fixed at 236px with an 8px gap during transitions, so
moving the owner into the overlay never shifts its sibling deck.
Short windows scroll vertically. C is amber and D violet.
Loading C/D from the library menu, CLI or a custom controller mapping reveals
four-deck view. C/D playback or active effects tails prevent hiding those decks.
Deck activation selects Space transport; headphone buttons independently toggle
cue, identically in two- and four-deck views. Loading and tool selection never
change headphone selection. Startup has all deck cues off; multiple cues sum
before the existing cue/master blend and level. Cued C/D cannot be hidden.
MonitorState.cue_decks is an optional A–D bitmask: Some(0) is all off, while
None preserves legacy single-deck monitoring via MonitorState.deck. Invalid
mask bits are rejected. Numark headphone buttons use ToggleHeadphoneCue and
LEDs follow every enabled deck; named legacy headphone-cue profile bindings
are upgraded in memory without rewriting custom mappings on disk.

The 360px four-deck mixer orders channels A/C/B/D. Volume faders remain below
their EQ knobs in the same 50px column; channel VU meters sit left of A/C
and right of B/D. Closely paired master VU meters sit between C/B,
with subtle channel separators. Sampler, centred master, cue gain and cue/master
blend share one knob row; master remains a fader in two-deck view. Master knob
dragging uses relative vertical motion and retains squared gain/unity reset.
Both mixer layouts use curve diagrams for crossfader variations, with names
on hover.
Crossfader side buttons independently toggle A–D bitmasks via
MixerAction::SetCrossfaderRouting. Defaults are A/C left and B/D right. Neither
side means bypass; both sides sum and cap at unity. Routing is session state,
with legacy serialized state defaulting to the original mapping. Changes are
smoothed on the audio thread and update live-load protection and play history.
Manual routing stops Automix; later Automix starts use the selected routing.
Headphones use separately smoothed cue gains for all four decks, pre-fader and
pre-crossfader. Pads, cues, loops, effects, stems, preparation, tempo returns,
waveform positions and play-history tracking have per-deck state throughout.
Four-deck Sync prefers the other playing deck with a known grid whose current
uninterrupted playback started earliest, independent of playhead position or
fader gain. Pause, track replacement and cue preview reset/exclude priority.
If no eligible deck is playing, Sync prefers the paired deck (A/B or C/D),
then another visible deck with a grid. Two-deck Sync keeps its A/B pairing.
It matches the reference's effective BPM and aligns the nearest beat only
when both decks play. All operations use the role-checked protocol.
Automix intentionally remains an A/B workflow; it never adopts or loads C/D.
Its shared crossfader still affects C/D, and manual deck control retains the
existing Automix takeover behaviour. Existing hardware profiles remain A/B;
four-deck hardware mapping and listening acceptance require separate device tests.
No dependency or library schema changes are needed.

Loop resize (including controller half/double) and Direct buttons now extend
to 0.125 and 0.0625 beats. Size labels retain four decimal places when needed.
The engine's existing 10ms minimum remains; headless playback checks cover
wrapping and releasing both micro-loop sizes at 140 BPM.

Verification: 305 workspace tests pass, one ignored; Windows workspace build,
formatting and licence/source checks pass. Clippy completes with existing
warnings. Headless regressions cover all-four summing, every crossfader curve,
cue isolation/switching with faders closed, independent C/D transport/controls,
invalid IDs/roles, custom C/D MIDI mappings and A/B-only Automix adoption.
Runtime layout checks use an isolated consistent SQLite backup of the existing
development library (968 tracks), with source audio read-only. No music moved.

## Linked EQ and Key Shift pads (23 October 2026)

Ctrl-scroll on EQ applies the actual clamped control delta in reverse to the
other deck's matching band, preserving existing offsets. This covers Classic
and stem EQ layouts, plus the two deck volume faders: cutting one raises the
other by the same fader amount within limits. Trim and monitor controls stay independent.

Key Shift is a new pad page with latched absolute offsets, not cumulative taps.
Parameter 1 selects semitones, whole tones, major or natural minor intervals;
parameter 2 steps the current shift in the selected mode, within ±12 semitones.
Pad 4 is Reset; Shift-click any pad also resets.
Scale intervals are relative to the original track tonic, using the explicitly
selected major/minor scale. No automatic key detection is required. The eight
offsets repeat in the optional 16-pad layout. Shifts are limited to ±12 semitones
and apply through DeckAction::SetKeyShift with key_shift in DeckState.
The existing preallocated Signalsmith stretcher handles transposition without
using an effect slot or changing tempo. Key Lock preserves the original key
plus the selected shift across tempo changes; without Key Lock, normal tempo
pitch follows the selected shift. Scratching uses the existing unshifted vinyl
path and resumes the shift afterwards. Shift is a deck control, not a file tag.
Below the waveform, outside each centred platter (left for A, right for B),
the deck shows the transposed note name and Camelot key plus the selected
shift. Keys follow the library's accepted/tagged/analysed value and refresh on
library changes. With Key Lock off, tempo contributes to the displayed pitch;
fractional semitones appear as cents. Unknown source keys stay explicitly unknown.

## Library folder, OneDrive and artwork (23 October 2026)

Settings > Library > Library folder selects a local or OneDrive directory,
effective after restart. An empty directory receives a consistent library
snapshot and copies of cues, preparation, loops, queue, waveform/stem caches,
artwork and sampler assets. Originals are retained. Selecting an existing
portable library opens it rather than merging the current library into it.
No data is moved to OneDrive until the user chooses that folder.

The folder contains `library.snapshot.db` plus the portable assets. The app
uses a machine-local working SQLite database under
`<machine-data>/library-workspaces/<folder-hash>/library.db`, restoring the shared
snapshot at startup and publishing a consistent `VACUUM INTO` snapshot after a
normal shutdown. Never sync or copy a live SQLite database and its WAL files as
an alternative to this handoff. `library-location.json`, `settings.json`, audio
device selections, credentials, build tools and the Python/model runtime stay
local. `DJCRAFT_DATA_DIR` still isolates development configuration and local work.

Use one computer at a time. Close faderCraft and let OneDrive finish syncing
before opening on another machine. Mark the portable library (and any music
needed offline) Always keep on this device. This is snapshot handoff, not a
concurrent cloud database or automatic merge service. A changed shared snapshot
prevents an older session from overwriting it; unpublished local work and its
session marker are retained for recovery. Startup shows storage failures rather
than silently opening a different library. Logs report failed publication.
An interrupted session can resume locally if the shared snapshot has not changed.

Music is not moved. On a computer with different music paths, use the existing
Find missing files / relink tools, preserving track IDs rather than reimporting
duplicates. Hot cues, cue appearance, saved loops, preparation and saved queues
now use stable `library-track:<token>` keys where tracks are in the library.
Schema 19 backfills permanent random track tokens and assigns them to new tracks;
deleted SQLite row IDs cannot reattach old cues to unrelated new music. Legacy
path keys are converted on load and persisted before later relinking, retaining
a `.before-portable-ids` backup of the original JSON file. Loose
tracks not registered in the library still use their original paths.

MusicBrainz release details now offer Cover Art Archive front covers (500px).
Download cover previews an image; Save to library fetches an available cover
and stores its relative `artwork/<release-id>-500.jpg`/`.png` filename in the saved
metadata. Missing/failed covers do not block metadata saving. Cached covers are
shown in Track info before embedded artwork, and no image is written to music
tags. Existing metadata snapshots tolerate the added optional image fields.
iTunes and Discogs artwork downloads are not implemented in this first pass.
`cargo run -p djcraft-library --example cover -- <release-id> <cache-root>`
provides a read-only source-file download/cache diagnostic.

## Saved online metadata (23 October 2026)

The lookup popup fetches detailed information for a selected result in the
background and caches it for that search. MusicBrainz recording/release lookups,
iTunes track/album lookups and authenticated Discogs release lookups supply
dates, duration, credits, labels/catalogue numbers, identifiers and release
track lists where available. AcoustID recording IDs can use MusicBrainz details.
Search-generation guards discard old replies; failed detail requests leave search
results available and offer retry. All MusicBrainz requests share a rate limiter.

Discogs searches find releases, so a selected release's actual track list is
matched against the requested version before copying title/artist. An unmatched
release remains inspectable but cannot copy fabricated track metadata. Detail
inspection never writes tags. The primary Save to library action stores the
accepted main fields, year, detailed provider snapshot and searchable text in
schema 18's track_metadata table. Main fields are applied to tracks and restored
after rescans; deliberate tag edits supersede corresponding accepted fields.
Search includes saved detail values and year, but excludes other tracks in the
release track list. Reopening Metadata reads the saved snapshot offline; only
Search or switching provider performs another lookup. Both library views refresh
after save. Copy to tag editor retains the separate reviewed file-write/undo flow.
`cargo run -p djcraft-library --example lookup -- <artist> <title> --details`
prints full details for the top result of each configured provider for diagnostics.
Discogs live verification requires a personal token; fixtures cover its parser.

## Controller bridge checkpoint (current-machine work)

Latest browser expansion: 140-entry profile wires Browse CC 0x1A, Back 0x77 and Load A/B 0x4B/0x34. Uses active full Library or compact browser, actual sorted/filtered TableState selection and auto-scroll. Back toggles tracks versus sources/playlists; source rotary opens All Tracks/source/playlist, then Back returns to tracks. Clears stale row selection on controller source changes. Explicit selected-track load uses existing LibraryEvent::Load -> load_from_library -> request_load output confirmation. Global browser commands do not cancel transport gestures. File/Folder LEDs reflect current host browser focus. Four new tests cover row bounds/empty filters, relative decoding/neutral packets, press-only button handling, transport isolation and invalid browse mappings. 279 workspace tests pass (one ignored); formatting and licence/source checks pass. Hardware retest pending.

Latest LED/Shift expansion: 134-entry profile adds 40 output-only Led bindings, state-derived cue/sample/loop/FX/transport/pitch lamps, 250 ms blinking, diff-only sends and forced lamp restoration after note release. Loop In uses host loop markers; mouse changes update LEDs. Output disconnect/shutdown clears owned lamps. Sync acknowledges held press only; no persistent sync-lock state exists. Mode/browser LEDs intentionally remain untouched. Shift + FX1/2/3/Tap runs 1/2/4/16-beat loops (same length toggles off, another selects it). Shift + Hot Cue 4 toggles per-port/deck cue-delete mode, blinking populated cues; deletes persist via existing edit-locked cue store. Disconnect resets delete mode. Existing Shift sample-stop/loop-double kept. Scratch unchanged.

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

Personal private repository: https://github.com/Dwell2012/faderCraft. Shared branch:
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
git clone https://github.com/Dwell2012/faderCraft.git "$env:USERPROFILE\repos\djcraft"
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

Current-set history: schema 17 adds play_sets/play_history; no music tags are changed. Shell HistoryTracker accumulates 10 seconds of audible playback per loaded deck track, excluding Cue and scrubbing. Library > History starts fresh sets and browses retained sets. Both track browsers display a checkmark icon beside the title (hover: Played in current set). Flags persist across restarts; explicitly start New set to clear them.

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

Controller tempo matching: host supplies current analysed base BPMs to Bridge. Absolute RangedTempo assigns the nearest raw MIDI position to the paired opposite deck's BPM times tempo / own BPM. Pickup remains based on unsnapped physical values, so external changes/range changes keep existing takeover behavior. Unknown/invalid BPM or out-of-range targets use ordinary scaling. No beat-phase sync or engine DSP changes.

FX timing: DeckAction::SetEffectBeats changes the deck-wide 1/16–2 beat division (default half a beat). DeckState.effect_beats is authoritative; EffectState.beats remains a snapshot mirror for compatibility. SetEffect normalizes its incoming beat field to the deck value, so changing kinds, pad effects or controller mix values cannot reset timing. One allocation-free EffectBeats command updates every DSP slot. UI has one selector per deck; manual timing and per-slot Sync remain available. New engine decks initialize slots 1–3 to Filter/Echo/Cut, disabled.

FX Beats knob update: Binding::EffectBeats handles relative CC with two's-complement/offset encoding; Mixtrack CC 0x21/0x22 maps decks A/B with no modifier. Shared EFFECT_BEATS catalogue is 0.125/0.25/0.5/1/2/4/8. Engine validates 0.125–8 (replaces the previous 1/16–2 range); default remains 0.5. Audio delay buffers are allocated off-thread for 25 seconds to accommodate 8 beats at the minimum effective 20 BPM plus grain offsets, instead of silently clipping long delays. Added real 24-second impulse-delay and controller encoder regression tests. Installed development profile upgraded additively, preserving custom bindings.

Mixtrack startup snapshot: live-tested official init SysEx now returns every absolute CC (14 controls). Output connection sends native::MIXTRACK_PRO_II_STARTUP after input opens; Bridge::prepare_startup_snapshot arms each absolute mapping for one reading within one second. Only when ALL decks are stopped can it bypass initial pickup. Then usual external-change pickup applies; reconnect during playback never adopts. The app's JogBatch coalesces pending JogScratch commands by deck across each drained input batch, flushes before other actions/host commands and at batch end. Every MIDI packet still updates the bridge's timestamp-based speed and physical target, but only the latest motion is forwarded to the engine. Added deterministic pickup/live/expiry and delayed-batch/reversal/release-order tests.

Modal analysis (29 October 2026): schema 21 adds tracks.key_modal JSON. Analysis version 12 refreshes existing tracks; base tonic/Camelot stay compatible, with a separate conservative ModalEstimate. Characteristic degrees, third quality and candidate margin gate mode labels; UI marks modal names with '?' and deck headers distinguish unresolved modes. Manual/preferred tagged keys suppress modal overrides. Full Eyeball remains modal-unresolved with legacy E-minor result; no hardcoded exception. See MODES.md for heuristic profiles, limitations and tests.

Mode presentation refined: modal names and unresolved/estimated status now appear only in key hover tooltips (compact headers, four-deck view and library). Inline labels retain their original compact key/Camelot presentation.

Waveform bass attacks: new analysis::bass_attacks reads existing low-band RMS values, with no beat-grid dependency. DisplayColumn carries unsmoothed attack salience; warm tint stays inside actual peak silhouette, and tiny 2px ticks sit under deck overviews (4px reserved inside existing bounds). Stems retain prior style. Unit tests cover sustained-bass breaks and smoothing; 48kHz Eyeball checks around 2:22 still show other bass attacks, so this is explicitly a bass-attack cue, not kick source separation. See WAVEFORMS.md.

AMD stem diagnosis (29 October 2026): this machine has an RX 9070 XT but its original stem runtime contained torch 2.5.1+cpu, so Automatic ran on CPU. Added official Windows ROCm 7.2.1 / torch+torchaudio 2.9.1 installation into a separate stem-runtime-rocm directory; the CPU runtime remains intact. Setup verifies real GPU inference without fallback before activating through DJCraft/stem-runtime-choice.txt. Explicit DJCRAFT_STEM_RUNTIME wins. Model cache can be shared via model-cache-path.txt. Check worker.log for both adapter name and actual inference device.

AMD validation completed: both Fast and Detailed passed strict GPU inference probes on RX 9070 XT. Fresh end-to-end Rust check_stems with Detailed validated four sources from eight seconds of Eyeball; prepare.log reports Stem device: cuda and 1.637s inference. On AMD, cuda is the PyTorch API name and worker.log identifies the Radeon adapter. Eleven Python worker tests pass on CPU and ROCm environments. Original CPU environment and cached stems were retained; no graphics driver changes were needed.

Waveform jitter follow-up: removed synchronous bass-attack detection from paint and repeated per-frame column construction. OverviewCache now owns a per-track Arc<OnceLock<attack list>> filled by one background worker, shared by scrolling and overview. Ready-state transitions invalidate cached columns, with a 256-column margin retaining data during panning; per-column event access uses partition-point searches. The existing visual clock is unchanged. Tests cover allocation reuse, source alignment across negative time/panning, readiness and zoom updates. Attack tint retains deck hue at 8% saturation and 94% lightness.

Persistent attack follow-up: Track exposes its disposable waveform-cache identity, never the music-file path. App attack_cache stores time/strength records in .bass-attacks-v1 sidecars with validation and atomic publication. Normal analysis populates them; the existing UI background task loads or backfills them, with no file I/O in painting. No global analysis version bump is needed: older tracks backfill on first load. Source-time records resample to display indices without re-detection. Removed the highlight's exponential tail fade; cached salience stays fixed across scrolling. Tests cover reload reuse, different sample rates, malformed cache repair and empty results.

Sampler dock: bottom-right SAMPLER tab reuses each deck's persisted 16 slots. A–D bank selector is independent of active deck; mode/loop/lock match the deck pad state. Pad/list toggle, load/clear selected slot, drag/drop tracks, Stop All across banks and global sampler master controls. Right-click stops; empty pad opens file picker. Hold releases via the existing root mouse-up handling. Activity strips indicate playing, not playback progress. Reference: VirtualDJ Sampler Sideview manual. Named user banks/groups/editor/progress remain pending.

Named sampler correction: the engine always had ONE shared sampler bus; the initial A–D UI selector did not represent distinct stored banks. It is now replaced by real named banks (New/Rename/select), a dedicated SamplerState and MixerAction::Sampler protocol path. Panel trigger/repeat/lock/hold state is independent of deck Pads and plays at unity sample gain through SetSamplerGain. Deck/controller sampler pads address the currently selected shared bank for compatibility. Sample work carries its bank root on one ordered worker; revisions reject late results after switching. banks-v1.txt persists stable IDs/names/selection. Default bank remains at the existing Sampler root, named My Samples, so existing imports are preserved. New banks use Sampler/banks/bank-N; switching clears only sampler voices and does not stop Automix or alter decks. No mixer/audio-thread code changes.

Bundled sounds: assets/sampler/frequency303 contains 32 original WAV files (about 20 MB), source paths/hashes and full CC0 legal text. Two named starter banks MS-20 Drums and Synth FX install once (bundled-v1.txt marker). Edits/cleared slots remain after restart. Existing default bank/active selection are preserved; user-created banks start empty. All 32 bundled files are decoded and validated by a headless engine integration test. No external runtime downloads. See assets/sampler/frequency303/README.md for publisher/archive details. Bank deletion/groups/editor/progress and beat-synchronised sampler playback remain pending.

Sampler-only repaint fix: the 50ms authoritative engine poll now compares and publishes SamplerState, notifying on changed slot/loading/error/bank metadata even when all decks are stopped. This avoids a completed worker load remaining visually stuck at Loading until another UI event.

Stem menu/automatic preparation: LibraryEvent::SeparateStems is emitted by the shared browser context menu and handled by Shell for both views. Loaded tracks use existing deck progress; other tracks decode off-thread at the current engine rate and prepare/cache without loading. Per-path menu jobs deduplicate; deck cache checks refresh after completion. Automatic deck jobs now prepare on cache miss and pass a 20%-duration budget to the serialized worker. Queue wait/cache lookup excluded, cold startup included. Receiver waits are deadline-bounded, so heartbeats cannot prolong automatic work. Timeout drops the worker (owned process-tree kill), clears partial outputs, does not publish complete, and leaves deck audio running. One attempt per loaded Track identity prevents retry loops. Manual actions retain existing longer safety limits. Completion progress identity guards stale jobs. -quick still skips warm-up; auto work begins only on track loading.

Stem-selective sampler capture: new MixerAction::Sampler/SamplerAction::Capture takes source deck, destination slot and source-time region. It requires prepared stems and at least one enabled source. Engine snapshots current stem_gains at request time; worker renders mixed_sample with that fixed selection into the saved WAV. Existing DeckAction::CaptureSample also snapshots gains, retaining full-mix compatibility without prepared stems. Non-worker headless voices carry fixed gains through Command::CaptureSample/SampleVoice, including retrigger tails; worker/bank-restored baked WAVs use unity gains. No allocation, locks, file access or logging added to the audio callback. Tests cover stereo vocals-only/vocals+drums/partial gain capture, changes/eject before queued work completes, unrelated playing decks, restore from disk, and invalid/unprepared/fully-muted sources.

Vinyl held-stop fix: a long fast turn can accumulate more logical travel than the bounded audio head has rendered. Old idle handling left that target/velocity live, causing catch-up after the hand stopped. Controller Jog now emits HoldJog once after 80ms of no movement with touch still down, clears target/timing and keeps ownership. Engine/Mixer freezes the actual audible head, speed and scratch velocity without allocation, locks or I/O; old targets are never Seek-applied on later release, and resumed packets rebase on current audio position. New atomic scratching state suppresses normal forward DisplayPosition prediction during manual scratching. Contact-release grace/free-spin paths remain covered. A failing headless long-turn regression advanced two extra seconds before the fix; it now holds steady in both directions. Direct engine tests cover key-lock on/off, tempo 1.08, paused/playing and normal release. Hardware listening retest pending.

Jog release follow-up: the first held-stop fix did not cover contact-release/idle finish(true), which still appended an absolute Seek. Trace showed touch-release notes. New Jog::finish_at_audio_head uses authoritative DeckState.scratching: once audio has rendered the grab, emit HoldJog then Scratch(None) without seeking; before a callback, retain short-burst reconciliation. Applied to contact release, transient idle and Scratch-mode-off. Regression before fix jumped 56.30s to 68.00s on a paused deck. Test covers playing/stopped and both directions; free-spin release retains audible position rather than forcing controller target.

Jog follower correction: user clarified continued rapid motion rather than a direct jump. Mixer ScratchTo velocity targets could remain far ahead and drive at the last measured speed indefinitely. For measured JogScratch only, out-of-window/opposite-direction targets now rebase at the actual read head plus one measured following horizon. The window is two velocity-scaled horizons; normal close targets retain their calibration and mouse position-only scrubbing is unchanged. Tests reproduce continued audible travel with no controller idle/release and cover a reversal against a stale forward target. Existing measured-speed, slow jog gain, free-spin and held/release tests remain required. No callback allocation/locks/I/O.
