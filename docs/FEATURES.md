# djcraft feature list

- [~] The first four DDJ-GRV6 Stems pad lamps follow source mute and solo
      state after stem preparation, including on the Shift layer.
      [Physical controller check pending.]

- [x] On Windows, DJCraft keeps the display on and prevents automatic idle
      sleep while the app is open. Closing the app releases both requests.

- [x] Deck BPM follows the library tempo preference and shows a file tag when
      analysis produces no reliable BPM. Direct file drops also read tagged BPM.

- [~] DDJ-GRV6 dedicated loop buttons set In/Out, start a four-beat loop from the initial IN press,
      exit or recall the last loop, and halve or double an active loop.
      [Loop edge adjustment, cue/loop memory controls and hardware verification pending.]

- [~] DDJ-GRV6 Master Cue adds the master feed to the headphone cue bus and preserves
      channel cues and headphone mix/level. [Physical controller check pending.]

- [~] Keyboard pads select a saved hot cue and retrigger it at eight ascending
      scale pitches. The pad page and DDJ-GRV6 Shift + Hot Cue use the track's
      detected mode when available, or its major/natural minor key.
      [Physical controller verification and Keyboard pad lamps pending.]

- [x] Optional ROCm PyTorch preparation runs Fast and Detailed Demucs on an AMD
      GPU. Library and deck preparation share the existing progress and deck
      attachment workflow. ONNX CPU remains available with separate caches.

- [~] DDJ-GRV6 Beat FX uses the selected deck's first FX slot or a shared
      processor for Master and Sampler. Selector, on/off, 14-bit depth, beat
      steps and LED feedback are mapped.
      Sampler pads trigger or stop the selected bank's 16 slots.
      Stems and Beat Jump pads mirror the app's existing pad pages.
      [Helix uses Beat Repeat, Mobius positions bypass, startup dial position is unavailable through the tested requests.]

- [x] Measured jog scratching eases into each incoming position so brief MIDI
      delivery gaps during steady motion cause fewer audible dropouts. Large
      backlogs still use a bounded target, and the wheel settles when input stops.
      Dense MIDI packets share a short velocity measurement window so packets
      with identical timestamps still contribute their full travel.

- [~] DDJ-GRV6 jog platter, rim and touch inputs share one wheel state per deck.
      Play, Cue, headphone Cue, Load, Slip, Vinyl, Quantize and hot-cue lamps use
      documented MIDI output. Jog-ring animation follows playback position at
      33⅓ rpm at unity tempo. [Physical sensitivity and lamp checks pending.]

- [x] DDJ-GRV6 Sound Color FX knobs control the four deck CFX amounts via
      14-bit MIDI. The ON/OFF button and lamp control global CFX bypass
      while preserving each channel's selection and knob amount.

- [x] Library analysis progress remains visible in Library and Perform. Deck loads promote pending tracks and pause other workers between DSP stages, then resume. Playlist imports notify the analysis owner; Check pending retries an idle/error pass.


- [x] Controller LED feedback batches after input actions: rapid hot-cue edges
      refresh the affected lamp once per drain instead of forcing the full LED
      bank for every press/release. Regression coverage includes a 32-hit burst
      reaching the final cue on the next audio callback without late retriggers.

- [~] Rhythm Shift replaces a prepared deck drum stem with a beat-synced groove, triggers four rolls at bar boundaries, and returns to the groove at each roll's pack-defined point while the tail plays. The Perform side panel and fcScript actions expose it without hardware; the DDJ-GRV6 starter profile maps Swap 1, Roll 1–4, Release and gain. [Additional groove slots, Capture and the GRV6 Stem FX mode switch remain pending.]

- [~] fcScript interaction conversion: mixer controls, play, eject, key-lock,
      tempo-range buttons, manual tempo gestures and Rhythm Shift use compiled
      commands. Controller profiles support note and absolute-value script bindings.
      [Other deck controls, relative encoder scripts, feedback expressions and a script editor remain pending.]

- [x] Stem preparation progress overlays the top edge of each deck overview waveform: 3px, deck colour at 50% opacity, no added layout height. Uses existing per-deck progress and refresh; queued/loading jobs show a short marker, and completion/failure/timeout clears it.

- [x] Right-deck performance boxes mirror the left-deck arrangement: Effects/Pads outside, Loops inside, label strips on the right. Controls inside each box retain their original order.

- [x] Serial FX order: deck rack → Pink/White Noise generators → Bitcrusher → Flanger → Reverb → Echo → Filter. CFX processes in reverse menu order, with Filter affecting all preceding audio and tails.

- [x] Deck FX names show a small muted, baseline-aligned beat duration (for example Echo (1/2 beat)) when beat sync and a track beat grid are available; free-running effects omit it.

- [x] Tempo-neutral gain fix: auto-trim remains based on source loudness across tempo/key-lock changes. Processed peak reports no longer lower track trim; the existing audio-side peak guard handles timestretch overshoots.

- [x] Main mixer knobs use antialiased SVG rings, arcs and pointers in two- and four-deck layouts, preserving each control’s neutral position and existing interactions.

- [x] Stopped decks scrub audibly from the jog rim with Scratch mode on or off; playing rim behaviour remains pitch bend. Mini FX knobs use antialiased SVG arcs and pointers matching the EQ/CFX design.

- [x] Deck FX 1 controls grey out and stop accepting edits when that deck's controller FX 1 is routed to CFX. FX dials use the shared smooth EQ/CFX renderer at miniature size.

- [x] With FX 1 routed to CFX, its pad centres the shared knob and kills all CFX/noise tails while retaining selections; the encoder engages the combination again on movement.

- [x] CFX selector is a persistent checkbox popout: combine effects without reopening; click outside to close. Mixer CFX text and spaced arrow share a vertically centred header.

- [x] Separate per-deck Colour FX chain: Filter/Echo/Reverb/Flanger/Bitcrusher plus stereo White/Pink Noise, multi-select from the mixer CFX arrow and one shared centre-neutral knob. Controller Settings route FX 1 button/encoder/LED to Deck Effects or selected CFX. Selections persist; noise starts neutral after launch.

- [x] Compact FX x3 layout uses three rows with click-to-toggle highlighted effect names, selector arrows, amount dials and detail access; Effects box aligns with the pads below, with beat timing in the cog popout. Other FX layouts remain available.

- [x] Deck FX beat timing lives in the Effects options cog popout; the compact box header has no beat selector.

- [x] Controller Hot Cue/Loop/Sampler pad presses select the matching on-screen page per deck, including empty decks. [Mode switch alone remains undetectable on Mixtrack; first pad press establishes mode.]

- [x] One unambiguous controller matching the loaded mapping connects automatically at startup/reconnect; splash shows the loaded mapping and actual connection status. Multiple matching devices retain the chooser prompt.

- [x] Stem FX pad page: select Vocals/Melody/Bass/Drums, toggle an independent source-insert effect per stem from four banks, adjust wet amount, persist assignments; requires prepared stems.

- [x] Mixtrack pitch-zero lamps follow each deck's neutral tempo; physical centre remains exact 0% even when the opposite-deck BPM match rounds to that position.

- [x] Per-deck hot-cue Quantize on set and its 5/10/20/30% beat window persist with deck setup.

- [x] Machine-local per-deck Scratch on/off and loaded effect slots (parameters, enabled state and shared beat timing) restore on startup; controller Scratch mode/LEDs survive reconnection.

- [x] Playing vinyl jogs retain fast forward/backward spins, then recover to playback speed once released motion slows to playback speed or below; residual wheel packets are ignored until idle or a fresh touch. Paused decks retain free spins.

- [x] Settings > Mixer crossfader controls: Slow–Fast adjustment, Linear/Power response, Reverse, Disable and per-deck side assignments; applies live and persists on this computer.

- [x] Select downloadable Fast or Detailed (fine-tuned) stem models in Audio settings, with model-specific caches and persistent GPU/CPU processing.

- [x] Preload a persistent stem model at startup with a splash screen; reuse the worker and GPU weights across tracks, with CPU override/fallback and worker recovery.

djcraft is an open-source DJ application: a music library manager, a live mixing
engine at rekordbox Performance-mode parity, in-app radio for listening to other
DJs' live mixes, and remote collaboration.

Status key: `[ ]` to do, `[~]` in progress, `[x]` done.

For this custom build's agreed requirements and proposed delivery order, see
[PRODUCT-PLAN.md](PRODUCT-PLAN.md). The broader upstream roadmap below is not
the committed scope of the custom build.

## Project decisions

- **Licence:** MIT. No GPL code is linked into the app.
- **Stack:** Rust, with GPUI for the desktop UI (via Longbridge's `gpui-kit`).
- **Plugins:** JavaScript/TypeScript on an embedded QuickJS runtime, sandboxed
  with per-plugin permissions.
- **Interface:** inspired by djay Pro's layout and interaction style.
- **Infrastructure:** bring your own. Sync, streaming and directories run on
  storage and servers the user chooses. No mandatory djcraft-operated service.
- **Architecture:** the audio engine and session logic are UI-free crates. Every
  user action is a message in a documented control protocol, and the desktop UI
  uses the same protocol as remote clients.

## Milestone 0: foundations

- [x] Cargo workspace with separate crates: `engine`, `library`, `protocol`, `app`
- [x] Licence allow-list enforced in CI with `cargo-deny`
- [x] Spike: GPUI window drawing a scrolling waveform while the engine plays a
      time-stretched track
- [~] Confirm `gpui-kit` builds and runs on macOS, Windows and Linux (builds
      and passes the tests on all three in CI; the app has only been run on
      macOS)
- [x] Real-time-safe engine design: no allocation, locking or UI calls on the
      audio thread; lock-free queues between engine and UI
- [x] Deck playback that follows an external speed and position signal,
      including reverse and near-zero speed (needed for jog-wheel scratching
      and DVS)
- [x] Control protocol v1: action messages, state broadcast, roles
      (host, co-DJ, viewer)

## Milestone 1: music library management

### Sources

- [x] Manual Refresh in the Library and performance browser rescans all added
      music folders in the background, with scan progress and analysis of new files.

- [x] Local files and folders, with watch-for-changes [Windows integration
      tests cover additions/removals; stopped or replaced watchers discard
      late notifications from their previous registration]
- [ ] Network shares
- [ ] External sources as optional plugins (YouTube and similar through a
      user-installed `yt-dlp`); see Plugins

### Analysis

- [~] BPM and beat grid detection (constant tempo and four beats to the bar
      only; kick phase uses uncompressed bass strength to reject quieter
      offbeat notes, with attack timing refined from spectral flux. Repeated
      descending-pitch kicks anchor on the first confirmed run, refine locally and project the
      grid backwards, avoiding an offbeat bass-intro lock. Covered by
      synthetic regression tests and checked against Castles In The Sky and ATB 9PM;
      broader real-track accuracy is unmeasured; manual grid editing is missing)
- [~] Key detection (one key per track, major or minor; tested on
      synthetic music only, so accuracy on real tracks is not measured yet)
- [x] Smooth deck waveform scrolling between audio-buffer updates, with
      audio-clock correction and immediate seek positioning
- [x] Drag the main waveform with audible vinyl-style forward/reverse
      scrubbing; click or drag the overview to seek. Release restores the
      previous playback state; holding the main waveform still stops its audio.
- [x] Waveform generation (overview and detail)
- [x] Analysis cache stored alongside the library database
- [x] Tempo range limits (minimum and maximum BPM) and precision setting
- [x] Option to keep an existing tempo or key instead of overwriting it

### Harmonic mixing and energy

Feature set modelled on Mixed In Key 10.

- [~] Camelot wheel notation, with a choice of notation (Camelot, musical,
      Open Key) and optional zero-padding (`08A`) (keys in tags are read in
      any of the three and shown as Camelot, as are detected keys; no choice
      of notation yet)
- [ ] Key segments: key changes within a track, each with a confidence value
- [ ] Tuning detection for tracks not pitched to A440
- [~] Energy level per track on a 1–10 scale (from loudness, so loudly
      mastered tracks crowd the top of the scale)
- [~] Energy segments along the track, drawn on the waveform (each cue point
      carries its section's energy; shown on Flow clips, not on deck
      waveforms)
- [ ] Danceability score
- [~] Automatic cue points (up to 8) placed at energy and structure changes
      (detected and stored; shown and editable in Flow's player; no export
      to other DJ software yet)
- [ ] Cue point export to rekordbox, Serato and Traktor, with an option not to
      overwrite existing cues
- [ ] Audio quality warnings: clipping, low volume, low-bitrate source
- [ ] On-screen piano to check a detected key by ear, playable from MIDI
- [~] Sort and filter by compatible key and by energy for set building (the
      Camelot wheel in the Library's side panel narrows the list to a key
      and the keys that mix with it; nothing by energy yet)

### Tags

- [x] Read and write tags across MP3, FLAC, AAC, WAV, OGG (also Opus and
      AIFF; title, artist, album, genre, BPM, key, comment and grouping are
      tested in each, apart from raw .aac files)
- [~] Audio fingerprint lookup (Chromaprint/AcoustID) (fingerprints are made
      by a pure-Rust Chromaprint port and sent to AcoustID with the user's own
      key; a successful match is untested, as no key was at hand)
- [~] Metadata match from MusicBrainz and Discogs (MusicBrainz, the iTunes
      Store and Discogs, searched from a popup that tidies untidy tags, ranks
      the results and prefers original releases; Discogs needs the user's own
      token and is untested against the live service)
- [x] Review-and-diff screen before any tag is written
- [x] Online lookup detail panel: selected MusicBrainz, iTunes and Discogs results
      fetch track/release dates, duration, credits, identifiers, label/catalogue
      information and release track lists where supplied, with a provider link.
      AcoustID matches can fetch MusicBrainz details. Discogs tag-copy requires a
      track-list match; detailed fields are for inspection, not automatic writes.
- [x] One-time online lookup saved to the library: accepted metadata and provider
      details persist offline and survive rescans. Local search includes saved
      credits, label/catalogue numbers, identifiers and dates. Metadata opens the
      saved snapshot without another request; Search explicitly refreshes it.
      Save to library never writes audio-file tags.
- [x] Download MusicBrainz release front covers through Cover Art Archive, cache
      them with the library and display offline without modifying music tags.
      iTunes/Discogs image downloads remain pending.
- [~] Portable library folder selectable in Settings, including OneDrive:
      asset/cue/stem storage plus consistent SQLite snapshot handoff between
      computers. Local working database and device settings remain local.
      One computer at a time, close and finish syncing before switching; no
      concurrent merge support. Registered tracks retain cue/preparation identity
      after relinking files from different music locations.
- [x] Reviewed recording date/year edits with undo; omitted dates preserve existing values (programmatic tag edits).
- [x] Batch tag alignment with undo (writes the analysed BPM, key and energy
      into the tags of every track in the list; any batch of tag changes can
      be undone)
- [x] Choose where key and energy are written: key field, comment, grouping,
      or as a prefix/suffix on title or artist
- [x] Tag clean-up tools: find and replace across the library, remove extra
      spaces, fix capitalisation
- [x] Rename files from tags using a format template (files stay in their
      folder)

### Flow (set planning)

- [~] Timeline editor with four lanes, matching the decks: place tracks,
      move them between lanes, and trim either end (built; planning only, no
      audio)
- [x] Trim snaps to the track's bars; a moved clip snaps to other clips' edges
- [~] Transition notes where one track comes in over another: tempo change
      and whether the keys mix
- [~] Add tracks by double-click (appended with a typical overlap) or by
      dragging onto a lane
- [ ] Export a flow to a playlist, or load it onto the decks in order
- [~] Player in the editor for one track at a time: whole-track waveform,
      cue point flags, jump between cues, add and delete cues (it plays
      through its own output, not the decks' headphone cue)
- [x] Opens on a new flow; saved flows are picked from a dropdown
- [ ] Preview transitions in the editor (needs sync and crossfade)
- [~] Waveforms and cue points on clips (cue points and energy are drawn,
      and trims snap to cue points; no waveforms)
- [x] Delete or Backspace removes the selected clip
- [~] Stretch a clip to another tempo by dragging its edge, with a Stretch
      toggle (Shift+M) and Alt for the other mode; snaps to whole BPM and to
      the tempo of overlapping clips (planning only: the key is assumed
      locked, and nothing is heard)

### Organisation

- [x] Track browser with virtualised table, sorting, column chooser, search
- [~] Playlists and folders (fixed playlist order with repeat entries, folders
      inside folders, rename, move, reorder by menu or drag and drop; sidebar
      reordering pending). Former playlist sublists are flattened on upgrade.
- [x] Crates and nested sub-crates in the Perform browser. Each crate contains
      a track at most once; its table can be sorted by any column.
- [x] Smart playlists (rule-based) (title, artist, album, genre, BPM,
      energy, year, key or keys that mix, and date added; every rule or any)
- [x] Auto-classification by genre, energy and era (automatic groups in the
      Library sidebar, from tags and analysis)
- [x] Key/BPM compatibility hints for the next track ("Suggest next" in the
      browser under the decks, for the track loaded last)
- [x] Duplicate detection (same artist and title with about the same length,
      or the same file size; copies leave the library, files stay on disk)
- [x] Missing-file relocation (missing tracks are kept and marked; files
      that move within a folder are followed by themselves)

### Interoperability

- [~] Import rekordbox XML (playlists and their tracks; cues, grids, BPM and
      key in the file are not read yet)
- [ ] Import an existing rekordbox library database
- [~] Export rekordbox XML (one playlist at a time, with BPM and key; no
      cue points or beat grids)
- [x] Export M3U playlists
- [ ] Export to USB for Pioneer players
- [~] Import VirtualDJ playlists (`.vdjfolder` and `.m3u`; written from the
      documented format, not yet tried on a real VirtualDJ folder)
- [x] Import M3U playlists
- [ ] Serato and Traktor metadata import (later)

### Sync

- [ ] Library and playlist sync through user-chosen storage (Dropbox, Syncthing,
      S3, WebDAV)
- [ ] Shared playlists using conflict-free merging, no server required
- [ ] No device limit

## Milestone 2: two-deck live mix

### Decks

- [x] Persistent RGB toggle in the performance window header: red bass, green mids,
      blue highs blended additively into yellow, pink/magenta, cyan and white
      for scrolling waveforms and deck overviews; off restores deck colours.
      Stem overlays retain their source colours.
- [x] Two decks with scrolling and overview waveforms
- [x] Deck-coloured waveforms: dark/primary/light low/mid/high overlays in each
      deck's lime/sapphire/amber/violet palette, including track overviews.
      Per-deck Wave / Stems switches the same scrolling row to translucent
      vocal/instrument/bass/drum overlays when prepared stems are available.
      Two rows in two-deck mode and four rows in four-deck mode; no extra lanes.
      Classic EQ scales each frequency overlay live in deck waveforms and
      overviews; killed bands disappear. Stem overlays follow the actual
      stem gains from EQ controls or pad mutes, hiding muted sources.
      Stem envelopes are prepared off the audio thread from real separated audio.
      Frequency envelopes use three independently smoothed translucent overlays
      and cached stereo band analysis (250 Hz/2.5 kHz splits)
- [~] Play/pause, cue, jump to start (play and pause only)
- [~] Tempo fader with selectable ranges (slower and faster buttons in 1%
      steps over one fixed range; no fader)
- [x] Master tempo (key lock)
- [~] Per-deck Sync matches effective BPM; with both decks playing it
      smoothly corrects to the nearest beat (one-shot sync; quantize pending)
- [ ] Hot cues (8 per deck)
- [ ] Memory cues
- [x] Manual loop in/out, auto beat loop, loop halve/double
- [ ] Beat jump

- [x] Click the tempo percentage to ease back to 0% over two seconds;
      manual tempo changes or Sync cancel the return
- [x] Deck header shows analysed BPM multiplied by the current tempo ratio
      (updates with speed/pitch controls; unknown BPM displays a dash)

- [x] Mousewheel adjusts mixer knobs/faders/crossfader and deck tempo/BPM;
      main waveforms scrub audibly when paused and nudge by 5 ms per wheel
      step while playing without stopping. Overviews and preview waveforms seek.
      Shift gives 10x finer steps. Flow zoom buttons accept the wheel;
      Ctrl+wheel zooms its timeline while ordinary scrolling remains available.

### Mixer

- [x] Centre mixer: trim (+/-12 dB), channel and master faders, crossfader
      with Smooth, Equal Power and Cut curves; smoothed audio gain changes.
      Vertical faders expand with panel height; master meters span the knob area
- [x] Four-deck mixer uses C/A/B/D channel order, full-width MIXER and OUTPUT tabs,
      grey control groups with 4px gaps, and horizontal master meters in the header.
      Master, sampler, headphones, and mic controls live on OUTPUT.
      The four channel meters sit together in the same order at the centre; each deck has one
      colour-coded group with trim, EQ, Colour FX, then a compact volume knob.
      Their meters use the same spaced clip light as the two-deck view; each
      crossfader side has a 2×2 grid of square 29px deck-routing buttons in
      A/B then C/D order. Channel groups use colour accents without deck letters.
      Four-deck control groups now end beneath the volume knob, with the
      crossfader row following after a 4px gap instead of filling spare height.
      Four-deck labels briefly switch to values during adjustment, then return
      after two seconds, matching the two-deck control-label behavior.
- [x] Two-deck mixer EQ and trim/volume columns carry the same thin deck-colour
      top accents as the four-deck control groups.
- [x] Four-deck track headers reuse the two-deck title/artwork, BPM/key, and
      elapsed/remaining layout, with a 29px overview/timing strip and 4px gaps.
      The lower four-deck controls no longer repeat BPM, key, or remaining time;
      compact load/unload, cue, and sync controls sit beside play and deck/cue
      selection, with a pitch knob nearest the mixer; the Stems button is absent
      from this layout.
      In four-deck mode the header is 40px high with title and artist on one
      line; key controls move beside elapsed/remaining times, without beat marks.
      B and D mirror the key/timing order across the mixer.
      Crossfader routing buttons follow A/B/C/D order; deck slots retain
      their original A/C and B/D positions.
      Four-deck Pads, Effects, and Loops now occupy the space beside transport
      controls, with vertical tabs mirrored at the outer edge of each deck.
      Selecting a tab shows that section; selecting it again opens its page
      menu. Effects opens by default, followed by Pads and Loops toward the
      mixer. Each vertical tab has its own settings cog, and the tabs and open
      section align with the 96px transport controls. The four-deck Effects
      section no longer repeats a header. The former cross-deck tools overlay
      is removed.
      Four-deck Effects uses three 29px rows with 4px gaps. All three tool
      sections have 4px padding at the label end only. A scaled two-deck platter sits between
      the tool sections and transport controls with a 4px horizontal gap.
- [x] Three-band complementary EQ (250 Hz / 2.5 kHz crossovers, neutral
      centre, band kill to +6 dB); drag knobs vertically, double-click to reset
- [ ] Channel filter
- [~] Independent per-deck headphone cue toggles in both two- and four-deck views,
      supporting multiple decks or all off; activation/keyboard focus is separate.
      Cue is post-EQ/effects and before channel,
      crossfader and master faders. Settings select a separate stereo pair on the
      same ASIO/shared multichannel device. Cue/master blend exists in the protocol;
      right-side mixer knobs provide headphone volume and cue/master blend
      (both sources at full gain at the midpoint)
      (hardware listening verification remains pending).
- [~] Mirrored channel meters and separate L/R master peak meters with clip indicators (output still
      hard-clips; a transparent master limiter is not implemented)
- [~] Audio device, buffer and separate master/headphone/booth stereo output pairs
      (restart required; exclusive WASAPI remains stereo; no separate-device cue).

### Core effects

- [ ] Beat FX: delay, echo, reverb, flanger, phaser, filter
- [ ] Sound Color FX: filter, noise, crush
- [ ] Beat-synced FX time, wet/dry, FX channel assign

### Recording

- [ ] Record the master output to file
- [ ] Tracklist saved with each recording

## Milestone 3: rekordbox parity

### Decks and transport

- [x] Four independent decks (A–D), with a saved 2 Decks / 4 Decks view switch,
      four mixer channels, waveforms, transport, pads, loops, effects, stems and
      headphone selection. Compact four-deck panels keep the library at the
      two-deck view's height. A/B tools expand over C/D; C/D slide up over A/B
      with tools underneath. The active tool tab closes the animated overlay.
      Mirrored control groups retain internal button order. Four-deck columns
      group key/pitch/tempo, transport, and activation/headphone cue. Stems has
      its own transport row above Play/Load; two-deck controls use one row.
      Compact Cue, Sync and Stems buttons match the Play/Load row width and size.
      Four-deck platters and BPM/key information sit at the outer edges;
      key details share one line with subtle vertical dividers.
      Mixer channels follow A/C/B/D, with volume faders below EQ, outer VU meters, central stereo
      master meters and a single sampler/master/headphone knob row.
      A/C default to the left crossfader side; B/D to the right. Independent
      A/B/C/D toggles on each side allow multiple assignments; neither side
      bypasses the crossfader, and both sides stay at unity without boosting.
      Four-deck Sync follows the longest continuously playing other deck with
      a beat grid; falls back to pairing when none is playing.
      Automix remains on A/B; custom controller mappings can address C/D.
      Existing two-deck hardware profiles retain their original assignments.
- [ ] Slip mode
- [ ] Reverse play
- [x] Loop move and independent In / Out adjustment
- [x] Beat-loop sizing down to 0.125 and 0.0625 beats through resize and Direct controls
- [ ] 16 hot cues per deck
- [~] Key shift and key sync (a block in Flow can be shifted by semitones
      for planning; the decks cannot shift key yet)

### Mixer

- [ ] EQ isolator mode
- [ ] Headphone split cue

### Effects

- [ ] Beat FX: ping pong, roll, slip roll, reverse roll, trans, pan, pitch,
      spiral, helix, vinyl brake, stretch
- [ ] Sound Color FX: dub echo, space, sweep, jet, pitch
- [ ] Release FX: echo out, vinyl brake, back spin
- [ ] Multi-FX and single-FX unit modes

### Pads

- [ ] Hot cue
- [ ] Pad FX
- [ ] Beat jump
- [ ] Beat loop
- [~] Sampler [persistent shared bank and disk imports implemented; multiple banks pending]
- [ ] Keyboard
- [ ] Key shift

### Stems and sampler

- [x] Sampler bus volume on the left of the mixer; affects all pads together
      while preserving their individual levels, with smoothed gain changes.

- [x] Local cached four-source separation (vocals, instruments, bass, drums)
      through native ONNX Runtime. Settings > Audio downloads the Fast or
      Detailed model; no Python runtime is required. Inference currently uses
      up to four CPU threads.
- [x] EQ dropdown beside Stems: Classic, EZRemix, Modern EQ and four-stem layouts
      (see [STEMS.md](STEMS.md) for source grouping and setup)
- [~] Per-stem level / full cut, mousewheel, double-click reset and smoothed gains
      (dedicated solo buttons still missing)
- [~] Sampler with banks, one-shot and loop modes [16-slot persistent bank with original defaults; multiple named banks pending]
- [ ] Vocal position markers on the waveform

### Step sequencer

- [ ] Record sampler pad hits into a pattern, quantised to the beat clock
- [ ] Pattern lengths of 1, 2 and 4 bars, with overdub and per-slot erase
- [ ] Pattern playback synced to the master deck, with per-slot mute
- [ ] Metronome while recording
- [ ] Save and load patterns per sampler bank

### Controllers

- [~] MIDI input and LED output [native input transport, mapping bridge and
      profile-owned Scratch LEDs implemented; other feedback pending. See CONTROLLERS.md.]
- [ ] djcraft mapping format: declarative file plus optional plugin script
- [ ] Built-in mappings for common DDJ controllers
- [~] MIDI Learn [Rust API and JSON profile import/export, Shift conditions,
      state-aware toggles, absolute/relative controls and fader soft takeover;
      graphical editor/Learn UI pending. Numark starter needs hardware validation.]
- [~] DDJ-GRV6 mapping [802 documented bindings, including fcScript pad modes,
      momentary Pad FX and Shift + Beat Loop rolls, Beat FX routing and stateful
      LED feedback; physical acceptance and user-facing mapping editor pending.
      See GRV6-REMAINING-WORK.md.]
- [~] Bundled MixTrack Pro II profile based on official Numark MIDI addresses
      [40 bindings: transport/mixer/pitch/headphone/hot-cue controls with pickup,
      plus latched Scratch-mode buttons/LED feedback, scratching while playing
      or paused with top contact, and rim pitch bend even with Scratch on.
      Mode remains latched after releasing the top. Hot cues now set empty slots and recall existing cues,
      and Sync/Cue/headphone Cue/Stutter, held pitch bends and Shift key lock are
      mapped. Scratch sensitivity reduced fourfold after hardware feedback,
      stopped jogs scrub with Scratch off; rim/button bends expire after 40 ms
      of audio to prevent queued correction tails. Pitch covers the app's full
      -50%/+100% range with exact neutral and pickup,
      with the desktop's persistence/near-beat snap/edit lock. Initial controls work; jog hardware
      calibration, LEDs and remaining buttons pending. Launch via tools/run-mixtrack.ps1.]
- [ ] Import mapping files from the user's own rekordbox install

## Milestone 4: radio

### Broadcast

- [ ] "Go live" from the master output
- [ ] Stream to a user-chosen server (Icecast-compatible or self-hosted)
- [ ] Now-playing metadata and artwork
- [ ] Station profile: name, genre, description

### Directory

- [ ] Open directory format that lists links and metadata only, never audio
- [ ] Self-hostable directory server
- [ ] Subscribe to multiple directories in the app
- [ ] Takedown and reporting process for the default directory

### Listen

- [ ] Browse live stations by genre and listener count
- [ ] In-app player with live tracklist
- [ ] Follow DJs and get notified when they go live
- [~] Auto-DJ mode: queued playlists, preload, beatmatching and staged EQ crossfades implemented; marker-based phrase planning implemented; stem-based vocal overlap ranking implemented; semantic understanding pending. Plays when nobody is
      mixing

## Milestone 5: remote collab

- [ ] Session create and join by link or code
- [ ] Peer-to-peer connection over WebRTC, with a self-hostable rendezvous service
- [ ] Host renders audio; guests send control actions through the protocol
- [ ] Low-latency monitor stream of the mix to guests
- [ ] Guests see live deck state: waveforms, positions, cues, mixer
- [ ] Roles and permissions: host, co-DJ, viewer
- [ ] Guest track requests from the host's library
- [ ] Session chat

## Milestone 6: phone and tablet

- [ ] Web client that joins a session by link or QR code
- [ ] Touch layout for deck control, mixer and FX
- [ ] Remote control of your own set (same mechanism, host-owned role)
- [ ] Library browsing and track loading from the device
- [ ] Native apps (later, if the web client is not enough)

## Milestone 7: DVS (timecode vinyl)

The decoder is written for djcraft; GPL decoders such as xwax are not linked.

- [ ] Audio input routing: assign sound card inputs to decks
- [ ] Timecode decoder: speed, direction and position from the control signal
- [ ] Support for common control vinyl and CD formats
- [ ] Absolute, relative and internal (through) modes per deck
- [ ] Calibration screen: signal scope, level, phase and noise threshold
- [ ] Needle-drop handling and lead-in detection
- [ ] Signal-loss fallback to internal playback

## Plugins

Plugins are a cross-cutting feature. The runtime lands with Milestone 1 (music
sources need it first); each later milestone adds its own extension points.

### Runtime

- [ ] Embedded JavaScript runtime built on QuickJS (through the `rquickjs`
      crate; both MIT)
- [ ] TypeScript support: types stripped at install time, with published type
      definitions for the plugin API
- [ ] One isolated context per plugin, with memory and execution-time limits
- [ ] No ambient access: network, files and library data are host functions
      granted per permission
- [ ] Async host API (promises) so plugins never block the UI
- [ ] Plugins run off the audio thread, always

### Packaging

- [ ] Plugin manifest: name, version, API version, permissions, entry point
- [ ] Install from a folder, an archive or a URL
- [ ] Permission prompt on install, with per-plugin enable/disable
- [ ] Versioned plugin API with a compatibility check on load
- [ ] Hot reload for plugin development
- [ ] Plugin log and error view

### Plugin store

- [~] "+" button beside the library's source tabs that opens the plugins page
      (page exists; nothing can be installed yet)
- [ ] Open store index format: a list of plugins with name, version,
      permissions, download link and checksum
- [ ] Self-hostable store; subscribe to more than one store in the app
- [ ] Browse and search plugins by kind (source, metadata, controller, …)
- [ ] Install, update and remove from the store, with the permission prompt
- [ ] Checksum verification on download, and signed listings
- [ ] Source plugins appear as tabs beside Local in the library

### Extension points

- [ ] Music sources: search, browse, resolve to a playable stream or file
- [ ] Metadata and tag providers
- [ ] Library importers and exporters
- [ ] Custom analysis columns and smart-playlist rules
- [ ] Controller mappings as scripts, with access to MIDI in/out and deck state
- [ ] Radio directory providers
- [ ] Commands, menu items and keyboard shortcuts
- [ ] UI panels built from a declarative description that the host renders
      (plugins do not draw directly)

### Other plugin kinds

- [ ] External-process plugins for wrapping existing tools or code under
      licences that cannot be linked (`yt-dlp`, GPL decoders), spoken to over a
      message protocol
- [ ] Audio effect plugins: host the CLAP format; VST3 and Audio Units later.
      JavaScript is not used for audio processing

## Interface direction

The interface is inspired by djay Pro's approach: few visible controls, large
waveforms, and options tucked into popovers.

- [~] Mode switcher: saved 2 Decks / 4 Decks performance views (classic
      two-turntable view, automix view and single-deck prep view remain pending)
- [ ] Horizontal and vertical waveform layouts
- [~] Library docked under the decks, expandable to full window, with a source
      sidebar (local, playlists, external sources, radio) (folders and
      playlists are there; not expandable yet)
- [ ] Deck tool tabs under each deck: cues, loops, beat jump, slicer, keyboard,
      stems, FX
- [ ] Popover panels for per-deck options: BPM, key, beat grid, EQ, crossfader
- [~] Drag and drop from library to deck, deck to deck, and into playlists
      (library tracks can be dropped onto either deck, Flow lanes, playlist rows and Automix; deck-to-deck pending)
- [x] Confirm replacing a playing deck with an open master output route;
      respects channel/master faders, crossfader curves and EQ/stem kills.
      The warning also covers library load actions and the deck file picker.
- [ ] First-run setup wizard: theme, waveform style, jog wheels on or off,
      import an existing library
- [~] Light and dark themes, plus user themes (light, dark and match-system;
      no user themes)
- [ ] Keyboard shortcut editor
- [ ] Separate metering window
- [ ] Layouts that scale from a laptop screen to a large display, with the same
      structure reused by the tablet client

## Out of scope for now

- Video output and VJ features
- Lyrics
- Lighting control
- CDJ network link (Pro DJ Link)
- Controller jog-wheel screens and on-device waveforms
- Licensed streaming services (Beatport, Tidal and similar)

## Known constraints


- Auto-Trim targets the complete decoded track's peak (including cubic interpolation) at -0.1 dBFS,
      with up to +12 dB boost. EQ/effects, stem changes and summed sources can
      still add gain; this is not a master limiter or true-peak measurement.

- The desktop window opens at 1920 × 1080 logical pixels and remains resizable.

- **Remote latency:** guests feel roughly 50–200 ms between an action and the
  sound. Mixing and effects work; scratching remotely does not.
- **GPUI:** pre-1.0 with frequent breaking changes; pin exact versions.
- **External sources:** downloading from YouTube breaks its terms of service, so
  this stays an optional plugin the user enables.
- **Public broadcasting:** streaming copyrighted music needs licences in most
  countries. Responsibility sits with whoever operates the stream host or a
  public directory.

### Performance layout refresh

- [x] Pro-inspired stacked rhythm waveforms, paired deck displays and central mixer above the browser
- [x] Neon yellow-green deck A and sapphire deck B accents, with existing frequency waveform colours
- [x] Large tempo/remaining-time displays, beat indicators and draggable platter displays
- [x] Perform / Library navigation in the top bar; unused social and placeholder toolbar controls removed

### Deck loops

- [x] Per-deck auto loops from 1/4 to 64 beats, aligned to the analysed grid
- [x] Manual In / Out, Exit, half/double and mousewheel loop sizing
- [x] Audio-thread wrapping with tempo, stems and key lock; seek/load exits loops
- [x] Sixteen saved slots per track (first four in the loop panel), persistent across restarts, with delete and reloop
- [x] Loop-region waveform shading and separate roll background-position marker
- [x] Main / Adjust / Saved / Record / Direct views, clickable size and loop progress
- [x] Smart manual loops, backward loops, 1-beat quantize and loop rolls
- [~] Record view exports original-source selections to Sampler Recordings (live post-mixer capture pending)

### Deck effects

- [x] Six independent serial pre-fader slots per deck with 20 native audio effects
- [x] Pro FX x1 / FX x3 / FX x3 Full / FX x6 layouts; hidden slots remain active
- [x] Click-name activation, adjacent selector, details dock beside the browser
- [x] Drag/wheel dials, Shift fine control, double-click reset, feedback controls
- [x] BPM/tempo-synced timing from 1/16 to 2 beats, with manual fallback
- [x] Delay/reverb tails after pause/bypass, optional tail cut, live-load tail protection
- [x] Allocation-free DSP with preallocated storage and validated protocol controls
- [~] Pitch Shift/Spiral use creative granular processing (listening/tuning still needed)
- [ ] Master/post-fader effects, stem routing, floating details, custom lists and external plugins

### Pro performance pads

- [x] Per-deck eight-pad layout, page dropdown/favourites, options and two parameter lanes
- [x] Split pages and FX-area Pads 16 (split disabled in 16-pad mode); compact split rows have their own labels and equal-height pads, with separate upper/lower page-options submenus.
- [x] Hot Cues, Loop, Loop Roll, Slicer, Beat Jump, CueLoop, Saved Loops, Manual Loop, Stems and Pad FX
- [x] Sixteen persistent hot cues per track, loop cues, shift-delete, edit lock and quantize-on-set (Hot Cues can snap within 5%, 10%, 20% or 30% of a beat, or leave quantization off)
- [x] All compact pad pages use two full-width rows of four square pads; rows equally fill the available height with 4px gaps, and page adjustments live in the side-rail cog rather than above the grid.
- [x] Loop Roll pads use the same beat-length banks as Loop pads, including alternate pad actions.
- [x] Beat Jump defaults to backward/forward pairs of 4, 8, 16 and 32 beats; its bank remains adjustable.
- [x] CueLoop uses the deck's current Loops-panel length (four beats by default) instead of maintaining a separate length setting.
- [x] Momentary roll/slicer with background transport, stutter cues, temporary FX slot restoration
- [x] Saved-loop arm versus Shift-jump, 16-slot storage with backwards-compatible four-slot loading
- [x] Stem mute/isolate pads work with Classic EQ and prepared stems
- [x] Cue flags on rhythm waveforms, parameter wheel controls and page cycling
- [x] Automated Scratch: eight native patterns, quantized starts, Over/Cut and transport recovery
- [x] Key Shift pads: latched ±12-semitone deck transposition, semitone/whole-tone/major/natural-minor pad modes, reset and protocol state; tempo remains independent with Key Lock enabled.
- [x] Below the waveform, the outer side of each platter displays the current musical/Camelot key after Key Shift, with cents for unlocked tempo changes; missing source keys are shown as unknown.
- [x] Ctrl-scroll on Classic or stem EQ moves the other deck's matching band by the opposite control amount, preserving offsets and respecting limits.
- [x] Ctrl-scroll on deck volume faders lowers one while raising the other by the same fader amount, within limits.
- [~] Sampler: 16 persistent slots, disk/library imports, 12 original defaults, source capture, On/Off/Hold/Stutter, looping and master output [multiple banks and routing pending]
- [ ] StemSwap, Keycue, Scratchbank, DMX, custom scripting/editor and hardware pad assignments


### Playlists and Automix

- [x] Intro-aware EQ enters before the outgoing outro marker, with bass exchange completed by that boundary rather than near file end (section-based estimate).

- [x] Queue a displayed playlist/view, individual library tracks or dropped tracks
- [x] Reorder/remove queued tracks and save the upcoming queue as a library playlist
- [x] Persistent remaining queue without autoplay on restart
- [x] Automatic alternate-deck preload, adjustable audio-thread crossfades, Next and Stop
- [x] Missing-file skipping, short-track handling and manual takeover
- [x] Beat + EQ automix: effective tempo match (±8%), audio-thread grid-phase alignment and staged high/mid/bass handover
- [x] Marker-based phrase-aware automix: 8/16/32-bar planning, preview, section-aware bass handover, manual point priority and cancellable audio-thread scheduling.
- [x] Intro-aware smart Automix: measures complete intro bars (4–64), bounds them at section/vocal entrances, back-times an outgoing section end, and stages EQ around a later bass exchange. Saved mix points/length overrides retain priority; missing structure uses the existing fallback. New analysis no longer saves generic IN/OUT overrides.
- [x] Long intros can span outgoing sections with incoming EQ held silent until the final safe section (up to eight bars of blending). Smart EQ mixes hold the crossfader at 50% through completion; non-smart fades retain crossfader movement. Grid confidence matches the library (0.10) with specific fallback reasons.
- [x] Vocal-aware transition ranking from cached stems, overlap preview, purple activity strip, manual marker priority and missing-stem fallback.
- [ ] Pitch-preserving automix, silence trim, LUFS/true-peak normalization, shuffle/repeat and named automix queues

See [AUTOMIX.md](AUTOMIX.md) for control ownership and current limits.

- [x] Automix Take over / Resume preserves the remaining queue and current mix;
  resumes loading on a non-playing deck, waiting when both decks are playing.

### Phrase and mix preparation

- [x] Background energy-based section suggestions with editable Intro/Build/Breakdown/Drop/Outro/Section labels, coloured overview strip, drag/wheel boundary editing, add/remove sections.
- [x] Per-track persistent MIX IN/MIX OUT markers, 8/16/32-bar transition override and adjustable EQ bass-swap point; automix reads saved preparation without changing the queue.
- [~] Phrase understanding: energy-based suggestions implemented; automatic phrase-to-phrase selection implemented in automix; stem-based vocal activity implemented; semantic recognition remains pending. See [PREPARATION.md](PREPARATION.md).

- [x] Suggested hot cues from phrase/vocal/grid data: ghost markers, individual acceptance, fill empty slots, existing cue/loop preservation and normal cue persistence.
- [x] Full-track peak-normalized Auto-Trim to -0.1 dBFS, including cubic interpolation extrema, +12 dB boost cap and sufficient attenuation for over-range sources; per-load manual override, paused/explicit application, background measurement and automix preload integration. RMS is informational.
- [ ] Standards-based integrated LUFS and oversampled true-peak measurement.

- [x] Compact performance workspace: aligned decks, pinned transport controls, bounded mixer and Loops / Effects / Pads tools. The two-deck view places these tools in a 2×2 grid beside each platter, with rotated dropdown labels; Loops occupies the narrower lower-right cell with square main controls, and the fourth cell remains open for a later layout decision.
- [ ] Move track preparation and its suggestions into the Library. The Perform view no longer shows a Prepare button; design and implement the Library workflow separately.

- [x] Performance actions use bordered buttons with hover descriptions across deck tools, preparation, cues, effects, loops and automix.
- [x] Windows executable reserves an 8 MiB main-thread stack for nested GPUI tool panels (including Loops in debug builds).

- [x] Main waveforms use playback-time scaling so matched tempos share beat spacing and scroll speed; deck elapsed, remaining and total times reflect the current tempo ratio. Overview positions and saved cues retain source time.

- [x] Continuous fractional-pixel waveform envelopes replace per-pixel bars, with optimised desktop rendering in development builds.

- [x] Opaque waveform layers: lows at the back, mids above lows and highs on top; whole-track overview envelopes cached between playhead updates.

- [x] Beat analysis anchors the first beat to a repeated opening attack rather than extrapolating into leading silence. Opening accents determine bar phase, with the first supported kick as the default for unaccented percussive intros. Analysis version 6 refreshes cached grids.
- [x] Track info can pin a manual first-beat time and bar phase, nudge the anchor by 10 ms or a beat, and reset to analysis. Manual corrections survive rescans, re-analysis and restarts. Short four-bar loops beginning on a kick no longer shift the first beat to beat two.

- [x] Suggested cue labels use compact minutes:seconds; saved cue positions retain full sub-second precision.

- [x] Hot cue names and colours: right-click editor (Enter/Return or Save commits; Escape cancels), deck-themed default slot colours, accepted suggestion names, persistent per-track metadata and numbered/named flags on rhythm waveforms and compact colour-only flags on deck overviews.

- [x] Polyphonic sampler: immediate overlapping one-shot triggers with 16 fixed voices per slot, loop toggle and whole-slot stop.

- [x] Prepare auto cues snap to the nearest analysed 4/4 bar start, respect opening silence and track end, and wait for a beat grid. Existing saved cues remain unchanged.

- [x] Mirrored deck displays with centred platters, track information beside inner vertical pitch faders, transport aligned opposite tabs, key lock enabled at startup, hold Cue preview/return, automatic deck selection on track load, deck-coloured A/B selection, matching active-deck borders with a subtle outer glow and Space transport.

- [x] `-quick` startup option skips stem model preloading for UI checks while leaving normal stem startup unchanged.

- [x] Ctrl-click Hot Cues to jump to the stored cue with playback paused; normal trigger and Shift-delete remain available.

- [x] Filter primary control sweeps 20 Hz–20 kHz low-pass below centre and high-pass above centre. Echo/reverb preserve dry attacks; modulation avoids duplicate dry attenuation.

- [x] Native external audio-file drops onto either deck or rhythm waveform, with audible-deck replacement confirmation and automatic deck selection. Multi-file drops load the first supported file.

- [x] Cue preview returns atomically and silently on release, retaining its anchor across rapid repeated presses until another transport action.

- [x] Windows ASIO output via CPAL, installed driver/device selection and requested buffer sizes in Audio settings. Changes apply on restart; master uses the first stereo output pair. Hardware latency remains driver-dependent.

- [x] Settings gear receives clicks independently of the Windows title-bar drag region; Ctrl+, opens Settings on Windows.

- [x] Direct Windows WASAPI exclusive stereo output, event-driven rendering with MMCSS priority, native format negotiation, buffer alignment and actual buffer duration display. Select in Audio settings and restart; shared WASAPI and ASIO remain available. Separate headphone pairs require ASIO/shared multichannel output; live device switching remains pending.

- [x] Crossfader curve selectors show deck-coloured gain diagrams using the actual mixer curves, with names on hover.

- [x] Seeking, waveform scrubbing and ordinary hot cues retain an active loop when the destination is inside its bounds. Outside jumps exit it; saved loop cues replace it.

- [x] Deck transport and tool-tab groups mirror their positions while retaining the same left-to-right button order on both decks.

- [x] Effect rows use compact toggles with all parameter knobs and relevant sync/tail controls inline; master stereo meters fit below their channel headings without overlapping volume labels.

- [x] Larger deck-selection buttons sit separately at each deck’s inner edge beside the mixer.

- [x] Sync compensates for entering/leaving Key Lock processing and tempo-dependent stretcher latency so forward and backward corrections reach the requested beat phase.

- [x] Timed effects default to beat sync and follow live effective deck tempo; inline effect controls distribute across the available row width.

Key-locked tempo changes (including Sync) now trigger a debounced background pass
through the full-track time stretcher. Auto-Trim applies the resulting peak while
playing; manual trim overrides remain respected, and stale tempo results are
ignored. A stereo-linked 64-frame lookahead peak guard protects each stretched
deck after EQ/effects while analysis is pending and during transport changes.
This does not limit the summed master or guarantee DAC true-peak headroom.

- [x] Artist/title and other unrelated tag edits preserve detected BPM/key selection, including undo; only edits to BPM/key can create their manual overrides.

- [x] Playback starts at full transport speed immediately (32-frame anti-click gain
attack). Pause retains the existing vinyl-style speed brake. Key-locked starts
pre-roll the stretcher at the cue position using preallocated buffers, discarding
its startup latency; paused decks prepare this ahead of Play. Starts immediately
after a seek/load prepare on the first callback. The 64-frame peak-protection
lookahead and device/output buffering still apply. No device latency is removed.

- [x] Sampler deck capture starts at the exact audible playhead (selected loops keep their bounds), with beat-based length and end-of-track clamping. Pad firing uses a 32-frame anti-click attack instead of the longer gain ramp. Captures are original-source audio and save asynchronously.

- [x] Refreshing/scanning folders from the deck browser notifies the full Library to run pending BPM/key/energy analysis, even when the Library tab is hidden.

### Current-set play history

- [x] Library and performance browser mark tracks with a checkmark icon after 10 seconds of audible master playback (not headphone/Cue previews or waveform scrubbing).
- [x] Library > History lists each set in play order, with Previous/Next and New set controls. Sets and flags survive restarts; starting a new set clears current flags without deleting earlier history. Each file appears once per set.

- [x] Track right-click menu: Remove from library (keep file), including missing tracks. Removes playlist/Flow references and excludes the file from future folder scans.

- [x] faderCraft display branding and embedded Windows taskbar/executable icon (MSVC).
- [x] Main waveform extends 16 beats before file start: continuous grid, negative-time drag/wheel seeking, silent playback to zero, pitch/key-lock and cue support. No separate pre-roll menu.
- [x] Beat-phase fallback keeps confident uncompressed bass timing when log-flux is weak; opening refinement checks 128 beats to handle longer intros (analysis version 7).
- [x] Opening-grid anchor tolerates modest first-kick attack variation without relaxing the timing consensus (analysis version 8).

- [x] Mixtrack Pro II headphone Cue LEDs follow the selected monitor deck.

- [x] Mixtrack Pro II Loop In/Out, Reloop/Exit, half/double length, per-deck sampler slots 1–4 (Shift stops), and FX1–3 slot toggles [hardware retest pending].

- [x] Smooth jog scratch velocity and stationary fade to remove hard cuts between discrete position targets [hardware listening retest pending].

- [x] Debounce pulsed jog contact releases to preserve a continuous scratch gesture [30 ms release grace; hardware retest pending].

- [x] Remove speed-dependent scratch volume pumping between sparse jog packets [hardware retest pending].

- [x] Mixtrack Pro II FX1–3 encoders adjust each deck’s corresponding effect parameter [relative CC, preserves other FX settings; Shift selection/Beats encoder pending].

- [x] FX encoders follow the first UI dial for every effect type: Filter cutoff, otherwise Mix, including synced FX.

- [x] Controller pad/FX/transport LED state feedback and Shift auto-loop/cue-delete layers [134 profile entries; hardware LED retest pending; browser/mode lamps remain controller-owned].

- [x] Mixtrack Pro II Browse/Back/Load A/B, source/playlist selection, visible track highlighting/scrolling, and File/Folder focus LEDs [hardware retest pending].

- [x] Per-deck Low/Medium/High/Wide tempo fader ranges (±8/16/32/48%), default Low, full hardware/app travel and soft takeover after range changes.
- [x] Transport Cue returns to the retained playback start; control changes preserve it and playing Cue pauses at that position.

- [x] Released scratch-surface jog gestures retain ownership during physical free spin in either direction; settle after 80 ms motion idle and restore transport [hardware retest pending].

- [x] Timestamped MIDI jog velocity drives scratch playback and free-spin deceleration; bounded phase correction preserves platter alignment and stationary silence [hardware listening retest pending].

- [x] Hardware ranged tempo faders land exactly on the opposite deck's effective BPM at the nearest physical step [requires known BPMs and an in-range target; hardware retest pending].

- [x] One shared FX beat division per deck, with a single panel selector driving every synced slot; default slots are Filter, Echo and Cut.

- [x] Mixtrack per-deck Beats encoders control shared FX timing across 1/8–8 beats; UI/protocol range extended and delay storage supports 8 beats at 20 effective BPM [hardware knob retest pending].

- [x] Mixtrack startup requests all physical absolute control values and adopts them while stopped; playback reconnects retain pickup. Hardware readback verified for all 14 absolute CCs.
- [x] Coalesce batched jog motion to latest per-deck measured velocity/position, preserving transport barriers so redundant scratch updates do not backlog the engine.

- [x] Filter slots expose smoothed resonance (Q 0.707–4), defaulting to zero for older snapshots; separate shaped LP/HP sweeps share cutoff mapping with the UI.

- [x] FX slot 1 is reserved for an always-on Filter, linked to each mixer channel FILTER knob. New effects default to 50% resonance; old snapshots retain their neutral resonance fallback.

- [x] Reserved Filter toggles reset its sweep to neutral without changing resonance; UI active count and controller LED show OFF at neutral.

- [x] Two-deck mixer Filter shortcuts sit below the sampler/cue controls, separated by rules and aligned with the bottom EQ controls; four-deck filters sit below the EQ/stem controls, above the volume faders.

- [x] Library load menus and live-track replacement warnings use the shared A/B/C/D deck labels, matching the mixer and deck headers.

- [x] Beat analysis anchors a clear kick entrance after quiet pre-roll before extending the grid backwards; kick references reject large strength jumps and comparable-strength sub-beat rolls while accepting one- and two-beat cadences. These are heuristics; syncopated/ambiguous music still needs grid review.

- [x] Two-deck headers group artwork and library title/artist on the left, prominent tempo and key/master-tempo controls on the right, with elapsed/remaining time beside the seekable overview. Artwork loads off the UI thread and stale results are discarded.

- [x] Two-deck performance view gives each rhythm waveform twice the four-deck height inside the same 100px panel and uses finer display sampling; platters have a dedicated transport row and tempo strip, with track statistics consolidated in the header.

- [x] Mixed waveforms retain maximum sample peaks separately from the smoothed frequency-band body, preserving attack spikes at each horizontal scale; peak rendering follows the displayed EQ gains.

- [x] Negotiate an advertised sample rate before creating DSP when an ASIO driver reports zero Hz at startup (Realtek ASIO); reject unavailable rates cleanly.

- [~] Eyeball key investigation: independent audio peaks and supplied MIDI support G-natural/C-sharp and an E-centred modal hypothesis [detector unchanged; tonal-centre/corpus validation pending; see KEY-INVESTIGATION-2026-10-29.md].

- [~] Estimated Ionian/Dorian/Phrygian/Lydian/Mixolydian/Aeolian/Locrian modes, stored separately and displayed in library/decks with ambiguity handling [tonic uses existing matcher; real-music corpus validation pending; see MODES.md].

- [~] Integrated low-frequency attack colour in waveforms and tiny attack ticks below deck overviews [audio-derived, not grid-generated; kicks may be masked and sharp bass notes may qualify; Eyeball 2:22 visual retest pending; see WAVEFORMS.md].

- [x] Native AMD ROCm stem runtime setup on Windows, separate from CPU/CUDA, with verified GPU inference before activation and multi-adapter selection [Fast/Detailed GPU inference validated on RX 9070 XT; see STEMS.md].

- [x] Waveform attack accents use near-white deck tints; shared background attack analysis and reusable scrolling column caches remove repeated analysis from painting [smoothness hardware retest pending].

- [x] Persistent, versioned waveform attack sidecars created during analysis and reused on reload/cross-rate playback; fixed highlight strength replaces fading tails [older cache entries backfill on next load].

- [x] Cached waveform envelope meshes for playback scrolling and overview rendering; EQ and source changes invalidate geometry. [Two-deck smoothness requires live confirmation.]

- [x] EQ-independent deck overviews with attack markers below only; engine-state polling wakes stalled playback visuals after asynchronous transport acknowledgement.

- [x] Deck overview fills its existing waveform container with square corners; header and elapsed/remaining layout retained.

- [x] Integrated overview/header styling: matching grey, square waveform edges, no gap or vertical divider beside timing.

- [x] Track identity background aligns exactly with overview width; BPM/key and timing retain the fixed right column.

- [x] Deck album artwork fills the 58px identity header height alongside the A/B label.

- [x] Mirrored right-deck identity and overview/timing layout, with 4px identity spacing matching the header-to-waveform gap.

- [x] Header BPM/key and timing boxes have 4px inner padding; right-deck key controls and elapsed/remaining order fully mirror the left.

- [x] Separate grey BPM, key, elapsed and remaining boxes with 4px padding. Key lock uses a header lock icon; duplicate pitch-area button removed. Elapsed/remaining order stays unchanged on both decks.

- [x] Centred BPM; consistent KEY / minus / key / plus / lock order on both decks. Combined elapsed/remaining box matches overview height.

- [x] Consistent 4px header separators: 40px BPM and 28px key boxes in a 72px header; 44px elapsed/remaining and overview row.

- [x] Key box uses equal-width edge slots: KEY left, lock right, minus/key/plus independently centred.

- [x] Key box has invisible 30/40/30 percent sections, each centring its label, key controls or lock icon.

- [x] BPM uses a centred 26px text line with optical baseline adjustment inside its fixed-height box.

- [x] Header key-lock button widened to 26px and filled with the deck accent while active.

- [x] RGB waveform blends with gentle saturation and softened brightness to clarify reds, yellows, pinks and cyan without harsh neon colours.

- [~] Controller discovery prompts on startup/reconnection before opening mapped MIDI/LED ports; installed or bundled MixTrack mapping works on normal launch. General profile catalogue/chooser and unknown-controller onboarding remain pending.

- [x] Settings > Controllers mapping selector: installed/bundled/custom JSON profiles and Disabled, persisted selection and live switching with held-control release.

- [x] Controller Settings displays Disabled when no matching device is detected, retaining the saved mapping for later connection.

- [x] Two-deck pitch faders have their own grey panel, with spacing matching the deck header blocks; four-deck layout remains separate.

- [x] Two-deck header BPM/key and overview-time blocks align to the same outer inset as the deck letter; pitch panel has top/bottom breathing room for its buttons.

- [x] Two-deck title/overview and BPM/key/timing blocks keep a 4px inner gap while sharing the same outer inset.

- [x] Two-deck transport controls stack beside the tempo fader at the header block width: Load/Eject, Sync, Cue, Play; square buttons, forest-green Play, red Cue text. Eject unloads the track through the protocol.

- [x] Active loop toggles use the deck colour in compact, direct and full loop panels, including four-deck colours.

- [x] Bottom-right Sampler tab alongside Automix/Saved Loops: 16-pad and list views, named independent sample banks, On/Off/Hold/Stutter, loop toggle, drag/drop or disk loading, edit lock, selected-slot clear, master level and Stop All. Shares the global sampler bus with deck/controller pads. [No per-sample playback progress, groups or sample editor yet.]

- [x] Named Sampler banks: New/Rename/select with persistent names and active selection; independent panel playback controls and protocol actions. Bank switching stops sampler voices only and preserves deck playback. Existing samples stay in My Samples. Bundled MS-20 Drums and Synth FX banks include 32 CC0 Frequency 303 recordings with source/licence manifests; starter edits survive restart.

- [x] Separate stems in both track-browser context menus, including unloaded tracks. Deck loads automatically reuse cached stems or queue separation; automatic processing stops after 20% of track duration (worker startup included, queue wait excluded), terminates the worker and keeps normal deck playback. Manual preparation remains available without the automatic short limit.

- [x] Sampler captures one deck's enabled stem mix into a named-bank slot. Source A–D and active-loop/1–32 beat length controls; selection and gain levels are frozen, stereo is preserved, and other decks/master are excluded. Existing deck-pad captures also respect stem gains. [Source-time/pre-fader capture; deck FX, EQ and tempo changes are not baked in.]

- [x] Vinyl jog held-stop discards accumulated unread travel after 80 ms without wheel motion while touch remains down. Audio holds at the actual read head; resumed movement rebases there and lifting after the hold does not seek ahead. Waveform prediction is disabled during scratching. [Hardware retest pending.]
