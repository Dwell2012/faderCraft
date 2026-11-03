# Rhythm Shift

Rhythm Shift replaces a playing deck's drum stem with a tempo-matched groove. It requires a loaded track with prepared stems and an analysed or manually set beat grid. The groove, rolls and their tails run through that deck's EQ, effects, channel fader, crossfader and headphone cue path.

The first pack is **House Test**, copied from the supplied WAVs to the machine-local `DJCraft/Rhythm Shift/House Test` folder under Local AppData. Choose the pack on the **Rhythm Shift** tab beside the Perform library, then switch it on for a deck. The groove starts at the next bar. R1–R4 trigger rolls at the next bar; the base groove resumes at each roll's return point while the remaining roll audio continues as a tail. Switching off restores the original drum stem.

Each pack lives in a folder under `%LOCALAPPDATA%/DJCraft/Rhythm Shift` with `pack.json`, `loop.wav`, and `roll-1.wav` through `roll-4.wav`. Audio is decoded to the current output sample rate before the pack reaches the audio thread. `pack.json` contains:

```json
{
  "name": "House Test",
  "bpm": 125,
  "loop_beats": 4,
  "return_beats": [4, 4, 4, 4]
}
```

`loop_beats` and each `return_beats` entry are independent. For example, a rock pack can use an eight-beat loop and a twelve-beat roll that returns after eight beats. Put the crash on the return beat in the roll file; the loop restarts on that same frame. The rest of the roll can be just a crash and decay. Keep the first beat of every WAV at the start of the file. The files can differ in length between packs and between rolls.

For a pack whose four rolls share a return point, `cargo run -p djcraft-engine --example check_rhythm_pack -- <folder> <bpm> <loop_beats> <return_beats>` checks decoding and timing at both 44.1 and 48 kHz without controller hardware.

The script layer exposes `do_action("deck_a", "toggle_rhythm_shift")`, `rhythm_shift_on`, `rhythm_shift_off`, and `rhythm_roll_1` through `rhythm_roll_4`. `set_param("deck_a", "rhythm_gain", $value)` maps 0–1 to zero through double gain, with unity at the midpoint. The usual `active` and `inactive` deck targets also work. These actions use the same protocol path as UI and controller actions, so controller maps can call them without the hardware attached while developing a profile.

The DDJ-GRV6 starter profile maps Drum Swap 1 to Rhythm Shift on, Drum Roll 1–4 to the four rolls, Drum Release to off, and the Groove Circuit gain knob to Rhythm Shift gain. The current test pack has one groove; Drum Swap 2–4 are reserved for packs with additional grooves. A future controller setting will also let those controls switch to Stem FX mode.
