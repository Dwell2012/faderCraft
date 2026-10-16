//! Streaming display analysis on the loader thread, never the audio callback.
//! Complementary splits at 250 Hz and 2.5 kHz; stereo energy avoids phase cancellation.
pub(crate) struct Spectrum {
    alpha: [f32; 2],
    low: [f32; 2],
    mid: [f32; 2],
    energy: [f32; 3],
    frames: usize,
}

impl Spectrum {
    pub(crate) fn new(rate: u32) -> Self {
        Self {
            alpha: [250., 2500.].map(|hz| 1. - (-std::f32::consts::TAU * hz / rate as f32).exp()),
            low: [0.; 2],
            mid: [0.; 2],
            energy: [0.; 3],
            frames: 0,
        }
    }

    pub(crate) fn push(&mut self, stereo: &[f32]) {
        for (ch, &sample) in stereo.iter().enumerate() {
            self.low[ch] += self.alpha[0] * (sample - self.low[ch]);
            let rest = sample - self.low[ch];
            self.mid[ch] += self.alpha[1] * (rest - self.mid[ch]);
            let bands = [self.low[ch], self.mid[ch], rest - self.mid[ch]];
            for (sum, value) in self.energy.iter_mut().zip(bands) {
                *sum += value * value;
            }
        }
        self.frames += 1;
    }

    pub(crate) fn levels(&self) -> [f32; 3] {
        self.energy
            .map(|energy| (energy / (2 * self.frames) as f32).sqrt().clamp(0., 1.))
    }

    pub(crate) fn next_block(&mut self) {
        self.energy = [0.; 3];
        self.frames = 0;
    }
}
