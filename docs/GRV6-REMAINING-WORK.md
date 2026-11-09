# GRV6 mapping: remaining work

The bundled profile contains 802 bindings. Functional input mappings use fcScript; the MIDI adapter still decodes jog motion and relative browser movement, and LED bindings report app state. The mapping is validated against the transcribed AlphaTheta MIDI list and offline packet tests, but the latest changes need a physical GRV6 check. Use [the hardware checklist](GRV6-HARDWARE-TEST-CHECKLIST.md) for that run.

## Next implementation work

- Expose pad-page parameter steps through fcScript, including Beat Jump and Loop Roll banks, loop mode, Slicer length/step, Key Shift layout, and the other implemented page options. Route the existing UI parameter buttons through the same action path.
- Add MIDI Learn and a mapping editor so a user can assign those actions without editing JSON. Profiles already support held-note modifiers, and the GRV6 reports Shift as a press/release note; some shifted controls also send separate channel/note addresses. Preserve both forms.
- Add a custom assignment for Slicer if desired. AlphaTheta's GRV6 MIDI list does not identify a dedicated Slicer mode or pad address, so the factory profile deliberately leaves it unassigned. fcScript already accepts its mode, pad press and release actions.
- Map CueLoop, Saved Loops and other pad pages through fcScript after choosing hardware gestures. The GRV6 has no documented dedicated mode buttons for those pages.
- Finish migrating any remaining direct skin handlers as their controls are touched. Keep MIDI decoding and LED output in the controller adapter, with functional actions dispatched through fcScript.

## Hardware verification

- Confirm Shift + Beat Loop pads enter and release Loop Roll on all four decks, while normal Beat Loop remains a toggle.
- Confirm Master Cue, other stateful LEDs, pad mode messages, Beat FX SP/MST routing, jog feel, and audio output against the checklist. The SP and MST target bindings and engine routes exist; they need an audible device test.
- Capture MIDI traces for any control whose physical message differs from the published table, then update the profile and its independent reference together.

Rhythm Shift pack controls and its UI will be revisited when the groove packs are ready. Controller-specific preferences and broader feedback options can follow the hardware tests and mapping editor.
