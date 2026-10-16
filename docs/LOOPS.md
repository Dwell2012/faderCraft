# Deck loop controls

The LOOP view menu offers Main, Adjust, Saved, Record and Direct. The design
follows https://virtualdj.com/manuals/virtualdj/interface/decks/decksadvanced/loops.html
while keeping DJCraft's neon deck colours.

Click the central beat count to engage or exit a loop. The arrows halve/double
its length; the mousewheel does the same. A progress bar follows the loop.
Reloop returns to the last successful loop on the currently loaded track.

- Main: In sets the start; Out closes and enables the manual loop.
- Adjust: move In, the whole region, or Out using the paired buttons or wheel.
  Steps are 1/16 beat with a grid, otherwise 10 ms; Shift-wheel is finer.
- Saved: four slots per track path, kept in saved-loops.json in app data.
  An empty slot saves the active loop, or creates a four-beat loop first.
  Filled slots recall the loop; the adjacent cross deletes the saved slot.
- Record: REC exports the active loop, including when paused. Without an active
  loop, REC marks the playing position and Stop REC exports that source interval.
  Files are 16-bit stereo WAV in the app data directory's Sampler Recordings folder.
  This currently exports original source audio, not the processed live output;
  tempo, EQ, stems and effects are not baked into the recording. Seek backwards
  during recording is rejected rather than exporting an invalid interval.
- Direct: eight buttons from 1/4 through 32 beats trigger loops or rolls. The
  arrows also reach 64 beats. Clicking the selected size exits that loop.

Options are per deck. Smart Loop snaps manually chosen bounds to the analysed
beat grid and corrects length to a whole number of beats. Loop Back anchors an
auto loop's end at the current position. Quantize snaps auto/manual placement
to the nearest beat (fixed one-beat resolution). Without a grid, manual loops
remain available and automatic quantization is unavailable.

Loop Roll advances a separate background position at the current tempo while
playing and freezes it while paused. Exiting the roll returns to that position;
resizing keeps its original background timeline. Rhythm waveforms show the loop
bounds and a gold background-position marker when it is within the visible range.
Seek or load cancels the loop; a new track also clears transient In/reloop points.

Hardware jog-wheel adjustment, a configurable global quantize resolution and
live post-mixer recording are not implemented yet.
