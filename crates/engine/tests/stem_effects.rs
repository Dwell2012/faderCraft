use djcraft_engine::{Engine, Mixer, Track};
use djcraft_protocol::{Action, DeckAction, EffectKind, EffectState, Rejection, Role};
use std::sync::Arc;

fn action(engine: &mut Engine, action: DeckAction) {
    engine
        .apply(Role::Host, Action::Deck { deck: 0, action })
        .unwrap();
}

fn setup(prepared: bool, wet: bool, tempo: f64) -> (Engine, Mixer) {
    let rate = 8000;
    let vocals: Vec<f32> = (0..rate * 4)
        .flat_map(|i| {
            [
                (i as f32 * std::f32::consts::TAU * 1400. / rate as f32).sin() * 0.2,
                0.,
            ]
        })
        .collect();
    let drums: Vec<f32> = (0..rate * 4)
        .flat_map(|i| {
            [
                0.,
                (i as f32 * std::f32::consts::TAU * 300. / rate as f32).sin() * 0.2,
            ]
        })
        .collect();
    let mix = vocals.iter().zip(&drums).map(|(a, b)| a + b).collect();
    let track = Arc::new(Track::from_samples("stem fx", rate, mix));
    if prepared {
        let empty = vec![0.; vocals.len()].into_boxed_slice();
        track
            .attach_stems([
                vocals.into_boxed_slice(),
                empty.clone(),
                empty,
                drums.into_boxed_slice(),
            ])
            .unwrap();
    }
    let (mut engine, mixer) = Engine::headless(rate);
    engine.load_track(0, track).unwrap();
    action(&mut engine, DeckAction::SetTempo { ratio: tempo });
    action(&mut engine, DeckAction::SetKeyLock { on: true });
    action(
        &mut engine,
        DeckAction::SetStemEffect {
            stem: 0,
            effect: EffectState {
                kind: EffectKind::Filter,
                enabled: wet,
                amount: 1.,
                parameter: 0.,
                resonance: 0.,
                ..Default::default()
            },
        },
    );
    action(&mut engine, DeckAction::Play);
    (engine, mixer)
}

#[test]
fn selected_stem_is_filtered_while_other_stems_remain_dry() {
    for tempo in [1., 1.08] {
        let (_dry_engine, mut dry) = setup(true, false, tempo);
        let (_wet_engine, mut wet) = setup(true, true, tempo);
        let mut dry_audio = vec![0.; 8000];
        let mut wet_audio = vec![0.; 8000];
        dry.render(&mut dry_audio);
        wet.render(&mut wet_audio);
        let rms = |audio: &[f32], channel: usize| {
            (audio[4000..]
                .chunks_exact(2)
                .map(|f| f[channel] * f[channel])
                .sum::<f32>()
                / 2000.)
                .sqrt()
        };
        assert!(
            rms(&wet_audio, 0) < rms(&dry_audio, 0) * 0.15,
            "vocals must receive the filter, tempo={tempo}"
        );
        let ratio = rms(&wet_audio, 1) / rms(&dry_audio, 1);
        assert!(
            (ratio - 1.).abs() < 0.03,
            "dry drums changed: {ratio}, tempo={tempo}"
        );
    }
}

#[test]
fn missing_stems_never_route_the_full_mix_through_a_stem_effect() {
    let (_dry_engine, mut dry) = setup(false, false, 1.);
    let (_wet_engine, mut wet) = setup(false, true, 1.);
    let mut a = vec![0.; 2000];
    let mut b = vec![0.; 2000];
    dry.render(&mut a);
    wet.render(&mut b);
    assert_eq!(a, b);
}

#[test]
fn stem_echo_tail_continues_after_pause_without_adding_dry_stems() {
    let (mut engine, mut mixer) = setup(true, false, 1.);
    action(
        &mut engine,
        DeckAction::SetStemEffect {
            stem: 0,
            effect: EffectState {
                kind: EffectKind::Echo,
                enabled: true,
                amount: 1.,
                parameter: 0.,
                sync: false,
                feedback: 0.6,
                ..Default::default()
            },
        },
    );
    let mut audio = vec![0.; 3200];
    mixer.render(&mut audio);
    action(&mut engine, DeckAction::Pause);
    mixer.render(&mut audio);
    assert!(
        audio[1600..]
            .chunks_exact(2)
            .any(|frame| frame[0].abs() > 0.001)
    );
    assert!(
        audio[1600..]
            .chunks_exact(2)
            .all(|frame| frame[1].abs() < 0.0001)
    );
    assert!(engine.state().decks[0].effect_tail);
}

#[test]
fn stem_effect_protocol_validates_target_parameters_and_permissions() {
    let (mut engine, _) = Engine::headless(8000);
    for (stem, effect) in [
        (4, EffectState::default()),
        (
            0,
            EffectState {
                amount: f32::NAN,
                ..Default::default()
            },
        ),
    ] {
        assert_eq!(
            engine.apply(
                Role::Host,
                Action::Deck {
                    deck: 0,
                    action: DeckAction::SetStemEffect { stem, effect }
                }
            ),
            Err(Rejection::InvalidValue)
        );
    }
    assert_eq!(
        engine.apply(
            Role::Viewer,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetStemEffect {
                    stem: 0,
                    effect: EffectState::default()
                }
            }
        ),
        Err(Rejection::NotPermitted)
    );
    action(&mut engine, DeckAction::SetEffectBeats { beats: 2. });
    assert!(
        engine.state().decks[0]
            .stem_effects
            .iter()
            .all(|effect| effect.beats == 2.)
    );
}
