# DDJ-GRV6 hardware test checklist

Use this with a current faderCraft build and the bundled DDJ-GRV6 mapping. For each failure, record the control, deck, mode and Shift state, expected and actual result, and whether the app UI and controller LED agree. A short MIDI trace or video helps for mismatched buttons.

## Connection and startup

- [ ] With **Automatic** selected in Settings → Controllers, connect the GRV6 before launching faderCraft. Confirm that the controller is detected and its input and output ports open. Record the displayed MIDI port name.
- [ ] Unplug and reconnect while faderCraft is running. Confirm that the mapping reconnects and controls work again.
- [ ] Start with the physical **Master Cue** lamp lit while Master Cue is off in faderCraft. On connection, the lamp should switch off. Press twice: the headphone master feed and lamp should switch on, then off together.
- [ ] Start with another stateful lamp already lit, such as Quantize or headphone Cue. Confirm that it reconciles with the app state on connection.

## Deck controls — repeat on A, B, C and D

- [ ] Load a selected track using that deck's Load button; check the correct deck receives it.
- [ ] Test Play, held Cue and Cue release, and Sync. Check the app's transport state, audible result, and that the Sync lamp stays lit while Sync is enabled, including when toggled in the app.
- [ ] Shift + Cue returns to track start. Sync stays enabled until toggled off; Shift + Sync selects the reference deck. Key Sync follows that deck's key, and Shift + Key Sync resets pitch shift. Shift + Master Tempo cycles tempo range. Tap Shift + Quantize at a steady rhythm and check the temporary TAP BPM display without changing the saved library BPM.
- [ ] Hold both deck-select buttons on the same side to enter Dual Deck. Check Play, Cue, Sync, jog, pad modes and tempo fader affect the pair while the two channel faders remain independent. A single deck-select press should exit Dual Deck.
- [ ] Test headphone Cue, Key Lock, Slip, Vinyl and Quantize. Check both the app state and the related hardware lamp.
- [ ] Play one deck at a time and check its channel meter. Lower its channel fader and confirm the meter falls. Move the crossfader and confirm the channel meter still follows the deck's post-fader level.
- [ ] Test deck selection, tempo fader, trim, high/mid/low EQ, channel volume and Sound Color FX. Confirm that each control affects only its intended deck.
- [ ] In Vinyl mode, scratch the platter in both directions, release it while moving, and release it while stationary. Try the rim while playing and stopped. Listen for jumps, lag or motion after the wheel stops.

## Mixer and outputs

- [ ] Move the crossfader and check deck routing. Test Master, Booth, headphone level and Headphones Mix controls.
- [ ] Toggle Sound Color FX On/Off. Move a CFX knob while bypassed, then re-enable it; the knob's new amount should take effect. Check the button lamp.

## Pad modes and actions — check each deck

- [ ] Select Hot Cue, Stems, Beat Jump and Sampler modes; use Shift with the mode buttons for Keyboard, Pad FX, Beat Loop and Key Shift. In four-deck view, selecting a pad mode should expand Pads if collapsed.
- [ ] Set and recall a Hot Cue, then Shift-delete it. Check the saved cue and pad lamp. In Slip mode, hold a Hot Cue and release it; playback should return to the advancing hidden playhead.
- [ ] Test Stems pads and Shift-solo; Beat Jump pads with a beat grid; and Sampler play and Shift-stop, including lamp feedback.
- [ ] Check the four Stems preset lamps and the shifted Hot Cue/Sampler lamps. Change pad mode and check that the previous mode lamp clears. Beat Jump, Keyboard and Pad FX pads should acknowledge a held press.
- [ ] Hold and release Pad FX pads, including the Shift bank. The effect should be momentary and previous slot settings should return on release.
- [ ] Test all Beat Loop lengths and Key Shift pads. Note the exact pad if its page changes correctly but its action does not.
- [ ] Hold each Shift + Beat Loop pad. It should open Loop Roll, loop only while held, and resume from the uninterrupted playhead on release. Normal Beat Loop pads should still toggle loops. No Slicer assignment is bundled because the MIDI table does not identify a dedicated Slicer control.

## Loops and Beat FX

- [ ] Test Loop In/Out, four-beat loop, Reloop/Exit and loop halve/double. Confirm that these buttons leave the current pad page selected and that loop lamps follow the loop state.
- [ ] Turn the Beat FX selector, route the effect to CH 1–4, Master and Sampler, and toggle On/Off. Check the audible target and app state.
- [ ] Sweep Level/Depth and step Beat −/+ through its range. Check the hardware division indicator. Select both Mobius positions and listen to the Shepard Tone Saw/Tri effects over a track and silence.

## Browser

- [ ] Turn the browse encoder through the visible, sorted/filtered track list. Switch between tracks and sources, then load the highlighted track. Check that the selection and loaded deck match the controller input.
- [ ] Tilt the Smart Rotary Selector in all four directions, with and without Shift. Check area changes and top/bottom jumps; press it to switch areas, then Shift+press to open a folder.
- [ ] Browse into a source subfolder, nested playlist folder and sub-crate. Shift+press should expand the selected tree node; Back should collapse it or return to its parent without changing the loaded decks.
- [ ] Use Discover, Back, and View with and without Shift. Check suggestions, related tracks, playlists, full Library, and Tag List membership.
- [ ] With a highlighted track different from the deck track, check Discover suggests from the highlight and Shift+Discover suggests from the loaded deck. Shift+Back should switch between folders and playlists.
- [ ] Turn Shift+Browse and confirm the main waveforms zoom in/out. Press Preview and confirm the selected track is heard only in headphones, with the deck and master unaffected. Shift+Preview should jump 30 seconds; a second Preview press should stop.

## Failure notes

| Control / deck | Mode / Shift | Expected | Actual | UI and LED agree? | MIDI trace / video |
| --- | --- | --- | --- | --- | --- |
| | | | | | |
