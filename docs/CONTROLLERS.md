# Controller mapping bridge

`djcraft-controllers` is the control-side Rust API between MIDI transports and
the existing faderCraft protocol. It does not depend on GPUI and does not run on
the audio thread. Its native transport uses MIT-licensed `midir`; all generated
deck/mixer operations still pass through `Engine::apply` under the host's Role.

## Try the Numark starter

To launch the development build with the bundled official-address profile:

```powershell
./tools/run-mixtrack.ps1
```

This copies `profiles/numark-mixtrack-pro-ii.json` into the isolated development
data directory's `controllers` folder on first use, then enables that copy. Later
launches preserve personal edits. Use `-MappingPath` to select another JSON file.
The profile is independently authored from Numark's published MIDI addresses;
it is not a manufacturer-issued faderCraft preset or a complete VirtualDJ port.
The supported controls and remaining limitations are listed below.

From the repository's development shell:

```powershell
. ./tools/dev-shell.ps1
# List the exact input port names:
cargo run -p djcraft-controllers --features native --example monitor
# Read messages without controlling playback (optional duration in seconds):
cargo run -p djcraft-controllers --features native --example monitor -- 'MixTrack Pro II' 30
# Create a NEW editable profile; this refuses to overwrite an existing file:
cargo run -p djcraft-controllers --example mixtrack_profile -- "$env:DJCRAFT_DATA_DIR\mixtrack-pro-ii.json"
$env:DJCRAFT_CONTROLLER_MAP = Join-Path $env:DJCRAFT_DATA_DIR 'mixtrack-pro-ii.json'
cargo run --locked
```

Create the destination directory first if it does not exist. The optional second
argument to `mixtrack_profile` sets another exact MIDI port name. The app opens
only ports explicitly named by the profile. Without `DJCRAFT_CONTROLLER_MAP`, it
opens no MIDI ports. Profile edits take effect on app restart; the underlying
`Bridge::replace` API supports a future editor without restarting.

The starter includes play/pause, channel volume, EQ, -50%/+100% pitch, master,
crossfader, headphone level/blend, four existing hot-cue triggers per deck,
and a jog mapping for each deck. The expanded profile has 84 bindings, including Loop/Sampler pads and FX1–3, plus
Sync, transport Cue, headphone Cue, Stutter, pitch bend -/+ and Shift + Pitch-
key lock on each deck.
Pitch and EQ have exact neutral values at MIDI 64. All absolute controls use
soft takeover. Addresses and inversion come from the official VirtualDJ device
definition; hardware direction, hot-cue mode messages and endpoints still need
physical validation. The user initially reported the original controls working,
then reported that empty hot-cue pads did nothing. The corrected `hot_cue`
binding sets an empty slot and recalls an existing one, with desktop persistence
and near-beat quantization matching the UI. The first set does not start playback.
The new jog calibration still needs listening tests. The starter does not
initialize pad modes; it sends Scratch and headphone Cue LED feedback.
See [controller references](CONTROLLER-REFERENCES.md) for source provenance.

Mapping MIDI does not select audio outputs. Configure audio routing separately.

## Profile format

Profiles are versioned JSON, containing a name and mappings with unique IDs.
This minimal example maps a channel fader:

```json
{
  "version": 1,
  "name": "My controller",
  "mappings": [{
    "id": "channel-a",
    "port": "MixTrack Pro II",
    "source": { "kind": "cc", "channel": 0, "number": 22 },
    "binding": {
      "type": "parameter",
      "parameter": { "control": "volume", "deck": 0 },
      "min": 0.0,
      "max": 1.0,
      "encoding": "absolute",
      "step": 0.01,
      "pickup": true
    }
  }]
}
```

Channels, decks and indices are zero-based; JSON numbers are decimal. Sources:
`note`, `cc`, and `pitch_bend` (the latter has no `number`). Note-off and note-on
with velocity zero normalize to the same release. MIDI 1 CC/note values span
0-127; pitch bend spans 0-16383. System traffic is ignored. Use the dedicated
`jog` binding for platters rather than relative parameter bindings.

Bindings:

- `host_button`: `press` / optional `release` contain `HostCommand` values:
  `sync`, `cue` (with `pressed`), `select_deck`, or `stutter`, all with `deck`.
  These are client intents; the desktop resolves them through its existing
  beat-grid/Cue/selection logic into ordinary protocol Actions under Host role.
  Other clients must drain `Bridge::take_commands` and implement their resolver.
- `bend`: `deck`, `forward`, and `amount` (0.001-0.08). While held, timed PitchBend
  actions apply a temporary correction without changing tempo/key-lock values.
  Paused decks receive no queued button bend; releasing or disconnecting stops
  repeats and cancels unconsumed correction. PitchBend expires after 40 ms of
  rendered audio even if the host stalls. Sync and ordinary Nudge are not timed out.
- `hot_cue`: a note with `deck` and `slot` (zero-based). First press sets an
  empty cue at the current playhead (or captures the active loop), without
  starting playback. Subsequent presses recall and play it. Releases are
  ignored; an empty deck or an endpoint outside loaded audio does not set a cue.
  Desktop actions persist through the same cue store as mouse edits, respecting
  cue editing lock and the UI's near-beat snap setting. Other API clients must
  persist `SetHotCues` actions through their own metadata store.
- `jog`: one movement CC plus a `config` object containing `deck`, `touch`
  (optional note source, or null), optional `scratch_toggle` and `scratch_led`
  (note sources), relative `encoding`,
  `seconds_per_tick`, `bend_seconds_per_tick`, `scratch_enabled` and optional
  `inverted` (defaults false, reverses scratch/bend direction). Jog
  mappings use their dedicated mode button and do not accept `modifiers`.
- `button`: `press` is any serialized protocol `Action`; `release` is an
  optional separate Action. Repeated down/up messages are debounced.
- `toggle_play` / `toggle_key_lock`: `deck` chooses a state-aware toggle.
- `parameter`: `parameter`, `min`, `max`, `encoding`, `step`, `pickup`, and
  optional `center`. Supported continuous targets are deck volume, tempo, trim,
  EQ band, master, crossfader, sampler gain and headphone gain/mix. `min`/`max`
  use engine units; reverse them to invert direction. `center` specifies the
  exact engine value at raw MIDI 64 or 8192.

Relative encodings are `twos_complement` (Numark, values 1-63 positive, 65-127
negative, 0/64 neutral) and `offset64` (64 neutral). `step` is engine units per
tick. Relative values accumulate from current session state and clamp to the
configured range. Relative controls cannot use pickup.

Optional `modifiers` is a list such as
`[{"source":{"kind":"note","channel":0,"number":97},"pressed":true}]`.
Modifiers are same-port held notes. Use `pressed:false` on an ordinary binding
and `pressed:true` on its Shift alternative. Without a condition, a binding is
unconditional, including while Shift is held. A held binding's release still
executes if its modifier is released first.

## Soft takeover

With `pickup:true`, physical movement does not emit an Action until it reaches
or crosses the current software value. At software 100%, hardware values 0-126
leave it untouched; 127 acquires control. Subsequent movements follow normally.
Half-step tolerance accommodates software values between MIDI steps. Changing
the software value through the UI or another controller re-arms pickup. Resetting
or reconnecting a profile also re-arms it. The bridge compares engine values,
including inverted axes, rather than assuming every control spans 0-1.

## MixTrack jog operation

The Numark profile starts with Scratch off. Press the physical Scratch button
once to latch Scratch on; release does not change it. Press again to switch off.
The Scratch LEDs receive the latched state (including again on button release),
so their software state is no longer momentary. Physical LED behavior needs retest.

With Scratch on, touching the top and turning the wheel scrubs audibly forwards
or backwards while playing or paused. Holding the top stationary holds the
playhead; releasing it resumes the prior transport without switching Scratch off.
The rim still pitch bends during playback, even with Scratch on. With Scratch
off, wheel motion sends temporary phase bends while playing (bounded to 8%
speed correction) and scrubs audibly while stopped, releasing after 80 ms idle.
Scratch mode on still distinguishes top contact from rim movement. Key lock and the tempo
fader are never changed by the jog.

The Numark mapping uses top-contact notes 4D/4E. The Scratch button is latched;
only platter contact is held. Optional `touch:null` profiles remain supported
for hardware without top detection; those release scratching after 80 ms idle.
Scratch targets accumulate across MIDI packets even if no audio callback has
run between them. Targets clamp to the decoded portion of the file (zero to
decoded duration); there is no hidden distance to unwind at either endpoint.
The controller jog does not extend into the waveform's negative-time space.

The official definition specifies 600 counts per revolution. After user feedback
that scratching was too fast, travel is 0.00075 source seconds per count (a
quarter of the initial setting), or 0.45 seconds per rotation.
Rim bend uses 0.00025 seconds per count. Both sensitivities are editable in the
profile and require hardware/listening calibration. Other controller LEDs and
device initialization remain unimplemented.

Rim bends use a distinct transient protocol action. Unconsumed phase correction
expires in 40 ms on the audio thread, so fast rotations cannot leave seconds of
bend backlog after stopping. The controller also cancels pending correction on
idle/disconnect/manual takeover. It never restores a cached or original tempo:
the current fader/Sync ratio is retained. An earlier Nudge-based implementation
could make a -8% deck run at original speed until its +8% backlog drained; this
is covered by audio regression tests with key lock on/off.

Hardware pitch now matches the app's full range: raw 0 is ratio 2 (+100%), 64 is
ratio 1 (neutral), and 127 is ratio 0.5 (-50%). Pickup remains enabled. Profiles
can still specify a narrower pitch range if desired.

Disconnect, profile replacement, track changes and manual transport takeover
release active controller grabs. Lift and touch again after manual takeover to
resume scratching. A very brief physical disconnection between discovery scans
is subject to the existing transport limitation below.

## API for editors and other clients

### Newly mapped buttons

- Sync invokes the same effective-BPM/beat-phase match as the UI; both tracks
  need analysed grids. It does not invent BPM for unanalysed audio.
- Transport Cue holds an anchored preview; releasing stops and returns to it.
  Disconnect also releases the preview. This is separate from headphone Cue.
- Headphone Cue selects that deck as active and monitors it, preserving the
  existing cue gain/blend. The current engine monitors one active deck.
- Stutter jumps to the retained transport-Cue anchor and plays, or starts at
  zero when no anchor has been set. Repeated presses keep that same anchor.
- Pitch -/+ bends by 3% while held without rewriting the pitch fader.
- Shift + Pitch- toggles key lock. Shift + Pitch+ (pitch range) remains unmapped.

### Calls

- `Profile::from_json`, `validate`, `to_json`: validated import/export. The
  caller owns filesystem persistence and reports errors; invalid replacement
  leaves the current profile untouched. Unsupported versions are rejected.
- `Event::parse`: parse one complete MIDI channel message.
- `Event::learn`: capture an actuated source, ignoring note releases.
- `Bridge::learn(id, port, event)`: reassign an existing mapping while preserving
  its target. The editor chooses absolute/relative encoding and target range.
- `Bridge::process(port, event, &SessionState)`: emit protocol Actions. Apply
  these using `Engine::apply(role, action)` and read fresh state before the next
  event. The bridge cannot bypass engine validation or role restrictions.
- `Bridge::take_commands`: drain after processing each event and after
  disconnect/profile replacement, including host-button cleanup releases.
- `Bridge::tick(&SessionState)`: call regularly even with no input to release
  idle paused scrubs and reset grabs for replaced tracks. `process_at`/`tick_at`
  accept an explicit monotonic `Instant` for deterministic tests/hosts.
- `Bridge::cancel_deck`: release controller grabs before manual transport or
  load actions, without seeking a newly selected track back to an old target.
- `Bridge::replace` and `disconnect`: return releases for held momentary
  bindings. The host must apply those releases to avoid stuck Cue/scratch holds.
- `native::input_ports`, `Input::open`, `Input::drain`: discovery and bounded
  transport. Callbacks enqueue messages, never call the engine. Duplicate port
  names are rejected rather than guessed. Overflow requires release/reset.

The desktop host polls events every 5 ms and discovers named devices every
second, releasing held actions when an input disappears or overflows and
attempting reconnection. A very brief unplug/replug between discovery scans may
not be observed; full device identity/connection notifications are future work.
It logs setup/connection/rejection errors to the launching console.
Set `DJCRAFT_MIDI_TRACE_NOTES` to a positive count to log at most that many note
events for diagnostics; continuous CC movement is not traced by this option.

## Remaining work

Graphical mapping editor, on-screen MIDI Learn/monitor, LEDs beyond Scratch and
device initialization, physical jog calibration, paired 14-bit CC assembly,
effect selection/mix/beat encoders, and browser/load-selected-track commands are
not implemented. VirtualDJ/Mixxx/Traktor scripts and presets are references, not
directly executable or importable faderCraft profiles. Factory-profile status
requires actual hardware acceptance; a visible port alone does not establish it.

Headphone Cue LEDs (notes 0x51/0x52) follow the actual monitor deck, including mouse selection. HostButton supports an optional note `led` for SelectDeck feedback. Pad modes use distinct incoming pad notes; the live mode-cycle capture produced only Shift release, with no separate mode-identifying Note/CC message. A pad press can identify its mode, but switching alone does not establish it for the host.

## Loop, sample and FX pads

The lower row in Loop mode maps left-to-right to Loop In, Loop Out, Reloop/Exit, and Half Length. Shift + the final pad doubles length. In/Out uses the app’s existing Smart/Quantize loop options and retained loop region. Sampler mode triggers the first four existing sample slots on each deck, with the app’s sample level/repeat settings; Shift + a pad stops that slot. Load samples in the app first. The fixed upper FX1–3 pads toggle effect slots 1–3, preserving their selected effects and parameters. Tap remains unmapped.

Scratch travel is restored to 0.003 seconds/count at the user’s request for scratch-sample testing; bend travel remains 0.00025 seconds/count.

Scratch audio now eases its speed between discrete position targets (4 ms velocity smoothing) and fades near stationary speed rather than abruptly cutting at the final sample. Travel remains 0.003 seconds/count. A headless constant-signal regression covers forward/reverse MIDI steps and silent settling.

Jog contact release now waits 30 ms. A new contact press within that interval cancels the pending release without re-grabbing or seeking, so pulsed contact notes during movement remain one continuous scratch. Real release still settles once, while disconnect/manual takeover remain immediate.

Scratch volume no longer depends on instantaneous speed. A regression reproduced gain pumping from 0.4 down to 0.294 with steady one-count MIDI updates every 40 ms. The engine now keeps unity scratch gain until it reaches the target, fading only at rest; velocity smoothing and 30 ms contact release grace remain.

The three FX encoders per deck now adjust the corresponding slot’s primary parameter in 1% relative steps (CC 0x1B–0x1D left, 0x1E–0x20 right). They preserve effect type, enabled state, mix, feedback and timing. Filter’s centre is neutral; enable its slot and turn away from centre to hear it. `Parameter::EffectParameter` exposes this through editable profiles; 0/64 are neutral packets and endpoints clamp to 0–1. Shift effect selection and the fourth Beats encoder remain pending.

FX encoder correction: `EffectParameter` now follows the first app dial dynamically: Filter cutoff (also sets Mix to 100%, matching the UI), otherwise wet/dry Mix. The original raw secondary-parameter mapping was ineffective for beat-synced effects. Timing, feedback, type and enable state are preserved.
