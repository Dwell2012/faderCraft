use djcraft_engine::{Engine, Track};
use djcraft_protocol::{Action, ColourFxState, DeckAction, Rejection, Role};
use std::sync::Arc;

fn rendered(selected: u8, position: f32, music: bool) -> Vec<f32> {
    let (mut engine, mut mixer) = Engine::headless(8000);
    if music {
        let samples = (0..32000)
            .flat_map(|i| [(i as f32 * std::f32::consts::TAU * 1100. / 8000.).sin() * 0.2; 2])
            .collect();
        engine
            .load_track(0, Arc::new(Track::from_samples("CFX", 8000, samples)))
            .unwrap();
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: 0,
                    action: DeckAction::Play,
                },
            )
            .unwrap();
    }
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetColourFx {
                    state: ColourFxState {
                        selected,
                        position,
                        enabled: true,
                    },
                },
            },
        )
        .unwrap();
    let mut output = vec![0.; 64000];
    mixer.render(&mut output);
    output
}

#[test]
fn combined_cfx_share_one_knob_and_centre_is_dry() {
    assert_eq!(rendered(0, 0.5, true), rendered(127, 0.5, true));
    let filter = rendered(1, 0.2, true);
    let crusher = rendered(16, 0.2, true);
    let combined = rendered(17, 0.2, true);
    let difference = |a: &[f32], b: &[f32]| {
        a[3000..]
            .iter()
            .zip(&b[3000..])
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
    };
    assert!(difference(&combined, &filter) > 0.01);
    assert!(difference(&combined, &crusher) > 0.01);
}

#[test]
fn white_and_pink_noise_are_standalone_stereo_generators_with_different_spectra() {
    assert!(rendered(96, 0.5, false).iter().all(|sample| *sample == 0.));
    let white = rendered(32, 1., false);
    let pink = rendered(64, 1., false);
    let roughness = |audio: &[f32]| {
        let left: Vec<_> = audio[16000..]
            .chunks_exact(2)
            .map(|frame| frame[0])
            .collect();
        let energy = left.iter().map(|sample| sample * sample).sum::<f32>();
        assert!(energy > 1.);
        left.windows(2)
            .map(|pair| (pair[1] - pair[0]).powi(2))
            .sum::<f32>()
            / energy
    };
    assert!(roughness(&pink) < roughness(&white) * 0.5);
    assert!(white.chunks_exact(2).any(|frame| frame[0] != frame[1]));
    assert!(pink.iter().all(|sample| sample.is_finite()));
}

#[test]
fn cfx_rejects_invalid_state_and_preserves_deck_isolation() {
    let (mut engine, _) = Engine::headless(8000);
    for state in [
        ColourFxState {
            position: f32::NAN,
            ..Default::default()
        },
        ColourFxState {
            selected: 128,
            ..Default::default()
        },
    ] {
        assert_eq!(
            engine.apply(
                Role::Host,
                Action::Deck {
                    deck: 0,
                    action: DeckAction::SetColourFx { state }
                }
            ),
            Err(Rejection::InvalidValue)
        );
    }
    let state = ColourFxState {
        position: 0.8,
        selected: 7,
        enabled: true,
    };
    assert_eq!(
        engine.apply(
            Role::Viewer,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetColourFx { state }
            }
        ),
        Err(Rejection::NotPermitted)
    );
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 1,
                action: DeckAction::SetColourFx { state },
            },
        )
        .unwrap();
    assert_eq!(engine.state().decks[0].colour_fx, ColourFxState::default());
    assert_eq!(engine.state().decks[1].colour_fx, state);
}

#[test]
fn cfx_reset_kills_noise_and_all_echo_reverb_tails_immediately() {
    let (mut engine, mut mixer) = Engine::headless(8000);
    let selected = 32 | 2 | 4;
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetColourFx {
                    state: ColourFxState {
                        selected,
                        position: 1.,
                        enabled: true,
                    },
                },
            },
        )
        .unwrap();
    let mut audio = vec![0.; 16000];
    mixer.render(&mut audio);
    assert!(audio.iter().any(|sample| sample.abs() > 0.001));
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetColourFx {
                    state: ColourFxState {
                        selected,
                        position: 0.5,
                        enabled: false,
                    },
                },
            },
        )
        .unwrap();
    mixer.render(&mut audio);
    assert!(audio.iter().all(|sample| sample.abs() < 0.000001));
    assert!(!engine.state().decks[0].effect_tail);
    assert_eq!(engine.state().decks[0].colour_fx.selected, selected);
}

#[test]
fn cfx_processors_match_reverse_menu_serial_rack_order() {
    use djcraft_protocol::{EffectKind, EffectState};
    let state = ColourFxState {
        // Echo and Reverb deck inserts now return after the channel fader;
        // only processors sharing the CFX insert route can be compared here.
        selected: 1 | 8 | 16,
        position: 0.2,
        enabled: true,
    };
    let (mut engine, mut mixer) = Engine::headless(8000);
    let samples = (0..32000)
        .flat_map(|i| [(i as f32 * std::f32::consts::TAU * 1100. / 8000.).sin() * 0.2; 2])
        .collect();
    engine
        .load_track(0, Arc::new(Track::from_samples("reference", 8000, samples)))
        .unwrap();
    for (slot, effect) in state.effects().into_iter().rev().enumerate() {
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: 0,
                    action: DeckAction::SetEffect {
                        slot: slot as u8,
                        effect,
                    },
                },
            )
            .unwrap();
    }
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::Play,
            },
        )
        .unwrap();
    let mut reference = vec![0.; 64000];
    mixer.render(&mut reference);
    let actual = rendered(state.selected, state.position, true);
    assert!(
        actual
            .iter()
            .zip(reference)
            .all(|(a, b)| (a - b).abs() < 1e-6)
    );

    // Standalone CFX noise is injected after the deck rack, so rack effects
    // cannot colour it; the downstream CFX Filter still can.
    let (mut engine, mut mixer) = Engine::headless(8000);
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetEffect {
                    slot: 0,
                    effect: EffectState {
                        kind: EffectKind::Bitcrusher,
                        enabled: true,
                        amount: 1.,
                        parameter: 1.,
                        ..Default::default()
                    },
                },
            },
        )
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetColourFx {
                    state: ColourFxState {
                        selected: 32,
                        position: 0.2,
                        enabled: true,
                    },
                },
            },
        )
        .unwrap();
    let mut noise = vec![0.; 64000];
    mixer.render(&mut noise);
    assert_eq!(noise, rendered(32, 0.2, false));
    let filtered = rendered(33, 0.2, false);
    let energy = |audio: &[f32]| audio[8000..].iter().map(|v| v * v).sum::<f32>();
    assert!(energy(&filtered) < energy(&noise) * 0.5);
}
