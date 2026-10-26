use djcraft_protocol::{PhraseLabel, PhraseMarker, TrackPreparation};
use std::{collections::BTreeMap, path::Path};
pub type Preparations = BTreeMap<String, TrackPreparation>;
pub fn load(root: &Path) -> Preparations {
    crate::track_identity::normalize(
        root,
        "track-preparation.json",
        std::fs::read(root.join("track-preparation.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Preparations>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, p)| p.valid())
            .collect(),
    )
}
pub fn save(root: &Path, preparations: &Preparations) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let temporary = root.join("track-preparation.json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(preparations).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, root.join("track-preparation.json")).map_err(|e| e.to_string())
}
pub fn suggest(
    track: &djcraft_engine::Track,
    tempo: Option<djcraft_analysis::Tempo>,
) -> TrackPreparation {
    let Some(structure) =
        djcraft_analysis::detect_structure(&track.mono(), track.sample_rate, tempo)
    else {
        return TrackPreparation::default();
    };
    use djcraft_analysis::SectionLabel as S;
    let phrases: Vec<_> = structure
        .cues
        .iter()
        .zip(djcraft_analysis::label_sections(&structure))
        .map(|(cue, label)| PhraseMarker {
            seconds: cue.seconds,
            label: match label {
                S::Intro => PhraseLabel::Intro,
                S::Build => PhraseLabel::Build,
                S::Breakdown => PhraseLabel::Breakdown,
                S::Drop => PhraseLabel::Drop,
                S::Outro => PhraseLabel::Outro,
                S::Section => PhraseLabel::Section,
            },
        })
        .collect();
    TrackPreparation {
        phrases,
        ..Default::default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_survive_reload_and_silence_has_no_invented_sections() {
        let root = std::env::temp_dir().join(format!("djcraft-preparation-{}", std::process::id()));
        let p = TrackPreparation {
            phrases: vec![PhraseMarker {
                seconds: 2.,
                label: PhraseLabel::Build,
            }],
            mix_in: Some(2.),
            mix_out: Some(12.),
            transition_bars: Some(16),
            bass_swap: Some(0.7),
        };
        let data = BTreeMap::from([("track.wav".into(), p)]);
        save(&root, &data).unwrap();
        assert_eq!(load(&root), data);
        let track = djcraft_engine::Track::from_samples("silence", 1000, vec![0.; 4000]);
        assert_eq!(suggest(&track, None), TrackPreparation::default());
        std::fs::remove_file(root.join("track-preparation.json")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

#[derive(Clone, Debug)]
pub struct SuggestedCue {
    pub seconds: f64,
    pub label: &'static str,
}
pub fn cue_suggestions(
    preparation: &TrackPreparation,
    vocals: Option<&[[f64; 2]]>,
    grid: Option<djcraft_analysis::Tempo>,
    duration: f64,
) -> Vec<SuggestedCue> {
    let Some(grid) = grid.filter(|g| {
        g.bpm.is_finite()
            && g.bpm > 0.
            && g.first_beat_seconds.is_finite()
            && g.first_beat_seconds >= 0.
            && g.downbeat < 4
    }) else {
        return Vec::new();
    };
    let first_bar = grid.first_bar_seconds();
    let bar = 4. * grid.beat_seconds();
    if !duration.is_finite() || duration <= first_bar {
        return Vec::new();
    }
    // Never extrapolate into leading silence or place a cue at/past the track end.
    let last_bar = ((duration - first_bar) / bar).ceil() - 1.;
    let mut cues = Vec::new();
    let mut add = |seconds: Option<f64>, label| {
        if let Some(seconds) = seconds.filter(|t| t.is_finite() && *t >= 0. && *t < duration) {
            let seconds =
                first_bar + ((seconds - first_bar) / bar).round().clamp(0., last_bar) * bar;
            if cues
                .iter()
                .all(|c: &SuggestedCue| (c.seconds - seconds).abs() > 0.25)
            {
                cues.push(SuggestedCue { seconds, label });
            }
        }
    };
    add(Some(first_bar), "First beat");
    add(vocals.and_then(|v| v.first()).map(|v| v[0]), "Vocal");
    for (kind, label) in [
        (PhraseLabel::Breakdown, "Break"),
        (PhraseLabel::Build, "Build"),
        (PhraseLabel::Drop, "Drop"),
        (PhraseLabel::Outro, "Outro"),
    ] {
        add(
            preparation
                .phrases
                .iter()
                .find(|p| p.label == kind)
                .map(|p| p.seconds),
            label,
        );
    }
    add(preparation.mix_out, "Mix out");
    cues.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    cues
}
pub fn fill_cues(
    mut slots: [Option<djcraft_protocol::HotCue>; djcraft_protocol::HOT_CUES],
    suggestions: &[SuggestedCue],
) -> [Option<djcraft_protocol::HotCue>; djcraft_protocol::HOT_CUES] {
    for suggestion in suggestions {
        if !suggestion.seconds.is_finite()
            || suggestion.seconds < 0.
            || slots
                .iter()
                .flatten()
                .any(|cue| (cue.seconds - suggestion.seconds).abs() <= 0.25)
        {
            continue;
        }
        if let Some(slot) = slots.iter_mut().find(|s| s.is_none()) {
            *slot = Some(djcraft_protocol::HotCue {
                seconds: suggestion.seconds,
                loop_end: None,
            });
        }
    }
    slots
}
#[cfg(test)]
mod suggestion_tests {
    use super::*;
    #[test]
    fn filling_gaps_preserves_manual_cues_and_loops_and_deduplicates() {
        let mut slots = [None; djcraft_protocol::HOT_CUES];
        slots[0] = Some(djcraft_protocol::HotCue {
            seconds: 2.,
            loop_end: Some(4.),
        });
        slots[2] = Some(djcraft_protocol::HotCue {
            seconds: 12.,
            loop_end: None,
        });
        let candidates = vec![
            SuggestedCue {
                seconds: 2.1,
                label: "same",
            },
            SuggestedCue {
                seconds: 6.,
                label: "new",
            },
            SuggestedCue {
                seconds: 8.,
                label: "new",
            },
        ];
        let filled = fill_cues(slots, &candidates);
        assert_eq!(filled[0], slots[0]);
        assert_eq!(filled[2], slots[2]);
        assert_eq!(filled[1].unwrap().seconds, 6.);
        assert_eq!(filled[3].unwrap().seconds, 8.);
        assert_eq!(fill_cues(filled, &candidates), filled);
    }
    #[test]
    fn suggestions_use_sections_and_vocals_without_inventing_missing_data() {
        let p = TrackPreparation {
            phrases: vec![PhraseMarker {
                seconds: 8.,
                label: PhraseLabel::Drop,
            }],
            mix_out: Some(18.),
            ..Default::default()
        };
        let cues = cue_suggestions(
            &p,
            Some(&[[4., 6.]]),
            Some(djcraft_analysis::Tempo {
                bpm: 120.,
                first_beat_seconds: 0.,
                downbeat: 0,
                confidence: 1.,
            }),
            20.,
        );
        assert_eq!(
            cues.iter().map(|c| c.seconds).collect::<Vec<_>>(),
            vec![0., 4., 8., 18.]
        );
        assert!(cue_suggestions(&TrackPreparation::default(), None, None, 20.).is_empty());
    }
}

#[cfg(test)]
mod bar_cue_tests {
    use super::*;
    #[test]
    fn every_suggestion_uses_the_analysed_bar_phase_and_stays_in_track() {
        let grid = djcraft_analysis::Tempo {
            bpm: 120.,
            first_beat_seconds: 0.425,
            downbeat: 2,
            confidence: 1.,
        };
        let p = TrackPreparation {
            phrases: vec![PhraseMarker {
                seconds: 6.1,
                label: PhraseLabel::Drop,
            }],
            mix_out: Some(9.9),
            ..Default::default()
        };
        let cues = cue_suggestions(&p, Some(&[[3.2, 4.]]), Some(grid), 10.);
        assert_eq!(
            cues.iter().map(|c| c.label).collect::<Vec<_>>(),
            vec!["First beat", "Vocal", "Drop", "Mix out"]
        );
        for (cue, expected) in cues.iter().zip([1.425, 3.425, 5.425, 9.425]) {
            assert!((cue.seconds - expected).abs() < 1e-9);
            assert!(cue.seconds < 10.);
        }
        assert!(cue_suggestions(&p, None, None, 10.).is_empty());
        assert!(cue_suggestions(&p, None, Some(grid), 1.).is_empty());
        // A candidate which rounds beyond the end uses the last complete bar start.
        let end = cue_suggestions(&p, None, Some(grid), 9.425);
        assert!(end.iter().all(|c| c.seconds < 9.425));
        // Several candidates snapped onto one bar produce only one cue.
        let duplicate = cue_suggestions(&p, Some(&[[1.5, 2.]]), Some(grid), 10.);
        assert_eq!(
            duplicate
                .iter()
                .filter(|c| (c.seconds - 1.425).abs() < 1e-9)
                .count(),
            1
        );
    }
}
