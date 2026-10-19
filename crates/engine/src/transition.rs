//! Pure control-thread planning. Manual markers take priority over suggestions.
use djcraft_protocol::{AutomixPlan, BeatGrid, PhraseLabel, TrackPreparation};
pub(crate) struct TrackContext<'a> {
    pub preparation: &'a TrackPreparation,
    pub grid: BeatGrid,
    pub duration: f64,
}
#[cfg(test)]
pub(crate) fn plan(
    out: TrackContext<'_>,
    incoming: TrackContext<'_>,
    position: f64,
    tempo: f64,
) -> Option<AutomixPlan> {
    plan_vocals(out, incoming, position, tempo, None, None)
}
pub(crate) fn plan_vocals(
    out: TrackContext<'_>,
    incoming: TrackContext<'_>,
    position: f64,
    tempo: f64,
    out_vocals: Option<&[[f64; 2]]>,
    in_vocals: Option<&[[f64; 2]]>,
) -> Option<AutomixPlan> {
    let a = out.preparation;
    let b = incoming.preparation;
    if a.phrases.is_empty() || b.phrases.is_empty() {
        return None;
    }
    if a.mix_out.is_none() && b.mix_in.is_none() && a.transition_bars.is_none() {
        if let Some(plan) = intro_plan(&out, &incoming, position, tempo, out_vocals, in_vocals) {
            return Some(plan);
        }
    }
    let ratio = out.grid.bpm * tempo / incoming.grid.bpm;
    let bar = 240. / (out.grid.bpm * tempo);
    let safe = |label| {
        matches!(
            label,
            PhraseLabel::Intro | PhraseLabel::Outro | PhraseLabel::Drop | PhraseLabel::Section
        )
    };
    let mut entries: Vec<_> = if let Some(t) = b.mix_in.filter(|t| *t < incoming.duration) {
        vec![(t, 0)]
    } else {
        b.phrases
            .iter()
            .filter(|p| p.seconds <= incoming.duration * 0.35 && safe(p.label))
            .map(|p| (p.seconds, if p.label == PhraseLabel::Intro { 0 } else { 1 }))
            .collect()
    };
    entries.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.total_cmp(&b.0)));
    let mut best: Option<AutomixPlan> = None;
    let manual_exit = a.mix_out.filter(|t| *t < out.duration);
    let mut exits: Vec<_> = if let Some(t) = manual_exit {
        vec![(t, 0)]
    } else {
        a.phrases
            .iter()
            .filter(|p| {
                p.seconds >= position
                    && p.seconds >= out.duration * 0.5
                    && p.seconds < out.duration
                    && safe(p.label)
            })
            .map(|p| (p.seconds, if p.label == PhraseLabel::Outro { 0 } else { 1 }))
            .collect()
    };
    exits.sort_by(|a, b| a.1.cmp(&b.1).then(b.0.total_cmp(&a.0)));
    let sizes: Vec<u8> = a
        .transition_bars
        .map_or_else(|| vec![32, 16, 8], |n| vec![n]);
    for (exit, _) in exits {
        for &(entry, _) in &entries {
            if exit + 0.02 < position {
                continue;
            }
            // An edited marker can intentionally be off-grid; don't move it to
            // manufacture alignment. The regular transition path honours it exactly.
            let phase = |t: f64, g: BeatGrid| {
                ((t - g.first_beat) * g.bpm / 60. - g.downbeat as f64).rem_euclid(4.)
            };
            let delta = (phase(exit, out.grid) - phase(entry, incoming.grid)).rem_euclid(4.);
            if delta.min(4. - delta) > 0.08 {
                continue;
            }
            for &bars in &sizes {
                let seconds = bars as f64 * bar;
                if exit + seconds * tempo > out.duration + 0.001
                    || entry + seconds * ratio > incoming.duration + 0.001
                {
                    continue;
                }
                // Avoid starting inside a build/break and do not carry the outgoing
                // track into a new build or breakdown partway through this transition.
                if a.phrases.iter().any(|p| {
                    p.seconds > exit + 0.02
                        && p.seconds < exit + seconds * tempo - 0.02
                        && matches!(p.label, PhraseLabel::Build | PhraseLabel::Breakdown)
                }) {
                    continue;
                }
                let bass_swap = if let Some(manual) = a.bass_swap {
                    manual
                } else {
                    // Prefer an actual section change. Otherwise use a four-bar
                    // subphrase boundary near the centre, never an arbitrary second.
                    let boundary = b
                        .phrases
                        .iter()
                        .map(|p| (p.seconds - entry) / ratio / seconds)
                        .chain(
                            a.phrases
                                .iter()
                                .map(|p| (p.seconds - exit) / tempo / seconds),
                        )
                        .filter(|f| (0.1..=0.9).contains(f))
                        .min_by(|x, y| (x - 0.5).abs().total_cmp(&(y - 0.5).abs()));
                    boundary.unwrap_or(
                        ((bars as f64 * 0.5 / 4.).round() * 4. / bars as f64).clamp(0.1, 0.9),
                    ) as f32
                };
                let mut candidate = AutomixPlan {
                    vocal_overlap_seconds: None,
                    vocal_note: String::new(),
                    outgoing_seconds: exit,
                    incoming_seconds: entry,
                    duration_seconds: seconds,
                    bars: Some(bars),
                    bass_swap,
                    eq_start: 0.,
                    phrase_aligned: true,
                    description: if manual_exit.is_some() || b.mix_in.is_some() {
                        "Phrase transition · saved mix points".into()
                    } else {
                        "Phrase transition · outro/section → intro/section".into()
                    },
                };
                annotate_vocals(&mut candidate, out_vocals, in_vocals, tempo, ratio);
                if best.as_ref().is_none_or(|previous| {
                    match (
                        candidate.vocal_overlap_seconds,
                        previous.vocal_overlap_seconds,
                    ) {
                        (Some(new), Some(old)) => new + 0.25 < old,
                        _ => false,
                    }
                }) {
                    best = Some(candidate);
                }
            }
        }
    }
    best
}

/// Measure the incoming intro, then back-time it from an outgoing section end.
/// All calculations stay in source time until constructing the playback plan.
fn intro_plan(
    out: &TrackContext<'_>,
    incoming: &TrackContext<'_>,
    position: f64,
    tempo: f64,
    out_vocals: Option<&[[f64; 2]]>,
    in_vocals: Option<&[[f64; 2]]>,
) -> Option<AutomixPlan> {
    if !tempo.is_finite() || tempo <= 0. || out.grid.bpm <= 0. || incoming.grid.bpm <= 0. {
        return None;
    }
    let in_bar = 240. / incoming.grid.bpm;
    let out_bar = 240. / out.grid.bpm;
    let anchor = |g: BeatGrid| g.first_beat + g.downbeat as f64 * 60. / g.bpm;
    let in_anchor = anchor(incoming.grid);
    let out_anchor = anchor(out.grid);
    let intro = incoming
        .preparation
        .phrases
        .iter()
        .filter(|p| p.label == PhraseLabel::Intro && p.seconds < incoming.duration * 0.35)
        .min_by(|a, b| a.seconds.total_cmp(&b.seconds))?;
    let entry = in_anchor + ((intro.seconds - in_anchor) / in_bar).ceil() * in_bar;
    // A new musical section or vocal entrance bounds the usable intro. Do not
    // assume that the first 32 bars of every record are safe to mix over.
    let section_end = incoming
        .preparation
        .phrases
        .iter()
        .filter(|p| p.seconds > entry + 0.02 && p.label != PhraseLabel::Intro)
        .map(|p| p.seconds)
        .min_by(f64::total_cmp)?;
    let vocal_start = in_vocals
        .into_iter()
        .flatten()
        .filter(|v| v[1] > entry)
        .map(|v| v[0].max(entry))
        .min_by(f64::total_cmp)
        .unwrap_or(section_end);
    let end = section_end.min(vocal_start).min(incoming.duration);
    let bars = ((end - entry) / in_bar + 0.01).floor().min(64.) as u8;
    if bars < 4 || entry < 0. {
        return None;
    }
    let ratio = out.grid.bpm * tempo / incoming.grid.bpm;
    let mut best: Option<(f64, AutomixPlan)> = None;
    // Finish before a breakdown/build, or at the end of the final section.
    // The latest complete bar avoids carrying a mix past the end of the file.
    let ends = out
        .preparation
        .phrases
        .iter()
        .filter(|p| {
            matches!(
                p.label,
                PhraseLabel::Breakdown | PhraseLabel::Build | PhraseLabel::Outro
            )
        })
        .map(|p| p.seconds)
        .chain(std::iter::once(out.duration));
    for boundary in ends {
        let finish = out_anchor + ((boundary - out_anchor) / out_bar).floor() * out_bar;
        let available = ((finish - position.max(out.duration * 0.5)) / out_bar + 0.00001)
            .floor()
            .max(0.) as u8;
        let bars = bars.min(available);
        if bars < 4 {
            continue;
        }
        let seconds = bars as f64 * out_bar / tempo;
        let start = finish - bars as f64 * out_bar;
        if start < position || start < out.duration * 0.5 || finish > out.duration {
            continue;
        }
        // Run the intro silently across earlier sections. Only expose it during
        // the final safe section, at most eight bars before the handover ends.
        let last_break = out
            .preparation
            .phrases
            .iter()
            .filter(|p| {
                p.seconds < finish - 0.02
                    && matches!(p.label, PhraseLabel::Build | PhraseLabel::Breakdown)
            })
            .map(|p| p.seconds)
            .max_by(f64::total_cmp);
        let safe_start = last_break
            .and_then(|at| {
                out.preparation
                    .phrases
                    .iter()
                    .filter(|p| {
                        p.seconds > at
                            && p.seconds < finish
                            && !matches!(p.label, PhraseLabel::Build | PhraseLabel::Breakdown)
                    })
                    .map(|p| p.seconds)
                    .min_by(f64::total_cmp)
            })
            .unwrap_or(start);
        if last_break.is_some_and(|at| at >= safe_start) {
            continue;
        }
        // An outro can lose its kick well before EOF. Complete the bass
        // exchange by that boundary, rather than centring it near file end.
        let rhythm_end = out
            .preparation
            .phrases
            .iter()
            .filter(|p| p.label == PhraseLabel::Outro && p.seconds > start && p.seconds <= finish)
            .map(|p| p.seconds)
            .min_by(f64::total_cmp)
            .unwrap_or(finish);
        let blend_start = safe_start.max(start).max(rhythm_end - 8. * out_bar);
        if finish - blend_start < out_bar {
            continue;
        }
        let eq_start = ((blend_start - start) / (finish - start)) as f32;
        let bass_end = ((rhythm_end - start) / (finish - start)) as f32;
        // mix_eq_held has an eight-percent half-width in its active span.
        let swap = (bass_end - (1. - eq_start) * 0.08).min(eq_start + (1. - eq_start) * 0.75);
        let mut plan = AutomixPlan {
            outgoing_seconds: start,
            incoming_seconds: entry,
            duration_seconds: seconds,
            bars: Some(bars),
            bass_swap: out
                .preparation
                .bass_swap
                .unwrap_or(swap)
                .max(eq_start + (1. - eq_start) * 0.1),
            eq_start,
            phrase_aligned: true,
            description: format!("Intro-aware · {bars} usable bars · section-end EQ handover"),
            vocal_overlap_seconds: None,
            vocal_note: String::new(),
        };
        annotate_vocals(&mut plan, out_vocals, in_vocals, tempo, ratio);
        // Prefer low vocal collision, then the latest safe musical exit.
        let score = plan.vocal_overlap_seconds.unwrap_or(0.) * 1000. - finish / out.duration;
        if best.as_ref().is_none_or(|(old, _)| score < *old) {
            best = Some((score, plan));
        }
    }
    best.map(|(_, plan)| plan)
}
/// Intersection of two sorted source-time vocal maps in transition wall-clock time.
pub(crate) fn overlap(
    out: &[[f64; 2]],
    incoming: &[[f64; 2]],
    exit: f64,
    entry: f64,
    duration: f64,
    out_speed: f64,
    in_speed: f64,
) -> f64 {
    let (mut i, mut j, mut seconds) = (0, 0, 0.);
    while i < out.len() && j < incoming.len() {
        let a = [
            (out[i][0] - exit) / out_speed,
            (out[i][1] - exit) / out_speed,
        ];
        let b = [
            (incoming[j][0] - entry) / in_speed,
            (incoming[j][1] - entry) / in_speed,
        ];
        seconds += (a[1].min(b[1]).min(duration) - a[0].max(b[0]).max(0.)).max(0.);
        if a[1] < b[1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    seconds
}
pub(crate) fn annotate_vocals(
    plan: &mut AutomixPlan,
    out: Option<&[[f64; 2]]>,
    incoming: Option<&[[f64; 2]]>,
    out_speed: f64,
    in_speed: f64,
) {
    plan.vocal_overlap_seconds = out.zip(incoming).map(|(a, b)| {
        overlap(
            a,
            b,
            plan.outgoing_seconds,
            plan.incoming_seconds,
            plan.duration_seconds,
            out_speed,
            in_speed,
        )
    });
    plan.vocal_note = match plan.vocal_overlap_seconds {
        Some(seconds) if seconds < 0.25 => "Vocal-aware · no significant overlap predicted".into(),
        Some(seconds) => {
            format!("Vocal overlap ~{seconds:.1}s · markers/available phrases constrain this mix")
        }
        None => "Vocal map unavailable · prepare stems on both tracks".into(),
    };
}
#[cfg(test)]
mod tests {
    use super::*;
    use djcraft_protocol::PhraseMarker;
    fn prep(points: &[(f64, PhraseLabel)]) -> TrackPreparation {
        TrackPreparation {
            phrases: points
                .iter()
                .map(|&(seconds, label)| PhraseMarker { seconds, label })
                .collect(),
            ..Default::default()
        }
    }
    fn grid() -> BeatGrid {
        BeatGrid {
            bpm: 120.,
            first_beat: 0.,
            downbeat: 0,
            confidence: 1.,
        }
    }
    #[test]
    fn lizard_motorcycle_sections_produce_intro_plan() {
        let a = prep(&[
            (0.30168, PhraseLabel::Intro),
            (291.78167, PhraseLabel::Breakdown),
            (306.00020, PhraseLabel::Drop),
            (355.76508, PhraseLabel::Outro),
        ]);
        let b = prep(&[
            (15.68686827, PhraseLabel::Intro),
            (87.17341614, PhraseLabel::Section),
        ]);
        let p = plan(
            TrackContext {
                preparation: &a,
                grid: BeatGrid {
                    bpm: 135.035,
                    first_beat: 0.3016818265,
                    confidence: 0.29602849,
                    ..grid()
                },
                duration: 371.,
            },
            TrackContext {
                preparation: &b,
                grid: BeatGrid {
                    bpm: 134.291,
                    first_beat: 15.2400773458,
                    confidence: 0.28559553,
                    ..grid()
                },
                duration: 689.,
            },
            261.,
            1.,
        )
        .unwrap();
        assert!(p.description.contains("Intro-aware"));
        assert_eq!(p.bars, Some(39));
        let eq_at = p.outgoing_seconds + p.duration_seconds * p.eq_start as f64;
        let bass_done = p.outgoing_seconds
            + p.duration_seconds * (p.bass_swap + (1. - p.eq_start) * 0.08) as f64;
        assert!(eq_at < 342.);
        assert!(bass_done <= 355.76508 + 0.001);
        assert!(p.duration_seconds > 60.);
        assert!(p.outgoing_seconds >= 261.);
    }
    #[test]
    fn intro_length_back_times_exit_and_vocals_bound_available_space() {
        let a = prep(&[
            (0., PhraseLabel::Intro),
            (64., PhraseLabel::Drop),
            (240., PhraseLabel::Outro),
        ]);
        for (intro, vocals, expected) in [
            (24., None, 12),
            (48., None, 24),
            (48., Some([[16., 40.]]), 8),
        ] {
            let b = prep(&[(0., PhraseLabel::Intro), (intro, PhraseLabel::Drop)]);
            let p = plan_vocals(
                TrackContext {
                    preparation: &a,
                    grid: grid(),
                    duration: 300.,
                },
                TrackContext {
                    preparation: &b,
                    grid: grid(),
                    duration: 200.,
                },
                100.,
                1.05,
                Some(&[]),
                vocals.as_ref().map(|v| v.as_slice()),
            )
            .unwrap();
            assert_eq!(p.bars, Some(expected));
            assert!((p.outgoing_seconds - (300. - expected as f64 * 2.)).abs() < 1e-6);
            assert!((p.duration_seconds * 1.05 - expected as f64 * 2.).abs() < 1e-6);
            assert!(p.description.contains("Intro-aware"));
            assert!(p.bass_swap > 0.5);
        }
    }
    #[test]
    fn prefers_outro_and_intro_and_fits_whole_phrases() {
        let a = prep(&[
            (0., PhraseLabel::Intro),
            (80., PhraseLabel::Breakdown),
            (120., PhraseLabel::Drop),
            (160., PhraseLabel::Outro),
        ]);
        let b = prep(&[(0., PhraseLabel::Intro), (32., PhraseLabel::Drop)]);
        let p = plan(
            TrackContext {
                preparation: &a,
                grid: grid(),
                duration: 200.,
            },
            TrackContext {
                preparation: &b,
                grid: grid(),
                duration: 180.,
            },
            50.,
            1.,
        )
        .unwrap();
        assert_eq!(p.outgoing_seconds, 168.);
        assert_eq!(p.incoming_seconds, 0.);
        assert_eq!(p.bars, Some(16));
        assert_eq!(p.bass_swap, 0.875);
        assert_eq!(
            plan(
                TrackContext {
                    preparation: &a,
                    grid: grid(),
                    duration: 165.
                },
                TrackContext {
                    preparation: &b,
                    grid: grid(),
                    duration: 180.
                },
                155.,
                1.
            )
            .unwrap()
            .bars,
            Some(4)
        );
    }
    #[test]
    fn respects_manual_points_and_lengths_and_never_moves_them() {
        let mut a = prep(&[(0., PhraseLabel::Intro), (100., PhraseLabel::Outro)]);
        let mut b = prep(&[(0., PhraseLabel::Intro)]);
        a.mix_out = Some(80.);
        a.transition_bars = Some(8);
        a.bass_swap = Some(0.25);
        b.mix_in = Some(12.);
        let make = |a: &TrackPreparation, b: &TrackPreparation| {
            plan(
                TrackContext {
                    preparation: a,
                    grid: grid(),
                    duration: 200.,
                },
                TrackContext {
                    preparation: b,
                    grid: grid(),
                    duration: 180.,
                },
                10.,
                1.,
            )
        };
        let p = make(&a, &b).unwrap();
        assert_eq!(p.outgoing_seconds, 80.);
        assert_eq!(p.incoming_seconds, 12.);
        assert_eq!(p.bars, Some(8));
        assert_eq!(p.bass_swap, 0.25);
        b.mix_in = Some(12.5);
        assert!(make(&a, &b).is_none());
    }
    #[test]
    fn refuses_missing_sections_and_avoids_crossing_a_build() {
        let a = prep(&[
            (0., PhraseLabel::Intro),
            (100., PhraseLabel::Drop),
            (110., PhraseLabel::Build),
        ]);
        let b = prep(&[(0., PhraseLabel::Intro)]);
        assert!(
            plan(
                TrackContext {
                    preparation: &a,
                    grid: grid(),
                    duration: 180.
                },
                TrackContext {
                    preparation: &b,
                    grid: grid(),
                    duration: 100.
                },
                80.,
                1.
            )
            .is_none()
        );
        assert!(
            plan(
                TrackContext {
                    preparation: &TrackPreparation::default(),
                    grid: grid(),
                    duration: 180.
                },
                TrackContext {
                    preparation: &b,
                    grid: grid(),
                    duration: 100.
                },
                80.,
                1.
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod vocal_tests {
    use super::*;
    use djcraft_protocol::PhraseMarker;
    fn preparation(points: &[(f64, PhraseLabel)]) -> TrackPreparation {
        TrackPreparation {
            phrases: points
                .iter()
                .map(|&(seconds, label)| PhraseMarker { seconds, label })
                .collect(),
            transition_bars: Some(8),
            ..Default::default()
        }
    }
    #[test]
    fn chooses_quieter_entry_but_keeps_manual_entry_and_reports_conflict() {
        let a = preparation(&[(0., PhraseLabel::Intro), (100., PhraseLabel::Outro)]);
        let mut b = preparation(&[
            (0., PhraseLabel::Intro),
            (32., PhraseLabel::Section),
            (150., PhraseLabel::Drop),
        ]);
        let g = BeatGrid {
            bpm: 120.,
            first_beat: 0.,
            downbeat: 0,
            confidence: 1.,
        };
        let run = |b: &TrackPreparation, in_map: Option<&[[f64; 2]]>| {
            plan_vocals(
                TrackContext {
                    preparation: &a,
                    grid: g,
                    duration: 140.,
                },
                TrackContext {
                    preparation: b,
                    grid: g,
                    duration: 180.,
                },
                10.,
                1.,
                Some(&[[100., 140.]]),
                in_map,
            )
            .unwrap()
        };
        let p = run(&b, Some(&[[0., 20.]]));
        assert_eq!(p.incoming_seconds, 32.);
        assert_eq!(p.vocal_overlap_seconds, Some(0.));
        let unknown = run(&b, None);
        assert_eq!(unknown.incoming_seconds, 0.);
        assert!(unknown.vocal_overlap_seconds.is_none());
        b.mix_in = Some(0.);
        let fixed = run(&b, Some(&[[0., 20.]]));
        assert_eq!(fixed.incoming_seconds, 0.);
        assert_eq!(fixed.vocal_overlap_seconds, Some(16.));
        assert!(fixed.vocal_note.contains("overlap"));
    }
    #[test]
    fn compares_source_regions_at_actual_playback_speeds() {
        assert_eq!(
            overlap(&[[10., 14.]], &[[20., 24.]], 10., 20., 8., 2., 1.),
            2.
        );
        assert_eq!(
            overlap(&[[0., 2.], [4., 6.]], &[[1., 5.]], 0., 0., 10., 1., 1.),
            2.
        );
        assert_eq!(overlap(&[], &[[0., 20.]], 0., 0., 10., 1., 1.), 0.);
    }
}
