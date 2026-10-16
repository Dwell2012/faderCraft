# Custom DJ app: living product plan

Updated 15 October 2026. This records requirements, not completed features.

## Confirmed by the user

- Build in Rust using DJCraft as the starting point.
- Target the Numark Mixtrack Pro and Pioneer DJ XDJ-RX3.
- Independent high, mid and low EQ for each deck.
- Per-deck volume trim and master volume.
- Crossfader with selectable curves.
- Live waveform display on the XDJ-RX3 is a desired integration target,
  following the user's Serato/VirtualDJ comparison. This means software-control
  screen support, not merely standalone USB library export.
- Aim for the DJ workflows and capability of VirtualDJ and Serato without
  dependence on software subscriptions.

## Proposed product rules

Core playback, mixing, controller use and library access work offline, with
no required account, subscription, activation service or paid hardware unlock.
Music, playlists and mappings stay local and can be backed up. Online lookups
are optional. Build our own interface and implementation around familiar DJ
workflows. These are proposed interpretations of the subscription-free goal;
distribution and pricing beyond personal use have not been decided.

Start with a dependable two-deck Windows application. Four decks, video,
karaoke, stems, broadcasting and remote sessions are candidates for later
discussion, not first-release requirements. DJCraft's wider upstream roadmap
does not automatically become our scope.

## Milestones

### 0. Working Windows baseline

Complete the C++ build prerequisites, build and test the upstream app, and
verify playback using a separate development library. Record actual audio
device availability and the exact Numark generation.

### 1. Mixer usable with mouse and keyboard

- Independent trim, three-band EQ and channel fader on each deck.
- Separate master volume.
- Crossfader presets: linear blend, equal-power blend and sharp scratch cut.
  Adjustable cut-in and reverse are proposed follow-ups.
- Channel and master level meters, clipping indication and deliberate
  headroom management; propose an optional final limiter.
- Proposed EQ starting point: centre detent at 0 dB, conventional cut/boost;
  full-kill isolator mode remains a user choice. Frequency boundaries and
  gain ranges will be documented and listening-tested before release.

Proposed signal path: deck playback -> trim -> EQ -> channel fader ->
crossfader -> sum -> master gain -> optional limiter -> master output.
Take headphone cue after EQ and before channel/crossfader attenuation, with
its own level and cue/master mix in milestone 2.

Acceptance: neutral EQ preserves the signal within numerical tolerance;
band cuts affect the intended frequencies; trim remains independent of channel
faders; crossfader endpoints and each curve behave correctly. Smooth gain and
EQ changes to avoid clicks, while preserving fast scratch-cut response.
Test correlated tracks at crossfader centre for overload: equal-power mixing
can increase summed amplitude. No allocations or locks on the audio thread.

### 2. Controller and headphone operation

- MIDI device discovery, event monitor and persistent MIDI Learn mappings.
- Seed the RX3 profile from the published manufacturer message list; see
  [CONTROLLER-REFERENCES.md](CONTROLLER-REFERENCES.md) for verified addresses
  and the inspected Mixtrack Pro II community mapping. Confirm the user's
  Numark generation before choosing that profile; preserve per-device jog
  encoding and pitch resolution. The community preset repurposes Beats
  encoders for trim, which is a proposed assignment rather than an agreed one.
- Map transport, mixer controls and pitch; then jog touch, bend and scratch.
- Distinguish press/release, absolute controls and relative encoders.
- Soft takeover for absolute controls to prevent unexpected parameter jumps.
- Explicit audio device selection, stereo master and independent stereo cue
  routing; cue buttons, headphone level and cue/master blend.
- Evaluate Windows low-latency backend and driver support, including ASIO,
  against actual hardware. Set latency goals from measurements.
- Test reconnect behaviour and LED feedback where supported.
- Investigate live RX3 display integration early: establish transport and
  handshake, then demonstrate one track's overview waveform and playback
  position on hardware before estimating full screen support. VirtualDJ's
  [RX3 display manual](https://virtualdj.com/manuals/hardware/pioneer/xdjrx3/layout/lcd.html)
  documents waveform views, zoom, track information and browsing, demonstrating
  that third-party screen integration is possible. Its wire protocol remains
  unverified for this project; the supplied MIDI list is insufficient by itself.

Acceptance: verify mappings and physical master/headphone channels on each
device individually. Do not claim RX3 screen/HID integration or full hardware
compatibility based on MIDI connectivity alone.

### 3. Performance essentials (proposed)

Transport cue, hot cues, loops, beat jump, pitch fader, beat-grid correction,
sync and quantize. Validate the existing key lock and waveform behaviour.
Prioritise these with the user before implementation.

### 4. Library, recording and effects (proposed)

Refine local search, crates/playlists and library migration using real exports.
Add master recording and a small useful effects set (filter, echo, reverb).
Make long-session stability, recovery and portable backups release criteria.

## Decisions still open

- Primary use: blending full tracks, scratching, or both?
- Conventional EQ cut/boost, full-kill isolator, or switchable modes?
- Which Serato/VirtualDJ features are indispensable after the mixer?
- Exact Mixtrack Pro generation; which controller to validate first?
- Which RX3 screen features are essential beyond waveforms: browser, artwork,
  beat grids, cue markers and touch seeking? Required first-release scope remains open.

## How work lands

Implement each feature through protocol actions, engine behaviour and UI,
then hardware mapping. Add meaningful headless DSP/protocol tests and perform
listening and controller checks. Update FEATURES.md only as implementation
lands; planned requirements are not marked complete. Preserve upstream MIT
attribution and dependency licence checks.
