use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use djcraft_engine::Track;
use djcraft_library::TempoResult;
use gpui_kit::*;

/// Playback seconds visible across the full width of the waveform.
pub const VISIBLE_SECONDS: f64 = 8.;
// One logical pixel per envelope sample keeps attacks defined; the symmetric
// smoothing kernel removes jagged edges without shifting transient positions.
const COLUMN_WIDTH: f32 = 1.;

/// Low/mid/high: shadow, deck primary, highlight.
pub(crate) fn deck_palette(deck: usize) -> [u32; 3] {
    [
        [0x607b1b, 0xc8ff38, 0xefffc2],
        [0x203b8e, 0x3864e8, 0xb6c8ff],
        [0x895218, 0xffb347, 0xffe3b5],
        [0x6c3894, 0xcf85ff, 0xefceff],
    ][deck % djcraft_protocol::DECKS]
}

fn attack_colour(primary: u32) -> Hsla {
    let mut colour = Hsla::from(rgb(primary));
    colour.s *= 0.08;
    colour.l = 0.94;
    colour
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
    peak: f32,
    attack: f32,
}

// Symmetric display-only filter: no temporal lag or shift of beat positions.
#[cfg(test)]
fn smooth_columns(raw: &[Option<DisplayColumn>]) -> Vec<Option<DisplayColumn>> {
    const WEIGHTS: [f32; 5] = [0.1, 0.2, 0.4, 0.2, 0.1];
    smooth_columns_weighted(raw, WEIGHTS)
}

fn smooth_columns_weighted(
    raw: &[Option<DisplayColumn>],
    weights: [f32; 5],
) -> Vec<Option<DisplayColumn>> {
    if weights == [0., 0., 1., 0., 0.] {
        return raw[2..raw.len() - 2].to_vec();
    }
    raw.windows(5)
        .map(|neighbours| {
            neighbours[2]?; // Never extend the waveform beyond available audio.
            let mut result = DisplayColumn {
                peak: neighbours[2]?.peak,
                attack: neighbours[2]?.attack,
                ..DisplayColumn::default()
            };
            for (column, weight) in neighbours.iter().zip(weights) {
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

#[cfg(test)]
fn display_columns(
    track: &Track,
    first: i64,
    count: usize,
    step: f64,
) -> Vec<Option<DisplayColumn>> {
    let levels: Vec<_> = (0..track.peak_count()).map(|i| track.bands(i)[0]).collect();
    let attacks = djcraft_analysis::bass_attacks(
        &levels,
        track.frames_per_peak() as f64 / track.sample_rate as f64,
    );
    display_source_columns(track, first, count, step, None, &attacks)
}

fn display_source_columns(
    track: &Track,
    first: i64,
    count: usize,
    step: f64,
    stem: Option<usize>,
    attacks: &[djcraft_analysis::BassAttack],
) -> Vec<Option<DisplayColumn>> {
    display_source_columns_weighted(
        track,
        first,
        count,
        step,
        stem,
        attacks,
        [0.1, 0.2, 0.4, 0.2, 0.1],
    )
}

fn display_source_columns_weighted(
    track: &Track,
    first: i64,
    count: usize,
    step: f64,
    stem: Option<usize>,
    attacks: &[djcraft_analysis::BassAttack],
    weights: [f32; 5],
) -> Vec<Option<DisplayColumn>> {
    let ready = track.peak_count() as i64;
    let dt = track.frames_per_peak() as f64 / track.sample_rate as f64;
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
            let peak = if stem.is_none() {
                range.map(|i| track.peak(i as usize)).fold(0., f32::max)
            } else {
                0.
            };
            let from = start.max(0) as f64;
            let to = end.max(start + 1) as f64;
            let lo = attacks.partition_point(|attack| attack.index as f64 + 0.045 / dt < from);
            let hi = attacks.partition_point(|attack| (attack.index as f64) < to);
            Some(DisplayColumn {
                peak,
                attack: attacks[lo.min(hi)..hi]
                    .iter()
                    .filter_map(|attack| {
                        let at = attack.index as f64;
                        if at < to && at + 0.045 / dt >= from {
                            Some(attack.strength)
                        } else {
                            None
                        }
                    })
                    .fold(0., f32::max),
                energy: bands.map(|v| v * v / (end - start).max(1) as f32),
            })
        })
        .collect();
    smooth_columns_weighted(&raw, weights)
}

fn paint_columns(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    columns: &[Option<DisplayColumn>],
    shift: f32,
    palette: [u32; 3],
    gains: [f32; 3],
    mesh: &mut Option<WaveformMesh>,
    show_attacks: bool,
    rgb_waveforms: bool,
) {
    // Keep the actual peak silhouette separate from the smoothed RMS body.
    // This preserves brief attacks instead of averaging their energy away.
    let mesh = mesh.get_or_insert_with(|| WaveformMesh::new(columns, gains));
    if mesh.gains != gains {
        *mesh = WaveformMesh::new(columns, gains);
    }
    if rgb_waveforms {
        let colours = mesh.rgb.get_or_insert_with(|| rgb_meshes(columns, gains));
        for (tint, path) in colours {
            paint_mesh(window, bounds, path, shift, *tint, 0.85);
        }
        return;
    }
    paint_mesh(window, bounds, &mesh.peak, shift, palette[1], 0.55);
    // Low sits behind mid and high; each keeps its own amplitude envelope.
    for (band, tint) in palette.into_iter().enumerate() {
        if gains[band] > 0. {
            paint_mesh(window, bounds, &mesh.bands[band], shift, tint, 0.85);
        }
    }
    // Warm attack colouring stays inside the existing peak silhouette.
    // It never draws a separate full-height event/grid line over the waveform.
    if !show_attacks {
        return;
    }
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for (i, column) in columns.iter().enumerate() {
            let Some(column) = column.filter(|column| column.attack > 0.04) else {
                continue;
            };
            let x = i as f32 * COLUMN_WIDTH - shift;
            if x < -COLUMN_WIDTH || x > bounds.size.width.as_f32() {
                continue;
            }
            let strength = (column.attack * gains[0]).clamp(0., 1.);
            let half = peak_height(column, gains) * bounds.size.height.as_f32() * 0.48;
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin
                        + point(
                            px(i as f32 * COLUMN_WIDTH - shift),
                            bounds.size.height / 2. - px(half),
                        ),
                    size(px(COLUMN_WIDTH + 0.2), px(half * 2.)),
                ),
                attack_colour(palette[1]).opacity((strength * 1.4).min(0.85)),
            ));
        }
    });
}

fn peak_height(column: DisplayColumn, gains: [f32; 3]) -> f32 {
    let energy: f32 = column.energy.iter().sum();
    let adjusted: f32 = column
        .energy
        .iter()
        .zip(gains)
        .map(|(e, g)| e * g * g)
        .sum();
    let gain = if energy > 1e-12 {
        (adjusted / energy).sqrt()
    } else {
        gains.into_iter().fold(0., f32::max)
    };
    (column.peak * gain).clamp(0., 1.)
}

// Consecutive envelope columns form trapezoids. Building these directly avoids
// general-purpose polygon tessellation and preserves the same linear silhouette.
fn envelope_mesh(
    columns: &[Option<DisplayColumn>],
    half_height: impl Fn(DisplayColumn) -> f32,
) -> Path<Pixels> {
    let mut path = Path::new(point(px(0.), px(0.)));
    let mut previous = None;
    for (i, column) in columns.iter().enumerate() {
        let Some(column) = column else {
            previous = None;
            continue;
        };
        let x = i as f32 * COLUMN_WIDTH;
        let half = half_height(*column);
        if let Some((left, old_half)) = previous {
            let a = point(px(left), px(0.5 - old_half));
            let b = point(px(x), px(0.5 - half));
            let c = point(px(x), px(0.5 + half));
            let d = point(px(left), px(0.5 + old_half));
            let fill = (point(0., 1.), point(0., 1.), point(0., 1.));
            path.push_triangle((a, b, c), fill);
            path.push_triangle((a, c, d), fill);
        }
        previous = Some((x, half));
    }
    path
}

struct WaveformMesh {
    gains: [f32; 3],
    peak: Path<Pixels>,
    bands: [Path<Pixels>; 3],
    rgb: Option<Vec<(u32, Path<Pixels>)>>,
}
impl WaveformMesh {
    fn new(columns: &[Option<DisplayColumn>], gains: [f32; 3]) -> Self {
        Self {
            gains,
            rgb: None,
            peak: envelope_mesh(columns, |column| peak_height(column, gains) * 0.48),
            bands: std::array::from_fn(|band| {
                envelope_mesh(columns, |column| {
                    (column.energy[band].max(0.).sqrt() * gains[band] * std::f32::consts::SQRT_2)
                        .min(1.)
                        * 0.5
                })
            }),
        }
    }
}

// Additive RGB: each channel follows its band's post-EQ amplitude. Normalize
// hue independently of waveform height so quiet passages retain readable colour.
fn mixed_rgb(energy: [f32; 3], gains: [f32; 3]) -> u32 {
    let amplitudes: [f32; 3] = std::array::from_fn(|i| {
        let value = energy[i].max(0.).sqrt() * gains[i].max(0.);
        if value.is_finite() { value } else { 0. }
    });
    let maximum = amplitudes.into_iter().fold(0., f32::max);
    if maximum <= 1e-12 {
        return 0;
    }
    // Quantization lets adjacent colours share cached geometry (512 hues).
    // Suppress weaker channels for more saturated hues without changing the
    // full-brightness dominant channel or balanced secondary colours.
    let channels = amplitudes.map(|v| {
        let contrast = (v / maximum).powf(1.2);
        (contrast.mul_add(7., 0.).round() as u32) * 255 / 7
    });
    channels[0] << 16 | channels[1] << 8 | channels[2]
}

fn rgb_meshes(columns: &[Option<DisplayColumn>], gains: [f32; 3]) -> Vec<(u32, Path<Pixels>)> {
    let mut paths = std::collections::BTreeMap::new();
    for (i, pair) in columns.windows(2).enumerate() {
        let (Some(left), Some(right)) = (pair[0], pair[1]) else {
            continue;
        };
        let energy = std::array::from_fn(|band| (left.energy[band] + right.energy[band]) * 0.5);
        let tint = mixed_rgb(energy, gains);
        if tint == 0 {
            continue;
        }
        let path = paths
            .entry(tint)
            .or_insert_with(|| Path::new(point(px(0.), px(0.))));
        let x = i as f32 * COLUMN_WIDTH;
        let a = peak_height(left, gains) * 0.48;
        let b = peak_height(right, gains) * 0.48;
        let tl = point(px(x), px(0.5 - a));
        let bl = point(px(x), px(0.5 + a));
        let tr = point(px(x + COLUMN_WIDTH), px(0.5 - b));
        let br = point(px(x + COLUMN_WIDTH), px(0.5 + b));
        let fill = (point(0., 1.), point(0., 1.), point(0., 1.));
        path.push_triangle((tl, tr, br), fill);
        path.push_triangle((tl, br, bl), fill);
    }
    paths.into_iter().collect()
}

fn placed_mesh(source: &Path<Pixels>, bounds: Bounds<Pixels>, shift: f32) -> Path<Pixels> {
    let mut path = source.clone();
    let height = bounds.size.height.as_f32();
    for vertex in &mut path.vertices {
        vertex.xy_position.x += bounds.origin.x - px(shift);
        vertex.xy_position.y = bounds.origin.y + px(vertex.xy_position.y.as_f32() * height);
    }
    path.bounds.origin.x += bounds.origin.x - px(shift);
    path.bounds.origin.y = bounds.origin.y + px(path.bounds.origin.y.as_f32() * height);
    path.bounds.size.height *= height;
    path
}

fn paint_mesh(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    mesh: &Path<Pixels>,
    shift: f32,
    tint: u32,
    opacity: f32,
) {
    if mesh.vertices.is_empty() {
        return;
    }
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_path(
            placed_mesh(mesh, bounds, shift),
            Hsla::from(rgb(tint)).opacity(opacity),
        );
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
    rgb_waveforms: bool,
    eq_gains: [f32; 3],
    stem_gains: [f32; 4],
    cache: Rc<RefCell<OverviewCache>>,
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
                    let mut cached = cache.borrow_mut();
                    cached.prepare_attacks(&track);
                    if cached.attacks_started && cached.attacks.get().is_none() {
                        window.request_animation_frame();
                    }
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
                                cached.scrolling_columns(
                                    &track,
                                    first,
                                    columns as usize,
                                    peaks_per_column,
                                    Some(stem),
                                );
                                let tile = cached.scrolling[stem + 1].as_mut().unwrap();
                                let gains = [stem_gains[stem], 0., 0.];
                                let mesh = tile
                                    .mesh
                                    .get_or_insert_with(|| WaveformMesh::new(&tile.data, gains));
                                if mesh.gains != gains {
                                    *mesh = WaveformMesh::new(&tile.data, gains);
                                }
                                paint_mesh(
                                    window,
                                    bounds,
                                    &mesh.bands[0],
                                    (first - tile.first) as f32 * COLUMN_WIDTH + shift,
                                    tint,
                                    0.55,
                                );
                            }
                        } else {
                            cached.scrolling_columns(
                                &track,
                                first,
                                columns as usize,
                                peaks_per_column,
                                None,
                            );
                            let tile = cached.scrolling[0].as_mut().unwrap();
                            paint_columns(
                                window,
                                bounds,
                                &tile.data,
                                (first - tile.first) as f32 * COLUMN_WIDTH + shift,
                                deck_palette(deck),
                                eq_gains,
                                &mut tile.mesh,
                                true,
                                rgb_waveforms,
                            );
                        }
                        let x = (columns / 2) as f32 * COLUMN_WIDTH;
                        window.paint_quad(fill(
                            Bounds::new(
                                bounds.origin + point(px(x), px(0.)),
                                size(px(2.), px(height)),
                            ),
                            playhead,
                        ));
                        // Paint grid ticks last so bright waveform peaks cannot hide them.
                        for (line_x, bar_line) in
                            grid_lines(grid, position_seconds, width, x, tempo)
                        {
                            paint_grid_ticks(window, bounds, line_x, height, bar_line);
                        }
                    });
                },
            )
            .size_full(),
        )
        .child(flags)
}

fn paint_grid_ticks(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    x: f32,
    height: f32,
    bar_line: bool,
) {
    let width = if bar_line { 2. } else { 1. };
    let length = if bar_line { 11. } else { 8. };
    let x = x.round();
    for top in [0., (height - length).max(0.)] {
        // Three concentric dark layers give the white tick a 3 px glow.
        for radius in (1..=3).rev() {
            let radius = radius as f32;
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + point(px(x - radius), px(top - radius)),
                    size(px(width + radius * 2.), px(length + radius * 2.)),
                ),
                rgb(0x000000).opacity(if radius == 3. { 0.45 } else { 0.8 }),
            ));
        }
        window.paint_quad(fill(
            Bounds::new(
                bounds.origin + point(px(x), px(top)),
                size(px(width), px(length)),
            ),
            rgb(0xffffff),
        ));
    }
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
    mesh: Option<WaveformMesh>,
    analysis_track: Option<Arc<Track>>,
    attacks: Arc<OnceLock<Vec<djcraft_analysis::BassAttack>>>,
    attacks_started: bool,
    overview_has_attacks: bool,
    detailed_overview: bool,
    scrolling: [Option<ScrollColumns>; 5],
}

struct ScrollColumns {
    first: i64,
    step: f64,
    ready: usize,
    has_attacks: bool,
    data: Vec<Option<DisplayColumn>>,
    mesh: Option<WaveformMesh>,
}

impl OverviewCache {
    fn prepare_attacks(&mut self, track: &Arc<Track>) {
        if self
            .analysis_track
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, track))
        {
            self.analysis_track = Some(track.clone());
            self.attacks = Arc::default();
            self.attacks_started = false;
            self.scrolling = std::array::from_fn(|_| None);
        }
        let ready = track.peak_count();
        let total = track.frames().div_ceil(track.frames_per_peak());
        if !self.attacks_started && ready > 0 && (track.is_complete() || ready == total) {
            self.attacks_started = true;
            let result = self.attacks.clone();
            let track = track.clone();
            std::thread::spawn(move || {
                let _ = result.set(crate::attack_cache::load_or_analyse(&track));
            });
        }
    }

    fn scrolling_columns(
        &mut self,
        track: &Track,
        first: i64,
        count: usize,
        step: f64,
        stem: Option<usize>,
    ) -> &[Option<DisplayColumn>] {
        const MARGIN: i64 = 256;
        let slot = stem.map_or(0, |stem| stem + 1);
        let ready = track.peak_count();
        let has_attacks = self.attacks.get().is_some();
        let reuse = self.scrolling[slot].as_ref().is_some_and(|cache| {
            cache.step == step
                && cache.ready == ready
                && cache.has_attacks == has_attacks
                && first >= cache.first
                && first + count as i64 <= cache.first + cache.data.len() as i64
        });
        if !reuse {
            let start = first.div_euclid(MARGIN) * MARGIN - MARGIN;
            let attacks = if stem.is_none() {
                self.attacks.get().map(Vec::as_slice).unwrap_or_default()
            } else {
                &[]
            };
            let data = display_source_columns(
                track,
                start,
                count + MARGIN as usize * 3,
                step,
                stem,
                attacks,
            );
            self.scrolling[slot] = Some(ScrollColumns {
                first: start,
                step,
                ready,
                has_attacks,
                data,
                mesh: None,
            });
        }
        let cache = self.scrolling[slot].as_ref().unwrap();
        let offset = (first - cache.first) as usize;
        &cache.data[offset..offset + count]
    }
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
    rgb_waveforms: bool,
    detailed: bool,
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
                    cached.prepare_attacks(&track);
                    let has_attacks = cached.attacks.get().is_some();
                    if cached.attacks_started && !has_attacks {
                        window.request_animation_frame();
                    }
                    let ready = track.peak_count();
                    let frames = track.frames();
                    if cached
                        .track
                        .as_ref()
                        .is_none_or(|old| !Arc::ptr_eq(old, &track))
                        || cached.width != width
                        || cached.ready != ready
                        || cached.frames != frames
                        || cached.overview_has_attacks != has_attacks
                        || cached.detailed_overview != detailed
                    {
                        cached.columns = display_source_columns_weighted(
                            &track,
                            0,
                            columns,
                            COLUMN_WIDTH as f64 / width as f64 * total as f64,
                            None,
                            cached.attacks.get().map(Vec::as_slice).unwrap_or_default(),
                            if detailed {
                                [0., 0., 1., 0., 0.]
                            } else {
                                [0.1, 0.2, 0.4, 0.2, 0.1]
                            },
                        );
                        cached.track = Some(track.clone());
                        cached.width = width;
                        cached.ready = ready;
                        cached.frames = frames;
                        cached.overview_has_attacks = has_attacks;
                        cached.detailed_overview = detailed;
                        cached.mesh = None;
                    }
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        let waveform_bounds = Bounds::new(
                            bounds.origin,
                            size(bounds.size.width, (bounds.size.height - px(4.)).max(px(1.))),
                        );
                        let OverviewCache { columns, mesh, .. } = &mut *cached;
                        paint_columns(
                            window,
                            waveform_bounds,
                            columns,
                            0.,
                            deck_palette(deck),
                            if detailed { [0.7; 3] } else { [1.; 3] },
                            mesh,
                            false,
                            rgb_waveforms,
                        );
                        for (i, column) in cached.columns.iter().enumerate() {
                            let Some(column) = column.filter(|column| column.attack > 0.04) else {
                                continue;
                            };
                            let strength = column.attack.clamp(0., 1.);
                            if strength <= 0. {
                                continue;
                            }
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin
                                        + point(
                                            px(i as f32 * COLUMN_WIDTH),
                                            bounds.size.height - px(2.),
                                        ),
                                    size(px(1.), px(2.)),
                                ),
                                attack_colour(deck_palette(deck)[1])
                                    .opacity((strength * 1.5).min(0.9)),
                            ));
                        }
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
    #[test]
    fn rgb_mixes_bands_and_respects_eq() {
        use super::mixed_rgb;
        assert_eq!(mixed_rgb([1., 0., 0.], [1.; 3]), 0xff0000);
        assert_eq!(mixed_rgb([0., 1., 0.], [1.; 3]), 0x00ff00);
        assert_eq!(mixed_rgb([0., 0., 1.], [1.; 3]), 0x0000ff);
        assert_eq!(mixed_rgb([1., 1., 0.], [1.; 3]), 0xffff00);
        assert_eq!(mixed_rgb([1., 0., 1.], [1.; 3]), 0xff00ff);
        assert_eq!(mixed_rgb([0., 1., 1.], [1.; 3]), 0x00ffff);
        assert_eq!(mixed_rgb([1.; 3], [1.; 3]), 0xffffff);
        assert_eq!(mixed_rgb([1.; 3], [1., 0., 1.]), 0xff00ff);
        assert_eq!(mixed_rgb([1.; 3], [0.; 3]), 0);
        assert_eq!(mixed_rgb([0.; 3], [1.; 3]), 0);
        assert_eq!(mixed_rgb([0.01, 0.01, 0.], [1.; 3]), 0xffff00);
        // Weak background energy no longer washes dominant or mixed hues out.
        assert_eq!(mixed_rgb([1., 0.01, 0.01], [1.; 3]), 0xff0000);
        assert_eq!(mixed_rgb([1., 1., 0.01], [1.; 3]), 0xffff00);
        assert_eq!(mixed_rgb([1., 0.01, 1.], [1.; 3]), 0xff00ff);
        assert_eq!(mixed_rgb([0.01, 1., 1.], [1.; 3]), 0x00ffff);
    }

    #[test]
    fn rgb_geometry_keeps_gaps_and_silhouette() {
        let column = super::DisplayColumn {
            peak: 0.8,
            energy: [1., 1., 0.],
            attack: 0.,
        };
        let meshes = super::rgb_meshes(
            &[Some(column), Some(column), None, Some(column), Some(column)],
            [1.; 3],
        );
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].0, 0xffff00);
        assert_eq!(meshes[0].1.vertices.len(), 12);
        assert!(super::rgb_meshes(&[Some(column); 2], [0.; 3]).is_empty());
    }

    #[test]
    fn cached_mesh_preserves_gaps_and_moves_without_changing_shape() {
        use gpui_kit::{Bounds, point, px, size};
        let column = super::DisplayColumn {
            peak: 0.8,
            energy: [0.2; 3],
            attack: 0.,
        };
        let source = super::envelope_mesh(
            &[Some(column), Some(column), None, Some(column), Some(column)],
            |_| 0.25,
        );
        assert_eq!(source.vertices.len(), 12);
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(100.), px(80.)));
        let moved = super::placed_mesh(&source, bounds, 3.);
        for (original, placed) in source.vertices.iter().zip(&moved.vertices) {
            assert_eq!(placed.xy_position.x, original.xy_position.x + px(7.));
            assert_eq!(
                placed.xy_position.y,
                px(20. + original.xy_position.y.as_f32() * 80.)
            );
        }
    }
    // Not `super::*`: that would bring in GPUI's own `test` attribute.
    #[test]
    fn scrolling_reuses_columns_and_keeps_global_source_alignment() {
        let track = djcraft_engine::Track::from_samples("cache", 48000, vec![0.3; 48000 * 8]);
        let mut cache = super::OverviewCache::default();
        cache.scrolling_columns(&track, -16, 160, 1., None);
        let allocation = cache.scrolling[0].as_ref().unwrap().data.as_ptr();
        for first in [-16, 0, 16, 128] {
            let actual = cache
                .scrolling_columns(&track, first, 160, 1., None)
                .to_vec();
            let expected = super::display_source_columns(&track, first, 160, 1., None, &[]);
            for (a, b) in actual.iter().zip(&expected) {
                assert_eq!(a.is_some(), b.is_some());
                if let (Some(a), Some(b)) = (a, b) {
                    assert_eq!(a.peak, b.peak);
                    assert_eq!(a.energy, b.energy);
                    assert_eq!(a.attack, b.attack);
                }
            }
            assert_eq!(
                cache.scrolling[0].as_ref().unwrap().data.as_ptr(),
                allocation
            );
        }
        // Background completion invalidates cached colour data exactly once.
        cache
            .attacks
            .set(vec![djcraft_analysis::BassAttack {
                index: 100,
                strength: 1.,
            }])
            .unwrap();
        let data = cache.scrolling_columns(&track, 0, 160, 1., None);
        assert!(data[100].unwrap().attack > 0.9);
        let data = cache.scrolling_columns(&track, 0, 160, 2., None);
        assert!(data[50].unwrap().attack > 0.9);
    }
    #[test]
    fn integrated_attack_colour_stops_when_only_sustained_bass_remains() {
        let rate = 48000;
        let mut samples = Vec::new();
        for i in 0..rate * 4 {
            let t = i as f64 / rate as f64;
            let mut sample = 0.15 * (std::f64::consts::TAU * 90. * t).sin();
            for hit in [0.5, 1., 1.5] {
                let age = t - hit;
                if (0. ..0.15).contains(&age) {
                    sample += 0.6
                        * (-age / 0.035).exp()
                        * (std::f64::consts::TAU * (110. * age - 200. * age * age)).sin();
                }
            }
            samples.extend([sample as f32; 2]);
        }
        let track = djcraft_engine::Track::from_samples("break", rate, samples);
        let step = track.peak_count() as f64 / 400.;
        let columns = super::display_columns(&track, 0, 400, step);
        assert!(columns[40..180].iter().flatten().any(|c| c.attack > 0.1));
        assert!(columns[220..380].iter().flatten().all(|c| c.attack == 0.));
    }
    use super::{DisplayColumn, VISIBLE_SECONDS, grid_lines, smooth_columns};
    use djcraft_library::TempoResult;

    #[test]
    fn brief_transient_survives_column_aggregation_without_spreading() {
        let mut audio = vec![0.; 256 * 2 * 12];
        audio[256 * 2 * 5] = 1.;
        let track = djcraft_engine::Track::from_samples("impulse", 48000, audio);
        let detailed = super::display_columns(&track, 0, 10, 1.);
        assert_eq!(detailed[5].unwrap().peak, 1.);
        assert_eq!(detailed[4].unwrap().peak, 0.);
        assert_eq!(detailed[6].unwrap().peak, 0.);
        let zoomed_out = super::display_columns(&track, 0, 5, 2.);
        assert_eq!(zoomed_out[2].unwrap().peak, 1.);
        assert_eq!(zoomed_out[1].unwrap().peak, 0.);
        assert_eq!(zoomed_out[3].unwrap().peak, 0.);
    }

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
            let columns = super::display_source_columns(&track, 0, 8, 1., Some(stem), &[]);
            assert!((columns[3].unwrap().energy[0] - expected * expected).abs() < 1e-5);
            let preroll = super::display_source_columns(&track, -8, 8, 1., Some(stem), &[]);
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
            peak: 1.,
            attack: 1.,
        });
        let smoothed = smooth_columns(&raw);
        let levels: Vec<_> = smoothed.iter().map(|c| c.unwrap().energy[0]).collect();
        assert_eq!(levels, vec![0.1, 0.2, 0.4, 0.2, 0.1]);
        assert_eq!(
            smoothed.iter().map(|c| c.unwrap().peak).collect::<Vec<_>>(),
            vec![0., 0., 1., 0., 0.]
        );
        assert_eq!(
            smoothed
                .iter()
                .map(|c| c.unwrap().attack)
                .collect::<Vec<_>>(),
            vec![0., 0., 1., 0., 0.]
        );
        assert_eq!(super::peak_height(smoothed[2].unwrap(), [0.; 3]), 0.);
        assert_eq!(super::peak_height(smoothed[2].unwrap(), [1.; 3]), 1.);
        assert!(smoothed.iter().all(|c| c.unwrap().energy[1..] == [0., 0.]));
        let constant = vec![
            Some(DisplayColumn {
                energy: [0.1, 0.2, 0.3],
                peak: 0.7,
                attack: 0.,
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
    fn unsmoothed_overview_keeps_each_source_column() {
        let raw: Vec<_> = (0..9)
            .map(|i| {
                Some(DisplayColumn {
                    energy: [i as f32, 0., 0.],
                    ..Default::default()
                })
            })
            .collect();
        let columns = super::smooth_columns_weighted(&raw, [0., 0., 1., 0., 0.]);
        assert_eq!(
            columns
                .iter()
                .map(|column| column.unwrap().energy[0])
                .collect::<Vec<_>>(),
            vec![2., 3., 4., 5., 6.]
        );
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
