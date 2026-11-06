# DDJ-GRV6 hardware test checklist

Use this with a current faderCraft build and the bundled DDJ-GRV6 mapping. For each failure, record the control, deck, mode and Shift state, expected and actual result, and whether the app UI and controller LED agree. A short MIDI trace or video helps for mismatched buttons.

## Connection and startup

- [ ] With **Automatic** selected in Settings → Controllers, connect the GRV6 before launching faderCraft. Confirm that the controller is detected and its input and output ports open. Record the displayed MIDI port name.
- [ ] Unplug and reconnect while faderCraft is running. Confirm that the mapping reconnects and controls work again.
- [ ] Start with the physical **Master Cue** lamp lit while Master Cue is off in faderCraft. On connection, the lamp should switch off. Press twice: the headphone master feed and lamp should switch on, then off together.
- [ ] Start with another stateful lamp already lit, such as Quantize or headphone Cue. Confirm that it reconciles with the app state on connection.

## Deck controls — repeat on A, B, C and D

- [ ] Load a selected track using that deck's Load button; check the correct deck receives it.
- [ ] Test Play, held Cue and Cue release, and Sync. Check the app's transport state and audible result.
- [ ] Test headphone Cue, Key Lock, Slip, Vinyl and Quantize. Check both the app state and the related hardware lamp.
- [ ] Test deck selection, tempo fader, trim, high/mid/low EQ, channel volume and Sound Color FX. Confirm that each control affects only its intended deck.
- [ ] In Vinyl mode, scratch the platter in both directions, release it while moving, and release it while stationary. Try the rim while playing and stopped. Listen for jumps, lag or motion after the wheel stops.

## Mixer and outputs

- [ ] Move the crossfader and check deck routing. Test Master, Booth, headphone level and Headphones Mix controls.
- [ ] Toggle Sound Color FX On/Off. Move a CFX knob while bypassed, then re-enable it; the knob's new amount should take effect. Check the button lamp.

## Pad modes and actions — check each deck

- [ ] Select Hot Cue, Stems, Beat Jump and Sampler modes; use Shift with the mode buttons for Keyboard, Pad FX, Beat Loop and Key Shift. In four-deck view, selecting a pad mode should expand Pads if collapsed.
- [ ] Set and recall a Hot Cue, then Shift-delete it. Check the saved cue and pad lamp. In Slip mode, hold a Hot Cue and release it; playback should return to the advancing hidden playhead.
- [ ] Test Stems pads and Shift-solo; Beat Jump pads with a beat grid; and Sampler play and Shift-stop, including lamp feedback.
- [ ] Hold and release Pad FX pads, including the Shift bank. The effect should be momentary and previous slot settings should return on release.
- [ ] Test all Beat Loop lengths and Key Shift pads. Note the exact pad if its page changes correctly but its action does not.
- [ ] Hold each Shift + Beat Loop pad. It should open Loop Roll, loop only while held, and resume from the uninterrupted playhead on release. Normal Beat Loop pads should still toggle loops. No Slicer assignment is bundled because the MIDI table does not identify a dedicated Slicer control.

## Loops and Beat FX

- [ ] Test Loop In/Out, four-beat loop, Reloop/Exit and loop halve/double. Confirm that these buttons leave the current pad page selected and that loop lamps follow the loop state.
- [ ] Turn the Beat FX selector, route the effect to CH 1–4, Master and Sampler, and toggle On/Off. Check the audible target and app state.
- [ ] Sweep Level/Depth and step Beat −/+ through its range. Check the hardware division indicator. Try an unsupported Mobius selection; it should not activate an unrelated effect.

## Browser

- [ ] Turn the browse encoder through the visible, sorted/filtered track list. Switch between tracks and sources, then load the highlighted track. Check that the selection and loaded deck match the controller input.

## Failure notes

| Control / deck | Mode / Shift | Expected | Actual | UI and LED agree? | MIDI trace / video |
| --- | --- | --- | --- | --- | --- |
| | | | | | |
