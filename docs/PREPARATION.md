# Phrase sections and mix preparation

Load a track onto either deck. Once decoding completes, a background job uses
bar-level energy changes to suggest section boundaries. Existing saved preparation
is reused and never replaced automatically. Suggestions are heuristic: Intro,
Build, Breakdown, Drop, Outro, or Section when the label is uncertain. This is not
vocal recognition or full musical phrase detection. Analyse the library's beat
grid first for bar-aligned suggestions; without it the analyser uses two-second
windows. Silence produces no suggested sections.

Under each overview waveform:

- Coloured sections show the suggested structure. Click a section to select it;
  use Label to change its type, Remove to delete it, or + Section to insert one
  at the current playhead. These do not modify hot cues.
- Drag a section to move its start boundary (beat-snapped when a grid exists).
  Boundaries cannot cross their neighbours. The wheel moves a boundary by beats,
  or 0.1 seconds without a grid.
- IN here and OUT here set mix markers at the current playhead. Drag the marker
  labels or use the wheel to adjust them. Clear IN/OUT restores default timing.
- MIX IN is the incoming entry point. MIX OUT is the START of the outgoing fade.
- Auto length uses the Automix panel's fade setting. Choose 8/16/32 bars in the
  dropdown, or use its wheel, for a per-track outgoing transition length.
  Without a beat grid this falls back to the global seconds setting. Available
  audio may shorten the transition.
- Bass percentage sets the centre of the low-band handover in Beat + EQ mode;
  wheel to change by 5 percentage points, click to restore 50%. Timed fade ignores it.

Automix reads the preparation for each track, including edits made after it was
queued. Changes during a running transition take effect on subsequent transitions.
Saved MIX IN is kept exactly. Off-grid manual markers fall back from phrase
alignment rather than being moved. Invalid/out-of-range
saved mix points fall back to normal track timing. Automatic suggestions use the
first detected section for MIX IN and a detected Outro for MIX OUT; when there is
no Outro, normal end-of-track timing remains in effect.

Edits are saved in track-preparation.json in app data, keyed by source path.
No source music files are changed. Moving/renaming a source file does not yet
relink its preparation. Analysis never overwrites edits made while its job ran.
Save errors appear in the preparation panel. Completed suggestions and settings
are shared by both decks when they load the same path.

Tests cover section classification, silence, saved edits, boundary limits,
protocol validation/roles, automix entry/exit timing and bass handover envelopes.
Visual and listening verification remain pending on the locked desktop.

Phrase-aware automix now uses these sections to plan whole 8/16/32-bar transitions;
see [AUTOMIX.md](AUTOMIX.md) for selection, preview and fallback behaviour.

## Suggested hot cues

Below the preparation strip, faint green ghost markers and labelled buttons
suggest the first grid beat, first detected vocal entry, first breakdown/build/drop,
and outro/mix-out when that information exists. Vocal entry uses the conservative
padded vocal map, so it can lead the audible word slightly. Suggestions are not
accepted automatically and are separate from the phrase boundaries.

Click one suggestion to put it into the first empty Hot Cue slot, or use Fill
empty slots. Existing hot cues and cue loops are never replaced; candidates within
250 ms of an existing cue are skipped. Full banks and locked pads are respected.
Accepted cues use the existing hot-cues.json persistence. Edit/remove them with
the normal pad controls. Semantic suggestion labels are shown in the suggestion
panel; accepted pads retain the normal numbered cue display.

## Automatic trim matching

Full decoded stereo audio is measured once per loaded track on a background
worker, independently of phrase analysis and stems. This version uses **gated
stereo RMS**, not LUFS: 400 ms blocks with 75% overlap, an absolute -70 dBFS gate
and a relative gate 10 dB below the initial mean. Channels are measured separately
before averaging power so opposite-phase stereo does not cancel.

Automatic trim targets -20 dBFS gated RMS and limits the suggested gain to keep
source sample peaks at or below -6 dBFS where the ±12 dB trim range permits. This
leaves mix headroom; it is not a true-peak meter, limiter or guarantee against
clipping after EQ, effects, multiple sources or master boosts. Some tracks cannot
reach the target within these limits; the panel reports headroom/range limiting.
Silence is not boosted. Measurements are recalculated on load, not written into
music files.

A new load starts with neutral trim and auto matching enabled. Once measured,
a paused deck receives the suggested trim, including preloaded automix decks.
If it is already playing, matching waits for pause; click Match trim to apply it
explicitly during playback. The regular mixer gain smoothing applies. Changing
trim manually turns auto matching off for that load, and polling cannot overwrite
it. Auto trim off leaves the current gain in place. Match trim / Auto trim on
re-enables matching. Master and channel faders are unchanged.

Tests cover cue de-duplication and existing-loop preservation, silence/stereo
measurement, gain headroom, late analysis, manual overrides, source replacement
and maintaining matched trim through an automix transition.
