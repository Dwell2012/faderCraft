//! Tempo and beat-grid detection for music with a steady beat.
//!
//! The track is reduced to an onset envelope (how much new energy arrives in
//! each short slice of time). The envelope's autocorrelation gives a rough
//! tempo; folding the whole envelope at nearby beat lengths then finds the
//! tempo at which beats stay lined up from start to finish. The first confirmed
//! kick run anchors phase locally, projecting back through the intro; bass and
//! broadband folding provide a fallback. Last, bar starts come from accents and
//! changes in loudness fall.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

/// Samples per analysis window.
const FRAME: usize = 1024;
/// The envelope has one value per this many seconds, whatever the sample
/// rate of the audio.
const ENVELOPE_STEP_SECONDS: f64 = 0.005;
/// Tracks shorter than this have too few beats to measure.
const MIN_SECONDS: f64 = 5.0;
/// The tempo listeners most often tap along to; used only to choose between
/// a tempo and its half or double.
const PREFERRED_BPM: f64 = 125.0;
/// Slices one beat is divided into when looking for where beats fall.
const PHASE_BINS: usize = 64;
/// Beats are placed by the bass drum, so their timing comes from below this
/// frequency when there is enough going on down there.
const BASS_LIMIT_HZ: f64 = 200.0;
/// How sharply bass onsets must gather before they are trusted for timing.
const BASS_CONFIDENCE: f32 = 0.15;
/// A drum hit registers as an onset this long after the middle of the
/// analysis window reaches it. Measured with synthetic drum hits at known
/// positions (see the tests); real kicks vary by a few milliseconds.
const ONSET_LAG_SECONDS: f64 = 0.015;
/// Beats in a bar. Music in other time signatures still gets a beat grid,
/// but its bar lines will be wrong.
pub const BEATS_PER_BAR: usize = 4;
/// How far either side of a beat its accent is looked for.
const ACCENT_REACH_SECONDS: f64 = 0.015;

/// The tempos a track may be reported at. A beat at 70 BPM is also a beat at
/// 140, so the range decides which is meant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoRange {
    pub min_bpm: f64,
    pub max_bpm: f64,
}

impl Default for TempoRange {
    fn default() -> Self {
        Self {
            min_bpm: 70.0,
            max_bpm: 180.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tempo {
    pub bpm: f64,
    /// Time of the first beat. Later beats follow every `60 / bpm` seconds.
    pub first_beat_seconds: f64,
    /// Which of the first [`BEATS_PER_BAR`] beats starts a bar, counting
    /// the first beat as 0.
    pub downbeat: u8,
    /// 0.0 to 1.0: how sharply onsets gather on the beat.
    pub confidence: f32,
}

impl Tempo {
    pub fn beat_seconds(&self) -> f64 {
        60.0 / self.bpm
    }

    /// Time of the first beat that starts a bar.
    pub fn first_bar_seconds(&self) -> f64 {
        self.first_beat_seconds + self.downbeat as f64 * self.beat_seconds()
    }
}

fn downbeat_opening(
    samples: &[f32],
    rate: u32,
    first: f64,
    period: f64,
    end: usize,
    fallback: u8,
) -> u8 {
    let mut scores = [0.; 4];
    let mut counts = [0; 4];
    for beat in 0..32 {
        let start = ((first + beat as f64 * period) * rate as f64) as usize;
        let stop = (start + (0.08 * rate as f64) as usize).min(end);
        if start >= stop {
            break;
        }
        // Transient brightness distinguishes a crash/accent from otherwise
        // identical bass kicks without letting their bass energy drown it out.
        scores[beat % 4] += samples[start..stop]
            .windows(2)
            .map(|w| (w[1] as f64 - w[0] as f64).powi(2))
            .sum::<f64>()
            / (stop - start) as f64;
        counts[beat % 4] += 1;
    }
    if counts.iter().any(|&n| n < 4) {
        return fallback;
    }
    for i in 0..4 {
        scores[i] /= counts[i] as f64;
    }
    let best = (0..4)
        .max_by(|&a, &b| scores[a].total_cmp(&scores[b]))
        .unwrap();
    let mean = scores.iter().sum::<f64>() / 4.;
    // Require agreement with the whole-track evidence before treating an
    // opening accent as a pickup. Otherwise use the first supported kick.
    if best == fallback as usize && scores[best] > mean * 1.2 {
        best as u8
    } else {
        0
    }
}

/// Refine the spectral phase against sharp time-domain attacks. Require a
/// repeated run near the measured beat phase, rather than treating the first
/// nonzero sample (which may be noise or a pickup) as a beat.
fn opening_attack(samples: &[f32], sample_rate: u32, phase: f64, period: f64) -> Option<f64> {
    let hop = (sample_rate as usize / 1000).max(1);
    let dt = hop as f64 / sample_rate as f64;
    let energy: Vec<f64> = samples
        .chunks(hop)
        .take((128. * period / dt) as usize)
        .map(|c| (c.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / c.len() as f64).sqrt())
        .collect();
    if energy.len() < 20 {
        return None;
    }
    let smooth: Vec<f64> = (0..energy.len())
        .map(|i| energy[i.saturating_sub(4)..=i].iter().sum::<f64>() / (i.min(4) + 1) as f64)
        .collect();
    let peak = smooth.iter().copied().fold(0., f64::max);
    if peak < 1e-4 {
        return None;
    }
    let reach = (0.08 / dt) as usize;
    let mut attacks = Vec::new();
    for beat in 0..128 {
        let expected = phase + beat as f64 * period;
        let centre = (expected / dt) as usize;
        let from = centre.saturating_sub(reach);
        let to = (centre + reach).min(smooth.len().saturating_sub(1));
        if from >= to {
            continue;
        }
        let rise = |i: usize| smooth[i] - smooth[i.saturating_sub(10)];
        let at = (from..=to).max_by(|&a, &b| rise(a).total_cmp(&rise(b)))?;
        if rise(at) < peak * 0.15 {
            continue;
        }
        let baseline = smooth[at.saturating_sub(20)..=at.saturating_sub(10)]
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let threshold = baseline + (smooth[at] - baseline) * 0.1;
        let onset = (at.saturating_sub(25)..=at)
            .rev()
            .find(|&i| smooth[i] <= threshold)
            .map_or(at.saturating_sub(25), |i| i + 1);
        attacks.push((beat, onset as f64 * dt, onset as f64 * dt - expected));
    }
    if attacks.len() < 8 {
        return None;
    }
    let mut offsets: Vec<_> = attacks.iter().map(|a| a.2).collect();
    offsets.sort_by(f64::total_cmp);
    let median = offsets[offsets.len() / 2];
    let consistent: Vec<_> = attacks
        .iter()
        .filter(|a| (a.2 - median).abs() < 0.012)
        .collect();
    if consistent.len() * 4 < attacks.len() * 3 {
        return None;
    }
    let mut first = consistent.first()?.0;
    // A first kick can have a different attack shape (fade-in, crash or
    // pickup) from the steady run. Keep the strict consensus for phase,
    // but recover adjacent strong opening attacks within 25 ms of it.
    // Do not jump across a missing beat into unrelated intro material.
    while first > 0 {
        if attacks
            .iter()
            .any(|a| a.0 + 1 == first && (a.2 - median).abs() < 0.025)
        {
            first -= 1;
        } else {
            break;
        }
    }
    // Keep the absolute first supported beat; do not wrap it back into silence.
    Some((phase + first as f64 * period + median).max(0.))
}

/// Finds the tempo and beat positions of mono audio, assuming the tempo does
/// not change. Returns `None` for audio that is too short or has no rhythm
/// to measure.
pub fn detect_tempo(samples: &[f32], sample_rate: u32, range: TempoRange) -> Option<Tempo> {
    let seconds = samples.len() as f64 / sample_rate as f64;
    if seconds < MIN_SECONDS || range.min_bpm <= 0.0 || range.max_bpm <= range.min_bpm {
        return None;
    }
    let hop = ((sample_rate as f64 * ENVELOPE_STEP_SECONDS).round() as usize).max(1);
    let rate = sample_rate as f64 / hop as f64;
    let (envelope, bass, bass_strength, kicks) = onset_envelopes(samples, sample_rate, hop);
    if envelope.iter().all(|value| *value <= 0.0) {
        return None;
    }

    let coarse = coarse_tempo(&envelope, rate, range)?;
    // Sweep around the rough answer, then around the best of that sweep.
    let (bpm, _) = best_fold(&envelope, rate, coarse, 1.5, 0.01);
    let (bpm, fold) = best_fold(&envelope, rate, bpm, 0.01, 0.001);

    // Log compression is useful for tempo, but can let a quiet offbeat bass
    // note outvote the kick. Locate the strong bass attack without compression,
    // then refine its timing using log flux in that neighbourhood.
    let strength_fold = self::fold(&bass_strength, rate, bpm);
    let bass_fold = if strength_fold.confidence >= BASS_CONFIDENCE {
        fold_near(
            &bass,
            rate,
            bpm,
            Some((strength_fold.phase, 0.060 * bpm / 60.0)),
        )
    } else {
        self::fold(&bass, rate, bpm)
    };
    let period = 60.0 / bpm;
    let window_centre = FRAME as f64 / 2.0 / sample_rate as f64;
    let anchor = kick_anchor(&kicks, rate, bpm);
    let mut kick_entrance = false;
    let (first_beat, opening) = if let Some(anchor) = anchor {
        let expected = anchor + window_centre + ONSET_LAG_SECONDS;
        // Refine only the confirmed kick run. The melodic intro must not move
        // this anchor back onto its own offbeat attacks afterwards.
        let from = ((expected - 0.1).max(0.) * sample_rate as f64) as usize;
        let to =
            (((expected + 16. * period + 0.1) * sample_rate as f64) as usize).min(samples.len());
        let start = from as f64 / sample_rate as f64;
        let refined = opening_attack(&samples[from..to], sample_rate, expected - start, period)
            .map(|attack| start + attack)
            .unwrap_or(expected);
        // A confirmed kick entrance after a quiet lead-in anchors beat one.
        // Keep the actual attack as origin; renderers can count backwards
        // from it instead of treating quiet pre-roll as the first beat.
        kick_entrance = quiet_before_kick(samples, sample_rate, refined, period);
        let first = if kick_entrance {
            refined
        } else {
            project_kick_back(samples, sample_rate, refined, period)
        };
        (
            first,
            (refined - first <= 2. * period + 0.025).then_some(refined),
        )
    } else {
        let phase = beat_phase(&bass_fold, &strength_fold, &fold);
        let phase_beat = (phase * period + window_centre + ONSET_LAG_SECONDS).rem_euclid(period);
        let opening = opening_attack(samples, sample_rate, phase_beat, period);
        (opening.unwrap_or(phase_beat), opening)
    };
    // Where on the envelope something heard at `seconds` shows.
    let envelope_index = |seconds: f64| (seconds - window_centre - ONSET_LAG_SECONDS) * rate;
    let mut downbeat = downbeat(
        samples,
        sample_rate,
        &envelope,
        envelope_index,
        first_beat,
        period,
    );
    // With a clean percussive opening, equal kicks do not establish an
    // alternative bar phase. Prefer the first actual attack in that case.
    if let Some(opening) = opening.filter(|_| first_beat > 0.05) {
        // A quiet lead-in can project the grid before the first actual kick.
        // Score the opening from that confirmed attack, then translate its
        // bar position back to the grid origin. Never score quiet pre-roll as
        // if it were the start of the kick pattern.
        let offset = ((opening - first_beat) / period).round().max(0.) as u8 % 4;
        let intro_end = ((opening + 32. * period) * sample_rate as f64) as usize;
        downbeat = (downbeat_opening(
            samples,
            sample_rate,
            opening,
            period,
            intro_end.min(samples.len()),
            (downbeat + 4 - offset) % 4,
        ) + offset)
            % 4;
    }
    if kick_entrance {
        downbeat = 0;
    }
    Some(Tempo {
        bpm,
        first_beat_seconds: first_beat,
        downbeat,
        confidence: fold.confidence,
    })
}

/// Which of the first [`BEATS_PER_BAR`] beats starts a bar. Two things mark
/// it: drums and new sounds tend to hit hardest at the start of a bar, and
/// sections (a break, a drop) begin on one. Each candidate is scored on both,
/// as a share of what the average candidate scores, so neither drowns out
/// the other.
fn downbeat(
    samples: &[f32],
    sample_rate: u32,
    envelope: &[f32],
    envelope_index: impl Fn(f64) -> f64,
    first_beat: f64,
    period: f64,
) -> u8 {
    let seconds = samples.len() as f64 / sample_rate as f64;
    let beats = ((seconds - first_beat) / period).floor().max(0.0) as usize;
    if beats < 2 * BEATS_PER_BAR {
        return 0;
    }
    let beat_time = |beat: usize| first_beat + beat as f64 * period;

    // How hard each beat hits.
    let reach = (ACCENT_REACH_SECONDS / ENVELOPE_STEP_SECONDS).ceil() as usize;
    let accent: Vec<f64> = (0..beats)
        .map(|beat| {
            let centre = envelope_index(beat_time(beat)).round();
            if centre < 0.0 {
                return 0.0;
            }
            let centre = centre as usize;
            envelope
                .get(centre.saturating_sub(reach)..(centre + reach + 1).min(envelope.len()))
                .map_or(0.0, |near| near.iter().copied().fold(0.0, f32::max) as f64)
        })
        .collect();

    // How loud each beat is, and how much that changes a bar either side.
    let loudness: Vec<f64> = (0..beats)
        .map(|beat| {
            let from = (beat_time(beat) * sample_rate as f64) as usize;
            let to = ((beat_time(beat + 1) * sample_rate as f64) as usize).min(samples.len());
            let part = &samples[from.min(to)..to];
            let power: f64 = part.iter().map(|s| (*s as f64).powi(2)).sum();
            10.0 * (power / part.len().max(1) as f64 + 1e-12).log10()
        })
        .collect();
    let mean = |from: usize, to: usize| loudness[from..to].iter().sum::<f64>() / (to - from) as f64;
    let change: Vec<f64> = (0..beats)
        .map(|beat| {
            if beat < BEATS_PER_BAR || beat + BEATS_PER_BAR > beats {
                return 0.0;
            }
            (mean(beat, beat + BEATS_PER_BAR) - mean(beat - BEATS_PER_BAR, beat)).abs()
        })
        .collect();

    let scores = |values: &[f64]| -> [f64; BEATS_PER_BAR] {
        let mut sums = [0.0; BEATS_PER_BAR];
        for (beat, value) in values.iter().enumerate() {
            sums[beat % BEATS_PER_BAR] += value;
        }
        let average = sums.iter().sum::<f64>() / BEATS_PER_BAR as f64;
        if average > 0.0 {
            sums.map(|sum| sum / average)
        } else {
            [0.0; BEATS_PER_BAR]
        }
    };
    let (accent, change) = (scores(&accent), scores(&change));
    (0..BEATS_PER_BAR)
        .max_by(|a, b| (accent[*a] + change[*a]).total_cmp(&(accent[*b] + change[*b])))
        .unwrap_or(0) as u8
}

/// Spectral flux: for each step, the total rise in level across frequency
/// bands since the previous step, with the slowly varying average removed.
/// Returns full-spectrum log flux, bass log flux, uncompressed bass flux,
/// and transient evidence for a kick's descending low-frequency pitch.
fn onset_envelopes(
    samples: &[f32],
    sample_rate: u32,
    hop: usize,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FRAME as f32).cos())
        .collect();
    let mut buffer = vec![Complex::new(0.0, 0.0); FRAME];
    let mut scratch = vec![Complex::new(0.0, 0.0); fft.get_inplace_scratch_len()];
    let bins = FRAME / 2;
    let mut previous = vec![0f32; bins];
    let mut previous_magnitude = vec![0f32; bins];

    let bass_bins =
        ((BASS_LIMIT_HZ * FRAME as f64 / sample_rate as f64).ceil() as usize).clamp(1, bins);

    let frames = samples.len().saturating_sub(FRAME) / hop + 1;
    let mut flux = Vec::with_capacity(frames);
    let mut bass_flux = Vec::with_capacity(frames);
    let mut bass_strength = Vec::with_capacity(frames);
    let mut kick_rise = Vec::with_capacity(frames);
    let mut bass_centroid = Vec::with_capacity(frames);
    for frame in 0..frames {
        let start = frame * hop;
        for (slot, (sample, weight)) in buffer
            .iter_mut()
            .zip(samples[start..start + FRAME].iter().zip(&window))
        {
            *slot = Complex::new(sample * weight, 0.0);
        }
        fft.process_with_scratch(&mut buffer, &mut scratch);
        let (mut rise, mut bass_rise, mut strength) = (0.0, 0.0, 0.0);
        let (mut kick, mut power, mut weighted) = (0., 0., 0.);
        for (index, (bin, last)) in buffer[..bins].iter().zip(previous.iter_mut()).enumerate() {
            // Compressing the level stops loud bass from drowning out
            // quieter but sharper sounds such as hi-hats.
            let magnitude = bin.norm();
            let frequency = index as f32 * sample_rate as f32 / FRAME as f32;
            if frequency < 250. {
                let energy = magnitude * magnitude;
                power += energy;
                weighted += energy * frequency;
            }
            if index > 0 && frequency < BASS_LIMIT_HZ as f32 {
                kick += (magnitude - previous_magnitude[index]).max(0.);
            }
            let level = (1.0 + 100.0 * magnitude).ln();
            let change = (level - *last).max(0.0);
            rise += change;
            if index < bass_bins {
                bass_rise += change;
                strength += (magnitude - previous_magnitude[index]).max(0.0);
            }
            *last = level;
            previous_magnitude[index] = magnitude;
        }
        flux.push(rise);
        bass_flux.push(bass_rise);
        bass_strength.push(strength);
        kick_rise.push(kick);
        bass_centroid.push(weighted / power.max(1e-9));
    }
    // A kick's initial pitch sweep falls into the sub-bass over ~30ms.
    // Sustained or harmonically rich bass attacks do not get this extra vote.
    let kicks = (0..frames)
        .map(|i| {
            let drop = bass_centroid[i.saturating_sub(3)] - bass_centroid[(i + 6).min(frames - 1)];
            if (90. ..170.).contains(&bass_centroid[i]) {
                kick_rise[i] * (drop - 15.).max(0.) / 100.
            } else {
                0.
            }
        })
        .collect();
    (
        above_local_average(flux),
        above_local_average(bass_flux),
        above_local_average(bass_strength),
        kicks,
    )
}

/// A sustained quiet lead-in followed by a confirmed kick run is an entrance,
/// not evidence of a pickup or a louder backbeat later in the bar.
fn quiet_before_kick(samples: &[f32], rate: u32, first: f64, period: f64) -> bool {
    if first < 2. * period {
        return false;
    }
    let power = |from: f64, to: f64| {
        let start = (from.max(0.) * rate as f64) as usize;
        let end = ((to * rate as f64) as usize).min(samples.len());
        let part = &samples[start.min(end)..end];
        part.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / part.len().max(1) as f64
    };
    let before = power(first - 2. * period, first - 0.025);
    let kick = power(first, first + 0.08);
    kick > 1e-6 && before < kick * 0.04
}

/// Project a confirmed kick through an intro without extending into silence.
fn project_kick_back(samples: &[f32], sample_rate: u32, anchor: f64, period: f64) -> f64 {
    let hop = (sample_rate as usize / 200).max(1);
    let energy: Vec<_> = samples
        .chunks(hop)
        .map(|part| {
            (part
                .iter()
                .map(|&sample| (sample as f64).powi(2))
                .sum::<f64>()
                / part.len() as f64)
                .sqrt()
        })
        .collect();
    let peak = energy.iter().copied().fold(0., f64::max);
    let sound = energy
        .iter()
        .position(|&value| value > peak * 0.05)
        .unwrap_or(0) as f64
        * hop as f64
        / sample_rate as f64;
    let phase = anchor.rem_euclid(period);
    // Project through a musical intro, but do not invent beats in leading
    // silence. Allow the short attack ramp after the exact beat timestamp.
    phase + ((sound - 0.03 - phase) / period).ceil().max(0.) * period
}

/// Find the first solid kick run independently of the whole-track phase.
fn kick_anchor(kicks: &[f32], rate: f64, bpm: f64) -> Option<f64> {
    let period = 60. / bpm;
    let peak = kicks.iter().copied().fold(0., f32::max);
    if peak <= 1e-6 {
        return None;
    }
    let reach = (0.015 * rate).ceil() as usize;
    let candidates = (1..kicks.len().saturating_sub(1))
        .filter(|&i| kicks[i] >= peak * 0.2 && kicks[i] >= kicks[i - 1] && kicks[i] > kicks[i + 1]);
    // Earliest confirmed run wins. A louder section later in the track must
    // not choose a different phase from the first clean kick entrance.
    for anchor in candidates {
        // Kick cadence is independent of the estimated beat tempo.
        for cadence in [1., 2.] {
            let strength_at = |beat: i32| {
                let at = anchor as f64 + beat as f64 * period * cadence * rate;
                if at < 0. || at >= kicks.len() as f64 {
                    return 0.;
                }
                let at = at.round() as usize;
                kicks[at.saturating_sub(reach)..(at + reach + 1).min(kicks.len())]
                    .iter()
                    .copied()
                    .fold(0., f32::max)
            };
            let levels: [f32; 9] = std::array::from_fn(|beat| strength_at(beat as i32));
            if levels.iter().filter(|&&value| value > peak * 0.08).count() < 8 {
                continue;
            }
            let mut sorted = levels;
            sorted.sort_by(f32::total_cmp);
            let typical = sorted[4];
            let similar = |a: f32, b: f32| a.min(b) >= a.max(b) * 0.5;
            // Reject large level changes on either side, including a loud final
            // hit of a build. Silence before a clean entrance is allowed.
            let previous = strength_at(-1);
            if !similar(levels[0], typical)
                || (previous > peak * 0.08 && !similar(previous, levels[0]))
                || levels.windows(2).any(|pair| !similar(pair[0], pair[1]))
            {
                continue;
            }
            // A roll can also land on every expected beat. Check between beats
            // for extra pitch-sweeping kicks of comparable strength. Weaker
            // offbeat bass/ghost hits must not disqualify the kick pattern.
            let spacing = period * cadence * rate;
            let margin = (0.03 * rate).max(reach as f64);
            let extra_kicks = (-1..8).any(|beat| {
                let from = (anchor as f64 + beat as f64 * spacing + margin).max(1.) as usize;
                let to = ((anchor as f64 + (beat + 1) as f64 * spacing - margin).max(0.) as usize)
                    .min(kicks.len().saturating_sub(1));
                (from..to).any(|i| {
                    kicks[i] > typical * 0.8 && kicks[i] >= kicks[i - 1] && kicks[i] > kicks[i + 1]
                })
            });
            if !extra_kicks {
                return Some(anchor as f64 / rate);
            }
        }
    }
    None
}

/// Keeps only what stands above the average of the surrounding half second.
fn above_local_average(mut flux: Vec<f32>) -> Vec<f32> {
    // The first frame "rises" from silence, which is not an onset.
    if let Some(first) = flux.first_mut() {
        *first = 0.0;
    }
    let radius = ((0.25 / ENVELOPE_STEP_SECONDS) as usize).max(1);
    let mut prefix = vec![0f64; flux.len() + 1];
    for (i, value) in flux.iter().enumerate() {
        prefix[i + 1] = prefix[i] + *value as f64;
    }
    (0..flux.len())
        .map(|i| {
            let (from, to) = (i.saturating_sub(radius), (i + radius + 1).min(flux.len()));
            let mean = (prefix[to] - prefix[from]) / (to - from) as f64;
            (flux[i] as f64 - mean).max(0.0) as f32
        })
        .collect()
}

/// A rough tempo from the envelope's autocorrelation, to within about one
/// BPM. Each candidate is scored by how well the envelope repeats after one,
/// two, three and four beats.
fn coarse_tempo(envelope: &[f32], rate: f64, range: TempoRange) -> Option<f64> {
    let max_lag = ((4.0 * 60.0 * rate / range.min_bpm).ceil() as usize + 2).min(envelope.len() / 2);
    let autocorrelation: Vec<f64> = (0..=max_lag)
        .map(|lag| {
            envelope[lag..]
                .iter()
                .zip(envelope)
                .map(|(a, b)| *a as f64 * *b as f64)
                .sum()
        })
        .collect();
    let at = |lag: f64| -> f64 {
        let index = lag.floor() as usize;
        match (autocorrelation.get(index), autocorrelation.get(index + 1)) {
            (Some(a), Some(b)) => a + (b - a) * (lag - index as f64),
            _ => 0.0,
        }
    };

    let mut best: Option<(f64, f64)> = None;
    let mut bpm = range.min_bpm;
    while bpm <= range.max_bpm {
        let lag = 60.0 * rate / bpm;
        let repeats: f64 = (1..=4).map(|beats| at(lag * beats as f64)).sum();
        // A gentle preference for middling tempos settles half/double ties.
        let octaves = (bpm / PREFERRED_BPM).log2();
        let score = repeats * (-0.5 * (octaves / 0.9).powi(2)).exp();
        if best.is_none_or(|(_, top)| score > top) {
            best = Some((bpm, score));
        }
        bpm += 0.25;
    }
    best.filter(|(_, score)| *score > 0.0).map(|(bpm, _)| bpm)
}

struct Fold {
    /// Where beats fall within a beat length, 0.0 to 1.0.
    phase: f64,
    sharpness: f64,
    confidence: f32,
}

// Prefer trusted kick evidence over broadband offbeat percussion, even when
// log compression spreads the bass envelope enough to lower its confidence.
fn beat_phase(bass: &Fold, strength: &Fold, broadband: &Fold) -> f64 {
    if bass.confidence >= BASS_CONFIDENCE {
        bass.phase
    } else if strength.confidence >= BASS_CONFIDENCE {
        strength.phase
    } else {
        broadband.phase
    }
}

/// Lays the envelope over itself one beat at a time. At the right tempo,
/// onsets pile up in the same place every beat; slightly off, they smear.
fn fold(envelope: &[f32], rate: f64, bpm: f64) -> Fold {
    fold_near(envelope, rate, bpm, None)
}

fn fold_near(envelope: &[f32], rate: f64, bpm: f64, neighbourhood: Option<(f64, f64)>) -> Fold {
    let period = 60.0 * rate / bpm;
    let mut bins = [0f64; PHASE_BINS];
    for (index, value) in envelope.iter().enumerate() {
        let position = (index as f64 / period).fract();
        bins[(position * PHASE_BINS as f64) as usize % PHASE_BINS] += *value as f64;
    }
    let total: f64 = bins.iter().sum();
    let (peak, sharpness) = (0..PHASE_BINS)
        .filter(|i| {
            neighbourhood.is_none_or(|(centre, reach)| {
                let distance = ((*i as f64 + 0.5) / PHASE_BINS as f64 - centre).rem_euclid(1.0);
                distance.min(1.0 - distance) <= reach
            })
        })
        .map(|i| {
            let around =
                bins[(i + PHASE_BINS - 1) % PHASE_BINS] + bins[i] + bins[(i + 1) % PHASE_BINS];
            (i, around)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0));

    // Place the peak between bins by the balance of its neighbours.
    let (before, after) = (
        bins[(peak + PHASE_BINS - 1) % PHASE_BINS],
        bins[(peak + 1) % PHASE_BINS],
    );
    let spread = before + bins[peak] + after;
    let offset = if spread > 0.0 {
        (after - before) / spread
    } else {
        0.0
    };
    let phase = ((peak as f64 + 0.5 + offset) / PHASE_BINS as f64).rem_euclid(1.0);
    // Three bins hold 3/64 of evenly spread onsets, and all of perfect ones.
    let even = 3.0 / PHASE_BINS as f64;
    let share = if total > 0.0 { sharpness / total } else { 0.0 };
    Fold {
        phase,
        sharpness,
        confidence: ((share - even) / (1.0 - even)).clamp(0.0, 1.0) as f32,
    }
}

/// The tempo within `centre ± reach` whose fold is sharpest.
fn best_fold(envelope: &[f32], rate: f64, centre: f64, reach: f64, step: f64) -> (f64, Fold) {
    let steps = (reach / step).round() as i64;
    (-steps..=steps)
        .map(|i| centre + i as f64 * step)
        .map(|bpm| (bpm, fold(envelope, rate, bpm)))
        .max_by(|a, b| a.1.sharpness.total_cmp(&b.1.sharpness))
        .expect("at least the centre tempo is tried")
}

#[cfg(test)]
mod tests {
    #[test]
    fn later_kick_run_overrides_an_offbeat_bass_intro() {
        let bpm = 140.;
        let period = 60. / bpm;
        let first = 0.12;
        let mut samples = vec![0.; RATE as usize * 60];
        for beat in 0..130 {
            // The intro and every offbeat have a harmonically rich bass note.
            let bass = ((first + (beat as f64 + 0.5) * period) * RATE as f64) as usize;
            for i in 0..(RATE as usize / 6) {
                let t = i as f32 / RATE as f32;
                for harmonic in 1..=5 {
                    samples[bass + i] += 0.15
                        * (-t * 18.).exp()
                        * (std::f32::consts::TAU * 65. * harmonic as f32 * t).sin();
                }
            }
            if beat < 32 {
                continue;
            }
            let kick = ((first + beat as f64 * period) * RATE as f64) as usize;
            for i in 0..(RATE as usize / 5) {
                let t = i as f32 / RATE as f32;
                let phase = 55. * t + 120. * (1. - (-70. * t).exp()) / 70.;
                samples[kick + i] += 0.9 * (-t * 30.).exp() * (std::f32::consts::TAU * phase).sin();
            }
        }
        let tempo = detect_tempo(&samples, RATE, TempoRange::default()).unwrap();
        assert!((tempo.bpm - bpm).abs() < 0.03, "{tempo:?}");
        let error = (tempo.first_beat_seconds - first).rem_euclid(period);
        assert!(error.min(period - error) < 0.025, "{tempo:?}");
    }

    #[test]
    fn isolated_kick_cannot_anchor_a_grid() {
        let mut kicks = vec![0.; 10_000];
        kicks[2345] = 1.;
        assert!(super::kick_anchor(&kicks, 200., 140.).is_none());
        // Repeated support is required, even when a single hit folds sharply.
        assert!(super::kick_anchor(&vec![0.; 10_000], 200., 140.).is_none());
    }

    #[test]
    fn first_kick_run_wins_over_a_louder_later_half_beat_phase() {
        let mut kicks = vec![0.; 24_000];
        for beat in 0..16 {
            kicks[6020 + beat * 100] = 1.;
        }
        for beat in 0..100 {
            kicks[12070 + beat * 100] = 2.;
        }
        assert_eq!(super::kick_anchor(&kicks, 200., 120.), Some(30.1));
    }

    use super::*;

    const RATE: u32 = 22_050;

    /// A drum-machine pattern: a kick on every beat, a quieter hi-hat on the
    /// off-beat, and background noise, starting at `first_beat` seconds.
    fn pattern(bpm: f64, first_beat: f64, seconds: f64) -> Vec<f32> {
        let mut samples = vec![0f32; (seconds * RATE as f64) as usize];
        let mut noise = 0x2545_f491u32;
        let mut random = move || {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            noise as f32 / u32::MAX as f32 - 0.5
        };
        for sample in samples.iter_mut() {
            *sample = 0.02 * random();
        }
        let period = 60.0 / bpm;
        let mut beat = 0;
        loop {
            let kick = ((first_beat + beat as f64 * period) * RATE as f64) as usize;
            let hat = kick + (period / 2.0 * RATE as f64) as usize;
            if hat + 2000 >= samples.len() {
                break;
            }
            for i in 0..2000 {
                let t = i as f32 / RATE as f32;
                samples[kick + i] +=
                    0.8 * (-t * 30.0).exp() * (std::f32::consts::TAU * 60.0 * t).sin();
            }
            for i in 0..400 {
                let t = i as f32 / RATE as f32;
                samples[hat + i] += 0.25 * (-t * 200.0).exp() * random();
            }
            beat += 1;
        }
        samples
    }

    fn assert_tempo(bpm: f64, first_beat: f64) {
        let tempo = detect_tempo(&pattern(bpm, first_beat, 90.0), RATE, TempoRange::default())
            .unwrap_or_else(|| panic!("no tempo found for {bpm}"));
        assert!(
            (tempo.bpm - bpm).abs() < 0.02,
            "{bpm} BPM read as {}",
            tempo.bpm
        );

        // The grid may start on any beat, so compare within one beat length.
        let period = 60.0 / bpm;
        let error = (tempo.first_beat_seconds - first_beat).rem_euclid(period);
        let error = error.min(period - error);
        assert!(error < 0.008, "{bpm} BPM: first beat off by {error:.4} s");
        assert!(
            tempo.confidence > 0.3,
            "{bpm} BPM: confidence {}",
            tempo.confidence
        );
    }

    #[test]
    fn phase_refinement_rejects_offbeat_peak_and_wraps_at_zero() {
        let rate = 200.;
        let bpm = 120.;
        let mut envelope = vec![0.; 6000];
        for beat in 0..60 {
            envelope[beat * 100 + 98] = 1.;
            envelope[beat * 100 + 48] = 2.;
        }
        let unrestricted = fold(&envelope, rate, bpm);
        assert!((unrestricted.phase - 0.48).abs() < 0.02);
        // The strong kick is just before phase zero; its weaker log-flux
        // peak must still win over the larger offbeat one across the wrap.
        let refined = fold_near(&envelope, rate, bpm, Some((0.01, 0.12)));
        assert!((refined.phase - 0.98).abs() < 0.02);
        assert!(refined.confidence >= BASS_CONFIDENCE);
    }

    #[test]
    fn kick_phase_wins_over_quieter_offbeat_bass() {
        let bpm = 140.0;
        let first = 0.12;
        let period = 60.0 / bpm;
        let mut samples = vec![0.; (60 * RATE) as usize];
        for beat in 0..138 {
            let start = ((first + beat as f64 * period) * RATE as f64) as usize;
            for i in 0..3000 {
                let t = i as f32 / RATE as f32;
                samples[start + i] +=
                    0.8 * (-t * 30.).exp() * (std::f32::consts::TAU * 60. * t).sin();
            }
        }
        for beat in 0..138 {
            let start = ((first + (beat as f64 + 0.5) * period) * RATE as f64) as usize;
            for i in 0..3000 {
                let t = i as f32 / RATE as f32;
                // A quieter, harmonically rich bass note has a large relative
                // rise in more bins than the kick, despite carrying less energy.
                for frequency in [45., 90., 135., 180.] {
                    samples[start + i] +=
                        0.10 * (-t * 18.).exp() * (std::f32::consts::TAU * frequency * t).sin();
                }
            }
        }
        let tempo = detect_tempo(&samples, RATE, TempoRange::default()).unwrap();
        assert!((tempo.bpm - bpm).abs() < 0.02, "{tempo:?}");
        let error = (tempo.first_beat_seconds - first).rem_euclid(period);
        assert!(error.min(period - error) < 0.012, "{tempo:?}");
    }

    #[test]
    fn reliable_kick_survives_weak_log_flux_and_offbeat_hats() {
        // Measured regression cases: both tracks used to fall back to the
        // broadband phase, half a beat from the reliable bass-strength peak.
        for (kick, log_phase, log_confidence, hat) in [
            (0.3664, 0.3345, 0.1256, 0.8346),
            (0.0071, 0.9462, 0.1030, 0.4450),
        ] {
            let make = |phase, confidence| super::Fold {
                phase,
                confidence,
                sharpness: 1.,
            };
            let bass = make(log_phase, log_confidence);
            let strength = make(kick, 0.23);
            let broadband = make(hat, 0.29);
            assert_eq!(super::beat_phase(&bass, &strength, &broadband), kick);
            assert_eq!(super::beat_phase(&bass, &make(kick, 0.01), &broadband), hat);
        }
    }

    #[test]
    fn first_beat_is_an_actual_attack_after_leading_silence() {
        for (bpm, first) in [
            (140., 60. / 140.),
            (136., 0.292),
            (128., 1.42),
            (138., 16.31),
        ] {
            let tempo =
                detect_tempo(&pattern(bpm, first, 60.), RATE, TempoRange::default()).unwrap();
            assert!(
                (tempo.first_beat_seconds - first).abs() < 0.012,
                "{tempo:?}; expected {first}"
            );
        }
    }

    #[test]
    fn kick_reference_rejects_intensity_builds_and_faster_rolls() {
        let rate = 100.;
        for rolling in [false, true] {
            let mut kicks = vec![0.; 2000];
            for i in (50..800).step_by(if rolling { 25 } else { 50 }) {
                kicks[i] = if rolling {
                    1.
                } else if i % 100 == 0 {
                    1.
                } else {
                    0.25
                };
            }
            for i in (800..1950).step_by(50) {
                kicks[i] = 1.;
            }
            let anchor = super::kick_anchor(&kicks, rate, 120.).unwrap();
            assert!(
                anchor >= 8. && anchor <= 8.5,
                "roll={rolling}, anchor={anchor}"
            );
        }
        let mut kicks = vec![0.; 1000];
        for i in (50..950).step_by(50) {
            kicks[i] = if i % 100 == 0 { 0.85 } else { 1. };
        }
        assert_eq!(super::kick_anchor(&kicks, rate, 120.), Some(0.5));
        // Weaker regular offbeat bass must not turn a steady kick run into
        // a supposed roll (the Better Off Alone opening has this pattern).
        let mut offbeat_bass = kicks.clone();
        for i in (75..925).step_by(50) {
            offbeat_bass[i] = 0.6;
        }
        assert_eq!(super::kick_anchor(&offbeat_bass, rate, 120.), Some(0.5));
        let mut half_time = vec![0.; 2000];
        for i in (50..1950).step_by(100) {
            half_time[i] = 1.;
        }
        assert_eq!(super::kick_anchor(&half_time, rate, 120.), Some(0.5));
        let rolls: Vec<f32> = (0..1000)
            .map(|i| if i > 0 && i % 25 == 0 { 1. } else { 0. })
            .collect();
        assert_eq!(super::kick_anchor(&rolls, rate, 120.), None);
    }

    #[test]
    fn quiet_lead_in_does_not_move_bar_one_past_the_first_kick() {
        let bpm = 137.;
        let first = 1.94;
        let mut samples = pattern(bpm, first, 60.);
        // Quiet audio before a clean kick entrance can extend the grid back
        // by two beats, but it is not the opening kick pattern itself.
        for (i, sample) in samples.iter_mut().enumerate() {
            let t = i as f64 / RATE as f64;
            if t >= first - 2. * 60. / bpm && t < first {
                *sample += 0.025 * (std::f32::consts::TAU * 330. * t as f32).sin();
            }
        }
        let tempo = detect_tempo(&samples, RATE, TempoRange::default()).unwrap();
        assert!((tempo.bpm - bpm).abs() < 0.02, "{tempo:?}");
        assert!(
            (tempo.first_beat_seconds - first).abs() < 0.015,
            "{tempo:?}"
        );
        assert!(
            (tempo.first_bar_seconds() - first).abs() < 0.015,
            "{tempo:?}"
        );
    }

    #[test]
    fn opening_kick_with_a_slower_attack_does_not_shift_bar_one_beat() {
        let mut samples = pattern(120., 0.25, 60.);
        let start = (0.25 * RATE as f64) as usize;
        let end = (0.45 * RATE as f64) as usize;
        let delay = (0.018 * RATE as f64) as usize;
        let hit = samples[start..end].to_vec();
        samples[start..end].fill(0.);
        samples[start + delay..end + delay].copy_from_slice(&hit);
        let first = super::opening_attack(&samples, RATE, 0.25, 0.5).unwrap();
        assert!((first - 0.25).abs() < 0.012, "first {first}");
    }

    #[test]
    fn opening_refinement_requires_repeated_attacks() {
        assert!(super::opening_attack(&vec![0.; RATE as usize * 10], RATE, 0., 0.5).is_none());
        let mut audio = vec![0.; RATE as usize * 10];
        audio[RATE as usize] = 1.;
        assert!(super::opening_attack(&audio, RATE, 0., 0.5).is_none());
    }

    #[test]
    fn finds_tempo_and_first_beat() {
        assert_tempo(128.0, 0.25);
        assert_tempo(174.0, 0.10);
        assert_tempo(90.5, 0.40);
        assert_tempo(122.37, 0.0);
    }

    /// The drum pattern with bars: a crash on the first beat of every bar,
    /// the first bar starting on beat `downbeat`, and the hi-hats doubling
    /// in level from the ninth bar on, as a new section would.
    fn bars(bpm: f64, first_beat: f64, downbeat: usize, seconds: f64) -> Vec<f32> {
        let mut samples = pattern(bpm, first_beat, seconds);
        let period = 60.0 / bpm;
        let mut noise = 0x1234_5678u32;
        let mut random = move || {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            noise as f32 / u32::MAX as f32 - 0.5
        };
        let mut bar = 0;
        loop {
            let start = first_beat + (downbeat + bar * BEATS_PER_BAR) as f64 * period;
            let at = (start * RATE as f64) as usize;
            if at + RATE as usize >= samples.len() {
                break;
            }
            for i in 0..RATE as usize / 2 {
                let t = i as f32 / RATE as f32;
                samples[at + i] += 0.3 * (-t * 8.0).exp() * random();
            }
            if bar >= 8 {
                for beat in 0..BEATS_PER_BAR {
                    let hat = ((start + (beat as f64 + 0.5) * period) * RATE as f64) as usize;
                    for i in 0..400.min(samples.len().saturating_sub(hat)) {
                        let t = i as f32 / RATE as f32;
                        samples[hat + i] += 0.25 * (-t * 200.0).exp() * random();
                    }
                }
            }
            bar += 1;
        }
        samples
    }

    #[test]
    fn finds_the_beat_that_starts_each_bar() {
        for (bpm, first_beat, downbeat) in [
            (128.0, 0.2, 0),
            (124.0, 0.1, 1),
            (174.0, 0.3, 2),
            (100.0, 0.05, 3),
        ] {
            let tempo = detect_tempo(
                &bars(bpm, first_beat, downbeat, 60.0),
                RATE,
                TempoRange::default(),
            )
            .unwrap();
            // The grid may begin a beat later than the music if its first
            // beat is too close to the start to measure.
            let period = 60.0 / bpm;
            let expected = first_beat + downbeat as f64 * period;
            let error =
                (tempo.first_bar_seconds() - expected).rem_euclid(BEATS_PER_BAR as f64 * period);
            let error = error.min(BEATS_PER_BAR as f64 * period - error);
            assert!(
                error < 0.01,
                "{bpm} BPM, downbeat {downbeat}: bar off by {error:.3} s"
            );
            assert!(tempo.first_bar_seconds() >= tempo.first_beat_seconds);
        }
    }

    #[test]
    fn the_range_decides_between_half_and_double() {
        let samples = pattern(140.0, 0.2, 60.0);
        let half = TempoRange {
            min_bpm: 60.0,
            max_bpm: 100.0,
        };
        let tempo = detect_tempo(&samples, RATE, half).unwrap();
        assert!((tempo.bpm - 70.0).abs() < 0.02, "read as {}", tempo.bpm);
    }

    #[test]
    fn works_at_other_sample_rates() {
        let samples: Vec<f32> = pattern(126.0, 0.3, 60.0)
            .iter()
            .flat_map(|s| [*s, *s])
            .collect();
        let tempo = detect_tempo(&samples, RATE * 2, TempoRange::default()).unwrap();
        assert!((tempo.bpm - 126.0).abs() < 0.02, "read as {}", tempo.bpm);
    }

    #[test]
    fn gives_up_on_audio_without_a_beat() {
        assert_eq!(
            detect_tempo(&vec![0.0; RATE as usize * 30], RATE, TempoRange::default()),
            None
        );
        assert_eq!(
            detect_tempo(&pattern(128.0, 0.0, 3.0), RATE, TempoRange::default()),
            None
        );
    }
}
