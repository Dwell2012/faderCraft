# Waveform attack emphasis

Short low-frequency attacks receive a near-white deck-tinted highlight inside the ordinary peak
silhouette. They do not become full-height overlay lines or a separate lane in
the scrolling waveform. Each deck's whole-track overview reserves four pixels
at its lower edge for tiny two-pixel attack ticks beneath the waveform body.
Strength controls opacity, and low-EQ gain affects the emphasis. Stem displays
retain their existing rendering.

The cues are derived from cached low-band RMS envelopes, independently of BPM,
beat phase or the beat grid. They do not invent a hit at every grid beat or
change audio playback, analysis grids, cue positions or cache formats.

Detection compares a 20 ms energy average with an earlier 80 ms baseline,
leaving a 20 ms gap. It requires both an absolute/relative rise and 25% contrast,
finds local maxima, backs up to the attack edge, and suppresses competing hits
within 140 ms. Display colour occupies a fixed approximately 45 ms source interval instead of being
spread by the waveform-body smoothing filter. A little context outside the
visible interval keeps viewport edges from creating attacks.

These are bass-attack cues, not isolated kick stems. Sharp bass notes and other
low-frequency transients can qualify; masked or quiet kicks can be missed.
Strength is display salience, not a calibrated kick probability. A synthetic
regression checks repeated kicks stopping while a sustained bass tone remains;
other checks reject silence, nonfinite levels, constant energy and slow ramps.
Display tests preserve attack timing through smoothing and exercise the full
sample-to-display path across a break.

Eyeball around 2:22 was checked with the read-only `bass_attacks` example at
48 kHz. The surrounding mix still contains bass attacks, so those markers
must not be claimed as proof that every marked sound is a kick. Hardware/user
visual confirmation of the contrast in that passage remains useful.

```powershell
cargo run -p djcraft-analysis --example bass_attacks --locked -- "<audio file>" 138 147
```

All work runs on the display/control side or in the offline diagnostic, with
the existing overview cache reused while its playhead moves. No allocation,
locking or analysis is added to the audio callback.

Attack accents retain the deck hue at 8% of its original saturation and 94% lightness. The same tint is used for the waveform and overview ticks, replacing the earlier orange accent.

## Smooth scrolling
Attack analysis is now performed once per loaded waveform on a background thread. The immutable result is shared by the scrolling and overview views, keeping strengths stable as the viewport moves. Rendering uses binary searches into the event list rather than rescanning it for every pixel. Scrolling columns are cached with extra margins, so fractional movement and small position changes reuse the same column allocation; zoom, source availability, track changes or completed attack analysis invalidate the relevant data. The overview refreshes once when attack analysis finishes. Display interpolation remains tied to the existing audio-following visual clock.

## Persistent events
Background track analysis now creates a versioned .bass-attacks-v1 sidecar beside the content-identified waveform cache. Each record stores source seconds and strength, so playback at a different sample rate reuses the same analysis. Loading an older track backfills a missing entry on its background worker; future loads read the file. Invalid versions, malformed/nonfinite/unsorted data and duration mismatches are discarded and rebuilt. Publication uses a temporary file and rename. The display no longer fades the attack tail: each event retains its cached strength while it scrolls. The analyse_attacks example can precompute selected tracks without changing library keys, beat grids or tags.

Scrolling and overview waveforms now cache normalized triangle meshes alongside their display columns. Playback translates cached geometry under the playhead; it no longer rebuilds and tessellates envelope polygons each frame. EQ changes rebuild the affected mesh, while zoom, track/source changes and completed analysis invalidate the relevant column tile. GPUI still submits a painted frame during animation. A geometry regression test covers gaps and placement without changing the source shape. Two-deck smoothness still needs live confirmation.

Overview rendering uses fixed unity gains, independently of deck EQ, and disables in-waveform attack accents; attack ticks remain below the overview and retain their original strength. The existing 50ms engine-state task now bootstraps visual animation when playback/loading becomes active before the first repaint observes it. It only wakes stalled frames, leaving normal animation-frame pacing in charge.

Deck overviews fill their existing waveform container with square corners. The original track header and adjacent elapsed/remaining timing layout are retained.

Removed the overview-to-timing horizontal gap and vertical header/timing separators. The track header uses the same grey background as the overview; BPM and elapsed/remaining controls retain their original positions.

The grey track identity section and overview share the same flexible width. BPM/key and elapsed/remaining each occupy the same fixed 146px right column, outside that background.

## RGB option
The performance header's RGB toggle saves a machine preference. It selects a blended RGB waveform from red low, green mid and blue high amplitudes for every visible deck's scrolling waveform and overview (two or four decks). Off restores deck palettes. Existing 250 Hz / 2.5 kHz band splits, EQ response, cached geometry and attack cues are reused; stems keep their source palette. This uses faderCraft's layered envelope renderer, not a replica of another application's blending algorithm. No reanalysis or audio changes are required.


RGB colours now combine post-EQ band amplitudes per segment: bass plus mids makes yellow, bass plus highs magenta/pink, mids plus highs cyan, and balanced bands white. Hue is normalized independently of signal height; the peak silhouette retains amplitude. Cached geometry groups quantized colours and rebuilds on source/EQ changes. RGB mode omits the deck-style attack wash so it cannot obscure frequency colours; overview attack ticks remain. This is faderCraft's own frequency blend, not an exact reproduction of proprietary rendering.

RGB saturation uses a 1.2 power curve on normalized band amplitudes, reducing weak-channel wash while retaining full-brightness dominant channels and balanced yellow/magenta/cyan mixtures. Height and EQ response are unchanged.

RGB brightness is softened with 85% paint opacity; the gentler 1.2 contrast curve retains more intermediate colour after visual feedback that the previous saturated reds/greens looked harsh.
