//! Gated stereo RMS matching, deliberately not labelled LUFS or true peak.
/// Returns (gated RMS dBFS, peak dBFS including cubic playback interpolation).
/// This is not an oversampled DAC true-peak measurement. Silence is None.
/// 400 ms blocks with 75% overlap; -70 dBFS absolute / -10 dB relative gates.
pub fn measure_loudness(stereo: &[f32], rate: u32) -> Option<(f32, f32)> {
    if rate == 0 || stereo.len() < 2 || stereo.iter().any(|s| !s.is_finite()) {
        return None;
    }
    let frames = stereo.len() / 2;
    let peak = playback_peak(stereo);
    if peak == 0. {
        return None;
    }
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
        // A quiet track or a brief transient outside the final RMS window still
        // needs a full-track peak report for Auto-Trim.
        let rms = 10. * ((0..frames).map(power).sum::<f64>() / frames as f64).log10();
        return Some((rms.max(-100.) as f32, (20. * peak.log10()).max(-100.)));
    }
    let gate = (audible.iter().sum::<f64>() / audible.len() as f64 * 0.1).max(1e-7);
    let selected: Vec<_> = audible.into_iter().filter(|p| *p >= gate).collect();
    let rms = 10. * (selected.iter().sum::<f64>() / selected.len() as f64).log10();
    Some((rms as f32, 20. * peak.log10()))
}

/// Exact extrema of the engine's Catmull-Rom cubic between adjacent samples.
/// Zero extension matches playback. Background analysis only.
fn playback_peak(stereo: &[f32]) -> f32 {
    let frames = stereo.len() / 2;
    let mut peak = stereo.iter().map(|s| s.abs() as f64).fold(0., f64::max);
    for i in 0..frames {
        for c in 0..2 {
            let p0 = if i == 0 {
                0.
            } else {
                stereo[(i - 1) * 2 + c] as f64
            };
            let p1 = stereo[i * 2 + c] as f64;
            let p2 = if i + 1 < frames {
                stereo[(i + 1) * 2 + c] as f64
            } else {
                0.
            };
            let p3 = if i + 2 < frames {
                stereo[(i + 2) * 2 + c] as f64
            } else {
                0.
            };
            let a = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
            let b = p0 - 2.5 * p1 + 2. * p2 - 0.5 * p3;
            let d = 0.5 * (p2 - p0);
            let mut check = |t: f64| {
                if t > 0. && t < 1. {
                    peak = peak.max((((a * t + b) * t + d) * t + p1).abs());
                }
            };
            if a.abs() < 1e-20 {
                if b.abs() > 1e-20 {
                    check(-d / (2. * b));
                }
            } else {
                let discriminant = 4. * b * b - 12. * a * d;
                if discriminant >= 0. {
                    let root = discriminant.sqrt();
                    check((-2. * b + root) / (6. * a));
                    check((-2. * b - root) / (6. * a));
                }
            }
        }
    }
    peak as f32
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconstructed_peak_includes_cubic_overshoot_on_either_channel() {
        let (_, peak) = measure_loudness(&[0., 0., 0., -1., 0., -1., 0., 0.], 1000).unwrap();
        assert!((peak - 20. * 1.125f32.log10()).abs() < 1e-5);
    }
    #[test]
    fn peak_in_final_partial_window_and_very_quiet_tracks_are_measured() {
        let mut samples = vec![0.; 2050];
        samples[2049] = -0.8;
        let (_, peak) = measure_loudness(&samples, 1000).unwrap();
        assert!((peak - 20. * 0.8f32.log10()).abs() < 1e-5);
        let (_, quiet_peak) = measure_loudness(&vec![0.0001; 4000], 1000).unwrap();
        assert!((quiet_peak - 20. * (0.0001f32 * 29. / 27.).log10()).abs() < 1e-4);
    }
    #[test]
    fn stereo_phase_and_silent_intro_do_not_change_matching() {
        let stereo: Vec<_> = (0..4000).flat_map(|_| [0.1, -0.1]).collect();
        let a = measure_loudness(&stereo, 1000).unwrap();
        assert!((a.0 + 20.).abs() < 0.001);
        assert!((a.1 - 20. * (0.1f32 * 29. / 27.).log10()).abs() < 0.001);
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
