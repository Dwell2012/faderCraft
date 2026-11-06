# Pro performance pads

**Keyboard** turns a saved hot cue into eight pitch pads. Choose Keyboard and
press a pad with a saved cue to select it. Then play pads 1–8 for the scale from
the cue's original pitch to one octave up. Shift + pad or right-click chooses
another saved cue. The DDJ-GRV6 opens Keyboard with Shift + Hot Cue.

The intervals follow the track's detected mode when available, or its major
or natural minor key. Use a cue on the tonic for scale notes that align with
the track key. A note retriggers playback and holds that pitch until another
note or key shift changes it. Pad 1 restores the original pitch.

Reference: https://virtualdj.com/manuals/virtualdj/interface/decks/decksadvanced/pads.html

Each deck has eight pads in two rows of four. The page dropdown selects the mode;
PADS toggles a favourites strip. Scroll the page header to cycle pages. Parameters
have previous/next arrows and wheel adjustment. Pages with no assigned parameter
show status text rather than inert controls.

The page menu can split the top and bottom rows between two pages. Each row then
uses that page's first four pads, and each parameter lane controls its own page.
Select Pads 16 in the FX view dropdown for pads 1–8 in the FX area and 9–16 in the
normal pad area. Click the upper "Pads 16 · FX" button to restore effects. Split
pages are disabled in this mode. Pages with only eight meaningful actions leave
the additional eight pads disabled.

## Pages

- **Stem FX:** top row selects Vocals, Melody, Bass or Drums. Bottom row toggles effects on that stem; selecting another type replaces that stem's effect. Effect bank arrows expose all 16 Pad FX types in four banks. Wet arrows adjust the selected stem's mix in 5% steps. Effects latch until toggled off, independently on each stem, and restore on restart. Prepare stems first. Source inserts run before the stem mix and deck rack; changing tempo/pitch also transforms their sound. The regular Stems page still controls mute/solo.

- **Hot Cues:** click an empty pad to save; click a saved pad to jump and play.
  Shift-click or right-click deletes unless editing is locked. Setting inside a
  loop stores a loop cue. Quantize on set snaps to the nearest analysed beat.
  Parameters step through cues (preserving paused state) and jump one beat.
  Stutter mode plays an existing cue while held, returning paused to it on release.
- **Loop:** beat-sized toggle loops. Parameter 1 shifts the size bank; parameter 2
  chooses Toggle, Hold or Roll. Sizes span 1/32–128 beats, clamped at range limits.
- **Loop Roll:** hold to loop; release resumes where uninterrupted playback would
  have reached. Shift/right-click uses an alternate size set including 3/4 beat.
- **Slicer:** an eight-step moving beat bank. Hold a slice to repeat it with roll
  recovery on release. Shift/right-click freezes that bank for later reuse; release
  the frozen bank through Options. Parameters adjust slice length and step size.
- **Beat Jump:** paired backwards/forwards jumps. Change bank for larger/smaller
  distances. Jumps use the audio-thread position and exit an existing loop.
- **CueLoop:** shares hot cues. A short click sets/jumps to the cue and leaves an
  In point; holding beyond 250 ms retains the loop. Right-click engages the loop
  directly. Hold mode exits the loop on release. Parameter 1 adjusts length.
- **Saved Loops:** save an active loop (or create one at the selected size) into an
  empty slot. Clicking a saved pad arms it without jumping forwards; Shift/right
  click jumps straight to it. Parameters resize/move the active loop. Options has
  an explicit next-pad delete mode, protected by the edit lock.
- **Manual Loop:** In, Out, Exit, Reloop, half, double and one-beat moves.
- **Stems:** mute/unmute Vocals, Melody, Bass or Drums; Shift/right-click isolates
  one. Additional pads provide Acapella, Instrumental, Reset and Mute All. Requires
  prepared stems; works alongside Classic EQ. Changing EQ modes resets stem gains.
- **Pad FX:** hold for one of 16 effect presets. Uses the first inactive FX slot
  and restores its previous settings on release, including its previous effect
  type (so this page cuts temporary effect tails). It never replaces an active
  slot; a message appears if all six are occupied.

Cue and saved-loop changes are stored under the app data directory, not in music
files. Cue slots are keyed by track path; moving a music file does not migrate
these keys. Existing four-slot saved-loop files load into the new sixteen slots.
Held actions are released on mouse-up, page changes and track replacement, with
track identity checks so an old hold cannot seek a newly loaded track.

The implementation supplies the Pro layout and the twelve pages listed above and below.
StemSwap, Keycue, Remix Points, Scratchbank, DMX,
custom scripting/editor, editable favourites and hardware pad assignments remain
future work. Smart Cue phase-preserving jumps and cue naming/colour editing are
also not implemented. Visual verification is pending while Windows is locked.


## Sampler and automated Scratch

Sampler uses a shared, persistent 16-slot bank. Empty pads capture the active
loop, or 4/8/16/32 beats from the quantized cursor (with no deck loaded they open
the file picker). Options → Capture deck also replaces a filled pad. Capture copies decoded
original audio (not post-EQ/FX audio); source files are never changed. The maximum
capture is 120 seconds. Filled pads support Trigger, Hold and Retrigger modes. One-shot hits overlap (up to 16 voices per slot); each press starts immediately. Trigger toggles looping samples on/off. Right-click stops all hits in a playing slot.
Options enables looping and protects captures/deletion with the edit lock.
Shift/right-click stops a playing slot; on a stopped slot it deletes unless locked.
Pads 16 exposes slots 9–16. Samples survive deck replacement and play independently
of deck faders, through the master gain. Wheel over a sampler pad adjusts sampler
volume, including currently playing samples. Initial level is 70%.
Options → Load from disk arms a one-shot pad selection; click the destination pad
and choose an audio file. Library tracks can also be dropped directly on a pad.
The edit lock protects imports and captures. Names and loading/errors appear on
the pads. Failed imports preserve the previous sound. WAV, MP3, FLAC, Ogg and
other formats supported by the engine are decoded/resampled off the audio thread.
Imports are limited to 64 MB and 120 seconds.

The bank is automatically saved in the app data `Sampler` directory as private
stereo PCM WAV copies and per-slot metadata. Imports and captures survive restart
and changes to the original source path. Deletions persist too. Only one shared
bank is implemented; multiple named banks and per-deck routing remain pending.
The private WAV copies use 16-bit PCM; imported playback initially retains decoder
precision. Do not treat this as an archival audio conversion tool.

First use seeds twelve original MIT-licensed synthesized sounds: Kick, Clap,
Snare, Crash, Air horn, Siren, Closed hat, Open hat, Low tom, Rimshot, Cowbell and
Riser. No third-party sample pack or download is needed. Slots 13–16 start empty.
Options → Restore default restores the selected factory slot (1–12) without
resetting other pads. StemSwap and direct OS file drag/drop are not implemented.

Scratch provides eight native four-beat patterns: Baby, Double, Forward,
Transformer, Crab, Chirp, Fast baby and Slow drag. These are our own trajectories,
not VirtualDJ DNA presets. Parameters select Cut/Over and Quantized/Immediate.
Over blends the scratch with the normal track at half level to provide headroom.
Quantized starts at the next grid beat while playing; without an analysed grid,
patterns use 120 BPM. Click starts/replaces a gesture; right-click cancels it.
Underlying playback continues and resumes at the uninterrupted position. Paused
tracks remain paused. Seek, cue, pause, track replacement and manual scratching
cancel the gesture. Custom DNA editing is not implemented.

Hot Cues: Ctrl-click an occupied pad to jump there and pause (empty slots are unchanged). Right-click an occupied pad to edit its name and colour. Shift-click deletes. Names and custom colours persist per track; automatic suggestions retain their names. Each slot has its own default deck-themed colour, shared by the pad and flags on both waveform views. Main waveform flags show names; small deck overviews use colour-only flags to avoid crowding. Editing respects the cue lock.
