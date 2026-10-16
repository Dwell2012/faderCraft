# Playlists and Automix

Playlists already support creation, renaming, folders, track ordering, smart rules,
and M3U/VirtualDJ/rekordbox import/export as described in FEATURES.md. Library
tracks can now be dropped onto a normal playlist row. Smart playlists still use
rules rather than manual membership.

Open a playlist and use **Queue all** to append the displayed tracks in their
current order, or right-click a track and choose **Add to Automix**. A snapshot is
queued; later playlist edits do not silently change it. Tracks can also be dropped
onto the Automix panel beside the performance browser. Use the arrows/remove
buttons to edit the waiting queue. **Save queue as playlist** saves the upcoming
selection to a new timestamped library playlist, which can be renamed normally.
Library playlists deduplicate repeated tracks; the live automix queue allows them.

**Resume** loads the next playlist item onto the non-playing deck, preserving the
playing deck’s position, tempo, controls and current crossfader position. If both
decks play, it waits until a deck is paused; it never chooses one to interrupt.
With neither playing, it starts the queue on A. Automix owns both decks and the
crossfader while enabled. It preloads the next track, starts it near the current
track's end, crossfades, then pauses/reuses the outgoing deck. The fade defaults to
8 seconds and adjusts from 0.1 to 30 seconds using buttons or the mousewheel.
Short incoming tracks shorten the fade without starting it prematurely.

The incoming deck resets tempo to unity, volume to unity, trim/EQ to neutral and
FX/loops off; loaded stems reset normally. The current playing deck is otherwise
preserved on adoption, including its loop. Master gain remains unchanged.
Transitions retain your crossfader curve and begin at its current position and execute inside the audio callback.
A 50 ms control timer advances loading/the queue independently of animation frames.
**Beat + EQ** is now the default mode. Switch to **Timed fade** before starting
for the original crossfader-only behaviour. Beat + EQ takes ownership of Classic
EQ on the newly prepared deck (leaving its stem EQ mode), hands over highs first, then mids, and
swaps the bass around the transition midpoint. Band envelopes are complementary,
never boost above their starting gains, and are smoothed by the mixer EQ. The
visible knobs follow the audio-engine targets. Completed transitions restore
neutral EQ; Stop/manual takeover preserves the current EQ blend.

Queued tracks carry analysed beat grids from the library. The incoming tempo
matches the outgoing deck's effective BPM, with a ±8% limit relative to the
incoming original tempo. The audio callback aligns the beat/bar phase against the
outgoing position at transition start. The incoming start can advance a few beats
past its first grid beat to achieve this. Transition targets use whole four-beat
bars when room permits, bounded by the available audio and selected fade length.
Phrase-aware planning now uses saved section markers when possible; vocal activity estimates from prepared stems are used when available.

Matching requires two valid grids with confidence at least 0.3. Missing/uncertain
grids, distant tempos and insufficient audio fall back to the staged EQ fade;
the status explicitly reports this. Analyse tracks before queueing them for best
results. Matching uses varispeed and disables incoming key lock: pitch follows
tempo. The matched speed remains after handover, avoiding a sudden speed reset.
Pitch-preserving matched transitions and silence trimming remain future work.
Gated-RMS automatic trim matching is now available (see PREPARATION.md).

**Next** transitions as soon as the next track is ready. **Take over** releases control
without stopping the music; during a fade both decks remain playing at the current
blend for manual takeover. Manual deck controls or crossfader/curve changes also
stop automation. Master and sampler controls remain usable without cancelling it.
**Clear queue** clears waiting/preloaded tracks; a transition already in progress
finishes. Unreadable files are skipped with status text; the queue ends after the
last playable track. If decoding is too slow, a gap is possible.

The remaining queue saves to `automix-queue.json` in app data and restores without
autoplay. The current playing track is not replayed on restart. Fade settings are
session-only. No music files are modified. Multiple named automix queues, shuffle,
repeat and dedicated automix history remain future work.

Engine tests cover roles, queue order, adoption, audio-thread fades, queue
completion, missing files, short tracks, clear, effective-tempo matching, grid
phase, staged EQ gains, fallback and manual takeover. Visual and
listening verification is pending while the Windows desktop is locked.

Taking over retains the queue cursor. A preloaded but unstarted track returns to
the front; a track already started in a transition is considered played. Resume
therefore continues with the next unstarted item, including after a manual track
insertion. Pausing a deck while Resume is waiting allows loading to proceed.

Per-track MIX IN/MIX OUT, bar-length overrides and bass-swap timing are now
available below the overview waveform. See [PREPARATION.md](PREPARATION.md).

## Phrase-aware transitions

Beat + EQ previews the next transition once the incoming track is ready. The
preview shows OUT/IN source times, duration in bars/seconds and the bass handover
time. With compatible beat grids and section markers on both tracks, the planner
prefers a late Outro and an incoming Intro, otherwise eligible Drop/Section
boundaries. It tries 32, 16, then 8 bars, using the longest that fits both tracks;
a per-track 8/16/32 setting overrides that choice. It avoids carrying the outgoing
track into a new Build or Breakdown during the fade.

Saved MIX IN/OUT override automatic selection. An off-grid manual entry is kept
exactly rather than moved to force phrase alignment. If no whole phrase fits, a
point has already passed, grids are missing/incompatible, or section markers are
missing, the existing timed/beat+EQ fade remains available and the preview explains
the fallback. Next explicitly requests an immediate transition.

The default bass swap prefers a section boundary inside the fade near its centre.
When there is none, it uses a four-bar subphrase boundary near the centre (4 bars
into an 8-bar transition, for example). An explicitly saved bass percentage wins.
Section labels remain energy-based estimates; vocal activity adds the overlap scoring described below.

Plans refresh after preparation/grid changes until committed. The transition is
armed about 200 ms before its boundary and then runs in the audio callback. Starts
have at most one internal audio-block delay (512 samples); the incoming position
and fade progress compensate that delay to retain alignment. A committed fade
keeps its plan. Manual takeover cancels even an armed transition: an unplayed
incoming item returns to the queue, while a track already started counts as played.

Tests cover marker preference, manual overrides, short/missing/incompatible
sections, preview timing, audio-thread starts without UI polling, bass timing,
and cancelling an armed transition without consuming the next queue item.

## Vocal-aware planning

Prepare stems for both tracks (the toolbar Stems button prepares the loaded decks).
Previously cached stems load automatically. A background worker measures the
separated vocal source in 100 ms stereo RMS windows, filters brief/quiet bleed,
bridges small gaps and pads detected passages. Purple strips below the phrase
sections show estimated vocal activity. This is not lyric or speech recognition;
separation bleed and very quiet vocals can produce false positives/negatives.

When both maps are available, Beat + EQ compares feasible phrase transitions by
predicted seconds of overlapping vocal activity, accounting for playback speeds.
A candidate must reduce overlap by more than 0.25 seconds to replace the current
choice; ties retain the existing outro/intro and longer-transition preferences.
Automatic incoming entries stay within the first 35% of the track. It can choose
a different eligible section or shorter 8/16/32-bar fade, but fixed MIX IN/OUT and
explicit transition lengths remain constraints. Clear IN/OUT to free automatic
selection (saved suggested points from earlier versions also act as fixed points).

The preview reports no significant predicted overlap, the estimated clash duration,
or missing vocal maps. When markers or available sections leave no clean option,
it reports the clash and continues the planned mix. It does not mute, duck or
rewrite vocal stems. Timed-fade mode reports overlap without changing its timing.
Late analysis refreshes an uncommitted plan; an armed/running transition stays
stable. Vocal maps are derived from the validated stem cache on each load, not
from filenames or the stereo master, and do not modify music files. No automatic
separation job is started simply by enabling automix.

Preloaded decks now receive automatic trim matching once their background
loudness measurement is ready. The transition keeps that trim; it does not reset
it to unity. See [PREPARATION.md](PREPARATION.md) for headroom and manual overrides.
