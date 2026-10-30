//! Conservative bass-attack cues for waveform display, independent of a grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BassAttack {
    pub index: usize,
    pub strength: f32,
}

/// Input is a uniformly spaced, nonnegative low-band RMS envelope. This runs
/// on the display/control side, never the audio callback. Sharp bass notes can
/// also qualify; this is not instrument separation or a kick classifier.
pub fn bass_attacks(levels: &[f32], seconds_per_level: f64) -> Vec<BassAttack> {
    if levels.len() < 3 || !seconds_per_level.is_finite() || seconds_per_level <= 0. {
        return Vec::new();
    }
    let steps = |seconds: f64| {
        ((seconds / seconds_per_level).round() as usize)
            .max(1)
            .min(levels.len())
    };
    let fast = steps(0.02);
    let gap = steps(0.02);
    let history = steps(0.08);
    let refractory = steps(0.14);
    let mut prefix = Vec::with_capacity(levels.len() + 1);
    prefix.push(0f64);
    let mut peak = 0f32;
    for &level in levels {
        let level = if level.is_finite() { level.max(0.) } else { 0. };
        peak = peak.max(level);
        prefix.push(prefix.last().unwrap() + f64::from(level).powi(2));
    }
    let rms = |from: usize, to: usize| {
        ((prefix[to] - prefix[from]) / (to - from).max(1) as f64)
            .max(0.)
            .sqrt() as f32
    };
    let mut scores = vec![0.; levels.len()];
    for i in history + gap..levels.len() {
        let level = rms((i + 1).saturating_sub(fast), i + 1);
        let baseline = rms(i - gap - history, i - gap);
        let rise = (level - baseline).max(0.);
        let contrast = rise / (level + 0.001);
        if rise > (peak * 0.06).max(0.004) && contrast > 0.25 {
            scores[i] = ((contrast - 0.2) / 0.8).clamp(0., 1.);
        }
    }
    let mut attacks: Vec<BassAttack> = Vec::new();
    for i in 1..scores.len() - 1 {
        if scores[i] <= 0. || scores[i] <= scores[i - 1] || scores[i] < scores[i + 1] {
            continue;
        }
        // Locate the attack edge rather than painting the envelope's later peak.
        let first = i.saturating_sub(steps(0.04));
        let baseline = rms(i.saturating_sub(gap + history), i.saturating_sub(gap));
        let threshold = baseline + (levels[i].max(0.) - baseline).max(0.) * 0.25;
        let index = (first..=i).find(|&n| levels[n] >= threshold).unwrap_or(i);
        let attack = BassAttack {
            index,
            strength: scores[i],
        };
        if let Some(previous) = attacks.last_mut()
            && index.saturating_sub(previous.index) < refractory
        {
            if attack.strength > previous.strength {
                *previous = attack;
            }
            continue;
        }
        attacks.push(attack);
    }
    attacks
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kicks_stop_at_break_while_sustained_bass_continues() {
        for dt in [0.005, 0.01] {
            let mut levels = vec![0.15; (4. / dt) as usize];
            for time in [0.5, 1., 1.5] {
                let first = (time / dt) as usize;
                for i in 0..(0.09 / dt) as usize {
                    levels[first + i] += 0.6 * (-(i as f64) * dt / 0.025).exp() as f32;
                }
            }
            let hits = bass_attacks(&levels, dt);
            assert_eq!(hits.len(), 3, "{dt}: {hits:?}");
            for (hit, time) in hits.iter().zip([0.5, 1., 1.5]) {
                assert!((hit.index as f64 * dt - time).abs() <= 0.02);
            }
            assert!(hits.iter().all(|hit| hit.index as f64 * dt < 2.));
        }
    }
    #[test]
    fn sustained_and_slowly_rising_bass_does_not_create_a_beat_grid() {
        assert!(bass_attacks(&vec![0.3; 1000], 0.005).is_empty());
        let ramp: Vec<_> = (0..1000).map(|i| 0.2 + i as f32 * 0.0005).collect();
        assert!(bass_attacks(&ramp, 0.005).is_empty());
        assert!(bass_attacks(&[0.; 1000], 0.005).is_empty());
        assert!(bass_attacks(&[f32::NAN; 1000], 0.005).is_empty());
    }
}
