# Controller mapping bridge

`djcraft-controllers` is the control-side Rust API between MIDI transports and
the existing faderCraft protocol. It does not depend on GPUI and does not run on
the audio thread. Its native transport uses MIT-licensed `midir`; all generated
deck/mixer operations still pass through `Engine::apply` under the host's Role.

## Settings

When no matching controller is detected, the mapping selector displays Disabled. The saved mapping is retained for later detection; an absent device does not permanently disable discovery. Explicitly selecting Disabled still disables controller input.

Settings > Controllers selects the mapping: Automatic (detected controller), bundled MixTrack, DDJ-GRV6, Disabled, or a valid JSON profile in the machine-data controllers folder. Automatic selects the bundled GRV6 or the installed/default MixTrack mapping when exactly one known MIDI device is present, including after hot-plugging; an explicit choice takes priority. Load mapping opens a file picker for another faderCraft JSON profile and validates it before switching. The chosen path is remembered; keep external files at that path. Files are not overwritten. Selecting a different mapping releases held controls and closes the previous ports. Controller hardware is matched by exact MIDI port name from the profile; an unknown or duplicate port is not guessed.

Controller options are stored per profile. The FX 1 controls choice appears only for a Numark MixTrack Pro II mapping; its bundled and installed versions share that preference. Other controllers do not inherit it. Older global FX 1 settings are migrated to the MixTrack option when settings are loaded.

## Startup and hardware changes

The bundled AlphaTheta DDJ-GRV6 starter is an explicit Settings > Controllers
choice. It maps play, cue, sync, key lock, headphone cue, tempo, trim, EQ and
volume for four decks, plus deck selection, browser scroll, four deck-load buttons, crossfader,
master, booth and headphone controls. Its `DDJ-GRV6` MIDI port name is also used by
the public Mixxx preset, though OS-specific names still need checking. The
Hot Cue pads and their saved-cue LEDs are mapped; the shifted pad messages
delete individual cues. The four Quantize buttons toggle each deck's general
quantize gate, and their LEDs report its state. Dedicated Hot Cue, Stems,
Beat Jump and Sampler mode buttons switch the visible pad page for each deck.
Shift + Hot Cue opens Keyboard. Pick a saved cue with its pad, then play the
eight scale notes. Shift + pad selects a different cue. See [pad modes](PADS.md).
Master Cue adds the master feed to the headphone cue bus. Its LED follows the
app's master-cue state, including at controller connection. Headphones Mix
blends that bus with the always-available master side.
Loop In and Out set a manual loop. Hold In / 4 Beat to start a four-beat loop
anchored to the initial press, using the track's beatgrid and loop options.
Reloop / Exit exits or recalls the last loop. While
a loop is active, Cue/Loop Call left and right halve and double it. The In and
Out lamps follow the loop marker and active loop. These buttons leave the
current performance pad page selected.
The profile has 849 entries. The Smart Rotary Selector tilt and press gestures
navigate the browser through fcScript. Shifted browsing jumps to list ends,
opens folders or playlists, and zooms the main waveforms. Preview auditions the
selected file on the headphone cue bus without loading a deck; Shift+Preview
jumps forward 30 seconds. Shift+View adds the highlighted track to a persistent
Tag List crate. These newly mapped gestures need hardware verification.
Each Sound Color FX knob drives its channel's
CFX amount, with the centre detent at neutral. Sound Color FX ON/OFF bypasses
all four channels and lights when CFX is enabled. Turning the knobs while
bypassed sets the amount that will be used on re-enable. Each deck's platter and rim
share a jog gesture. GRV6 scratch travel uses 6,200 counts per revolution at
33⅓ rpm, or about 0.290 ms of source audio per count. A hand turn slightly
over a quarter revolution measured 1,577 net counts.
Vinyl mode uses platter touch to scratch. The rim bends pitch during playback
and scrubs when stopped. Play, Cue, headphone Cue, Load, Slip and Vinyl have lamp
feedback alongside Quantize and Hot Cues. Jog sensitivity and visible lamp
behaviour still need a hardware check.

For Beat FX, turn the selector to choose an effect, press CH 1–4, MST
(Master), or SP (Sampler) to route it, then press On/Off. Level/Depth sets the
effect amount. Beat −/+ steps from ¼ to 32 beats on decks, Master and Sampler.
The hardware indicator follows the division.
Deck routing uses the selected deck's first FX slot. On startup, On/Off uses
that slot until the GRV6 reports a selector change. The tested startup requests
did not return a resting selector position.
Deck timing follows that deck's effective BPM. Master and Sampler follow the
first playing deck with a beat grid, with 120 BPM as a fallback.

The selector uses faderCraft's Delay, Echo, Low-cut Echo, Spiral, Reverb,
Flanger, Phaser, Filter, Cut (Trans), Pitch Shift, and Beat Repeat (Roll).
Helix uses Beat Repeat as an approximation. Shepard Tone Saw and Shepard Tone Tri generate
an octave-cycling tone over the selected source, including silence; Beat sets
the glide cycle and Level/Depth sets tone volume. Changing the target switches off the previous deck's first
FX slot. Master processing feeds both Master and
Booth, and Sampler processing affects only sample playback.

In Sampler mode the left pads trigger slots 1–8 of the selected sample bank
and the right pads trigger slots 9–16. Shift stops the corresponding sample.
The lamps show populated and playing slots.
Stems and Beat Jump pads mirror the corresponding faderCraft pad pages.
Shift solos a source on the first four Stems pads. Those pads light for
unmuted sources once stems are ready, including while Shift is held.
Beat Jump uses the
selected page's beat bank and requires a beat grid.

The GRV6 Hot Cue page uses note 0–7 on MIDI channels 8/10/12/14 for decks
A/B/C/D (zero-based channels 7/9/11/13 in the profile). Shifted pads arrive
on channels 9/11/13/15 and delete the corresponding cue. Feedback uses the
unshifted note address: a populated cue lights its pad, and deleting it clears
the light. This is based on the supplied AlphaTheta MIDI message table and
offline packet tests; LED behaviour has not yet been checked on the device.

The GRV6 Pad FX bank sends notes 80–87 on the same per-deck pad channels;
the Shift channels select effects 9–16. Each note runs an fcScript press action
and its matching release action, so the effect is momentary and the previous
slot settings return on release. Pressing a Pad FX note also opens that deck's
Pad FX page. These addresses come from the supplied MIDI table and have
offline packet tests; the hardware response still needs checking.
The remaining shifted mode buttons now open Pad FX (note 107), Beat Loop
(109), and Key Shift (111). Beat Loop notes 96–103 toggle the existing loop
pad lengths; Key Shift notes 112–119 use the existing key-shift pad bank.
Shift + Beat Loop pads (the documented shifted notes 96–103) now open Loop Roll
and hold a momentary roll of the corresponding length; releasing the pad
returns to the uninterrupted playhead. Normal Beat Loop pads keep their toggle
behaviour. AlphaTheta's message list has no Slicer mode or Slicer pad address,
so the bundled GRV6 profile does not claim a dedicated Slicer mapping. fcScript
does expose `pad_mode_slicer`, `pad_press_slicer_N` and `pad_release` for a
custom mapping once a suitable hardware gesture is chosen.
The GRV6's Shift + Key Shift range choice remains unmapped until its exact
range selection can be matched to the app's bank choices. Pad-mode selectors
and these two pad banks now invoke fcScript. The script dispatcher passes
host-owned pad and mode actions to the existing UI handler, which keeps page
selection, loop behaviour, and controller feedback consistent.

Hardware ownership is **not** a prerequisite for authoring a profile. For each
device, derive MIDI addresses and value formats from the manufacturer's MIDI
table, cross-check intended behaviour against official software manuals and
public mappings, then run profile validation and simulated packet tests without
the device. Keep a per-control source and status (`documented`, `cross-checked`,
or `hardware-verified`) so a published profile does not imply more testing than
it has received. Hardware reports can later correct quirks, timing, port names,
LED initialization and audio routing; they should not gate writing the mapping.
For GRV6, sources are AlphaTheta's *DDJ-GRV6 MIDI Message List E1*, the
[Mixxx GRV6 mapping](https://github.com/joergbeyer/mixxx/blob/main/res/controllers/Pioneer-DDJ-GRV6.midi.xml),
and the [VirtualDJ GRV6 manual](https://virtualdj.com/manuals/hardware/alphatheta/ddjgrv6/setup.html).

The transcribed GRV6 MIDI addresses live in
`profiles/references/alphatheta-ddj-grv6.json`, separately from the active
profile. Run `python tools/audit_controller_profile.py` after editing the map.
It checks all 218 mapped addresses against the reference, checks for missing
or undocumented entries, and rejects ambiguous input addresses (while allowing
a button and its LED to share a MIDI address). CI runs the same audit plus
negative tests that deliberately swap a mixer address and collide two buttons.
The audit exposed and fixed three real errors: Master Level had been bound to
Booth Level, and Headphones Level and Mix were reversed. Passing this audit
means the addresses match the document; it does not verify USB port naming,
firmware behaviour, LED colour handling, or audio routing without hardware.
Booth Level now uses the documented CC 9/41 pair on MIDI channel 7 and calls
`set_param("mixer", "booth_gain", $value)`. Booth taps the stereo mix before the
Master volume, has its own level knob in the Output tab, and requires a distinct
stereo output pair selected in Audio settings. It stays independent when Master
is turned down.

Normal launches now offer **Use controller / Not now** when a uniquely named MIDI input matches the active mapping. Ports and LED output open only after acceptance. Detection polls once per second; disconnecting releases held controls and clears the decision, so reconnecting asks again. Not now suppresses repeat prompts until disconnect or restart. Identically named devices are not guessed between.

Automatic starts with the bundled Numark MixTrack Pro II profile, with an existing machine-data `controllers/numark-mixtrack-pro-ii.json` taking precedence to preserve custom edits, then switches to the bundled GRV6 profile if that is the one known controller detected. `DJCRAFT_CONTROLLER_MAP` overrides Automatic with an explicit profile. Unknown controllers are not assigned guessed mappings; a general profile chooser/catalogue and support for other hardware remain future work. Profile files are read on startup, but MIDI ports open only when a known, unambiguous controller is detected.

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
offers the installed or bundled MixTrack profile when matching hardware is detected. Profile edits take effect on app restart; the underlying
`Bridge::replace` API supports a future editor without restarting.

The starter includes play/pause, channel volume, EQ, -50%/+100% pitch, master,
crossfader, headphone level/blend, four existing hot-cue triggers per deck,
and a jog mapping for each deck. The expanded profile has 142 bindings, including Loop/Sampler pads and FX1–3, plus
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
  starting playback. Subsequent presses recall and play it. In Slip mode,
  releasing a held cue returns to the advancing hidden playhead. The binding
  executes fcScript's `hot_cue_N` and `release_hot_cue` actions; an empty deck
  or an endpoint outside loaded audio does not set a cue.
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

- Sync latches effective-BPM/beat-phase matching and can be toggled off from
  either the controller or UI. Both tracks need analysed grids. Shift + Sync
  selects the reference deck. Shift + Cue returns to track start. Shift +
  Master Tempo cycles pitch range; Key Sync matches the reference key and
  Shift + Key Sync resets the key shift. Shift + Quantize taps a temporary BPM
  estimate shown briefly in the deck header without overwriting library BPM.
  Pressing both deck-select buttons on one side enables Dual Deck transport;
  a single deck-select press exits it. The tempo fader moves both paired decks
  to the same pitch ratio; mixer levels remain independent. The GRV6 Sync lamp
  follows the latched Sync state, including changes made in the app.
- Transport Cue holds an anchored preview; releasing stops and returns to it.
  Disconnect also releases the preview. This is separate from headphone Cue.
- Headphone Cue toggles that deck in the headphone bus, independently of the
  active deck, preserving gain/blend and other cued decks. Multiple decks or
  no decks may be cued. LEDs follow the cue mask. Bundled headphone buttons use
  ToggleHeadphoneCue; named legacy headphone-cue mappings are upgraded in memory.
  SelectDeck now activates keyboard controls without changing headphones.
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

Graphical mapping editor, on-screen MIDI Learn/monitor, browser/mode LED integration and
device initialization, physical jog calibration,
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

## LED and Shift pad feedback

The 134-entry profile includes 40 explicit output-only LED bindings. FX buttons reflect enabled slots; populated hot cues are lit; loaded samples are lit and playing samples blink at 250 ms intervals. Loop In lights when its marker is set (including mouse changes), and the other loop pads light with an active loop. Play/Stutter are steady during playback and blink when loaded/paused; Cue lights while held and blinks when paused. Pitch lamps show neutral tempo. Sync lamps acknowledge a held press; there is no persistent Sync-lock state. Scratch/headphone Cue feedback is retained. Feedback refreshes from host state, sends changed messages only, restores locally overwritten lamps after button releases, and clears owned LEDs on disconnect/shutdown. Mode LEDs stay controller-owned because mode changes are not separately reported; browser LEDs await browser control integration.

Shift + FX1/2/3/Tap selects 1/2/4/16-beat auto loops using the app grid and loop options. Selecting the same active length exits; another length replaces the loop. Shift + Hot Cue 4 toggles per-deck cue-delete mode: populated cue pads blink, and pressing one deletes it through the normal cue persistence/edit-lock path. Toggle Shift + Hot Cue 4 again to exit. Disconnect also exits delete mode. Shift + Sample stops that slot; Shift + Loop Size doubles the loop as before.

## Hardware browser

Browse CC 0x1A moves the highlighted row in the active full Library table or compact browser under the decks. It uses the visible sorted/filtered rows, scrolls the selection into view and clamps at list ends. Back (note 0x77) switches between tracks and the source/playlist list. In source mode the encoder opens All Tracks, a source, or a playlist; Back returns to track browsing. Load A/B (notes 0x4B/0x34) load the selected row onto that deck through the existing confirmation when its output is open. No selection means no load. File/Folder LEDs show the encoder focus. Radio switches to Live when a browse command arrives. Browsing does not cancel a held Cue or scratch gesture. The profile now has 140 entries. Shift + Back and a browse push are not defined by this starter.

## Tempo fader ranges and transport Cue

Each deck starts in Low (±8%). Shift + Pitch+ (the controller Range function), or the on-screen ±% range button, cycles Low 8% → Medium 16% → High 32% → Wide 48% → Low. Both hardware and app faders use their entire travel for each range, with unity at centre; the hardware retains its inverted direction and exact neutral at MIDI 64. Range is per-deck protocol state (`TempoFaderRange`, default Low for older state payloads), applied through SetTempoRange without changing playback tempo or stopping Automix. `RangedTempo` resets pickup on a range change; existing generic Tempo mappings retain their configured endpoints. A tempo outside the newly selected range is preserved until reset or pickup becomes possible at a wider range. Wide’s endpoints are ratios 0.52 and 1.48. The starter now contains 142 entries.

Transport Cue remembers where Play started, including a pending paused seek, rather than creating a new cue at the current playing position. Press during playback returns there and pauses; while paused, holding Cue previews from the retained point and release returns to it. Ordinary tempo/mixer/pad/scratch operations preserve that anchor. Paused repositioning prepares a new start, Play captures it, and loading another track clears the old anchor. Hot cues remain separate.

## Platter free spin

After a scratch-surface gesture, releasing contact keeps scratching while real jog movement packets continue, forwards or backwards. The last surface contact retains ownership through the release instead of immediately converting the remaining spin into pitch bend. After 80 ms without motion the gesture settles once and restores the prior playing/paused transport; a later rim gesture pitch bends normally while playing. A stationary contact release still uses the 30 ms contact grace. Retouching during free spin keeps the same accumulated target. Disabling Scratch, manual transport takeover, disconnect, profile/track replacement still cancel the gesture. No synthetic spin or new audio-thread work is added; sensitivity, tempo and key lock are unchanged.

## Measured platter speed

Native MIDI input now preserves the driver’s microsecond timestamps through a timed event queue; UI drain time is not used as a substitute. Jog displacement divided by packet interval produces signed platter speed, retaining the 0.003 source-seconds/count calibration and existing ±4× engine bound. `JogScratch` carries position, measured velocity and interval through protocol validation. The audio callback uses measured velocity with a small bounded position correction and a packet-sized following buffer, instead of position chase alone. Reversals, phase alignment and stationary fade remain bounded by the physical target; no motion is generated beyond it. Waveform ScratchTo remains unchanged. Paused final seeks also clear residual varispeed so releasing a spin does not drift the settled position.

Ranged hardware tempo faders snap their nearest MIDI position to the opposite deck's exact effective BPM (A/B and C/D pairs). Both analysed BPMs must be known and the target must fit the selected range. Soft takeover still applies; the next fader step leaves the snap. This matches tempo only, without changing beat phase or enabling Sync.

Mixtrack Beats encoders (CC 21/22 hexadecimal, two's-complement relative) step the shared FX division independently per deck: 1/8, 1/4, 1/2, 3/4, 1, 2, 4, 8, 16, 32 beats. Steps clamp at the endpoints and neutral packets do nothing. The bundled and development profiles now contain 144 mappings. The app selector uses the same divisions.

On Mixtrack Pro II output connection, the app sends the manufacturer's verified startup request after opening input. One absolute reading per mapped control may adopt the physical position within one second, while every deck is stopped. Active playback, expired snapshots and later software changes retain soft takeover. Relative encoders/buttons are not restored as absolute positions. A drained MIDI batch now forwards only the latest JogScratch per deck, retaining timestamp-derived velocity and cumulative physical position. Non-jog actions and host commands flush pending motion first so touch/transport ordering remains intact.

## Abrupt stop while holding the platter

Playing-deck motor pickup: after 30 ms of confirmed touch release, forward and reverse scratches return to normal playback at the audible head once wheel speed falls to playback speed or below; fast spins continue until they slow or go idle. Remaining wheel-inertia packets are ignored until 80 ms of wheel silence or a new touch, so they cannot drag playback away from the selected tempo. Paused decks retain free-spin scratching; touchless profiles still rely on packet/idle handling. This supersedes the playing free-spin behaviour described in the earlier notes below.
After 80 ms without wheel packets while touch remains down, the bridge sends one HoldJog action. The audio callback freezes its actual read head and clears scratch velocity; the controller discards its accumulated target. This prevents delayed catch-up after a long fast spin. The hand remains grabbed and later wheel packets rebase from the current audio position. A later lift releases the hold without a Seek to the old target. Existing free spin after physical release, touch-note grace and touchless transient behavior are retained.

DeckState now publishes scratching through an atomic so the UI does not extrapolate normal forward playback while a jog/waveform scratch is held. Tests reproduce two seconds of leftover forward movement before the fix, check forward/reverse held stops and rebased movement, and verify paused/playing release with key lock enabled/disabled. Hardware retest is still needed.

Release-path follow-up: MIDI trace contained touch-release packets, which took the older grace/idle completion path and could still Seek to the cumulative target. All settled releases (including transient scrubs and Scratch-mode-off) now preserve the actual audio head when DeckState reports an active rendered scratch: HoldJog then Scratch release, with no Seek. The original tiny touch/move/release-before-first-audio-callback path still reconciles its final position. A new regression reproduced an 11.7-second forward jump from the stale target and covers playing/paused plus both directions. Free-spin tests now assert release from the actual audible position. Hardware retest pending.

Rapid catch-up follow-up: stopping/release fixes alone did not bound the audio follower while it was still chasing a distant cumulative target. Measured JogScratch targets are now rebased to one packet-following horizon in the measured direction if their distance exceeds two horizons or contradicts the measured velocity. Ordinary nearby targets and position-only ScratchTo keep their prior behavior. This bounds late travel after a long/over-speed/coalesced turn and makes reversals follow the wheel immediately instead of the old target. New audio-only regressions stop packet delivery without sending an idle/release command, verify silence/stability after settling, and reverse while the old cumulative target is still far ahead. Hardware retest pending.

## Single-controller automatic startup
A single unambiguous input matching the loaded mapping now connects automatically at startup and on reconnect. The splash displays the mapping name and actual open input connection. Unrelated MIDI ports do not count as matching controllers. Multiple matching inputs still prompt, duplicate names are not guessed, and Disabled / an explicit Not now decision are honoured. This replaces the earlier requirement to approve every single-device connection.

## GRV6 deck setup scripts

Key Lock, Slip, and headphone Cue buttons on all four decks use fcScript `do_action` bindings. The button press reads current session state and dispatches the same protocol action as the matching skin control; repeated presses and note releases do not trigger a second toggle. Vinyl remains a host command because it changes controller scratch-mode settings rather than an engine parameter.
