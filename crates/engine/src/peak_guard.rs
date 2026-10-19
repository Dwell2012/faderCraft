//! Stereo-linked lookahead peak protection. Fixed storage; no callback allocation.
pub(crate) const DELAY: usize = 64;
const QUEUE: usize = DELAY + 1;
const CEILING: f32 = 0.9885531; // -0.1 dBFS

pub(crate) struct PeakGuard {
    audio: [[f32; 2]; DELAY],
    peaks: [f32; QUEUE],
    positions: [u64; QUEUE],
    head: usize,
    len: usize,
    frame: u64,
    gain: f32,
    release: f32,
}
impl PeakGuard {
    pub fn new(rate: u32) -> Self {
        Self {
            audio: [[0.; 2]; DELAY],
            peaks: [0.; QUEUE],
            positions: [0; QUEUE],
            head: 0,
            len: 0,
            frame: 0,
            gain: 1.,
            release: 1. - (-1. / (rate.max(1) as f32 * 0.08)).exp(),
        }
    }
    pub fn reset(&mut self) {
        self.audio.fill([0.; 2]);
        self.head = 0;
        self.len = 0;
        self.frame = 0;
        self.gain = 1.;
    }
    pub fn reduction_db(&self) -> f32 {
        20. * self.gain.log10()
    }
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let input = input.map(|s| if s.is_finite() { s } else { 0. });
        let slot = self.frame as usize % DELAY;
        let delayed = self.audio[slot];
        self.audio[slot] = input;
        let oldest = self.frame.saturating_sub(DELAY as u64);
        while self.len > 0 && self.positions[self.head] < oldest {
            self.head = (self.head + 1) % QUEUE;
            self.len -= 1;
        }
        let peak = input[0].abs().max(input[1].abs());
        while self.len > 0 {
            let back = (self.head + self.len - 1) % QUEUE;
            if self.peaks[back] > peak {
                break;
            }
            self.len -= 1;
        }
        let tail = (self.head + self.len) % QUEUE;
        self.peaks[tail] = peak;
        self.positions[tail] = self.frame;
        self.len += 1;
        let target = (CEILING / self.peaks[self.head].max(CEILING)).min(1.);
        self.gain = if target < self.gain {
            target
        } else {
            self.gain + (target - self.gain) * self.release
        };
        self.frame += 1;
        delayed.map(|sample| sample * self.gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quiet_signal_is_delayed_without_change_and_reset_flushes_it() {
        let mut guard = PeakGuard::new(48000);
        for _ in 0..DELAY {
            assert_eq!(guard.process([0.25, -0.125]), [0.; 2]);
        }
        assert_eq!(guard.process([0.25, -0.125]), [0.25, -0.125]);
        guard.reset();
        assert_eq!(guard.process([0.; 2]), [0.; 2]);
    }
    #[test]
    fn peaks_are_bounded_stereo_linked_and_release_recovers() {
        let mut guard = PeakGuard::new(48000);
        for i in 0..12000 {
            let sample = if i % 137 == 0 { -12. } else { 0.25 };
            let [left, right] = guard.process([sample, sample * -0.5]);
            assert!(left.abs() <= CEILING + 1e-6);
            assert!((right + left * 0.5).abs() < 1e-6);
        }
        for _ in 0..48000 {
            guard.process([0.25, -0.125]);
        }
        assert!((guard.process([0.25, -0.125])[0] - 0.25).abs() < 1e-4);
    }
}
