//! Vocal activity estimates from an already separated stereo vocal stem.
/// Empty regions mean analysed silence; absent stems must be represented separately.
/// 100 ms RMS windows, an absolute/relative noise gate, short-gap bridging,
/// minimum burst length and conservative padding. This is not speech recognition.
pub fn detect_vocal_regions(stereo: &[f32], rate: u32) -> Vec<[f64; 2]> {
    if rate == 0 || stereo.is_empty() {
        return Vec::new();
    }
    let chunk = ((rate as usize / 10).max(1)) * 2;
    let power: Vec<f64> = stereo
        .chunks(chunk)
        .map(|s| s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / s.len() as f64)
        .collect();
    let peak = power.iter().copied().fold(0., f64::max);
    let threshold = (peak * 0.01).max(0.00001); // -20 dB relative power / -50 dBFS absolute gate
    let step = chunk as f64 / (rate as f64 * 2.);
    let duration = stereo.len() as f64 / (rate as f64 * 2.);
    let mut regions: Vec<[f64; 2]> = Vec::new();
    let mut start = None;
    let mut last = 0;
    for (i, &energy) in power.iter().enumerate() {
        if energy >= threshold {
            start.get_or_insert(i);
            last = i;
        }
        if start.is_some() && (i - last >= 3 || i + 1 == power.len()) {
            let first = start.take().unwrap();
            if last + 1 - first >= 3 {
                let range = [
                    (first as f64 * step - 0.15).max(0.),
                    ((last + 1) as f64 * step + 0.2).min(duration),
                ];
                if let Some(previous) = regions.last_mut().filter(|p| p[1] >= range[0]) {
                    previous[1] = range[1];
                } else {
                    regions.push(range);
                }
            }
        }
    }
    regions
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_silence_and_brief_bleed_but_keeps_stereo_vocals() {
        let mut samples = vec![0.; 1000 * 2 * 5];
        assert!(detect_vocal_regions(&samples, 1000).is_empty());
        for frame in 1000..2000 {
            samples[frame * 2] = 0.2;
            samples[frame * 2 + 1] = -0.2;
        }
        for frame in 3000..3100 {
            samples[frame * 2] = 0.1;
        }
        let regions = detect_vocal_regions(&samples, 1000);
        assert_eq!(regions.len(), 1);
        assert!(regions[0][0] <= 1. && regions[0][1] >= 2.);
    }
    #[test]
    fn bridges_breaths_and_keeps_final_burst() {
        let mut samples = vec![0.; 1000 * 2 * 3];
        for frame in 1000..3000 {
            if !(1500..1700).contains(&frame) {
                samples[frame * 2] = 0.2;
            }
        }
        let regions = detect_vocal_regions(&samples, 1000);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0][1], 3.);
    }
}
