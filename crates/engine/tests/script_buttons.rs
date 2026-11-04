use djcraft_engine::Engine;
use djcraft_protocol::script::{ControlSource, Program};
use djcraft_protocol::{Action, DECKS, DeckAction, Role, TempoFaderRange};

#[test]
fn scripted_deck_buttons_use_live_state_and_preserve_tempo() {
    let (mut engine, _mixer) = Engine::headless(48_000);
    for deck in 0..DECKS {
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: deck as u8,
                    action: DeckAction::SetTempo { ratio: 1.05 },
                },
            )
            .unwrap();
        let target = ["deck_a", "deck_b", "deck_c", "deck_d"][deck];
        for command in [
            "key_lock_off",
            "toggle_key_lock",
            "key_lock_on",
            "toggle_key_lock",
        ] {
            let program =
                Program::compile(&format!("do_action(\"{target}\", \"{command}\")")).unwrap();
            let before = engine.state();
            let dispatch = program
                .run_with_state(ControlSource::Skin, 0., Some(&before))
                .unwrap();
            let expected = match command {
                "key_lock_off" => false,
                "key_lock_on" => true,
                _ => !before.decks[deck].key_lock,
            };
            assert!(engine.apply(Role::Viewer, dispatch.action.clone()).is_err());
            assert_eq!(
                engine.state().decks[deck].key_lock,
                before.decks[deck].key_lock
            );
            engine.apply(Role::Host, dispatch.action).unwrap();
            let after = engine.state();
            assert_eq!(after.decks[deck].key_lock, expected);
            assert_eq!(after.decks[deck].tempo, 1.05);
            for other in 0..DECKS {
                if other != deck {
                    assert_eq!(after.decks[other].key_lock, before.decks[other].key_lock);
                }
            }
        }
        let cycle =
            Program::compile(&format!("do_action(\"{target}\", \"cycle_tempo_range\")")).unwrap();
        for expected in [
            TempoFaderRange::Medium,
            TempoFaderRange::High,
            TempoFaderRange::Wide,
            TempoFaderRange::Low,
        ] {
            let before = engine.state();
            let dispatch = cycle
                .run_with_state(ControlSource::Controller, 0., Some(&before))
                .unwrap();
            assert!(engine.apply(Role::Viewer, dispatch.action.clone()).is_err());
            engine.apply(Role::Host, dispatch.action).unwrap();
            let after = engine.state();
            assert_eq!(after.decks[deck].tempo_range, expected);
            assert_eq!(after.decks[deck].tempo, 1.05);
            for other in 0..DECKS {
                if other != deck {
                    assert_eq!(
                        after.decks[other].tempo_range,
                        before.decks[other].tempo_range
                    );
                }
            }
        }
    }
}
