//! Offline diagnostic: cargo run -p djcraft --example check_levels -- <audio file>
use djcraft_engine::{Engine, Track};
use djcraft_protocol::{Action, DeckAction, LoudnessInfo, Role};
fn main() {
    let path = std::env::args().nth(1).expect("audio file");
    let track = Track::decode(std::path::Path::new(&path), 48000).expect("decode");
    let samples = track.export_stereo();
    let (rms_db, peak_db) =
        djcraft_analysis::measure_loudness(&samples, 48000).expect("non-silent");
    let report = LoudnessInfo { rms_db, peak_db };
    track.set_loudness(Some(report)).unwrap();
    println!(
        "RMS {rms_db:.3} peak {peak_db:.3} trim {:.3}",
        report.recommended_trim()
    );
    for (ratio, lock) in [(1., false), (1.04, false), (1.04, true)] {
        let (mut engine, mut mixer) = Engine::headless(48000);
        engine.load_track(0, track.clone()).unwrap();
        engine.state();
        for action in [
            DeckAction::SetTempo { ratio },
            DeckAction::SetKeyLock { on: lock },
            DeckAction::Play,
        ] {
            engine
                .apply(Role::Host, Action::Deck { deck: 0, action })
                .unwrap();
        }
        if lock {
            let processed = djcraft_engine::measure_key_lock_peak(&track, ratio).unwrap();
            println!("Processed full-track peak {processed:.3} dB");
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: 0,
                        action: DeckAction::SetProcessedPeak {
                            ratio,
                            peak_db: processed,
                        },
                    },
                )
                .unwrap();
        }
        let mut block = [0.; 1024];
        let mut peak = 0f32;
        let mut first_clip = None;
        for i in 0..(samples.len() / 2 * 2 / 512) {
            mixer.render(&mut block);
            let value = engine.state().master_peak;
            peak = peak.max(value);
            if value >= 1. && first_clip.is_none() {
                first_clip = Some(i as f64 * 512. / 48000.);
            }
        }
        println!(
            "speed {ratio} key_lock {lock}: peak {peak:.6} ({:.3} dB), first clip {first_clip:?}",
            20. * peak.log10()
        );
    }
}
