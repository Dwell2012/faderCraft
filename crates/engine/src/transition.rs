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
        assert_eq!(p.outgoing_seconds, 160.);
        assert_eq!(p.incoming_seconds, 0.);
        assert_eq!(p.bars, Some(16));
        assert_eq!(p.bass_swap, 0.5);
        assert!(
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
            .is_none()
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
