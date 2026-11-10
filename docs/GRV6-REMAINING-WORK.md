# GRV6 mapping: remaining work

The goal is for every GRV6 control and shifted function to do useful work in faderCraft. faderCraft features without a corresponding GRV6 control do not need a factory GRV6 assignment. The bundled profile contains 1321 bindings. Functional input mappings use fcScript; the MIDI adapter still decodes jog motion and relative browser movement, and LED bindings report app state. The mapping is validated against the transcribed AlphaTheta MIDI list and offline packet tests, but the latest changes need a physical GRV6 check. Use [the hardware checklist](GRV6-HARDWARE-TEST-CHECKLIST.md) for that run.

## Next implementation work

- Audit every GRV6 control and Shift function against the manufacturer's MIDI list and software diagrams. Mark each as working, awaiting hardware verification, requiring a faderCraft feature, or unavailable through MIDI. Smart Rotary Selector directions, browsing shortcuts, waveform zoom, headphone Preview and Tag List now have offline mappings; verify their MIDI messages and feel on hardware. Prioritize remaining Rhythm Shift and unresolved shifted functions.
- Expose pad-page parameter steps through fcScript where a documented GRV6 control needs them. Route the corresponding UI controls through the same action path.
- Add MIDI Learn and a mapping editor so a user can assign optional actions without editing JSON. Profiles already support held-note modifiers, and the GRV6 reports Shift as a press/release note; some shifted controls also send separate channel/note addresses. Preserve both forms.
- Leave Slicer, CueLoop, Saved Loops and other faderCraft-only pages off the factory GRV6 checklist unless an actual GRV6 gesture is deliberately assigned to them.
- Finish migrating any remaining direct skin handlers as their controls are touched. Keep MIDI decoding and LED output in the controller adapter, with functional actions dispatched through fcScript.

## Hardware verification

- Confirm Shift + Beat Loop pads enter and release Loop Roll on all four decks, while normal Beat Loop remains a toggle.
- Confirm Master Cue, other stateful LEDs, pad mode messages, Beat FX SP/MST routing, jog feel, and audio output against the checklist. The SP and MST target bindings and engine routes exist; they need an audible device test.
- Capture MIDI traces for any control whose physical message differs from the published table, then update the profile and its independent reference together.

Rhythm Shift pack controls and its UI will be revisited when the groove packs are ready. Controller-specific preferences and broader feedback options can follow the hardware tests and mapping editor.
