# fcScript

fcScript is faderCraft's developing control language. It turns a control
gesture into a validated faderCraft action. `set_param` and `do_action` are
implemented as compiled commands; this page documents **what works now**.
A script editor, custom buttons, controller script bindings, state queries,
and custom skins are future work. There is currently no place in the app to
type a script.

## Commands

The public API has two kinds of command:

```text
set_param("deck_a", "eq_high", $value)
do_action("deck_a", "toggle_play")
```

`set_param(target, parameter, value)` is for controls that hold a value.
Supported targets are `deck_a`, `deck_b`, `deck_c`, `deck_d`, `active`, and
`inactive`. `active` resolves to the currently selected deck when the command
runs. `inactive` resolves to **all visible decks except the active one**: one
deck in two-deck mode, or three decks in four-deck mode. A command targeting
`inactive` can therefore produce multiple actions. The third
argument is either a normalized numeric constant or `$value`, which uses
the invoking control's normalized value. The target and parameter names
must be quoted with double quotes.

| Parameter | Domain | Minimum | Neutral | Maximum | State needed? |
| --- | --- | --- | --- | --- | --- |
| `eq_low`, `eq_mid`, `eq_high` | −1…1 | Cut at −1 | Unity at 0 | Approximately +6 dB at +1 | No; Classic EQ only |
| `volume` | 0…1 | Silent at 0 | — | Unity at 1 | No |
| `trim` | −1…1 | −12 dB at −1 | 0 dB at 0 | +12 dB at +1 | No; manual trim disables auto-trim |
| `tempo` | −1…1 | Slow end at −1 | Normal speed at 0 | Fast end at +1 | Yes, for selected range |
| `cfx` | −1…1 | Full left at −1 | Neutral at 0 | Full right at +1 | Yes, to preserve selected effect |

These are normalized **parameter values**, independent of a skin's knob
coordinates or a MIDI device's raw byte range. A skin or controller maps
its physical position into the parameter domain before invoking fcScript.
The script then projects that value onto the parameter's existing curve.
For example, `set_param("deck_a", "trim", 0)` sets 0 dB, while
`set_param("deck_a", "volume", 0.5)` sets 25% linear gain.

`do_action(target, action)` is for discrete commands. The currently accepted
actions are `play`, `pause`, `toggle_play`, and `unload`. `toggle_play` reads
the latest session state at execution time. These actions occur once per
invocation; a button binding must trigger on its press edge, not on every
repeated MIDI packet. Momentary controls such as cue preview need an explicit
press/release contract and are not yet supported by `do_action`.

The deck-focused Space shortcut uses
`do_action("active", "toggle_play")`. It retains its existing toggle behaviour.

The two-deck pitch fader, four-deck tempo knob and tempo scroll gestures use
`set_param` with `tempo`. Each gesture supplies a bipolar value; fcScript reads
the current deck range at invocation time. The fader retains its downward-fast
direction, the knob retains its upward-fast direction, and scrolling retains
its 0.1 percentage-point step. Sync, tempo-range selection and the gradual
return-to-zero ramp still use their existing protocol paths.

## Legacy `set_eq` spelling

```text
set_eq <deck> <band> $value
```

| Argument | Accepted values | Meaning |
| --- | --- | --- |
| `deck` | `A`, `B`, `C`, `D` (case-insensitive) | Physical audio deck. It does not change when the mixer columns are reordered. |
| `band` | `low`, `mid`, `high` | Classic EQ band. Band names are lowercase. |
| `$value` | Literal placeholder | The invoking knob/fader supplies a number from 0.0 to 1.0 at run time. |

Examples of valid programs:

```text
set_eq A high $value
set_eq B mid $value
set_eq C low $value
set_eq D high $value
```

`$value` is a **knob position**, not decibels. At 0.0 the band is cut; at
0.5 it has unity gain; at 1.0 it reaches the current Classic EQ maximum
(approximately +6 dB). Positions below neutral map linearly to gain 0–1;
positions above neutral use the existing mixer curve. This preserves the
response users already get from the built-in EQ knobs.

`set_eq A high $value` remains accepted for older bindings. Its `$value`
is a **0…1 knob position** with 0.5 neutral; the generic `set_param` uses
−1…1 for EQ with 0 neutral. Both produce the same `DeckAction::SetEq`
and then goes through normal
engine validation and audio-thread command delivery. The engine accepts this
action only while that deck uses **Classic EQ**. The Modern, EZ Remix and
Stems modes use stem gains instead; `set_eq` does not silently change those
controls. The two-deck and four-deck Classic EQ controls already use this
same command path.

## Errors and current API

Compilation rejects unknown commands, targets, parameters, actions, malformed
arguments and nonnumeric fixed values. Execution rejects parameter values
outside their declared domain, including NaN and infinity. Stateful commands require a current
session snapshot; they return an error if it is missing. The host can still reject an otherwise valid
action, for example when the deck is in a different EQ mode. A rejected
action must not be presented as a committed state change.

The Rust API is `djcraft_protocol::script::Program`:

```rust
use djcraft_protocol::script::{ControlSource, Program};

let program = Program::compile("set_param(\"deck_a\", \"eq_high\", $value)")?;
let dispatch = program.run(ControlSource::Skin, 0.0)?; // neutral EQ
// The host applies dispatch.action under its normal role checks.
```

`Program::set_param(deck_index, param)` and `Program::do_action(deck_index,
action)` construct typed programs without parsing text; deck indices are
zero-based. Compile once for a binding and run it for each control movement.
Use `run_with_state(source, value, Some(&session))` for `tempo`, `cfx`, and
`toggle_play` when targeting a named deck. Use `run_in(InvocationContext,
value)` for `active` and `inactive`; the context supplies the selected deck,
visible deck count and session state. It returns one dispatch per resolved
deck. Parsing on every knob tick is unnecessary.

## Origin and UI feedback

Each invocation supplies a `ControlSource`: `Skin`, `Custom`, or `Controller`.
The returned dispatch carries this origin alongside its action. Origin is
control-side metadata; the engine action itself stays independent of the UI.
As fcScript grows, a common invocation context can carry the origin and any
other gesture metadata so individual commands do not accumulate arguments.

The authoritative session state is the feedback path. After an action is
accepted, the active skin, custom controls and controller LEDs should observe
the changed state. A controller-originated EQ change must update whichever
skin is active; a skin-originated change must reach the controller's feedback.
Do not rely on a direct callback into one skin. A view may update its local
drag indicator immediately, then reconcile it with the next state snapshot.

**Current boundary:** the built-in mixer's channel EQ, trim, volume and CFX
controls and the deck play button extract actions from dispatches and use
their existing state-refresh paths. Controller scripts and a common
dispatcher/subscription layer are not implemented yet. The origin field is
preserved by the fcScript API but is not yet consumed by the app dispatcher.
