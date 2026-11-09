//! Allocation-free stereo effects; all delay storage is constructed off the audio thread.
use djcraft_protocol::{EffectKind as Kind, EffectState};
use std::f32::consts::{PI, TAU};

struct Delay {
    data: Box<[[f32; 2]]>,
    cursor: usize,
    valid: usize,
}
impl Delay {
    fn new(frames: usize) -> Self {
        Self {
            data: vec![[0.; 2]; frames + 2].into_boxed_slice(),
            cursor: 0,
            valid: 0,
        }
    }
    fn clear(&mut self) {
        self.valid = 0;
    }
    fn read(&self, lag: f32) -> [f32; 2] {
        let lag = lag.clamp(1., (self.data.len() - 2) as f32);
        let n = lag.floor() as usize;
        if self.valid < n + 1 {
            return [0.; 2];
        }
        let a = self.data[(self.cursor + self.data.len() - n) % self.data.len()];
        let b = self.data[(self.cursor + self.data.len() - n - 1) % self.data.len()];
        std::array::from_fn(|c| a[c] + (b[c] - a[c]) * lag.fract())
    }
    fn write(&mut self, input: [f32; 2]) {
        self.data[self.cursor] = input;
        self.cursor = (self.cursor + 1) % self.data.len();
        self.valid = (self.valid + 1).min(self.data.len() - 1);
    }
}
struct Reverb {
    lines: [Delay; 4],
    damp: [[f32; 2]; 4],
}
impl Reverb {
    fn new(rate: u32) -> Self {
        Self {
            lines: std::array::from_fn(|_| Delay::new((rate as f32 * 0.15) as usize)),
            damp: [[0.; 2]; 4],
        }
    }
    fn clear(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
        self.damp = [[0.; 2]; 4];
    }
    fn process(&mut self, input: [f32; 2], rate: f32, size: f32, feedback: f32) -> [f32; 2] {
        let times = [0.0297, 0.0371, 0.0411, 0.0437];
        let taps: [[f32; 2]; 4] =
            std::array::from_fn(|i| self.lines[i].read(rate * times[i] * (0.6 + size * 2.4)));
        for i in 0..4 {
            for c in 0..2 {
                self.damp[i][c] += 0.35 * (taps[i][c] - self.damp[i][c]);
            }
        }
        // Orthogonal Hadamard feedback network: stable for feedback strictly below one.
        let d = self.damp;
        let signs = [
            [1., 1., 1., 1.],
            [1., -1., 1., -1.],
            [1., 1., -1., -1.],
            [1., -1., -1., 1.],
        ];
        for (i, line) in self.lines.iter_mut().enumerate() {
            line.write(std::array::from_fn(|c| {
                input[c] * 0.35
                    + (0..4).map(|j| signs[i][j] * d[j][c]).sum::<f32>()
                        * 0.5
                        * (0.2 + feedback * 0.8)
            }));
        }
        [
            (taps[0][0] + taps[1][0] - taps[2][0] + taps[3][1]) * 0.5,
            (taps[0][1] - taps[1][1] + taps[2][1] + taps[3][0]) * 0.5,
        ]
    }
}

pub(crate) struct Effect {
    state: EffectState,
    rate: f32,
    smoothing: f32,
    delay: Delay,
    reverb: Reverb,
    low: [f32; 2],
    allpass: [[f32; 2]; 6],
    previous: [f32; 2],
    phase: f32,
    shepard_phases: [f32; 8],
    shepard_glide: f32,
    grain: f32,
    wet: f32,
    amount: f32,
    parameter: f32,
    feedback: f32,
    resonance: f32,
    period: f32,
    target_period: f32,
    source_running: bool,
    elapsed: f32,
    capture_len: usize,
    capture_offset: f32,
    captured: bool,
    hold: [f32; 2],
    hold_counter: usize,
    rng: u32,
    envelope: f32,
    history_left: usize,
    idle: bool,
    tail_left: usize,
    tail_mix: f32,
}
impl Effect {
    pub fn set_beats(&mut self, beats: f32) {
        self.state.beats = beats;
    }
    pub fn new(rate: u32) -> Self {
        // Eight beats at 20 BPM plus room for pitch-shifting grain offsets.
        Self::with_delay_seconds(rate, 25)
    }

    pub fn with_delay_seconds(rate: u32, seconds: usize) -> Self {
        Self {
            state: EffectState::default(),
            rate: rate as f32,
            smoothing: 1. - (-1. / (rate as f32 * 0.01)).exp(),
            delay: Delay::new(rate as usize * seconds),
            reverb: Reverb::new(rate),
            low: [0.; 2],
            allpass: [[0.; 2]; 6],
            previous: [0.; 2],
            phase: 0.,
            shepard_phases: [0.; 8],
            shepard_glide: 0.,
            grain: 0.,
            wet: 0.,
            amount: 0.5,
            parameter: 0.5,
            feedback: 0.45,
            resonance: 0.,
            period: 0.525,
            target_period: 0.525,
            source_running: true,
            elapsed: 0.,
            capture_len: 0,
            capture_offset: 0.,
            captured: false,
            hold: [0.; 2],
            hold_counter: 0,
            rng: 0x7a42b31d,
            envelope: 0.,
            history_left: 0,
            idle: true,
            tail_left: 0,
            tail_mix: 0.,
        }
    }
    pub fn clear(&mut self) {
        self.delay.clear();
        self.reverb.clear();
        self.low = [0.; 2];
        self.allpass = [[0.; 2]; 6];
        self.previous = [0.; 2];
        self.phase = 0.;
        self.shepard_phases = [0.; 8];
        self.shepard_glide = 0.;
        self.grain = 0.;
        self.elapsed = 0.;
        self.captured = false;
        self.capture_offset = 0.;
        self.hold = [0.; 2];
        self.hold_counter = 0;
        self.envelope = 0.;
        self.tail_left = 0;
        self.history_left = 0;
    }
    pub fn kill(&mut self) {
        self.clear();
        self.wet = 0.;
        self.tail_mix = 0.;
        self.idle = true;
    }

    pub fn tail_active(&self) -> bool {
        self.state.kind.has_tail()
            && self.history_left > 0
            && self.amount > 0.00001
            && (self.state.enabled || self.tail_left > 0 || self.wet > 0.00001)
    }

    pub fn processing(&self) -> bool {
        self.state.enabled || self.wet > 0.00001 || self.tail_active()
    }
    pub fn post_fader(&self) -> bool {
        matches!(
            self.state.kind,
            Kind::Echo | Kind::PingPong | Kind::LowCutEcho | Kind::EchoOut | Kind::Reverb
        )
    }
    pub fn set(&mut self, state: EffectState) {
        if self.state.kind != state.kind {
            self.clear();
            self.wet = 0.;
        }
        if state.enabled && !self.state.enabled {
            self.elapsed = 0.;
            self.captured = false;
            self.capture_offset = 0.;
            self.tail_left = 0;
        }
        if !state.enabled && self.state.enabled && state.tails && state.kind.has_tail() {
            self.tail_left = (self.rate * 30.) as usize;
            self.tail_mix = self.amount * self.wet;
        }
        if !state.tails {
            self.tail_left = 0;
        }
        if state.enabled {
            self.idle = false;
        }
        self.state = state;
    }
    pub fn clock(&mut self, bpm: Option<f64>, source_running: bool) {
        self.source_running = source_running;
        self.target_period = if self.state.sync && self.state.kind.timed() && bpm.is_some() {
            (60. / bpm.unwrap().clamp(20., 600.) * self.state.beats as f64) as f32
        } else {
            match self.state.kind {
                Kind::Flanger | Kind::Chorus | Kind::Phaser => {
                    1. / (0.1 + self.state.parameter * 1.9)
                }
                Kind::Cut | Kind::Tremolo | Kind::AutoPan => {
                    1. / (0.1 + self.state.parameter * 15.9)
                }
                Kind::Brake => 0.2 + self.state.parameter * 3.8,
                Kind::BeatRepeat => 0.03 + self.state.parameter * 1.97,
                _ => 0.05 + self.state.parameter * 0.95,
            }
        };
    }
    fn shifted(&mut self, base: f32, semitones: f32) -> [f32; 2] {
        let window = self.rate * 0.045;
        let ratio = 2f32.powf(semitones / 12.);
        self.grain = (self.grain + (1. - ratio) / window).rem_euclid(1.);
        let other = (self.grain + 0.5).fract();
        let weight = 0.5 - 0.5 * (TAU * self.grain).cos();
        let a = self.delay.read(base + 1. + self.grain * window);
        let b = self.delay.read(base + 1. + other * window);
        std::array::from_fn(|c| a[c] * weight + b[c] * (1. - weight))
    }
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let kind = self.state.kind;
        self.wet += (if self.state.enabled { 1. } else { 0. } - self.wet) * self.smoothing;
        self.amount += (self.state.amount - self.amount) * self.smoothing;
        self.parameter += (self.state.parameter - self.parameter) * self.smoothing;
        self.feedback += (self.state.feedback - self.feedback) * self.smoothing;
        self.resonance += (self.state.resonance - self.resonance) * self.smoothing;
        self.period += (self.target_period - self.period) * self.smoothing;
        if !self.state.enabled && self.wet < 0.00001 && self.tail_left == 0 {
            if !self.idle {
                self.clear();
                self.idle = true;
            }
            if kind == Kind::BeatRepeat {
                self.delay.write(input);
            } else {
                self.delay.clear();
            }
            return input;
        }
        if !self.source_running && !self.state.tails && kind.has_tail() {
            self.clear();
            return input;
        }
        let p = self.parameter;
        let send = if self.state.enabled { 1. } else { self.wet };
        let feed = input.map(|x| x * send);
        self.history_left = self.history_left.saturating_sub(1);
        if kind.has_tail() && feed.iter().any(|x| x.abs() > 0.00001) {
            self.history_left = (self.rate * 30.) as usize;
        }
        let mut effected = input;
        match kind {
            Kind::Filter => {
                let cutoff = EffectState::filter_cutoff(p);
                // Topology-preserving two-pole filter. Zero resonance retains
                // Butterworth damping; the bounded Q peaks at 4. Centre is bypass.
                let g = (PI * cutoff.min(self.rate * 0.45) / self.rate).tan();
                let q = std::f32::consts::FRAC_1_SQRT_2
                    + self.resonance * (4. - std::f32::consts::FRAC_1_SQRT_2);
                let k = 1. / q;
                let a = 1. / (1. + g * (g + k));
                // Tiny neutral plateau also absorbs f32 smoothing residue.
                let blend = (((p - 0.5).abs() - 0.0001) / 0.0099).clamp(0., 1.);
                for c in 0..2 {
                    let v1 = (self.previous[c] + g * (input[c] - self.low[c])) * a;
                    let v2 = self.low[c] + g * v1;
                    self.previous[c] = 2. * v1 - self.previous[c];
                    self.low[c] = 2. * v2 - self.low[c];
                    let filtered = if p < 0.5 { v2 } else { input[c] - k * v1 - v2 };
                    effected[c] = input[c] + (filtered - input[c]) * blend;
                }
            }
            Kind::Echo
            | Kind::Delay
            | Kind::PingPong
            | Kind::LowCutEcho
            | Kind::EchoOut
            | Kind::Spiral => {
                let mut delayed = if kind == Kind::Spiral {
                    self.shifted(self.period * self.rate, 7.)
                } else {
                    self.delay.read(self.period * self.rate)
                };
                if kind == Kind::LowCutEcho {
                    let a = 1. - (-TAU * 300. / self.rate).exp();
                    for c in 0..2 {
                        self.low[c] += a * (delayed[c] - self.low[c]);
                        delayed[c] -= self.low[c];
                    }
                }
                let feedback = if kind == Kind::Delay {
                    0.
                } else {
                    self.feedback
                };
                let feed_gain = if kind == Kind::EchoOut {
                    (-self.elapsed / self.period.max(0.01)).exp()
                } else {
                    1.
                };
                let write = if kind == Kind::PingPong {
                    [
                        (feed[0] + feed[1]) * 0.5 + delayed[1] * feedback,
                        delayed[0] * feedback,
                    ]
                } else {
                    std::array::from_fn(|c| feed[c] * feed_gain + delayed[c] * feedback)
                };
                self.delay.write(write.map(|x| x.clamp(-8., 8.)));
                effected = delayed;
                if kind == Kind::EchoOut && self.state.enabled {
                    self.elapsed += 1. / self.rate;
                }
            }
            Kind::Reverb => {
                effected = self.reverb.process(feed, self.rate, p, self.feedback);
            }
            Kind::Flanger | Kind::Chorus => {
                let base = if kind == Kind::Chorus { 0.018 } else { 0.002 };
                let depth = if kind == Kind::Chorus { 0.006 } else { 0.0018 };
                for c in 0..2 {
                    let offset = if kind == Kind::Chorus {
                        c as f32 * PI * 0.5
                    } else {
                        0.
                    };
                    let lag = self.rate * (base + depth * (self.phase + offset).sin());
                    effected[c] = self.delay.read(lag)[c];
                }
                self.delay.write(std::array::from_fn(|c| {
                    input[c]
                        + effected[c]
                            * if kind == Kind::Flanger {
                                self.feedback * 0.8
                            } else {
                                0.
                            }
                }));
                effected = std::array::from_fn(|c| input[c] + effected[c]);
            }
            Kind::Phaser => {
                let frequency = 300. * 8f32.powf(0.5 + 0.5 * self.phase.sin());
                let t = (PI * frequency.min(self.rate * 0.4) / self.rate).tan();
                let a = (1. - t) / (1. + t);
                for c in 0..2 {
                    let mut x = input[c] + self.previous[c] * self.feedback * 0.7;
                    for stage in &mut self.allpass {
                        let y = -a * x + stage[c];
                        stage[c] = x + a * y;
                        x = y;
                    }
                    self.previous[c] = x;
                    effected[c] = input[c] + x;
                }
            }
            Kind::Cut | Kind::Tremolo | Kind::AutoPan => {
                let wave = 0.5 + 0.5 * self.phase.sin();
                let gain = if kind == Kind::Cut {
                    ((wave - 0.5) * 8. + 0.5).clamp(0., 1.)
                } else {
                    wave
                };
                effected = [
                    input[0] * gain,
                    input[1]
                        * if kind == Kind::AutoPan {
                            1. - gain
                        } else {
                            gain
                        },
                ];
            }
            Kind::Distortion => {
                let drive = 1. + p * 29.;
                effected = input.map(|x| (x * drive).tanh() / drive.tanh());
            }
            Kind::Bitcrusher => {
                let step = 2f32.powf((4. + (1. - p) * 12.).round() - 1.);
                let hold = (1. + p * 63.) as usize;
                if self.hold_counter == 0 {
                    self.hold = input.map(|x| (x * step).round() / step);
                }
                self.hold_counter = (self.hold_counter + 1) % hold;
                effected = self.hold;
            }
            Kind::Noise => {
                let level = input[0].abs().max(input[1].abs());
                self.envelope +=
                    (level - self.envelope) * (if level > self.envelope { 0.01 } else { 0.0002 });
                let a =
                    1. - (-TAU * (200. * 80f32.powf(p)).min(self.rate * 0.45) / self.rate).exp();
                for c in 0..2 {
                    self.rng ^= self.rng << 13;
                    self.rng ^= self.rng >> 17;
                    self.rng ^= self.rng << 5;
                    let noise = (self.rng as f32 / u32::MAX as f32 * 2. - 1.) * self.envelope;
                    self.low[c] += a * (noise - self.low[c]);
                    effected[c] = input[c] + self.low[c];
                }
            }
            Kind::PitchShift => {
                self.delay.write(input);
                let semitones = (p - 0.5) * 24.;
                effected = if semitones.abs() < 0.01 {
                    input
                } else {
                    self.shifted(0., semitones)
                };
            }
            Kind::BeatRepeat => {
                let length = (self.period * self.rate).round().max(2.) as usize;
                if !self.source_running {
                    self.captured = false;
                    self.delay.clear();
                    effected = [0.; 2];
                } else if !self.captured && self.delay.valid >= length + 1 {
                    self.capture_len = length;
                    self.capture_offset = 0.;
                    self.captured = true;
                }
                if self.captured {
                    let offset = self.capture_offset.min((self.capture_len - 1) as f32);
                    effected = self.delay.read(self.capture_len as f32 - offset);
                    let edge = (self.rate * 0.002)
                        .min(self.capture_len as f32 * 0.1)
                        .max(1.);
                    let fade = (offset / edge)
                        .min(((self.capture_len - 1) as f32 - offset) / edge)
                        .min(1.);
                    effected = effected.map(|x| x * fade);
                    self.capture_offset = (offset
                        + self.capture_len as f32 / (self.period * self.rate).max(1.))
                    .rem_euclid(self.capture_len as f32);
                } else if self.source_running {
                    self.delay.write(input);
                }
            }
            Kind::Brake => {
                self.delay.write(input);
                if self.source_running {
                    self.elapsed += 1. / self.rate;
                    let time = self.elapsed.min(self.period);
                    let lag = 1. + self.rate * time * time / (2. * self.period.max(0.01));
                    effected = self
                        .delay
                        .read(lag)
                        .map(|x| x * ((self.period - self.elapsed) / 0.02).clamp(0., 1.));
                } else {
                    effected = [0.; 2];
                }
            }
            Kind::ShepardToneSaw | Kind::ShepardToneTri => {
                // Eight octave-spaced voices trade places at the inaudible
                // edges of a bell-shaped spectrum. Each travels one octave
                // per selected beat period, creating an endless pitch glide.
                let travel = self.shepard_glide;
                let octave = travel.floor() as usize;
                let fraction = travel.fract();
                let pitch = 2f32.powf(fraction);
                const WEIGHTS: [f32; 8] = [0.014, 0.114, 0.458, 0.917, 0.917, 0.458, 0.114, 0.014];
                let mut tone = 0.;
                for voice in 0..8 {
                    let band = (voice + octave) % 8;
                    let gain = WEIGHTS[band] * (1. - fraction) + WEIGHTS[(band + 1) % 8] * fraction;
                    let frequency = 55. * (1u32 << band) as f32 * pitch;
                    if frequency < self.rate * 0.4 {
                        let step = frequency / self.rate;
                        let phase = self.shepard_phases[voice];
                        let wave = if kind == Kind::ShepardToneSaw {
                            let edge = |t: f32| {
                                if t < step {
                                    let x = t / step;
                                    x + x - x * x - 1.
                                } else if t > 1. - step {
                                    let x = (t - 1.) / step;
                                    x * x + x + x + 1.
                                } else {
                                    0.
                                }
                            };
                            2. * phase - 1. - edge(phase)
                        } else {
                            1. - 4. * (phase - 0.5).abs()
                        };
                        tone += wave * gain;
                        self.shepard_phases[voice] = (phase + step).fract();
                    }
                }
                self.shepard_glide = (travel + 1. / (self.period.max(0.01) * self.rate)) % 8.;
                effected = [tone * 0.25; 2];
            }
        }
        self.phase = (self.phase + TAU / (self.period.max(0.01) * self.rate)) % TAU;
        let mix = self.wet * self.amount;
        if !self.state.enabled && self.tail_left > 0 {
            self.tail_left -= 1;
            let fade = (self.tail_left as f32 / self.rate).min(1.);
            return std::array::from_fn(|c| input[c] + effected[c] * self.tail_mix * fade);
        }
        if kind == Kind::EchoOut && self.state.enabled {
            let dry = 1. - mix + mix * (-self.elapsed / self.period.max(0.01)).exp();
            return std::array::from_fn(|c| input[c] * dry + effected[c] * mix);
        }
        if matches!(
            kind,
            Kind::Echo
                | Kind::Delay
                | Kind::PingPong
                | Kind::LowCutEcho
                | Kind::Spiral
                | Kind::Reverb
                | Kind::ShepardToneSaw
                | Kind::ShepardToneTri
        ) {
            return std::array::from_fn(|c| input[c] + effected[c] * mix);
        }
        std::array::from_fn(|c| input[c] + (effected[c] - input[c]) * mix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RATE: u32 = 8000;
    #[test]
    fn shepard_tone_generates_tone_without_input_and_keeps_dry_audio() {
        for kind in [Kind::ShepardToneSaw, Kind::ShepardToneTri] {
            let mut fx = Effect::new(RATE);
            fx.set(EffectState {
                kind,
                enabled: true,
                amount: 1.,
                beats: 1.,
                ..EffectState::default()
            });
            fx.clock(Some(120.), false);
            let mut energy = 0.;
            for _ in 0..RATE {
                let sample = fx.process([0.; 2]);
                assert!(
                    sample
                        .iter()
                        .all(|value| value.is_finite() && value.abs() < 1.)
                );
                energy += sample[0] * sample[0];
            }
            assert!(energy > 1., "{kind:?} should sound over silence");
            let dry = fx.process([0.1; 2]);
            assert!((dry[0] - 0.1).abs() > 0.0001);
            fx.set(EffectState {
                kind,
                enabled: false,
                amount: 1.,
                ..EffectState::default()
            });
            for _ in 0..RATE {
                fx.process([0.; 2]);
            }
            assert_eq!(fx.process([0.1; 2]), [0.1; 2]);
        }
    }
    #[test]
    fn resonance_emphasizes_cutoff_on_both_sides_and_keeps_stereo_separate() {
        for rate in [8000, 44100, 48000, 96000] {
            for parameter in [
                0.49 * (1. - (1000f32 / 20_000.).log(0.001).powf(1. / 1.2)),
                0.51 + 0.49 * 50f32.log(1000.).powf(1. / 1.4),
            ] {
                let response = |resonance| {
                    let mut fx = Effect::new(rate);
                    fx.set(EffectState {
                        enabled: true,
                        amount: 1.,
                        parameter,
                        resonance,
                        ..EffectState::default()
                    });
                    let mut power = 0.;
                    for frame in 0..rate {
                        let input = 0.1 * (TAU * 1000. * frame as f32 / rate as f32).sin();
                        let out = fx.process([input, 0.]);
                        assert_eq!(out[1], 0.);
                        assert!(out[0].is_finite() && out[0].abs() < 0.5);
                        if frame >= rate / 2 {
                            power += out[0] * out[0];
                        }
                    }
                    power
                };
                assert!(
                    response(1.) > response(0.) * 20.,
                    "rate={rate}, p={parameter}"
                );
            }
        }
    }

    #[test]
    fn resonance_sweeps_stay_bounded_and_center_remains_neutral() {
        for rate in [8000, 44100, 48000, 96000] {
            let mut fx = Effect::new(rate);
            for frame in 0..rate {
                let parameter = if frame < rate / 2 {
                    frame as f32 / (rate / 2) as f32
                } else {
                    0.5
                };
                fx.set(EffectState {
                    enabled: true,
                    amount: 1.,
                    parameter,
                    resonance: if (frame / 128) % 2 == 0 { 1. } else { 0. },
                    ..EffectState::default()
                });
                let input = [0.1 * (TAU * 440. * frame as f32 / rate as f32).sin(), -0.05];
                let out = fx.process(input);
                assert!(out.iter().all(|v| v.is_finite() && v.abs() < 2.));
                if frame > rate * 3 / 4 {
                    assert!((out[0] - input[0]).abs() < 0.0001);
                    assert!((out[1] - input[1]).abs() < 0.0001);
                }
            }
        }
    }
    fn configured(kind: Kind, parameter: f32) -> Effect {
        let mut fx = Effect::new(RATE);
        fx.set(EffectState {
            kind,
            enabled: true,
            amount: 1.,
            parameter,
            ..EffectState::default()
        });
        fx.clock(None, true);
        for _ in 0..RATE {
            fx.process([0.; 2]);
        }
        fx
    }
    #[test]
    fn every_effect_bypasses_exactly_and_stays_finite_at_extremes() {
        for kind in Kind::ALL {
            for parameter in [0., 0.5, 1.] {
                let mut fx = Effect::new(RATE);
                fx.set(EffectState {
                    kind,
                    ..EffectState::default()
                });
                for _ in 0..10 {
                    assert_eq!(fx.process([0.3, -0.2]), [0.3, -0.2]);
                }
                fx.set(EffectState {
                    kind,
                    enabled: true,
                    parameter,
                    feedback: 0.9,
                    amount: 1.,
                    ..EffectState::default()
                });
                fx.clock(Some(120.), true);
                let mut energy = 0.;
                for i in 0..RATE * 2 {
                    let x = (i as f32 * TAU * 220. / RATE as f32).sin() * 0.25;
                    let out = fx.process([x, -x * 0.8]);
                    assert!(
                        out.iter().all(|x| x.is_finite() && x.abs() < 10.),
                        "{kind:?} {parameter}: {out:?}"
                    );
                    energy += out[0] * out[0];
                }
                assert!(energy > 0.01, "silent {kind:?}");
                fx.set(EffectState {
                    kind,
                    tails: false,
                    ..EffectState::default()
                });
                for _ in 0..RATE {
                    fx.process([0.; 2]);
                }
                assert_eq!(fx.process([0.3, -0.2]), [0.3, -0.2]);
            }
        }
    }
    #[test]
    fn synced_echo_tracks_effective_bpm_and_has_independent_stereo() {
        for (bpm, expected) in [(120., 2000), (240., 1000)] {
            let mut fx = configured(Kind::Echo, 0.5);
            fx.set(EffectState {
                kind: Kind::Echo,
                enabled: true,
                amount: 1.,
                sync: true,
                beats: 0.5,
                ..EffectState::default()
            });
            fx.clock(Some(bpm), true);
            for _ in 0..RATE {
                fx.process([0.; 2]);
            }
            let mut peak = (0, 0f32);
            for i in 0..4000 {
                let out = fx.process(if i == 0 { [0.5, 0.] } else { [0.; 2] });
                assert_eq!(out[1], 0.);
                if i > 10 && out[0] > peak.1 {
                    peak = (i, out[0]);
                }
            }
            assert!((peak.0 as i32 - expected).abs() <= 1, "{bpm}: {peak:?}");
            assert!(peak.1 > 0.48);
        }
    }
    #[test]
    fn ping_pong_alternates_and_tails_can_be_disabled() {
        let mut fx = configured(Kind::PingPong, 0.);
        let first = 400;
        let mut left = 0f32;
        let mut right = 0f32;
        for i in 0..1000 {
            let out = fx.process(if i == 0 { [0.5, 0.5] } else { [0.; 2] });
            if (first - 2..=first + 2).contains(&i) {
                left = left.max(out[0]);
                assert!(out[1].abs() < 0.001);
            }
            if (first * 2 - 2..=first * 2 + 2).contains(&i) {
                right = right.max(out[1]);
            }
        }
        assert!(left > 0.48 && right > 0.2, "{left}, {right}");
        for tails in [false, true] {
            let mut fx = configured(Kind::Echo, 0.5);
            fx.process([0.5, 0.]);
            fx.set(EffectState {
                kind: Kind::Echo,
                amount: 1.,
                tails,
                ..EffectState::default()
            });
            fx.clock(None, false);
            let mut energy = 0.;
            for _ in 0..8000 {
                let out = fx.process([0.; 2]);
                energy += out[0] * out[0];
            }
            assert_eq!(energy > 0.01, tails, "tails={tails} energy={energy}");
        }
    }
    #[test]
    fn reverb_is_diffuse_and_decays() {
        let mut fx = configured(Kind::Reverb, 0.5);
        let mut early = 0.;
        let mut late = 0.;
        let mut nonzero = 0;
        for i in 0..RATE * 3 {
            let out = fx.process(if i == 0 { [0.5, 0.] } else { [0.; 2] });
            let power = out[0] * out[0] + out[1] * out[1];
            if i < RATE {
                early += power;
                if power > 1e-8 {
                    nonzero += 1;
                }
            } else {
                late += power;
            }
        }
        assert!(early > 0.001 && nonzero > 20, "{early} {nonzero}");
        assert!(late < early * 0.01, "{late} >= {early}");
    }
    #[test]
    fn pitch_shift_moves_tone_one_octave_without_changing_output_length() {
        let mut fx = configured(Kind::PitchShift, 1.);
        let mut target = [0f32; 2];
        let mut original = [0f32; 2];
        for i in 0..RATE * 2 {
            let t = i as f32 / RATE as f32;
            let out = fx.process([(TAU * 220. * t).sin() * 0.5; 2])[0];
            if i >= RATE {
                target[0] += out * (TAU * 440. * t).sin();
                target[1] += out * (TAU * 440. * t).cos();
                original[0] += out * (TAU * 220. * t).sin();
                original[1] += out * (TAU * 220. * t).cos();
            }
        }
        let power = |x: [f32; 2]| x[0] * x[0] + x[1] * x[1];
        assert!(
            power(target) > power(original) * 20. && power(target) > 100.,
            "{target:?} vs {original:?}"
        );
    }
    #[test]
    fn repeat_replays_history_and_brake_reaches_silence() {
        let mut fx = Effect::new(RATE);
        let state = EffectState {
            kind: Kind::BeatRepeat,
            parameter: 0.,
            amount: 1.,
            ..EffectState::default()
        };
        fx.set(state);
        fx.clock(None, true);
        for i in 0..RATE {
            fx.process([i as f32 / RATE as f32; 2]);
        }
        fx.set(EffectState {
            enabled: true,
            ..state
        });
        let mut output = Vec::new();
        for _ in 0..1200 {
            output.push(fx.process([0.; 2])[0]);
        }
        assert!(output[800..].iter().any(|x| *x > 0.8));
        for i in 800..900 {
            assert!((output[i] - output[i + 240]).abs() < 0.001);
        }
        let mut fx = configured(Kind::Brake, 0.);
        // Activation retriggers the brake after warming up its controls.
        fx.set(EffectState {
            kind: Kind::Brake,
            parameter: 0.,
            amount: 1.,
            ..EffectState::default()
        });
        fx.set(EffectState {
            kind: Kind::Brake,
            parameter: 0.,
            amount: 1.,
            enabled: true,
            ..EffectState::default()
        });
        let mut last = 0.;
        let mut early = 0;
        let mut late = 0;
        for i in 0..RATE / 2 {
            let out = fx.process([(TAU * 400. * i as f32 / RATE as f32).sin() * 0.5; 2])[0];
            if last <= 0. && out > 0. {
                if i < 400 {
                    early += 1;
                }
                if (1000..1400).contains(&i) {
                    late += 1;
                }
            }
            if i > 2000 {
                assert!(out.abs() < 0.001);
            }
            last = out;
        }
        assert!(early > late * 2, "{early} vs {late}");
    }
}

#[cfg(test)]
mod level_regressions {
    use super::*;
    fn ready(kind: Kind, parameter: f32) -> Effect {
        let mut fx = Effect::new(48_000);
        fx.set(EffectState {
            kind,
            enabled: true,
            amount: 1.,
            parameter,
            ..Default::default()
        });
        fx.clock(None, true);
        for _ in 0..48_000 {
            fx.process([0.; 2]);
        }
        fx
    }
    #[test]
    fn send_effects_keep_the_dry_attack_before_the_first_return() {
        for kind in [
            Kind::Echo,
            Kind::Delay,
            Kind::PingPong,
            Kind::LowCutEcho,
            Kind::Spiral,
            Kind::Reverb,
        ] {
            let mut fx = ready(kind, 0.5);
            let out = fx.process([0.2, -0.1]);
            assert!(
                (out[0] - 0.2).abs() < 1e-6 && (out[1] + 0.1).abs() < 1e-6,
                "{kind:?}: {out:?}"
            );
        }
    }
    #[test]
    fn filter_sweep_passes_at_inner_endpoints_and_rejects_at_outer_endpoints() {
        let energy = |parameter| {
            let mut fx = ready(Kind::Filter, parameter);
            let mut sum = 0.;
            for i in 0..24_000 {
                let input = 0.2 * (TAU * 1000. * i as f32 / 48_000.).sin();
                let out = fx.process([input; 2])[0];
                if i > 1000 {
                    sum += out * out;
                }
            }
            sum
        };
        let neutral = energy(0.5);
        assert!(energy(0.) < neutral * 0.001);
        assert!(energy(1.) < neutral * 0.001);
        assert!((energy(0.49) / neutral - 1.).abs() < 0.01);
        assert!((energy(0.51) / neutral - 1.).abs() < 0.01);
    }
}

#[cfg(test)]
mod live_sync_tests {
    use super::*;
    #[test]
    fn eight_beat_delay_retains_full_length_at_minimum_tempo() {
        let mut effect = Effect::new(1000);
        effect.set(EffectState {
            kind: Kind::Delay,
            enabled: true,
            amount: 1.,
            beats: 8.,
            ..EffectState::default()
        });
        effect.clock(Some(20.), true);
        for _ in 0..2000 {
            effect.process([0.; 2]);
        }
        effect.process([0.5; 2]);
        let mut peak = (0usize, 0f32);
        for frame in 1..25000 {
            let sample = effect.process([0.; 2])[0].abs();
            if sample > peak.1 {
                peak = (frame, sample);
            }
        }
        assert!(
            (peak.0 as i32 - 24000).abs() <= 2,
            "delay was truncated: {peak:?}"
        );
        assert!(peak.1 > 0.4, "delay lost the impulse: {peak:?}");
    }
    #[test]
    fn timed_effect_defaults_follow_live_tempo_and_manual_override() {
        for kind in Kind::ALL.into_iter().filter(|kind| kind.timed()) {
            let mut effect = Effect::new(48000);
            let state = EffectState {
                kind,
                enabled: true,
                ..EffectState::default()
            };
            assert!(state.sync);
            effect.set(state);
            effect.clock(Some(120.), true);
            assert!((effect.target_period - 0.25).abs() < 1e-6);
            effect.clock(Some(150.), true);
            assert!((effect.target_period - 0.2).abs() < 1e-6);
            for _ in 0..48000 {
                effect.process([0.; 2]);
            }
            assert!((effect.period - 0.2).abs() < 0.001);
            effect.set(EffectState {
                sync: false,
                ..state
            });
            effect.clock(Some(120.), true);
            let manual = effect.target_period;
            effect.clock(Some(150.), true);
            assert_eq!(effect.target_period, manual);
        }
    }
}
