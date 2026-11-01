use djcraft_controllers::{Bridge, Event, Profile};
use djcraft_engine::Engine;
use djcraft_protocol::{Action, DeckAction, Role, SessionState, TempoFaderRange};
const PORT: &str = "MixTrack Pro II";

#[test]
fn physical_center_stays_at_zero_when_nearby_bpm_match_rounds_to_center() {
    let (mut engine, _) = Engine::headless(48000);
    for deck in 0..2u8 {
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck,
                    action: DeckAction::SetTempoRange {
                        range: TempoFaderRange::Wide,
                    },
                },
            )
            .unwrap();
        let mut b = bridge();
        b.set_tempo_bpms(&[Some(120.), Some(120.4)]);
        assert_eq!(
            fader(&mut b, deck, 64, &engine.state()),
            vec![Action::Deck {
                deck,
                action: DeckAction::SetTempo { ratio: 1. }
            }]
        );
    }
}

#[test]
fn fader_lands_on_opposite_bpm_and_leaves_snap_on_next_step() {
    let (mut engine, _mixer) = Engine::headless(48000);
    let mut state = engine.state();
    let mut b = bridge();
    b.set_tempo_bpms(&[Some(120.), Some(123.)]);
    state.decks[1].tempo = 1.003;
    let target: f64 = 123. * 1.003 / 120.;
    // Start at unity to acquire pickup, then approach the nearest step.
    fader(&mut b, 0, 64, &state);
    let raw = ((1.08 - target) / 0.08 * 64.).round() as u8;
    let actions = fader(&mut b, 0, raw, &state);
    assert_eq!(
        actions,
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::SetTempo { ratio: target }
        }]
    );
    state.decks[0].tempo = target;
    let actions = fader(&mut b, 0, raw - 1, &state);
    assert!(
        matches!(actions.as_slice(), [Action::Deck { action: DeckAction::SetTempo { ratio }, .. }] if (*ratio - target).abs() > 0.0001)
    );
    // Missing BPM and targets outside the range leave ordinary scaling intact.
    for bpms in [
        [Some(120.), None],
        [Some(120.), Some(180.)],
        [Some(f64::NAN), Some(123.)],
    ] {
        let mut b = bridge();
        b.set_tempo_bpms(&bpms);
        state.decks[0].tempo = 1.;
        fader(&mut b, 0, 64, &state);
        assert_eq!(
            fader(&mut b, 0, 44, &state),
            vec![Action::Deck {
                deck: 0,
                action: DeckAction::SetTempo { ratio: 1.025 }
            }]
        );
    }
    // A distant fader must still pick up the application's tempo first.
    let mut b = bridge();
    b.set_tempo_bpms(&[Some(120.), Some(123.)]);
    state.decks[0].tempo = 0.95;
    assert!(fader(&mut b, 0, raw, &state).is_empty());
}
fn bridge() -> Bridge {
    Bridge::new(
        Profile::from_json(include_str!(
            "../../../profiles/numark-mixtrack-pro-ii.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn fader(b: &mut Bridge, deck: u8, value: u8, s: &SessionState) -> Vec<Action> {
    b.process(PORT, Event::parse(&[0xb0, 0x0d + deck, value]).unwrap(), s)
}
#[test]
fn every_range_uses_full_fader_travel_exact_neutral_and_independent_decks() {
    let (mut engine, _mixer) = Engine::headless(48000);
    for deck in 0..2u8 {
        for range in [
            TempoFaderRange::Low,
            TempoFaderRange::Medium,
            TempoFaderRange::High,
            TempoFaderRange::Wide,
        ] {
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck,
                        action: DeckAction::SetTempoRange { range },
                    },
                )
                .unwrap();
            for (raw, ratio) in [
                (0, 1. + range.extent()),
                (32, 1. + range.extent() / 2.),
                (64, 1.),
                (127, 1. - range.extent()),
            ] {
                engine
                    .apply(
                        Role::Host,
                        Action::Deck {
                            deck,
                            action: DeckAction::SetTempo { ratio },
                        },
                    )
                    .unwrap();
                let actions = fader(&mut bridge(), deck, raw, &engine.state());
                assert!(
                    matches!(actions.as_slice(), [Action::Deck { deck: target, action: DeckAction::SetTempo { ratio: actual } }] if *target == deck && (*actual - ratio).abs() < 1e-12)
                );
            }
        }
    }
}
#[test]
fn shift_range_cycles_once_per_press_wraps_and_never_rewrites_tempo() {
    let (mut engine, _mixer) = Engine::headless(48000);
    let mut b = bridge();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetTempo { ratio: 1.06 },
            },
        )
        .unwrap();
    b.process(
        PORT,
        Event::parse(&[0x90, 0x61, 127]).unwrap(),
        &engine.state(),
    );
    for expected in [
        TempoFaderRange::Medium,
        TempoFaderRange::High,
        TempoFaderRange::Wide,
        TempoFaderRange::Low,
    ] {
        let actions = b.process(
            PORT,
            Event::parse(&[0x90, 0x44, 127]).unwrap(),
            &engine.state(),
        );
        assert_eq!(
            actions,
            vec![Action::Deck {
                deck: 0,
                action: DeckAction::SetTempoRange { range: expected }
            }]
        );
        for action in actions {
            engine.apply(Role::Host, action).unwrap();
        }
        assert_eq!(engine.state().decks[0].tempo, 1.06);
        assert_eq!(engine.state().decks[1].tempo_range, TempoFaderRange::Low);
        assert!(
            b.process(
                PORT,
                Event::parse(&[0x90, 0x44, 127]).unwrap(),
                &engine.state()
            )
            .is_empty()
        );
        assert!(
            b.process(
                PORT,
                Event::parse(&[0x90, 0x44, 0]).unwrap(),
                &engine.state()
            )
            .is_empty()
        );
    }
    assert!(b.take_commands().is_empty());
}
#[test]
fn changing_range_rearms_pickup_even_if_the_playing_tempo_is_unchanged() {
    let (mut engine, _mixer) = Engine::headless(48000);
    let mut b = bridge();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetTempo { ratio: 1.04 },
            },
        )
        .unwrap();
    assert_eq!(fader(&mut b, 0, 32, &engine.state()).len(), 1);
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetTempoRange {
                    range: TempoFaderRange::Medium,
                },
            },
        )
        .unwrap();
    assert!(fader(&mut b, 0, 32, &engine.state()).is_empty());
    assert!(!fader(&mut b, 0, 48, &engine.state()).is_empty());
    // A software tempo outside a smaller range is preserved until an explicit
    // tempo adjustment or a larger range makes physical pickup possible.
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetTempo { ratio: 1.3 },
            },
        )
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetTempoRange {
                    range: TempoFaderRange::Low,
                },
            },
        )
        .unwrap();
    for raw in [0, 64, 127] {
        assert!(fader(&mut b, 0, raw, &engine.state()).is_empty());
    }
    assert_eq!(engine.state().decks[0].tempo, 1.3);
}
