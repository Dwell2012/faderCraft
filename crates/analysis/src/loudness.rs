//! Gated stereo RMS matching, deliberately not labelled LUFS or true peak.
/// Returns (gated RMS dBFS, sample peak dBFS). Silence is None.
/// 400 ms blocks with 75% overlap; -70 dBFS absolute / -10 dB relative gates.
pub fn measure_loudness(stereo: &[f32], rate: u32) -> Option<(f32, f32)> {
    if rate == 0 || stereo.len() < 2 || stereo.iter().any(|s| !s.is_finite()) {
        return None;
    }
    let frames = stereo.len() / 2;
    let window = ((rate as usize * 4) / 10).max(1).min(frames);
    let hop = (rate as usize / 10).max(1);
    let power = |i: usize| {
        let a = stereo[i * 2] as f64;
        let b = stereo[i * 2 + 1] as f64;
        (a * a + b * b) * 0.5
    };
    let mut sum = (0..window).map(power).sum::<f64>();
    let mut levels = Vec::new();
    let mut start = 0;
    loop {
        levels.push((sum / window as f64).max(0.));
        if start + window + hop > frames {
            break;
        }
        for i in start..start + hop {
            sum -= power(i);
            sum += power(i + window);
        }
        start += hop;
    }
    let audible: Vec<_> = levels.iter().copied().filter(|p| *p >= 1e-7).collect();
    if audible.is_empty() {
        return None;
    }
    let gate = (audible.iter().sum::<f64>() / audible.len() as f64 * 0.1).max(1e-7);
    let selected: Vec<_> = audible.into_iter().filter(|p| *p >= gate).collect();
    let rms = 10. * (selected.iter().sum::<f64>() / selected.len() as f64).log10();
    let peak = stereo.iter().map(|s| s.abs()).fold(0f32, f32::max);
    Some((rms as f32, 20. * peak.log10()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stereo_phase_and_silent_intro_do_not_change_matching() {
        let stereo: Vec<_> = (0..4000).flat_map(|_| [0.1, -0.1]).collect();
        let a = measure_loudness(&stereo, 1000).unwrap();
        assert!((a.0 + 20.).abs() < 0.001);
        assert!((a.1 + 20.).abs() < 0.001);
        let mut padded = vec![0.; 8000];
        padded.extend_from_slice(&stereo);
        let b = measure_loudness(&padded, 1000).unwrap();
        assert!((a.0 - b.0).abs() < 0.5);
        let louder: Vec<_> = stereo.iter().map(|v| v * 2.).collect();
        assert!((measure_loudness(&louder, 1000).unwrap().0 - a.0 - 6.0206).abs() < 0.001);
        assert!(measure_loudness(&vec![0.; 8000], 1000).is_none());
    }
    #[test]
    fn peak_retains_short_transients_while_gate_ignores_silence() {
        let mut samples = vec![0.1; 8000];
        samples[11] = 1.;
        let (_, peak) = measure_loudness(&samples, 1000).unwrap();
        assert_eq!(peak, 0.);
        assert!(measure_loudness(&[f32::NAN, 0.], 1000).is_none());
    }
}
