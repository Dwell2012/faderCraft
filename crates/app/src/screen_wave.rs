//! Overview waveform for the DDJ-1000 jog screens: 600 columns spread over the
//! whole track, the peak in an outer colour and the average level in a
//! brighter inner colour. Colour follows the track's low/mid/high energy as the
//! on-screen waveform does (low = red, mid = green, high = blue).
use djcraft_controllers::ddj1000_link::{WAVE_COLUMNS, WAVE_MAX_HEIGHT, WaveColumn, rgb565};
use djcraft_engine::Track;

/// Columns for a fully decoded track, or `None` while it is still decoding.
pub fn columns(track: &Track) -> Option<Vec<WaveColumn>> {
    if !track.is_complete() {
        return None;
    }
    let count = track.peak_count();
    if count == 0 {
        return None;
    }
    let loudest = (0..count).map(|i| track.peak(i)).fold(0f32, f32::max);
    // Fill the display on quiet tracks, but do not blow up near-silence.
    let scale = 1. / loudest.max(0.05);
    Some(
        (0..WAVE_COLUMNS)
            .map(|column| {
                let start = column * count / WAVE_COLUMNS;
                let end = ((column + 1) * count / WAVE_COLUMNS).clamp(start + 1, count);
                let mut peak = 0f32;
                let mut squares = 0f32;
                let mut bands = [0f32; 3];
                for block in start..end {
                    let value = track.peak(block);
                    peak = peak.max(value);
                    squares += value * value;
                    for (sum, band) in bands.iter_mut().zip(track.bands(block)) {
                        *sum += band * band;
                    }
                }
                let average = (squares / (end - start) as f32).sqrt();
                let colour = band_colour(bands.map(f32::sqrt));
                WaveColumn {
                    outer: rgb565(
                        scale_channel(colour[0], 0.75),
                        scale_channel(colour[1], 0.75),
                        scale_channel(colour[2], 0.75),
                    ),
                    inner: rgb565(lighten(colour[0]), lighten(colour[1]), lighten(colour[2])),
                    outer_height: height(peak * scale),
                    inner_height: height(average * scale),
                }
            })
            .collect(),
    )
}

fn height(level: f32) -> u8 {
    (level.clamp(0., 1.) * f32::from(WAVE_MAX_HEIGHT)).round() as u8
}

/// Normalise the strongest band to full brightness and suppress weaker ones
/// so each column keeps a readable hue.
fn band_colour(energy: [f32; 3]) -> [f32; 3] {
    let maximum = energy
        .into_iter()
        .filter(|v| v.is_finite())
        .fold(0f32, f32::max);
    if maximum <= 1e-12 {
        return [0.; 3];
    }
    energy.map(|v| {
        if v.is_finite() {
            (v / maximum).powf(1.2)
        } else {
            0.
        }
    })
}

fn scale_channel(value: f32, brightness: f32) -> u8 {
    (value.clamp(0., 1.) * brightness * 255.).round() as u8
}

fn lighten(value: f32) -> u8 {
    // Halfway to white from the colour itself.
    ((value.clamp(0., 1.) * 0.6 + 0.4) * 255.).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo(frames: usize, amplitude: impl Fn(usize) -> f32) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let value = amplitude(i) * (i as f32 * 0.05).sin();
                [value, value]
            })
            .collect()
    }

    #[test]
    fn a_complete_track_gives_600_columns_that_follow_loudness() {
        // Quiet first half, loud second half.
        let frames = 44_100 * 10;
        let track = Track::from_samples(
            "test",
            44_100,
            stereo(frames, |i| if i < frames / 2 { 0.1 } else { 0.9 }),
        );
        let columns = columns(&track).expect("complete track");
        assert_eq!(columns.len(), WAVE_COLUMNS);
        let quiet = columns[100];
        let loud = columns[500];
        assert!(loud.outer_height > quiet.outer_height);
        assert!(loud.outer_height >= 36, "loudest part fills the display");
        assert!(loud.inner_height <= loud.outer_height);
        assert!(columns.iter().all(|c| c.outer_height <= WAVE_MAX_HEIGHT));
    }

    #[test]
    fn silence_is_flat_and_does_not_panic() {
        let track = Track::from_samples("silence", 44_100, vec![0.; 44_100 * 2]);
        let columns = columns(&track).unwrap();
        assert!(
            columns
                .iter()
                .all(|c| c.outer_height == 0 && c.inner_height == 0)
        );
    }

    #[test]
    fn very_short_tracks_still_fill_every_column() {
        let track = Track::from_samples("blip", 44_100, stereo(512, |_| 0.5));
        assert_eq!(columns(&track).unwrap().len(), WAVE_COLUMNS);
    }

    #[test]
    fn bass_is_red_and_treble_is_blue() {
        assert_eq!(band_colour([1., 0., 0.])[0], 1.);
        assert_eq!(band_colour([0., 0., 2.])[2], 1.);
        assert_eq!(band_colour([0., 0., 0.]), [0.; 3]);
    }
}
