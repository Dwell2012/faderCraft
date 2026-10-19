use crate::Track;
use signalsmith_stretch::Stretch;

/// Measure the complete time-stretched track off the audio thread, before trim/EQ/protection.
pub fn measure_key_lock_peak(track: &Track, ratio: f64) -> Option<f32> {
    if !track.is_complete() || !(crate::MIN_TEMPO..=crate::MAX_TEMPO).contains(&ratio) {
        return None;
    }
    let samples = track.export_stereo();
    let mut stretch = Stretch::preset_default(2, track.sample_rate);
    // Match playback's latency-compensated start, including the cue's audio.
    let first = stretch.input_latency();
    let prefix: Vec<_> = (0..first * 2)
        .map(|i| samples.get(i).copied().unwrap_or(0.))
        .collect();
    stretch.seek(&prefix, ratio);
    let wanted = stretch.output_latency() as f64 * ratio;
    let second = wanted as usize;
    let preroll: Vec<_> = (0..second * 2)
        .map(|i| samples.get(first * 2 + i).copied().unwrap_or(0.))
        .collect();
    let mut discard = vec![0.; stretch.output_latency() * 2];
    stretch.process(&preroll, &mut discard);
    let mut input = vec![0.; (512. * ratio).ceil() as usize * 2 + 2];
    let mut output = [0.; 1024];
    let total = samples.len() / 2
        + stretch.input_latency() as usize
        + (stretch.output_latency() as f64 * ratio).ceil() as usize
        + 1024;
    let mut position = first + second;
    let mut carry = wanted - second as f64;
    let mut peak = 0f32;
    while position < total {
        let wanted = 512. * ratio + carry;
        let count = wanted as usize;
        carry = wanted - count as f64;
        for (i, sample) in input[..count * 2].iter_mut().enumerate() {
            *sample = samples.get(position * 2 + i).copied().unwrap_or(0.);
        }
        position += count;
        stretch.process(&input[..count * 2], &mut output[..]);
        for sample in output {
            peak = peak.max(sample.abs());
        }
    }
    (peak > 0.).then(|| 20. * peak.log10())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn includes_late_transient_and_rejects_invalid_tempo() {
        let mut samples = vec![0.; 48000 * 4];
        let end = samples.len();
        samples[end - 400..].fill(2.);
        let track = Track::from_samples("late peak", 48000, samples);
        let peak = measure_key_lock_peak(&track, 1.04).unwrap();
        assert!(peak > 0., "late peak was missed: {peak}");
        assert!(measure_key_lock_peak(&track, f64::NAN).is_none());
    }
}
