use djcraft_controllers::{Bridge, Event, Profile};
use djcraft_engine::{Engine, Track};
use djcraft_protocol::{Action, DeckAction, Role};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn fast_rim_bend_returns_to_selected_tempo_promptly_without_a_correction_backlog() {
    for ratio in [0.92, 1., 1.08] {
        for key_lock in [false, true] {
            let (mut engine, mut mixer) = Engine::headless(48000);
            let samples: Vec<f32> = (0..48000 * 10)
                .flat_map(|i| {
                    let v = (i as f32 * std::f32::consts::TAU * 220. / 48000.).sin() * 0.1;
                    [v; 2]
                })
                .collect();
            engine
                .load_track(0, Arc::new(Track::from_samples("bend", 48000, samples)))
                .unwrap();
            for action in [
                DeckAction::SetTempo { ratio },
                DeckAction::SetKeyLock { on: key_lock },
                DeckAction::Play,
            ] {
                engine
                    .apply(Role::Host, Action::Deck { deck: 0, action })
                    .unwrap();
            }
            let mut buffer = [0.; 480]; // 5 ms callbacks.
            for _ in 0..40 {
                mixer.render(&mut buffer);
            }
            let mut bridge = Bridge::new(
                Profile::from_json(include_str!(
                    "../../../profiles/numark-mixtrack-pro-ii.json"
                ))
                .unwrap(),
            )
            .unwrap();
            let origin = Instant::now();
            for tick in 0..400 {
                let now = origin + Duration::from_millis(tick * 5);
                let actions = bridge.process_at(
                    "MixTrack Pro II",
                    Event::parse(&[0xb0, 0x19, 20]).unwrap(),
                    &engine.state(),
                    now,
                );
                for action in actions {
                    engine.apply(Role::Host, action).unwrap();
                }
                mixer.render(&mut buffer);
            }
            for tick in 400..440 {
                let now = origin + Duration::from_millis(tick * 5);
                for action in bridge.tick_at(&engine.state(), now) {
                    engine.apply(Role::Host, action).unwrap();
                }
                mixer.render(&mut buffer);
            }
            let before = engine.state().decks[0].position_seconds;
            for _ in 0..200 {
                mixer.render(&mut buffer);
            }
            let local = engine.state().decks[0].clone();
            assert!(
                (local.position_seconds - before - ratio).abs() < 0.001,
                "ratio {ratio}, key lock {key_lock}: one second advanced {}",
                local.position_seconds - before
            );
            assert_eq!(local.tempo, ratio);
            assert_eq!(local.key_lock, key_lock);
        }
    }
}
