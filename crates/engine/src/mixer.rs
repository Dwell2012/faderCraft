//! The audio-thread side of the engine.
//!
//! Nothing in here allocates, locks or blocks once a [`Mixer`] is built.
//! Commands arrive on a lock-free queue, state leaves through atomics, and
//! replaced tracks are sent back to the control side to be freed there.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::eq::Eq;
use djcraft_protocol::{CrossfaderCurve, EqMode};
use signalsmith_stretch::Stretch;

use crate::track::{CHANNELS, Track};

pub const DECKS: usize = 2;

/// Cue stays at unity through the left half; master stays at unity through the right.
pub fn cue_mix_gains(position: f32) -> [f32; 2] {
    let position = position.clamp(0., 1.);
    [(2. * (1. - position)).min(1.), (2. * position).min(1.)]
}
pub const MIN_TEMPO: f64 = 0.5;
pub const MAX_TEMPO: f64 = 2.0;
pub const MAX_SCRATCH_SPEED: f64 = 4.0;

/// Frames rendered per internal block.
const BLOCK: usize = 512;
/// Time constant for speed changes on the varispeed path, to avoid clicks.
const SPEED_SMOOTHING_SECONDS: f64 = 0.004;

pub(crate) enum Command {
    SamplerGain {
        level: f32,
    },
    Monitor {
        monitor: djcraft_protocol::MonitorState,
    },
    AutomixFade {
        from: usize,
        to: usize,
        frames: u64,
        id: u64,
        smart: bool,
        incoming_start: Option<f64>,
        bass_swap: f32,
        eq_start: f32,
        schedule: Option<(f64, f64)>,
        preserve_entry: bool,
        matching: Option<(djcraft_protocol::BeatGrid, djcraft_protocol::BeatGrid, f64)>,
    },
    CancelFade,
    CaptureSample {
        slot: usize,
        track: Arc<Track>,
        start: f64,
        end: f64,
    },
    Sample {
        slot: usize,
        play: bool,
        restart: bool,
        repeat: bool,
        level: f32,
    },
    ClearSample {
        slot: usize,
    },
    AutoScratch {
        deck: usize,
        pattern: Option<u8>,
        beat: f64,
        first: f64,
        quantize: bool,
        over: bool,
    },
    ArmLoop {
        deck: usize,
        start: f64,
        end: f64,
    },
    Cue {
        deck: usize,
        frame: f64,
        end: Option<f64>,
        play: bool,
        keep_loop: bool,
    },
    Jump {
        deck: usize,
        frames: f64,
    },
    PadLoop {
        deck: usize,
        start: f64,
        end: f64,
        roll: bool,
    },
    EffectTempo {
        deck: usize,
        bpm: Option<f64>,
    },
    Effect {
        deck: usize,
        slot: usize,
        effect: djcraft_protocol::EffectState,
    },
    LoopRoll {
        deck: usize,
        enabled: bool,
    },
    Loop {
        deck: usize,
        region: Option<[f64; 2]>,
    },
    EqMode {
        deck: usize,
        mode: EqMode,
        eq: [f32; 3],
    },
    StemGains {
        deck: usize,
        gains: [f32; 4],
    },
    Nudge {
        deck: usize,
        frames: f64,
        transient: bool,
    },
    EndNudge {
        deck: usize,
    },
    Sync {
        deck: usize,
        ratio: f64,
        frames: f64,
    },
    Load {
        deck: usize,
        track: Arc<Track>,
    },
    Play {
        deck: usize,
    },
    Pause {
        deck: usize,
    },
    Seek {
        deck: usize,
        frame: f64,
    },
    Tempo {
        deck: usize,
        ratio: f64,
    },
    KeyLock {
        deck: usize,
        on: bool,
    },
    Volume {
        deck: usize,
        level: f32,
    },
    Scratch {
        deck: usize,
        speed: Option<f64>,
    },
    ScratchTo {
        deck: usize,
        frame: f64,
    },
    Trim {
        deck: usize,
        gain: f32,
    },
    Eq {
        deck: usize,
        band: usize,
        gain: f32,
    },
    Crossfader {
        position: f32,
    },
    CrossfaderCurve {
        curve: CrossfaderCurve,
    },
    MasterGain {
        level: f32,
    },
}

#[derive(Default)]
pub(crate) struct DeckShared {
    protection_db: AtomicU32,
    eq_gains: [AtomicU32; 3],
    auto_scratch: AtomicBool,
    effect_tail: AtomicBool,
    roll_position: AtomicU64,
    /// Audible position in frames, as `f64` bits.
    position: AtomicU64,
    playing: AtomicBool,
    peak: AtomicU32,
}

impl DeckShared {
    pub(crate) fn protection_db(&self) -> f32 {
        f32::from_bits(self.protection_db.load(Ordering::Relaxed))
    }
    pub(crate) fn eq_gains(&self) -> [f32; 3] {
        std::array::from_fn(|i| f32::from_bits(self.eq_gains[i].load(Ordering::Relaxed)))
    }
    pub(crate) fn auto_scratch(&self) -> bool {
        self.auto_scratch.load(Ordering::Relaxed)
    }
    pub(crate) fn effect_tail(&self) -> bool {
        self.effect_tail.load(Ordering::Relaxed)
    }
    pub(crate) fn roll_position(&self) -> Option<f64> {
        let value = f64::from_bits(self.roll_position.load(Ordering::Relaxed));
        value.is_finite().then_some(value)
    }
    pub(crate) fn peak(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed))
    }
    pub(crate) fn position(&self) -> f64 {
        f64::from_bits(self.position.load(Ordering::Relaxed))
    }

    pub(crate) fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
pub(crate) struct Shared {
    fade_done: AtomicU64,
    cross_position: AtomicU32,
    samples: [AtomicU32; 16],
    pub(crate) decks: [DeckShared; DECKS],
    master_peak: [AtomicU32; 2],
}

impl Shared {
    pub(crate) fn fade_done(&self) -> u64 {
        self.fade_done.load(Ordering::Relaxed)
    }
    pub(crate) fn cross_position(&self) -> f32 {
        f32::from_bits(self.cross_position.load(Ordering::Relaxed))
    }
    pub(crate) fn sample_states(&self) -> [u8; 16] {
        std::array::from_fn(|i| self.samples[i].load(Ordering::Relaxed) as u8)
    }
    pub(crate) fn master_stereo_peak(&self) -> [f32; 2] {
        self.master_peak
            .each_ref()
            .map(|peak| f32::from_bits(peak.load(Ordering::Relaxed)))
    }
    pub(crate) fn master_peak(&self) -> f32 {
        self.master_stereo_peak().into_iter().fold(0., f32::max)
    }
}

/// Deck gains at a crossfader position in 0..=1, also used by UI curve previews.
pub fn crossfader_gains(position: f32, curve: CrossfaderCurve) -> [f32; 2] {
    match curve {
        CrossfaderCurve::Smooth => [(2. * (1. - position)).min(1.), (2. * position).min(1.)],
        CrossfaderCurve::EqualPower => [
            (position * std::f32::consts::FRAC_PI_2).cos().max(0.),
            (position * std::f32::consts::FRAC_PI_2).sin(),
        ],
        CrossfaderCurve::Cut => [((1. - position) * 20.).min(1.), (position * 20.).min(1.)],
    }
}

struct Deck {
    auto: Option<AutoScratch>,
    loop_armed: bool,
    loop_roll: bool,
    roll_position: Option<f64>,
    loop_region: Option<[f64; 2]>,
    track: Option<Arc<Track>>,
    /// Read position in frames.
    position: f64,
    playing: bool,
    tempo: f64,
    nudge_frames: f64,
    nudge_timeout: Option<usize>,
    key_lock: bool,
    volume: f32,
    eq: Eq,
    effect_bpm: Option<f64>,
    effects: [crate::effects::Effect; djcraft_protocol::EFFECT_SLOTS],
    stem_gains: [f32; 4],
    current_stems: [f32; 4],
    cross_gain: f32,
    scratch: Option<f64>,
    scratch_position: Option<f64>,
    scratch_follow_frames: f64,
    scratch_gain: f64,
    /// Current speed on the varispeed path; follows the target smoothly.
    speed: f64,
    speed_smoothing: f64,
    gain: f32,
    start_attack: usize,
    stretch: Stretch,
    stretch_input: Box<[f32]>,
    stretch_preroll: Box<[f32]>,
    stretch_discard: Box<[f32]>,
    prepared_at: Option<(f64, f64)>,
    prepared_read_position: f64,
    /// Latency of the stretcher, in input frames per unit of tempo.
    stretch_input_latency: f64,
    stretch_output_latency: f64,
    stretching: bool,
    /// Fraction of an input frame carried between stretched blocks.
    input_carry: f64,
}

impl Deck {
    fn new(sample_rate: u32) -> Self {
        let mut stretch = Stretch::preset_default(CHANNELS as u32, sample_rate);
        let preroll_frames = stretch
            .input_latency()
            .max((stretch.output_latency() as f64 * MAX_TEMPO).ceil() as usize + 1);
        let stretch_preroll = vec![0.; preroll_frames * CHANNELS].into_boxed_slice();
        let mut stretch_discard = vec![0.; stretch.output_latency() * CHANNELS].into_boxed_slice();
        // Warm the seek/process scratch storage on the control thread.
        stretch.seek(&*stretch_preroll, 1.);
        stretch.process(&*stretch_preroll, &mut *stretch_discard);
        stretch.reset();
        let max_input = (BLOCK as f64 * (MAX_TEMPO + 0.08)) as usize + 1;
        Self {
            auto: None,
            track: None,
            loop_region: None,
            loop_roll: false,
            loop_armed: false,
            roll_position: None,
            position: 0.0,
            playing: false,
            tempo: 1.0,
            nudge_frames: 0.,
            nudge_timeout: None,
            key_lock: false,
            volume: 1.0,
            eq: Eq::new(sample_rate),
            effect_bpm: None,
            effects: std::array::from_fn(|_| crate::effects::Effect::new(sample_rate)),
            stem_gains: [1.; 4],
            current_stems: [1.; 4],
            cross_gain: 1.,
            scratch: None,
            scratch_position: None,
            scratch_follow_frames: sample_rate as f64 * 0.015,
            scratch_gain: 0.,
            speed: 0.0,
            speed_smoothing: 1.0 - (-1.0 / (SPEED_SMOOTHING_SECONDS * sample_rate as f64)).exp(),
            gain: 1.0,
            start_attack: 0,
            stretch_input_latency: stretch.input_latency() as f64,
            stretch_output_latency: (stretch.output_latency() + crate::peak_guard::DELAY) as f64,
            stretch,
            stretch_preroll,
            stretch_discard,
            prepared_at: None,
            prepared_read_position: 0.,
            stretch_input: vec![0.0; max_input * CHANNELS].into_boxed_slice(),
            stretching: false,
            input_carry: 0.0,
        }
    }

    /// Bounded pre-roll using storage allocated at construction. While paused,
    /// this prepares the DSP without advancing the public transport position.
    fn prepare_stretch(&mut self) {
        if self.prepared_at == Some((self.position, self.tempo)) {
            return;
        }
        let Some(track) = self.track.as_deref() else {
            return;
        };
        self.stretch.reset();
        let available = track.decoded_frames();
        let needed = self.position
            + self.stretch_input_latency
            + self.stretch.output_latency() as f64 * self.tempo;
        if !self.playing && !track.is_complete() && (available as f64) < needed {
            self.prepared_at = None;
            return;
        }
        let region = self.loop_region.filter(|_| self.scratch.is_none());
        let mut position = self.position;
        let mut armed = self.loop_armed;
        let first = self.stretch_input_latency as usize;
        let wanted = self.stretch.output_latency() as f64 * self.tempo;
        let second = wanted as usize;
        for (pass, count) in [first, second].into_iter().enumerate() {
            for frame in self.stretch_preroll[..count * CHANNELS].chunks_exact_mut(CHANNELS) {
                if armed && region.is_some_and(|r| position >= r[0]) {
                    armed = false;
                }
                position = armed_position(position, region, armed);
                frame.copy_from_slice(&interpolate(track, available, position, self.current_stems));
                position = armed_position(position + 1., region, armed);
            }
            if pass == 0 {
                self.stretch
                    .seek(&self.stretch_preroll[..count * CHANNELS], self.tempo);
            } else {
                self.stretch.process(
                    &self.stretch_preroll[..count * CHANNELS],
                    &mut *self.stretch_discard,
                );
            }
        }
        self.input_carry = wanted - second as f64;
        self.prepared_read_position = position;
        self.prepared_at = Some((self.position, self.tempo));
    }

    /// Fills `out` with the deck's signal before gain. Returns false, leaving
    /// `out` untouched, when the deck is silent.
    fn render(&mut self, out: &mut [f32]) -> bool {
        // Controller bends must expire even if the UI stalls. Sync and ordinary
        // one-shot nudges retain their existing completion behavior.
        if let Some(remaining) = &mut self.nudge_timeout {
            *remaining = remaining.saturating_sub(out.len() / CHANNELS);
            if *remaining == 0 {
                self.nudge_frames = 0.;
                self.nudge_timeout = None;
            }
        }
        // Count negative transport time in the audio callback, not UI timers.
        // Split the crossing block so the first sample is not delayed by a buffer.
        if self.position < 0. {
            out.fill(0.);
            if !self.playing || self.track.is_none() {
                return false;
            }
            let frames = out.len() / CHANNELS;
            let correction = self
                .nudge_frames
                .clamp(-(frames as f64) * 0.08, frames as f64 * 0.08);
            let tempo = self.tempo + correction / frames.max(1) as f64;
            let silent = ((-self.position / tempo).ceil() as usize).min(frames);
            self.nudge_frames -= correction * silent as f64 / frames.max(1) as f64;
            self.position = (self.position + silent as f64 * tempo).min(0.);
            self.speed = self.tempo;
            if silent * CHANNELS < out.len() {
                return self.render(&mut out[silent * CHANNELS..]);
            }
            return false;
        }
        let use_stretch = self.key_lock
            && self.scratch.is_none()
            && self.auto.is_none()
            && (self.tempo - 1.).abs() > 1e-3;
        if use_stretch && !self.stretching && (self.playing || self.speed == 0.) {
            self.prepare_stretch();
            if self.playing && self.prepared_at.is_some() {
                self.position = self.prepared_read_position;
                self.prepared_at = None;
                self.stretching = true;
            } else if !self.playing {
                return false;
            }
        }
        let Some(track) = self.track.as_deref() else {
            return false;
        };
        let frames = out.len() / CHANNELS;
        // Only what has been decoded so far can be read. While the track is
        // still loading, playback that catches up with the decoder waits.
        let available = track.decoded_frames();
        let complete = track.is_complete();
        if let Some(mut scratch) = self.auto.take() {
            self.stretching = false;
            for frame in out.chunks_exact_mut(CHANNELS) {
                smooth_stems(
                    &mut self.current_stems,
                    self.stem_gains,
                    self.speed_smoothing,
                );
                if self.playing {
                    if let Some(position) = &mut self.roll_position {
                        *position = (*position + self.tempo).min(available as f64);
                    }
                    if self.loop_armed && self.loop_region.is_some_and(|r| self.position >= r[0]) {
                        self.loop_armed = false;
                    }
                }
                let dry = if self.playing {
                    interpolate(track, available, self.position, self.current_stems)
                } else {
                    [0.; 2]
                };
                let wet = if scratch.wait > 0. {
                    scratch.wait -= self.tempo;
                    dry
                } else if scratch.elapsed < scratch.beat * 4. {
                    let phase = scratch.elapsed / scratch.beat;
                    let (offset, gate) = scratch_motion(scratch.pattern, phase);
                    let signal = interpolate(
                        track,
                        available,
                        scratch.anchor + offset * scratch.beat,
                        self.current_stems,
                    );
                    let edge = (scratch.elapsed / 64.)
                        .min((scratch.beat * 4. - scratch.elapsed) / 64.)
                        .clamp(0., 1.) as f32;
                    scratch.gate += (gate - scratch.gate) * 0.08;
                    let gain = scratch.gate * edge;
                    scratch.elapsed += self.tempo;
                    std::array::from_fn(|c| {
                        if scratch.over {
                            dry[c] * (1. - edge * 0.5) + signal[c] * gain * 0.5
                        } else {
                            signal[c] * gain + dry[c] * (1. - edge)
                        }
                    })
                } else {
                    dry
                };
                frame.copy_from_slice(&wet);
                if self.playing {
                    self.position = armed_position(
                        self.position + self.tempo,
                        self.loop_region,
                        self.loop_armed,
                    )
                    .min(available as f64);
                }
            }
            if scratch.elapsed < scratch.beat * 4. {
                self.auto = Some(scratch);
            }
            self.speed = if self.playing { self.tempo } else { 0. };
            return true;
        }
        let region = self.loop_region.filter(|_| self.scratch.is_none());

        // At most 8% temporary speed correction, spread across audio blocks.
        // This changes real playback timing; beat markers follow that timing.
        let correction = if self.playing && self.scratch.is_none() {
            self.nudge_frames
                .clamp(-(frames as f64) * 0.08, frames as f64 * 0.08)
        } else {
            0.
        };
        self.nudge_frames -= correction;
        let effective_tempo = self.tempo + correction / frames.max(1) as f64;
        if self.playing {
            if let Some(position) = &mut self.roll_position {
                *position = (*position + frames as f64 * effective_tempo).min(available as f64);
            }
        }
        let stretch = self.key_lock
            && self.scratch.is_none()
            && self.playing
            && (self.tempo - 1.0).abs() > 1e-3;
        if stretch {
            if !self.stretching {
                self.stretch.reset();
                self.stretching = true;
                self.input_carry = 0.0;
            }
            let wanted = frames as f64 * effective_tempo + self.input_carry;
            let count = wanted as usize;
            self.input_carry = wanted - count as f64;

            let input = &mut self.stretch_input[..count * CHANNELS];
            for frame in input.chunks_exact_mut(CHANNELS) {
                if self.loop_armed && region.is_some_and(|r| self.position >= r[0]) {
                    self.loop_armed = false;
                }
                self.position = armed_position(self.position, region, self.loop_armed);
                smooth_stems(
                    &mut self.current_stems,
                    self.stem_gains,
                    self.speed_smoothing,
                );
                frame.copy_from_slice(&interpolate(
                    track,
                    available,
                    self.position,
                    self.current_stems,
                ));
                self.position = armed_position(self.position + 1., region, self.loop_armed);
                if !complete {
                    self.position = self.position.min(available as f64);
                }
            }
            self.stretch.process(&*input, &mut *out);
            self.speed = effective_tempo;
        } else {
            self.stretching = false;
            let target = self
                .scratch
                .unwrap_or(if self.playing { effective_tempo } else { 0.0 });
            if self.scratch_position.is_none() && target == 0.0 && self.speed.abs() < 1e-4 {
                self.speed = 0.0;
                return false;
            }
            for frame in out.chunks_exact_mut(CHANNELS) {
                if let Some(destination) = self.scratch_position {
                    let distance = destination - self.position;
                    if distance.abs() < 1.0 {
                        self.position = destination;
                        self.speed = 0.;
                    } else {
                        // Ease velocity between coarse MIDI targets, without
                        // jumping the read head or overshooting the hand.
                        let desired = (distance / self.scratch_follow_frames)
                            .clamp(-MAX_SCRATCH_SPEED, MAX_SCRATCH_SPEED);
                        self.speed += (desired - self.speed) * self.speed_smoothing;
                        if self.speed.signum() == distance.signum() {
                            self.speed = self.speed.clamp(-distance.abs(), distance.abs());
                        }
                    }
                    // Keep full volume even at slow wheel speeds and through
                    // reversals. Only fade once the read head reaches the hand;
                    // a speed-dependent gate pumps between sparse MIDI packets.
                    let gain = if distance.abs() < 1.0 { 0. } else { 1. };
                    self.scratch_gain += (gain - self.scratch_gain) * self.speed_smoothing;
                    if gain == 0. && self.scratch_gain < 1e-6 {
                        self.scratch_gain = 0.;
                    }
                } else {
                    self.speed += (target - self.speed) * self.speed_smoothing;
                }
                smooth_stems(
                    &mut self.current_stems,
                    self.stem_gains,
                    self.speed_smoothing,
                );
                if self.loop_armed && region.is_some_and(|r| self.position >= r[0]) {
                    self.loop_armed = false;
                }
                self.position = armed_position(self.position, region, self.loop_armed);
                frame.copy_from_slice(&interpolate(
                    track,
                    available,
                    self.position,
                    self.current_stems,
                ));
                if self.scratch_position.is_some() {
                    for sample in frame.iter_mut() {
                        *sample *= self.scratch_gain as f32;
                    }
                }
                self.position = armed_position(self.position + self.speed, region, self.loop_armed);
            }
            self.position = self.position.clamp(0.0, available as f64);
        }

        // The read head is ahead after pre-roll: drain the stretcher and guard
        // until the audible head reaches the end, rather than cutting the tail.
        let ended = if self.stretching {
            self.audible_position() >= available as f64
        } else {
            self.position >= available as f64
        };
        if complete && self.playing && self.scratch.is_none() && ended {
            self.playing = false;
        }
        true
    }

    /// The position currently being heard, allowing for stretcher latency.
    fn audible_position(&self) -> f64 {
        if self.stretching {
            let latency = self.stretch_input_latency + self.stretch_output_latency * self.tempo;
            armed_position(
                (self.position - latency).max(0.0),
                self.loop_region,
                self.loop_armed,
            )
        } else {
            self.position
        }
    }
}

fn armed_position(position: f64, region: Option<[f64; 2]>, armed: bool) -> f64 {
    if armed && region.is_some_and(|r| position < r[0]) {
        position
    } else {
        loop_position(position, region)
    }
}
fn loop_position(position: f64, region: Option<[f64; 2]>) -> f64 {
    match region {
        Some([start, end]) => start + (position - start).rem_euclid(end - start),
        None => position,
    }
}

fn smooth_stems(current: &mut [f32; 4], target: [f32; 4], smoothing: f64) {
    for (value, target) in current.iter_mut().zip(target) {
        *value += (target - *value) * smoothing as f32;
        if (*value - target).abs() < 1e-5 {
            *value = target;
        }
    }
}

/// Cubic Hermite interpolation of the first `frames` frames of a track.
/// Positions outside that range read as silence.
fn interpolate(track: &Track, frames: usize, position: f64, gains: [f32; 4]) -> [f32; CHANNELS] {
    let base = position.floor();
    let t = (position - base) as f32;
    let index = base as isize;
    let frames = frames as isize;
    let frame = |i: isize| -> [f32; CHANNELS] {
        if i < 0 || i >= frames {
            [0.0; CHANNELS]
        } else {
            let i = i as usize * CHANNELS;
            [
                track.mixed_sample(i, gains),
                track.mixed_sample(i + 1, gains),
            ]
        }
    };
    let (p0, p1, p2, p3) = (
        frame(index - 1),
        frame(index),
        frame(index + 1),
        frame(index + 2),
    );
    let mut out = [0.0; CHANNELS];
    for c in 0..CHANNELS {
        let a = -0.5 * p0[c] + 1.5 * p1[c] - 1.5 * p2[c] + 0.5 * p3[c];
        let b = p0[c] - 2.5 * p1[c] + 2.0 * p2[c] - 0.5 * p3[c];
        let d = 0.5 * (p2[c] - p0[c]);
        out[c] = ((a * t + b) * t + d) * t + p1[c];
    }
    out
}

// Native, deterministic scratch trajectories: offset in beats and crossfader gate.
fn scratch_motion(pattern: u8, phase: f64) -> (f64, f32) {
    let cycles = [1., 2., 1., 2., 3., 1., 4., 0.5][pattern as usize];
    let t = (phase * cycles).fract();
    let offset = if t < 0.5 { t * 2. } else { (1. - t) * 2. } * 0.5;
    let gate = match pattern {
        2 => {
            if t < 0.45 {
                1.
            } else {
                0.
            }
        }
        3 => {
            if (t * 4.).fract() < 0.65 {
                1.
            } else {
                0.
            }
        }
        4 => {
            if (t * 6.).fract() < 0.5 {
                1.
            } else {
                0.
            }
        }
        5 => {
            if t > 0.12 && t < 0.88 {
                1.
            } else {
                0.
            }
        }
        _ => 1.,
    };
    (offset, gate)
}
#[derive(Clone, Copy)]
struct AutoScratch {
    pattern: u8,
    gate: f32,
    anchor: f64,
    beat: f64,
    elapsed: f64,
    wait: f64,
    over: bool,
}
#[derive(Default)]
struct SampleVoice {
    // Fixed storage: overlapping hits never allocate on the audio thread.
    tails: [Option<(f64, f32)>; 15],
    track: Option<Arc<Track>>,
    start: f64,
    end: f64,
    position: f64,
    playing: bool,
    repeat: bool,
    level: f32,
    gain: f32,
    attack_remaining: usize,
}
impl SampleVoice {
    fn render(&mut self, out: &mut [f32]) {
        let Some(track) = &self.track else {
            return;
        };
        for frame in out.chunks_exact_mut(CHANNELS) {
            for tail in &mut self.tails {
                let Some((position, gain)) = tail.as_mut() else {
                    continue;
                };
                *gain += ((if self.playing { self.level } else { 0. }) - *gain) * 0.02;
                let signal = interpolate(track, track.decoded_frames(), *position, [1.; 4]);
                let edge = ((self.end - *position) / 64.).clamp(0., 1.) as f32;
                for c in 0..CHANNELS {
                    frame[c] += signal[c] * *gain * edge;
                }
                *position += 1.;
                if *position >= self.end || (!self.playing && *gain < 0.00001) {
                    *tail = None;
                }
            }
            if self.playing && self.attack_remaining > 0 {
                self.gain = self.level * (33 - self.attack_remaining) as f32 / 32.;
                self.attack_remaining -= 1;
            } else {
                self.gain += ((if self.playing { self.level } else { 0. }) - self.gain) * 0.02;
            }
            if self.gain < 0.00001 && !self.playing {
                continue;
            }
            let signal = interpolate(track, track.decoded_frames(), self.position, [1.; 4]);
            let edge = ((self.end - self.position) / 64.).clamp(0., 1.) as f32;
            for c in 0..CHANNELS {
                frame[c] += signal[c] * self.gain * edge;
            }
            self.position += 1.;
            if self.position >= self.end {
                if self.repeat && self.playing {
                    self.position = self.start;
                    self.gain = 0.;
                    self.attack_remaining = 32;
                } else {
                    self.playing = false;
                    self.gain = 0.;
                }
            }
        }
    }
}

/// Renders all decks to an interleaved stereo stream. Owned by the audio
/// callback; also usable offline.
#[derive(Clone, Copy)]
struct MixFade {
    from: usize,
    to: usize,
    total: u64,
    elapsed: u64,
    id: u64,
    smart: bool,
    baseline: [[f32; 3]; 2],
    start_cross: f32,
    bass_swap: f32,
    eq_start: f32,
}
// Each band is handed over once; weights never boost and sum to unity.
#[cfg(test)]
fn mix_eq(progress: f32) -> [[f32; 3]; 2] {
    mix_eq_at(progress, 0.5)
}
fn mix_eq_at(progress: f32, bass_swap: f32) -> [[f32; 3]; 2] {
    let smooth = |a: f32, b: f32| {
        let t = ((progress - a) / (b - a)).clamp(0., 1.);
        t * t * (3. - 2. * t)
    };
    let incoming = [
        smooth(bass_swap - 0.08, bass_swap + 0.08),
        smooth(0.15, (bass_swap + 0.35).min(1.)),
        smooth(0., (bass_swap + 0.15).min(0.9)),
    ];
    [incoming.map(|g| 1. - g), incoming]
}
fn mix_eq_held(progress: f32, bass_swap: f32, start: f32) -> [[f32; 3]; 2] {
    let blend = ((progress - start) / (1. - start)).clamp(0., 1.);
    let swap = ((bass_swap - start) / (1. - start)).clamp(0.1, 0.9);
    mix_eq_at(blend, swap)
}
#[derive(Clone, Copy)]
struct PendingMix {
    from: usize,
    to: usize,
    frames: u64,
    id: u64,
    smart: bool,
    incoming_start: Option<f64>,
    bass_swap: f32,
    eq_start: f32,
    schedule: (f64, f64),
    preserve_entry: bool,
    matching: Option<(djcraft_protocol::BeatGrid, djcraft_protocol::BeatGrid, f64)>,
}
pub struct Mixer {
    deck_guards: [crate::peak_guard::PeakGuard; DECKS],
    sampler_gain: f32,
    current_sampler_gain: f32,
    sampler_buffer: Box<[f32]>,
    monitor_gain: f32,
    monitor_mix: f32,
    monitor: djcraft_protocol::MonitorState,
    cue_gain: f32,
    monitor_buffer: Box<[f32]>,
    pending_mix: Option<PendingMix>,
    fade: Option<MixFade>,
    samples: [SampleVoice; 16],
    commands: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Arc<Track>>,
    shared: Arc<Shared>,
    decks: [Deck; DECKS],
    master_gain: f32,
    current_master: f32,
    crossfader: f32,
    curve: CrossfaderCurve,
    sample_rate: u32,
    deck_buffer: Box<[f32]>,
}

impl Mixer {
    pub(crate) fn new(
        sample_rate: u32,
        commands: rtrb::Consumer<Command>,
        garbage: rtrb::Producer<Arc<Track>>,
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            monitor: Default::default(),
            deck_guards: std::array::from_fn(|_| crate::peak_guard::PeakGuard::new(sample_rate)),
            sampler_gain: 1.,
            current_sampler_gain: 1.,
            sampler_buffer: vec![0.; BLOCK * CHANNELS].into_boxed_slice(),
            monitor_gain: 0.5,
            monitor_mix: 0.,
            cue_gain: 1.,
            monitor_buffer: vec![0.; BLOCK * CHANNELS].into_boxed_slice(),
            fade: None,
            pending_mix: None,
            samples: std::array::from_fn(|_| SampleVoice::default()),
            commands,
            garbage,
            shared,
            decks: std::array::from_fn(|_| Deck::new(sample_rate)),
            master_gain: 1.0,
            current_master: 1.,
            crossfader: 0.5,
            curve: CrossfaderCurve::default(),
            sample_rate,
            deck_buffer: vec![0.0; BLOCK * CHANNELS].into_boxed_slice(),
        }
    }

    fn apply(&mut self, command: Command) {
        // Direct transport and hand movements cancel the automated gesture.
        let cancel = match &command {
            Command::Load { deck, .. }
            | Command::Seek { deck, .. }
            | Command::Jump { deck, .. }
            | Command::Cue { deck, .. }
            | Command::Scratch { deck, .. }
            | Command::ScratchTo { deck, .. }
            | Command::Pause { deck } => Some(*deck),
            _ => None,
        };
        if let Some(deck) = cancel {
            self.deck_guards[deck].reset();
            self.decks[deck].auto = None;
        }
        let invalidate_preparation = match &command {
            Command::Cue { deck, .. }
            | Command::Seek { deck, .. }
            | Command::Jump { deck, .. }
            | Command::Scratch { deck, .. }
            | Command::ScratchTo { deck, .. }
            | Command::StemGains { deck, .. }
            | Command::ArmLoop { deck, .. }
            | Command::PadLoop { deck, .. }
            | Command::Loop { deck, .. }
            | Command::LoopRoll { deck, .. } => Some(*deck),
            _ => None,
        };
        if let Some(deck) = invalidate_preparation {
            self.decks[deck].prepared_at = None;
        }
        match command {
            Command::CancelFade => {
                self.fade = None;
                self.pending_mix = None;
            }
            Command::AutomixFade {
                from,
                to,
                frames,
                id,
                smart,
                incoming_start,
                bass_swap,
                eq_start,
                schedule,
                preserve_entry,
                matching,
            } => {
                if let Some(target) = schedule {
                    if self.decks[from].audible_position() / (self.sample_rate as f64) < target.0 {
                        self.pending_mix = Some(PendingMix {
                            from,
                            to,
                            frames,
                            id,
                            smart,
                            incoming_start,
                            bass_swap,
                            eq_start,
                            schedule: target,
                            preserve_entry,
                            matching,
                        });
                        return;
                    }
                }
                self.pending_mix = None;
                if let Some(seconds) = incoming_start {
                    self.decks[to].position = seconds * self.sample_rate as f64;
                }
                if let Some((out_grid, in_grid, ratio)) = matching {
                    if let Some((out_at, in_at)) = schedule {
                        let late = (self.decks[from].audible_position() / self.sample_rate as f64
                            - out_at)
                            .max(0.)
                            / self.decks[from].tempo;
                        self.decks[to].position = (in_at + late * ratio) * self.sample_rate as f64;
                    } else if !preserve_entry {
                        let position =
                            self.decks[from].audible_position() / self.sample_rate as f64;
                        let phase = ((position - out_grid.first_beat) * out_grid.bpm / 60.
                            - out_grid.downbeat as f64)
                            .rem_euclid(4.);
                        let anchor = incoming_start.unwrap_or(in_grid.first_beat);
                        let base_bar = (((anchor - in_grid.first_beat) * in_grid.bpm / 60.
                            - in_grid.downbeat as f64)
                            / 4.)
                            .ceil()
                            .max(0.)
                            * 4.;
                        self.decks[to].position = (in_grid.first_beat
                            + (base_bar + in_grid.downbeat as f64 + phase) * 60. / in_grid.bpm)
                            * self.sample_rate as f64;
                    }
                    self.decks[to].tempo = ratio;
                    self.decks[to].key_lock = false;
                    self.decks[to].stretching = false;
                    self.decks[to].effect_bpm = Some(in_grid.bpm);
                    self.decks[from].nudge_frames = 0.;
                    self.decks[to].nudge_frames = 0.;
                }
                self.decks[to].playing = true;
                self.decks[to].speed = self.decks[to].tempo;
                let baseline = [self.decks[from].eq.gains, self.decks[to].eq.gains];
                self.fade = Some(MixFade {
                    from,
                    to,
                    total: frames.max(1),
                    elapsed: schedule.map_or(0, |(at, _)| {
                        ((self.decks[from].audible_position() / self.sample_rate as f64 - at)
                            .max(0.)
                            / self.decks[from].tempo
                            * self.sample_rate as f64) as u64
                    }),
                    id,
                    smart,
                    baseline,
                    start_cross: self.crossfader,
                    bass_swap,
                    eq_start,
                });
            }
            Command::CaptureSample {
                slot,
                track,
                start,
                end,
            } => {
                if let Some(old) = self.samples[slot].track.take() {
                    let _ = self.garbage.push(old);
                }
                self.samples[slot] = SampleVoice {
                    track: Some(track),
                    start,
                    end,
                    position: start,
                    level: 0.7,
                    ..Default::default()
                };
            }
            Command::ClearSample { slot } => {
                if let Some(old) = self.samples[slot].track.take() {
                    let _ = self.garbage.push(old);
                }
                self.samples[slot].playing = false;
                self.samples[slot].tails.fill(None);
            }
            Command::Sample {
                slot,
                play,
                restart,
                repeat,
                level,
            } => {
                let sample = &mut self.samples[slot];
                if restart && play && !repeat && sample.playing && sample.position < sample.end {
                    let index = sample
                        .tails
                        .iter()
                        .position(Option::is_none)
                        .unwrap_or_else(|| {
                            sample
                                .tails
                                .iter()
                                .enumerate()
                                .max_by(|(_, a), (_, b)| a.unwrap().0.total_cmp(&b.unwrap().0))
                                .map(|(i, _)| i)
                                .unwrap_or(0)
                        });
                    sample.tails[index] = Some((sample.position, sample.gain));
                }
                if repeat {
                    sample.tails.fill(None);
                }
                if restart || sample.position >= sample.end {
                    sample.position = sample.start;
                    sample.gain = 0.;
                }
                if play && (restart || !sample.playing) {
                    sample.attack_remaining = 32;
                }
                sample.playing = play && sample.track.is_some();
                sample.repeat = repeat;
                sample.level = level;
            }
            Command::AutoScratch {
                deck,
                pattern,
                beat,
                first,
                quantize,
                over,
            } => {
                let deck = &mut self.decks[deck];
                let anchor = deck.audible_position();
                let wait = if quantize && deck.playing {
                    (first - anchor).rem_euclid(beat)
                } else {
                    0.
                };
                deck.position = anchor;
                deck.stretching = false;
                deck.scratch = None;
                deck.scratch_position = None;
                deck.auto = pattern.map(|pattern| AutoScratch {
                    pattern,
                    gate: 0.,
                    anchor: anchor + wait,
                    beat,
                    wait,
                    elapsed: 0.,
                    over,
                });
            }
            Command::ArmLoop { deck, start, end } => {
                let deck = &mut self.decks[deck];
                deck.loop_region = Some([start, end]);
                deck.loop_roll = false;
                deck.roll_position = None;
                deck.loop_armed = true;
                if deck.position >= end {
                    deck.position = start;
                    deck.stretching = false;
                }
            }
            Command::Cue {
                deck,
                frame,
                end,
                play,
                keep_loop,
            } => {
                let deck = &mut self.decks[deck];
                deck.position = frame;
                deck.prepared_at = None;
                if frame < 0. {
                    deck.auto = None;
                    deck.eq.reset();
                    for effect in &mut deck.effects {
                        effect.clear();
                    }
                }
                let retained = if keep_loop && end.is_none() {
                    loop_at(deck.loop_region, frame)
                } else {
                    None
                };
                deck.loop_armed &= retained.is_some();
                deck.loop_region = end.map(|end| [frame, end]).or(retained);
                if retained.is_none() {
                    deck.roll_position = None;
                }
                deck.nudge_frames = 0.;
                deck.stretching = false;
                deck.scratch = None;
                deck.scratch_position = None;
                deck.playing = play;
                deck.start_attack = if play { 32 } else { 0 };
                deck.speed = if play { deck.tempo } else { 0. };
            }
            Command::Jump { deck, frames } => {
                let deck = &mut self.decks[deck];
                if let Some(track) = &deck.track {
                    deck.position =
                        (deck.audible_position() + frames).clamp(0., track.decoded_frames() as f64);
                }
                deck.loop_armed = false;
                deck.loop_region = None;
                deck.roll_position = None;
                deck.nudge_frames = 0.;
                deck.stretching = false;
            }
            Command::PadLoop {
                deck,
                start,
                end,
                roll,
            } => {
                let deck = &mut self.decks[deck];
                let resume = deck
                    .roll_position
                    .unwrap_or_else(|| deck.audible_position());
                deck.roll_position = roll.then_some(resume);
                deck.loop_roll = roll;
                deck.loop_armed = false;
                deck.loop_region = Some([start, end]);
                deck.position = start;
                deck.stretching = false;
                deck.nudge_frames = 0.;
            }
            Command::EffectTempo { deck, bpm } => {
                self.decks[deck].effect_bpm = bpm;
            }
            Command::Effect { deck, slot, effect } => {
                self.decks[deck].effects[slot].set(effect);
            }
            Command::LoopRoll { deck, enabled } => {
                let deck = &mut self.decks[deck];
                deck.loop_roll = enabled;
                if !enabled {
                    deck.roll_position = None;
                } else if deck.loop_region.is_some() && deck.roll_position.is_none() {
                    deck.roll_position = Some(deck.audible_position());
                }
            }
            Command::Loop { deck, region } => {
                let deck = &mut self.decks[deck];
                deck.loop_armed = false;
                if region.is_some() && deck.loop_roll && deck.roll_position.is_none() {
                    deck.roll_position = Some(deck.audible_position());
                }
                if region.is_none() {
                    if let Some(position) = deck.roll_position.take() {
                        deck.position = position;
                        deck.stretching = false;
                    }
                }
                if deck.loop_region.is_none() {
                    if let Some([start, end]) = region {
                        if deck.position < start || deck.position >= end {
                            deck.position = start;
                        }
                    }
                }
                deck.loop_region = region;
                deck.position = loop_position(deck.position, region);
            }
            Command::EqMode { deck, mode, eq } => {
                self.decks[deck].eq.gains = if mode == EqMode::Classic { eq } else { [1.; 3] };
                self.decks[deck].stem_gains = [1.; 4];
            }
            Command::StemGains { deck, gains } => self.decks[deck].stem_gains = gains,
            Command::Load { deck, track } => {
                self.deck_guards[deck].reset();
                let deck = &mut self.decks[deck];
                if let Some(old) = deck.track.replace(track) {
                    // Freeing memory is the control side's job. If its queue
                    // is full the old track is dropped here as a last resort.
                    let _ = self.garbage.push(old);
                }
                deck.loop_armed = false;
                deck.loop_region = None;
                deck.roll_position = None;
                deck.nudge_frames = 0.;
                deck.position = 0.0;
                deck.eq.reset();
                for effect in &mut deck.effects {
                    effect.clear();
                }
                deck.stem_gains = [1.; 4];
                deck.current_stems = [1.; 4];
                deck.playing = false;
                deck.scratch = None;
                deck.scratch_position = None;
                deck.speed = 0.0;
                deck.stretching = false;
                deck.prepared_at = None;
            }
            Command::Play { deck } => {
                let deck = &mut self.decks[deck];
                if let Some(track) = &deck.track {
                    if track.is_complete() && deck.position >= track.frames() as f64 {
                        deck.position = 0.0;
                    }
                    if !deck.playing {
                        deck.start_attack = 32;
                    }
                    deck.playing = true;
                    if deck.scratch.is_none() && deck.auto.is_none() {
                        deck.speed = deck.tempo;
                    }
                }
            }
            Command::Pause { deck } => {
                let position = self.decks[deck].audible_position();
                self.decks[deck].position = position;
                self.decks[deck].stretching = false;
                self.decks[deck].prepared_at = None;
                self.decks[deck].playing = false;
                self.decks[deck].nudge_frames = 0.;
            }
            Command::Seek { deck, frame } => {
                let deck = &mut self.decks[deck];
                deck.loop_region = loop_at(deck.loop_region, frame);
                if deck.loop_region.is_none() {
                    deck.loop_armed = false;
                    deck.roll_position = None;
                }
                deck.nudge_frames = 0.;
                if let Some(track) = &deck.track {
                    deck.position = frame.min(track.decoded_frames() as f64);
                    deck.prepared_at = None;
                    if frame < 0. {
                        deck.auto = None;
                        deck.eq.reset();
                        for effect in &mut deck.effects {
                            effect.clear();
                        }
                    }
                    deck.stretching = false;
                }
            }
            Command::Tempo { deck, ratio } => {
                if self.decks[deck].stretching && (ratio - 1.).abs() <= 1e-3 {
                    self.decks[deck].position = self.decks[deck].audible_position();
                    self.decks[deck].stretching = false;
                }
                self.decks[deck].tempo = ratio;
                self.decks[deck].nudge_frames = 0.;
            }
            Command::Nudge {
                deck,
                frames,
                transient,
            } => {
                self.decks[deck].nudge_timeout =
                    transient.then_some((self.sample_rate as usize * 40) / 1000);
                self.decks[deck].nudge_frames = (self.decks[deck].nudge_frames + frames).clamp(
                    -(self.sample_rate as f64) * 0.5,
                    self.sample_rate as f64 * 0.5,
                );
            }
            Command::EndNudge { deck } => {
                self.decks[deck].nudge_frames = 0.;
                self.decks[deck].nudge_timeout = None;
            }
            Command::Sync {
                deck,
                ratio,
                frames,
            } => {
                let deck = &mut self.decks[deck];
                // Account for entering/leaving key-lock processing as well as
                // its tempo-dependent output latency. Apply it gradually with
                // the phase correction, keeping ordinary cue/scrub timing intact.
                let old_latency = if deck.stretching {
                    deck.stretch_input_latency + deck.stretch_output_latency * deck.tempo
                } else {
                    0.
                };
                let new_latency = if deck.key_lock
                    && deck.playing
                    && deck.scratch.is_none()
                    && (ratio - 1.).abs() > 1e-3
                {
                    deck.stretch_input_latency + deck.stretch_output_latency * ratio
                } else {
                    0.
                };
                let latency_change = if old_latency > 0. && new_latency > 0. {
                    new_latency - old_latency
                } else {
                    // Entering stretch now pre-rolls to the audible position;
                    // leaving it reads from that audible position directly.
                    if old_latency > 0. {
                        deck.position = deck.audible_position();
                        deck.stretching = false;
                    }
                    0.
                };
                deck.tempo = ratio;
                deck.nudge_frames = frames + latency_change;
                deck.nudge_timeout = None;
            }
            Command::KeyLock { deck, on } => {
                let deck = &mut self.decks[deck];
                if deck.stretching && !on {
                    deck.position = deck.audible_position();
                    deck.stretching = false;
                }
                if deck.key_lock != on {
                    deck.prepared_at = None;
                }
                deck.key_lock = on;
            }
            Command::Volume { deck, level } => self.decks[deck].volume = level,
            Command::Scratch { deck, speed } => {
                self.decks[deck].nudge_frames = 0.;
                self.decks[deck].scratch = speed;
                self.decks[deck].scratch_position = None;
            }
            Command::ScratchTo { deck, frame } => {
                let deck = &mut self.decks[deck];
                if let Some(track) = &deck.track {
                    if deck.scratch_position.is_none() {
                        deck.scratch_gain = if deck.playing { 1. } else { 0. };
                    }
                    if deck.stretching {
                        deck.position = deck.audible_position();
                    }
                    deck.stretching = false;
                    deck.loop_region = loop_at(deck.loop_region, frame);
                    if deck.loop_region.is_none() {
                        deck.loop_armed = false;
                        deck.roll_position = None;
                    }
                    deck.nudge_frames = 0.;
                    deck.scratch = Some(0.);
                    if frame < 0. || deck.position < 0. {
                        // Drag through empty timeline space directly: no scrub
                        // audio from sample zero and no stranded negative head.
                        deck.position = frame.min(track.decoded_frames() as f64);
                        deck.scratch_position = None;
                        deck.speed = 0.;
                        deck.prepared_at = None;
                        deck.auto = None;
                        deck.eq.reset();
                        for effect in &mut deck.effects {
                            effect.clear();
                        }
                    } else {
                        deck.scratch_position = Some(frame.min(track.decoded_frames() as f64));
                    }
                }
            }
            Command::Trim { deck, gain } => {
                let deck = &mut self.decks[deck];
                let silent = !deck.playing
                    && deck.scratch.is_none()
                    && deck.auto.is_none()
                    && deck.speed.abs() < 0.0001;
                deck.eq.set_trim(gain, silent);
            }
            Command::Eq { deck, band, gain } => self.decks[deck].eq.gains[band] = gain,
            Command::Crossfader { position } => self.crossfader = position,
            Command::CrossfaderCurve { curve } => self.curve = curve,
            Command::MasterGain { level } => self.master_gain = level,
            Command::SamplerGain { level } => self.sampler_gain = level,
            Command::Monitor { monitor } => self.monitor = monitor,
        }
    }

    /// Renders the next `out.len() / 2` frames of interleaved stereo.
    pub fn render(&mut self, out: &mut [f32]) {
        self.render_with_monitor(out, None);
    }

    /// Both buses share one transport/effects pass; cue is pre-fader and pre-crossfader.
    pub fn render_with_monitor(&mut self, out: &mut [f32], mut headphones: Option<&mut [f32]>) {
        if let Some(cue) = &headphones {
            assert_eq!(cue.len(), out.len());
        }
        while self.garbage.slots() > 0 {
            let Ok(command) = self.commands.pop() else {
                break;
            };
            self.apply(command);
        }

        let mut peaks = [0f32; DECKS];
        let mut master_peak = [0f32; 2];
        let decay = (-(out.len() as f32) / CHANNELS as f32 / self.sample_rate as f32 / 0.4).exp();
        for (block_index, block) in out.chunks_mut(BLOCK * CHANNELS).enumerate() {
            self.monitor_buffer[..block.len()].fill(0.);
            if let Some(p) = self.pending_mix {
                if self.decks[p.from].audible_position() / self.sample_rate as f64 >= p.schedule.0 {
                    self.apply(Command::AutomixFade {
                        from: p.from,
                        to: p.to,
                        frames: p.frames,
                        id: p.id,
                        smart: p.smart,
                        incoming_start: p.incoming_start,
                        bass_swap: p.bass_swap,
                        eq_start: p.eq_start,
                        schedule: Some(p.schedule),
                        preserve_entry: p.preserve_entry,
                        matching: p.matching,
                    });
                }
            }
            block.fill(0.0);
            let frames = block.len() / CHANNELS;
            if let Some(fade) = self.fade {
                let fraction =
                    ((fade.elapsed + frames as u64) as f64 / fade.total as f64).min(1.) as f32;
                self.crossfader = if fade.smart {
                    0.5
                } else {
                    fade.start_cross + (fade.to as f32 - fade.start_cross) * fraction
                };
                if fade.smart {
                    let weights = mix_eq_held(fraction, fade.bass_swap, fade.eq_start);
                    for (side, deck) in [fade.from, fade.to].into_iter().enumerate() {
                        self.decks[deck].eq.gains = std::array::from_fn(|band| {
                            fade.baseline[side][band] * weights[side][band]
                        });
                    }
                }
            }
            let cross = crossfader_gains(self.crossfader, self.curve);
            for (index, deck) in self.decks.iter_mut().enumerate() {
                let buffer = &mut self.deck_buffer[..block.len()];
                let target = deck.volume;
                let source_running = deck.render(buffer);
                if !deck.stretching {
                    self.deck_guards[index].reset();
                }
                if !source_running {
                    buffer.fill(0.);
                }
                let bpm = deck.effect_bpm.map(|bpm| bpm * deck.tempo);
                for effect in &mut deck.effects {
                    effect.clock(bpm, source_running);
                }
                let step = (target - deck.gain) / frames as f32;
                let cue_target = if self.monitor.deck as usize == index {
                    1.
                } else {
                    0.
                };
                let mut cue_gain = if index == 0 {
                    self.cue_gain
                } else {
                    1. - self.cue_gain
                };
                let cue_step = (cue_target - cue_gain) / frames as f32;
                let cross_step = (cross[index] - deck.cross_gain) / frames as f32;
                for ((out, input), cue) in block
                    .chunks_exact_mut(CHANNELS)
                    .zip(buffer.chunks_exact(CHANNELS))
                    .zip(self.monitor_buffer.chunks_exact_mut(CHANNELS))
                {
                    cue_gain += cue_step;
                    deck.gain += step;
                    deck.cross_gain += cross_step;
                    let mut signal = deck.eq.process(input);
                    for effect in &mut deck.effects {
                        signal = effect.process(signal);
                    }
                    if deck.start_attack > 0 {
                        let gain = (33 - deck.start_attack) as f32 / 32.;
                        signal = signal.map(|sample| sample * gain);
                        deck.start_attack -= 1;
                    }
                    if deck.stretching {
                        signal = self.deck_guards[index].process(signal);
                    }
                    for c in 0..CHANNELS {
                        let channel = signal[c] * deck.gain;
                        cue[c] += signal[c] * cue_gain;
                        peaks[index] = peaks[index].max(channel.abs());
                        out[c] += channel * deck.cross_gain;
                    }
                }
                deck.gain = target;
                self.shared.decks[index].protection_db.store(
                    self.deck_guards[index].reduction_db().to_bits(),
                    Ordering::Relaxed,
                );
                self.shared.decks[index].effect_tail.store(
                    deck.effects.iter().any(|fx| fx.tail_active()),
                    Ordering::Relaxed,
                );
            }
            let sampler = &mut self.sampler_buffer[..block.len()];
            sampler.fill(0.);
            for sample in &mut self.samples {
                sample.render(sampler);
            }
            let sampler_step = (self.sampler_gain - self.current_sampler_gain) / frames as f32;
            for (out, input) in block
                .chunks_exact_mut(CHANNELS)
                .zip(sampler.chunks_exact(CHANNELS))
            {
                self.current_sampler_gain += sampler_step;
                for c in 0..CHANNELS {
                    out[c] += input[c] * self.current_sampler_gain;
                }
            }
            self.current_sampler_gain = self.sampler_gain;
            let master_step = (self.master_gain - self.current_master) / frames as f32;
            for frame in block.chunks_exact_mut(CHANNELS) {
                self.current_master += master_step;
                for (channel, sample) in frame.iter_mut().enumerate() {
                    *sample *= self.current_master;
                    master_peak[channel] = master_peak[channel].max(sample.abs());
                    *sample = sample.clamp(-1., 1.);
                }
            }
            self.current_master = self.master_gain;
            self.cue_gain = if self.monitor.deck == 0 { 1. } else { 0. };
            if let Some(cue_out) = headphones.as_deref_mut() {
                let start = block_index * BLOCK * CHANNELS;
                let gain_step = (self.monitor.gain - self.monitor_gain) / frames as f32;
                let mix_step = (self.monitor.mix - self.monitor_mix) / frames as f32;
                for ((output, cue), master) in cue_out[start..start + block.len()]
                    .chunks_exact_mut(CHANNELS)
                    .zip(self.monitor_buffer.chunks_exact(CHANNELS))
                    .zip(block.chunks_exact(CHANNELS))
                {
                    self.monitor_gain += gain_step;
                    self.monitor_mix += mix_step;
                    let [cue_gain, master_gain] = cue_mix_gains(self.monitor_mix);
                    for c in 0..CHANNELS {
                        output[c] = ((cue[c] * cue_gain + master[c] * master_gain)
                            * self.monitor_gain)
                            .clamp(-1., 1.);
                    }
                }
            }
            self.monitor_gain = self.monitor.gain;
            self.monitor_mix = self.monitor.mix;
            if let Some(mut fade) = self.fade {
                fade.elapsed += frames as u64;
                if fade.elapsed >= fade.total {
                    self.decks[fade.from].playing = false;
                    self.crossfader = if fade.smart { 0.5 } else { fade.to as f32 };
                    if fade.smart {
                        self.decks[fade.from].eq.gains = [0.; 3];
                        self.decks[fade.to].eq.gains = [1.; 3];
                    }
                    self.fade = None;
                    self.shared.fade_done.store(fade.id, Ordering::Relaxed);
                } else {
                    self.fade = Some(fade);
                }
            }
            self.shared
                .cross_position
                .store(self.crossfader.to_bits(), Ordering::Relaxed);
        }

        for (sample, shared) in self.samples.iter().zip(&self.shared.samples) {
            shared.store(
                if sample.playing {
                    2
                } else if sample.track.is_some() {
                    1
                } else {
                    0
                },
                Ordering::Relaxed,
            );
        }
        for (channel, peak) in master_peak.into_iter().enumerate() {
            let previous = f32::from_bits(self.shared.master_peak[channel].load(Ordering::Relaxed));
            self.shared.master_peak[channel]
                .store(peak.max(previous * decay).to_bits(), Ordering::Relaxed);
        }
        for ((deck, shared), peak) in self.decks.iter().zip(&self.shared.decks).zip(peaks) {
            shared
                .peak
                .store(peak.max(shared.peak() * decay).to_bits(), Ordering::Relaxed);
            shared
                .position
                .store(deck.audible_position().to_bits(), Ordering::Relaxed);
            shared.playing.store(deck.playing, Ordering::Relaxed);
            for (shared, gain) in shared.eq_gains.iter().zip(deck.eq.gains) {
                shared.store(gain.to_bits(), Ordering::Relaxed);
            }
            shared
                .auto_scratch
                .store(deck.auto.is_some(), Ordering::Relaxed);
            shared.roll_position.store(
                deck.roll_position.unwrap_or(f64::NAN).to_bits(),
                Ordering::Relaxed,
            );
        }
    }
}

#[cfg(test)]
mod automix_eq_tests {
    use super::mix_eq;
    #[test]
    fn long_intro_stays_muted_until_final_section_then_hands_over() {
        for step in 0..=75 {
            assert_eq!(
                super::mix_eq_held(step as f32 / 100., 0.9375, 0.75),
                [[1.; 3], [0.; 3]]
            );
        }
        let mid = super::mix_eq_held(0.85, 0.9375, 0.75);
        assert!(mid[1][2] > mid[1][1] && mid[1][1] > mid[1][0]);
        assert_eq!(super::mix_eq_held(1., 0.9375, 0.75), [[0.; 3], [1.; 3]]);
    }
    #[test]
    fn staged_eq_handover_is_complementary_and_never_boosts() {
        for step in 0..=1000 {
            let gains = mix_eq(step as f32 / 1000.);
            for band in 0..3 {
                assert!((gains[0][band] + gains[1][band] - 1.).abs() < 0.00001);
                assert!((0. ..=1.).contains(&gains[0][band]));
                assert!((0. ..=1.).contains(&gains[1][band]));
            }
        }
        let early = mix_eq(0.3);
        assert_eq!(early[1][0], 0.);
        assert!(early[1][2] > early[1][1]);
        assert_eq!(mix_eq(0.), [[1.; 3], [0.; 3]]);
        assert_eq!(mix_eq(1.), [[0.; 3], [1.; 3]]);
    }
}

#[cfg(test)]
mod preparation_tests {
    use super::*;
    #[test]
    fn bass_swap_moves_without_boosting_either_side() {
        let early = mix_eq_at(0.4, 0.25);
        let late = mix_eq_at(0.4, 0.75);
        assert!(early[1][0] > 0.99 && late[1][0] < 0.01);
        for swap in [0.1, 0.5, 0.9] {
            for step in 0..=100 {
                let eq = mix_eq_at(step as f32 / 100., swap);
                for band in 0..3 {
                    assert!((eq[0][band] + eq[1][band] - 1.).abs() < 1e-6);
                }
            }
        }
    }
}

/// Loop end is exclusive: jumping exactly there exits the loop.
pub(crate) fn loop_at(region: Option<[f64; 2]>, position: f64) -> Option<[f64; 2]> {
    region.filter(|[start, end]| position >= *start && position < *end)
}
