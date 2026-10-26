use std::sync::Arc;

use djcraft_engine::{Engine, Mixer, Track};
use djcraft_protocol::{
    Action, CrossfaderCurve, DECKS, DeckAction, MixerAction, MonitorState, Rejection, Role,
};

const RATE: u32 = 48_000;
const LEVELS: [f32; DECKS] = [0.04, 0.08, 0.12, 0.16];

fn deck(engine: &mut Engine, index: usize, action: DeckAction) {
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: index as u8,
                action,
            },
        )
        .unwrap();
}

fn mixer_action(engine: &mut Engine, action: MixerAction) {
    engine.apply(Role::Host, Action::Mixer { action }).unwrap();
}

fn playing_decks() -> (Engine, Mixer) {
    let (mut engine, mixer) = Engine::headless(RATE);
    for (index, level) in LEVELS.into_iter().enumerate() {
        engine
            .load_track(
                index,
                Arc::new(Track::from_samples(
                    format!("Deck {index}"),
                    RATE,
                    vec![level; RATE as usize * 4],
                )),
            )
            .unwrap();
        deck(&mut engine, index, DeckAction::Play);
    }
    (engine, mixer)
}

#[test]
fn crossfader_routing_supports_multiple_decks_bypass_and_both_sides() {
    use djcraft_protocol::CrossfaderRouting;
    let (mut engine, mut mixer) = playing_decks();
    let mut output = [0.; 2048];
    for routing in [
        CrossfaderRouting {
            left: 0b0011,
            right: 0b1100,
        },
        CrossfaderRouting {
            left: 0b1000,
            right: 0b0001,
        },
        CrossfaderRouting { left: 0, right: 0 },
        CrossfaderRouting {
            left: 0b1111,
            right: 0b1111,
        },
    ] {
        mixer_action(&mut engine, MixerAction::SetCrossfaderRouting { routing });
        assert_eq!(engine.state().crossfader_routing, routing);
        for curve in [
            CrossfaderCurve::Smooth,
            CrossfaderCurve::EqualPower,
            CrossfaderCurve::Cut,
        ] {
            mixer_action(&mut engine, MixerAction::SetCrossfaderCurve { curve });
            for position in [0., 0.5, 1.] {
                mixer_action(&mut engine, MixerAction::SetCrossfader { position });
                mixer.render(&mut output);
                mixer.render(&mut output);
                let gains = djcraft_engine::crossfader_gains(position, curve);
                let mut expected = 0.;
                for (index, level) in LEVELS.into_iter().enumerate() {
                    let sides = (routing.left >> index & 1, routing.right >> index & 1);
                    let gain = match sides {
                        (1, 0) => gains[0],
                        (0, 1) => gains[1],
                        _ => 1.,
                    };
                    expected += level * gain;
                    assert_eq!(engine.deck_has_open_output(index), gain > 1e-6);
                }
                assert!((output[2046] - expected).abs() < 1e-4);
            }
        }
    }
    let previous = engine.state().crossfader_routing;
    let invalid = Action::Mixer {
        action: MixerAction::SetCrossfaderRouting {
            routing: CrossfaderRouting {
                left: 0b10000,
                right: 0,
            },
        },
    };
    assert_eq!(
        engine.apply(Role::Host, invalid),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(engine.state().crossfader_routing, previous);
    assert_eq!(
        engine.apply(
            Role::Viewer,
            Action::Mixer {
                action: MixerAction::SetCrossfaderRouting {
                    routing: CrossfaderRouting::default()
                }
            }
        ),
        Err(Rejection::NotPermitted)
    );
}

#[test]
fn all_four_decks_mix_with_ac_left_and_bd_right_for_every_curve() {
    let (mut engine, mut mixer) = playing_decks();
    let mut output = [0.; 2048];
    assert_eq!(engine.state().decks.len(), DECKS);
    for curve in [
        CrossfaderCurve::Smooth,
        CrossfaderCurve::EqualPower,
        CrossfaderCurve::Cut,
    ] {
        mixer_action(&mut engine, MixerAction::SetCrossfaderCurve { curve });
        for position in [0., 0.5, 1.] {
            mixer_action(&mut engine, MixerAction::SetCrossfader { position });
            mixer.render(&mut output);
            mixer.render(&mut output);
            let gains = djcraft_engine::crossfader_gains(position, curve);
            let expected = (LEVELS[0] + LEVELS[2]) * gains[0] + (LEVELS[1] + LEVELS[3]) * gains[1];
            assert!(
                (output[2046] - expected).abs() < 1e-4,
                "curve {curve:?} position {position}: {} vs {expected}",
                output[2046]
            );
            for index in 0..DECKS {
                assert_eq!(engine.deck_has_open_output(index), gains[index % 2] > 1e-6);
            }
        }
    }
}

#[test]
fn headphones_isolate_each_deck_even_with_all_four_playing_and_faders_closed() {
    let (mut engine, mut mixer) = playing_decks();
    for index in 0..DECKS {
        deck(&mut engine, index, DeckAction::SetVolume { level: 0. });
    }
    mixer_action(&mut engine, MixerAction::SetMasterGain { level: 0. });
    let mut output = [0.; 2048];
    let mut cue = [0.; 2048];
    for selected in [3, 2, 0, 1, 3] {
        mixer_action(
            &mut engine,
            MixerAction::SetMonitor {
                monitor: MonitorState {
                    deck: selected as u8,
                    gain: 1.,
                    mix: 0.,
                    ..Default::default()
                },
            },
        );
        let before = engine.state().decks[2].position_seconds;
        mixer.render_with_monitor(&mut output, Some(&mut cue));
        mixer.render_with_monitor(&mut output, Some(&mut cue));
        assert!(output.iter().all(|x| x.abs() < 1e-6));
        assert!(
            cue[1024..]
                .iter()
                .all(|x| (*x - LEVELS[selected]).abs() < 1e-4),
            "other decks leaked into cue {selected}: {}",
            cue[2046]
        );
        let after = engine.state().decks[2].position_seconds;
        assert!((after - before - 2048. / RATE as f64).abs() < 1e-4);
    }
}

#[test]
fn extra_decks_have_independent_transport_and_controls_and_reject_out_of_range_ids() {
    let (mut engine, mut mixer) = playing_decks();
    deck(&mut engine, 2, DeckAction::SetTempo { ratio: 1.25 });
    deck(&mut engine, 2, DeckAction::SetTrim { db: -3. });
    deck(
        &mut engine,
        2,
        DeckAction::SetEq {
            band: 1,
            gain: 0.25,
        },
    );
    deck(&mut engine, 3, DeckAction::Seek { seconds: 0.5 });
    deck(&mut engine, 3, DeckAction::SetVolume { level: 0. });
    let mut output = [0.; 2048];
    mixer.render(&mut output);
    let state = engine.state();
    for index in [0, 1] {
        assert_eq!(state.decks[index].tempo, 1.);
        assert_eq!(state.decks[index].trim_db, 0.);
        assert_eq!(state.decks[index].eq, [1.; 3]);
        assert_eq!(state.decks[index].volume, 1.);
    }
    assert_eq!(state.decks[2].tempo, 1.25);
    assert_eq!(state.decks[2].eq, [1., 0.25, 1.]);
    assert_eq!(state.decks[3].volume, 0.);
    assert!(state.decks[3].position_seconds > 0.5);
    assert_eq!(
        engine.apply(
            Role::Host,
            Action::Deck {
                deck: DECKS as u8,
                action: DeckAction::Play,
            }
        ),
        Err(Rejection::UnknownDeck)
    );
    assert_eq!(
        engine.apply(
            Role::Viewer,
            Action::Deck {
                deck: 3,
                action: DeckAction::Pause,
            }
        ),
        Err(Rejection::NotPermitted)
    );
}

#[test]
fn automix_still_waits_for_ab_when_cd_are_running() {
    let (mut engine, mut mixer) = playing_decks();
    mixer.render(&mut [0.; 2048]);
    mixer_action(&mut engine, MixerAction::AutomixStart);
    let state = engine.state();
    assert!(state.automix.enabled);
    assert_eq!(state.automix.active_deck, None);
    assert!(state.automix.status.contains("Waiting"));
    // Stopping A allows B to be selected, regardless of the two extra decks.
    deck(&mut engine, 0, DeckAction::Pause);
    mixer.render(&mut [0.; 2048]);
    // Manual control stops Automix; resuming must only inspect its A/B pair.
    mixer_action(&mut engine, MixerAction::AutomixStart);
    let state = engine.state();
    assert_eq!(state.automix.active_deck, Some(1));
    assert!(state.decks[2].playing && state.decks[3].playing);
    assert_eq!(state.decks[2].track.as_ref().unwrap().title, "Deck 2");
    assert_eq!(state.decks[3].track.as_ref().unwrap().title, "Deck 3");
}
