use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use djcraft_engine::Track;
use djcraft_library::TempoResult;
use gpui_kit::*;

/// Playback seconds visible across the full width of the waveform.
pub const VISIBLE_SECONDS: f64 = 8.;
const COLUMN_WIDTH: f32 = 2.;

/// Low/mid/high: shadow, deck primary, highlight.
pub(crate) fn deck_palette(deck: usize) -> [u32; 3] {
    [
        [0x607b1b, 0xc8ff38, 0xefffc2],
        [0x203b8e, 0x3864e8, 0xb6c8ff],
        [0x895218, 0xffb347, 0xffe3b5],
        [0x6c3894, 0xcf85ff, 0xefceff],
    ][deck % djcraft_protocol::DECKS]
}

pub(crate) const STEM_COLOURS: [u32; 4] = [0xff6ba9, 0x48d1dd, 0xffb347, 0xd8e4f0];
pub(crate) const STEM_LABELS: [&str; 4] = ["Vocals", "Instruments", "Bass", "Drums"];

fn column_bands(track: &Track, range: std::ops::Range<usize>) -> [f32; 3] {
    let mut sum = [0f32; 3];
    for i in range {
        for (sum, band) in sum.iter_mut().zip(track.bands(i)) {
            *sum += band * band;
        }
    }
    sum.map(f32::sqrt)
}

#[derive(Clone, Copy, Default)]
struct DisplayColumn {
    energy: [f32; 3],
}

// Symmetric display-only filter: no temporal lag or shift of beat positions.
fn smooth_columns(raw: &[Option<DisplayColumn>]) -> Vec<Option<DisplayColumn>> {
    const WEIGHTS: [f32; 5] = [0.1, 0.2, 0.4, 0.2, 0.1];
    raw.windows(5)
        .map(|neighbours| {
            neighbours[2]?; // Never extend the waveform beyond available audio.
            let mut result = DisplayColumn::default();
            for (column, weight) in neighbours.iter().zip(WEIGHTS) {
                if let Some(column) = column {
                    for (sum, energy) in result.energy.iter_mut().zip(column.energy) {
                        *sum += energy * weight;
                    }
                }
            }
            Some(result)
        })
        .collect()
}

fn display_columns(
    track: &Track,
    first: i64,
    count: usize,
    step: f64,
) -> Vec<Option<DisplayColumn>> {
    display_source_columns(track, first, count, step, None)
}

fn display_source_columns(
    track: &Track,
    first: i64,
    count: usize,
    step: f64,
    stem: Option<usize>,
) -> Vec<Option<DisplayColumn>> {
    let ready = track.peak_count() as i64;
    let raw: Vec<_> = (-2..count as i64 + 3)
        .map(|offset| {
            let start = ((first + offset) as f64 * step).floor() as i64;
            let end = ((first + offset + 1) as f64 * step).floor() as i64;
            let range = start.max(0)..end.max(start + 1).min(ready);
            if range.is_empty() {
                return None;
            }
            let bands = if let Some(stem) = stem {
                let energy = range
                    .clone()
                    .map(|i| track.stem_levels(i as usize).unwrap_or_default()[stem].powi(2))
                    .sum::<f32>();
                [energy.sqrt(), 0., 0.]
            } else {
                column_bands(track, range.start as usize..range.end as usize)
            };
            Some(DisplayColumn {
                energy: bands.map(|v| v * v / (end - start).max(1) as f32),
            })
        })
        .collect();
    smooth_columns(&raw)
}

fn paint_columns(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    columns: &[Option<DisplayColumn>],
    shift: f32,
    palette: [u32; 3],
    gains: [f32; 3],
) {
    // Low sits behind mid and high; each keeps its own amplitude envelope.
    for (band, tint) in palette.into_iter().enumerate() {
        paint_band(
            window,
            bounds,
            columns,
            shift,
            band,
            tint,
            0.85,
            gains[band],
        );
    }
}

fn paint_band(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    columns: &[Option<DisplayColumn>],
    shift: f32,
    band: usize,
    tint: u32,
    opacity: f32,
    gain: f32,
) {
    if gain <= 0. {
        return;
    }
    let height = bounds.size.height.as_f32();
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let mut path = PathBuilder::fill();
        let mut start = 0;
        while start < columns.len() {
            if columns[start].is_none() {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < columns.len() && columns[end].is_some() {
                end += 1;
            }
            let edge = |i: usize, bottom: bool| {
                let amplitude = columns[i].unwrap().energy[band].max(0.).sqrt() * gain;
                let half = (amplitude * std::f32::consts::SQRT_2).min(1.) * height / 2.;
                bounds.origin
                    + point(
                        px(i as f32 * COLUMN_WIDTH - shift),
                        px(height / 2. + if bottom { half } else { -half }),
                    )
            };
            path.move_to(edge(start, false));
            for i in start + 1..end {
                path.line_to(edge(i, false));
            }
            for i in (start..end).rev() {
                path.line_to(edge(i, true));
            }
            path.close();
            start = end;
        }
        if let Ok(path) = path.build() {
            let color: Hsla = rgb(tint).into();
            window.paint_path(path, color.opacity(opacity));
        }
    });
}

fn cue_flags(
    cues: [Option<djcraft_protocol::HotCue>; djcraft_protocol::HOT_CUES],
    styles: &[djcraft_protocol::CueAppearance; djcraft_protocol::HOT_CUES],
    deck: usize,
    left: f64,
    span: f64,
    show_labels: bool,
) -> Div {
    let mut flags = div().absolute().inset_0().overflow_hidden();
    for (slot, cue) in cues
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| c.map(|c| (i, c)))
    {
        let x = (cue.seconds - left) / span.max(0.001);
        if !(0.0..=1.).contains(&x) {
            continue;
        }
        let colour = rgb(crate::pad_store::cue_colour(deck, slot, &styles[slot]));
        let label = if styles[slot].label.is_empty() {
            format!("{}", slot + 1)
        } else {
            format!("{} {}", slot + 1, styles[slot].label)
        };
        let flag = if show_labels {
            div()
                .max_w(px(100.))
                .px_1()
                .text_size(px(9.))
                .text_color(rgb(0x070b12))
                .bg(colour)
                .truncate()
                .child(label)
        } else {
            div().w(px(7.)).h(px(6.)).bg(colour)
        };
        flags = flags.child(
            div()
                .absolute()
                .left(relative(x as f32))
                .top_0()
                .h_full()
                .border_l_2()
                .border_color(colour)
                .child(flag),
        );
    }
    flags
}

/// A waveform that scrolls under a fixed, centred playhead, with the beat
/// grid drawn over it when there is one: a faint line on each beat and a
/// stronger one where each bar starts.
pub fn waveform(
    track: Arc<Track>,
    position_seconds: f64,
    tempo: f64,
    grid: Option<TempoResult>,
    area: Rc<Cell<Bounds<Pixels>>>,
    playhead: Hsla,
    loop_region: Option<[f64; 2]>,
    roll_position: Option<f64>,
    accent: Hsla,
    hot_cues: [Option<djcraft_protocol::HotCue>; djcraft_protocol::HOT_CUES],
    cue_styles: [djcraft_protocol::CueAppearance; djcraft_protocol::HOT_CUES],
    deck: usize,
    show_stems: bool,
    eq_gains: [f32; 3],
    stem_gains: [f32; 4],
) -> impl IntoElement {
    let flags = cue_flags(
        hot_cues,
        &cue_styles,
        deck,
        position_seconds - VISIBLE_SECONDS * tempo / 2.,
        VISIBLE_SECONDS * tempo,
        true,
    );
    div()
        .relative()
        .size_full()
        .child(
            canvas(
                move |bounds, _, _| area.set(bounds),
                move |bounds, _, window, _| {
                    let visible_seconds = VISIBLE_SECONDS * tempo;
                    let width = bounds.size.width.as_f32();
                    let height = bounds.size.height.as_f32();
                    // Peaks arrive as the track decodes, so the waveform fills in.
                    let peaks_per_second =
                        track.sample_rate as f64 / track.frames_per_peak() as f64;

                    // Columns cover fixed groups of peaks, and the whole row shifts
                    // by a fraction of a column, so bars keep their shape as they
                    // scroll.
                    let columns = (width / COLUMN_WIDTH).ceil() as i64 + 1;
                    let peaks_per_column =
                        visible_seconds * peaks_per_second / (width / COLUMN_WIDTH) as f64;
                    let centre = position_seconds * peaks_per_second / peaks_per_column;
                    let first = centre.floor() as i64 - columns / 2;
                    let shift = (centre.fract() as f32) * COLUMN_WIDTH;

                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        let centre_x = (columns / 2) as f32 * COLUMN_WIDTH;
                        let time_x = |time: f64| {
                            centre_x
                                + ((time - position_seconds) / visible_seconds * width as f64)
                                    as f32
                        };
                        if let Some([start, end]) = loop_region {
                            let left = time_x(start).clamp(0., width);
                            let right = time_x(end).clamp(0., width);
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin + point(px(left), px(0.)),
                                    size(px((right - left).max(0.)), px(height)),
                                ),
                                accent.opacity(0.13),
                            ));
                            for edge in [start, end] {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        bounds.origin + point(px(time_x(edge)), px(0.)),
                                        size(px(2.), px(height)),
                                    ),
                                    accent,
                                ));
                            }
                        }
                        if let Some(position) = roll_position {
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin + point(px(time_x(position)), px(0.)),
                                    size(px(2.), px(height)),
                                ),
                                rgb(0xffdb55),
                            ));
                        }
                        if show_stems && track.stem_levels(0).is_some() {
                            for (stem, tint) in STEM_COLOURS.into_iter().enumerate() {
                                if stem_gains[stem] <= 0. {
                                    continue;
                                }
                                let data = display_source_columns(
                                    &track,
                                    first,
                                    columns as usize,
                                    peaks_per_column,
                                    Some(stem),
                                );
                                paint_band(
                                    window,
                                    bounds,
                                    &data,
                                    shift,
                                    0,
                                    tint,
                                    0.55,
                                    stem_gains[stem],
                                );
                            }
                        } else {
                            let data =
                                display_columns(&track, first, columns as usize, peaks_per_column);
                            paint_columns(
                                window,
                                bounds,
                                &data,
                                shift,
                                deck_palette(deck),
                                eq_gains,
                            );
                        }
                        let x = (columns / 2) as f32 * COLUMN_WIDTH;
                        for (line_x, bar_line) in
                            grid_lines(grid, position_seconds, width, x, tempo)
                        {
                            let (line_width, opacity) =
                                if bar_line { (2., 0.55) } else { (1., 0.2) };
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin + point(px(line_x), px(0.)),
                                    size(px(line_width), px(height)),
                                ),
                                playhead.opacity(opacity),
                            ));
                        }
                        window.paint_quad(fill(
                            Bounds::new(
                                bounds.origin + point(px(x), px(0.)),
                                size(px(2.), px(height)),
                            ),
                            playhead,
                        ));
                    });
                },
            )
            .size_full(),
        )
        .child(flags)
}

/// Where the beat grid's lines fall across a scrolling waveform `width`
/// pixels wide whose playhead, at `position_seconds`, is `playhead_x` pixels
/// in: each line's x and whether it starts a bar. Beats before the track
/// starts are left out.
fn grid_lines(
    grid: Option<TempoResult>,
    position_seconds: f64,
    width: f32,
    playhead_x: f32,
    tempo: f64,
) -> Vec<(f32, bool)> {
    let Some(grid) = grid.filter(|grid| grid.bpm > 0.) else {
        return Vec::new();
    };
    let pixels_per_second = width as f64 / (VISIBLE_SECONDS * tempo);
    let beat = grid.beat_seconds();
    let half = VISIBLE_SECONDS * tempo / 2.;
    let first = ((position_seconds - half - grid.first_beat_seconds) / beat).ceil() as i64;
    let last = ((position_seconds + half - grid.first_beat_seconds) / beat).floor() as i64;
    (first..=last)
        .filter_map(|index| {
            let seconds = grid.first_beat_seconds + index as f64 * beat;
            let bar_line = (index - grid.downbeat as i64).rem_euclid(4) == 0;
            let x = playhead_x + ((seconds - position_seconds) * pixels_per_second) as f32;
            (seconds >= -16. * beat).then_some((x, bar_line))
        })
        .collect()
}

/// Reuse whole-track envelopes while only the playhead is moving.
#[derive(Default)]
pub(crate) struct OverviewCache {
    track: Option<Arc<Track>>,
    width: f32,
    ready: usize,
    frames: usize,
    columns: Vec<Option<DisplayColumn>>,
}

/// A track's whole waveform, with the part already played in another colour
/// and a line at the playing position.
pub fn overview(
    track: Arc<Track>,
    played_fraction: f64,
    cache: Rc<RefCell<OverviewCache>>,
    area: Rc<Cell<Bounds<Pixels>>>,
    playhead: Hsla,
    cues: [Option<djcraft_protocol::HotCue>; djcraft_protocol::HOT_CUES],
    styles: [djcraft_protocol::CueAppearance; djcraft_protocol::HOT_CUES],
    deck: usize,
    eq_gains: [f32; 3],
) -> impl IntoElement {
    let duration = track.frames() as f64 / track.sample_rate as f64;
    let flags = cue_flags(cues, &styles, deck, 0., duration, false);
    div()
        .absolute()
        .size_full()
        .child(
            canvas(
                move |bounds, _, _| area.set(bounds),
                move |bounds, _, window, _| {
                    let width = bounds.size.width.as_f32();
                    // Peaks arrive as the track decodes, so the waveform fills in.
                    let total = track.frames().div_ceil(track.frames_per_peak()).max(1) as f32;
                    let columns = (width / COLUMN_WIDTH).ceil() as usize;
                    let played_x = played_fraction as f32 * width;
                    let mut cached = cache.borrow_mut();
                    let ready = track.peak_count();
                    let frames = track.frames();
                    if cached
                        .track
                        .as_ref()
                        .is_none_or(|old| !Arc::ptr_eq(old, &track))
                        || cached.width != width
                        || cached.ready != ready
                        || cached.frames != frames
                    {
                        cached.columns = display_columns(
                            &track,
                            0,
                            columns,
                            COLUMN_WIDTH as f64 / width as f64 * total as f64,
                        );
                        cached.track = Some(track.clone());
                        cached.width = width;
                        cached.ready = ready;
                        cached.frames = frames;
                    }
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        paint_columns(
                            window,
                            bounds,
                            &cached.columns,
                            0.,
                            deck_palette(deck),
                            eq_gains,
                        );
                    });
                    window.paint_quad(fill(
                        Bounds::new(
                            bounds.origin + point(px(played_x), px(0.)),
                            size(px(2.), bounds.size.height),
                        ),
                        playhead,
                    ));
                },
            )
            .size_full(),
        )
        .child(flags)
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that would bring in GPUI's own `test` attribute.
    use super::{DisplayColumn, VISIBLE_SECONDS, grid_lines, smooth_columns};
    use djcraft_library::TempoResult;

    #[test]
    fn stem_view_samples_real_source_envelopes_without_moving_the_timeline() {
        let track = djcraft_engine::Track::from_samples("stems", 48000, vec![0.5; 256 * 2 * 8]);
        track
            .attach_stems([
                vec![0.1; 256 * 2 * 8].into_boxed_slice(),
                vec![0.; 256 * 2 * 8].into_boxed_slice(),
                vec![0.05; 256 * 2 * 8].into_boxed_slice(),
                vec![0.2; 256 * 2 * 8].into_boxed_slice(),
            ])
            .unwrap();
        for (stem, expected) in [0.1f32, 0.15, 0.05, 0.2].into_iter().enumerate() {
            let columns = super::display_source_columns(&track, 0, 8, 1., Some(stem));
            assert!((columns[3].unwrap().energy[0] - expected * expected).abs() < 1e-5);
            let preroll = super::display_source_columns(&track, -8, 8, 1., Some(stem));
            assert!(preroll[..8].iter().all(Option::is_none));
        }
    }

    #[test]
    fn matched_tempos_have_equal_beat_spacing_and_motion() {
        let a = TempoResult {
            bpm: 140.,
            first_beat_seconds: 0.25,
            downbeat: 0,
            confidence: 1.,
        };
        let b = TempoResult { bpm: 136., ..a };
        let ratio = a.bpm / b.bpm;
        for elapsed in [0., 0.1, 1., 7.] {
            let left = grid_lines(
                Some(a),
                0.25 + 32. * a.beat_seconds() + elapsed,
                800.,
                400.,
                1.,
            );
            let right = grid_lines(
                Some(b),
                0.25 + 32. * b.beat_seconds() + elapsed * ratio,
                800.,
                400.,
                ratio,
            );
            // Ignore viewport-edge rounding when a beat falls exactly on the edge.
            let left: Vec<_> = left
                .into_iter()
                .filter(|(x, _)| *x > 1. && *x < 799.)
                .collect();
            let right: Vec<_> = right
                .into_iter()
                .filter(|(x, _)| *x > 1. && *x < 799.)
                .collect();
            assert_eq!(left.len(), right.len());
            for ((x, bar), (y, other_bar)) in left.iter().zip(&right) {
                assert!((x - y).abs() < 0.001);
                assert_eq!(bar, other_bar);
            }
        }
    }

    #[test]
    fn smoothing_keeps_bands_separate_and_transients_centred() {
        let mut raw = vec![Some(DisplayColumn::default()); 9];
        raw[4] = Some(DisplayColumn {
            energy: [1., 0., 0.],
        });
        let smoothed = smooth_columns(&raw);
        let levels: Vec<_> = smoothed.iter().map(|c| c.unwrap().energy[0]).collect();
        assert_eq!(levels, vec![0.1, 0.2, 0.4, 0.2, 0.1]);
        assert!(smoothed.iter().all(|c| c.unwrap().energy[1..] == [0., 0.]));
        let constant = vec![
            Some(DisplayColumn {
                energy: [0.1, 0.2, 0.3]
            });
            7
        ];
        for column in smooth_columns(&constant).into_iter().flatten() {
            for (actual, expected) in column.energy.into_iter().zip([0.1, 0.2, 0.3]) {
                assert!((actual - expected).abs() < 1e-6);
            }
        }
        assert!(smooth_columns(&[None; 5])[0].is_none());
    }

    #[test]
    fn grid_lines_sit_on_beats_with_bars_every_fourth() {
        // 120 BPM: a beat every half second, the first at 0.25 s, and bars
        // starting on the second beat (0.75 s, 2.75 s, ...).
        let grid = TempoResult {
            bpm: 120.,
            first_beat_seconds: 0.25,
            downbeat: 1,
            confidence: 1.,
        };
        let width = 800.;
        let pixels_per_second = width / VISIBLE_SECONDS as f32;
        let (playhead_x, position) = (400., 10.);
        let lines = grid_lines(Some(grid), position, width, playhead_x, 1.);

        // Eight seconds are in view, so sixteen beats.
        assert_eq!(lines.len(), 16);
        for (x, bar_line) in &lines {
            let seconds = position + ((x - playhead_x) / pixels_per_second) as f64;
            let beats = (seconds - 0.25) / 0.5;
            assert!(
                (beats - beats.round()).abs() < 1e-3,
                "{seconds} is not on a beat"
            );
            let bar = ((seconds - 0.75) / 2.).rem_euclid(1.);
            let on_bar = bar < 1e-3 || bar > 1. - 1e-3;
            assert_eq!(*bar_line, on_bar, "at {seconds} s");
        }
        assert_eq!(lines.iter().filter(|(_, bar)| *bar).count(), 4);
        // The beat at 9.75 s is a quarter second, 25 pixels, left of the playhead.
        assert!(lines.iter().any(|(x, _)| (x - 375.).abs() < 0.01));
    }

    #[test]
    fn grid_extends_sixteen_beats_before_file_start() {
        let grid = TempoResult {
            bpm: 120.,
            first_beat_seconds: 1.25,
            downbeat: 0,
            confidence: 1.,
        };
        assert!(grid_lines(None, 0., 800., 400., 1.).is_empty());
        let lines = grid_lines(Some(grid), 0., 800., 400., 1.);
        assert!(lines.iter().any(|(x, _)| *x < 400.));
        let start = grid_lines(Some(grid), -8., 800., 400., 1.);
        assert!(start.iter().all(|(x, _)| *x >= 400.));
        assert!(start.iter().any(|(x, _)| *x > 400.));
    }
}
