//! Complementary three-band mixer EQ. State and coefficients are preallocated.
pub(crate) struct Eq {
    low: [f32; 2],
    upper: [f32; 2],
    coefficients: [f32; 2],
    pub gains: [f32; 3],
    current: [f32; 3],
    pub trim: f32,
    current_trim: f32,
    smoothing: f32,
}
impl Eq {
    pub fn new(rate: u32) -> Self {
        Self {
            low: [0.; 2],
            upper: [0.; 2],
            coefficients: [250., 2500.]
                .map(|hz| 1. - (-std::f32::consts::TAU * hz / rate as f32).exp()),
            gains: [1.; 3],
            current: [1.; 3],
            trim: 1.,
            current_trim: 1.,
            smoothing: 1. - (-1. / (0.005 * rate as f32)).exp(),
        }
    }
    pub fn reset(&mut self) {
        self.low = [0.; 2];
        self.upper = [0.; 2];
    }
    pub fn process(&mut self, input: &[f32]) -> [f32; 2] {
        self.current_trim += (self.trim - self.current_trim) * self.smoothing;
        for i in 0..3 {
            self.current[i] += (self.gains[i] - self.current[i]) * self.smoothing;
        }
        std::array::from_fn(|c| {
            let sample = input[c] * self.current_trim;
            self.low[c] += self.coefficients[0] * (sample - self.low[c]);
            self.upper[c] += self.coefficients[1] * (sample - self.upper[c]);
            let bands = [
                self.low[c],
                self.upper[c] - self.low[c],
                sample - self.upper[c],
            ];
            bands
                .iter()
                .zip(self.current)
                .map(|(sample, gain)| sample * gain)
                .sum()
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn energy(frequency: f32, gains: [f32; 3], trim: f32) -> f32 {
        let mut eq = Eq::new(48000);
        eq.gains = gains;
        eq.trim = trim;
        let mut total = 0.;
        for i in 0..48000 {
            let sample = (std::f32::consts::TAU * frequency * i as f32 / 48000.).sin() * 0.1;
            let output = eq.process(&[sample, -sample]);
            assert!((output[0] + output[1]).abs() < 1e-6);
            if i > 24000 {
                total += output[0] * output[0];
            }
        }
        total.sqrt()
    }
    #[test]
    fn neutral_trim_kill_and_independent_bands() {
        for (band, frequency) in [20., 800., 16000.].into_iter().enumerate() {
            let neutral = energy(frequency, [1.; 3], 1.);
            assert!((energy(frequency, [1.; 3], 2.) / neutral - 2.).abs() < 0.001);
            assert!(energy(frequency, [0.; 3], 1.) < 0.0001);
            let mut gains = [1.; 3];
            gains[band] = 0.;
            assert!(energy(frequency, gains, 1.) < neutral * 0.55, "band {band}");
            gains[band] = 2.;
            assert!(energy(frequency, gains, 1.) > neutral * 1.5, "band {band}");
        }
    }
}
