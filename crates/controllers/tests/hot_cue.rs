use djcraft_controllers::{Bridge, Event, Profile};
use djcraft_engine::{Engine, Track};
use djcraft_protocol::{Action, DeckAction, Role};
use std::sync::Arc;

fn bridge() -> Bridge {
    Bridge::new(
        Profile::from_json(include_str!(
            "../../../profiles/numark-mixtrack-pro-ii.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn press(bridge: &mut Bridge, engine: &mut Engine, note: u8, value: u8) -> Vec<Action> {
    bridge.process(
        "MixTrack Pro II",
        Event::parse(&[0x90, note, value]).unwrap(),
        &engine.state(),
    )
}
fn apply(engine: &mut Engine, actions: Vec<Action>) {
    for a in actions {
        engine.apply(Role::Host, a).unwrap();
    }
}

#[test]
fn empty_hot_cue_is_set_without_starting_playback_and_next_press_recalls_it() {
    let (mut engine, mut mixer) = Engine::headless(48000);
    engine
        .load_track(
            0,
            Arc::new(Track::from_samples("cue", 48000, vec![0.2; 48000 * 8])),
        )
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::Seek { seconds: 1.25 },
            },
        )
        .unwrap();
    mixer.render(&mut [0.; 960]);
    let mut b = bridge();
    let set = press(&mut b, &mut engine, 0x6d, 127);
    assert!(matches!(
        &set[..],
        [Action::Deck {
            deck: 0,
            action: DeckAction::SetHotCues { .. }
        }]
    ));
    apply(&mut engine, set);
    assert_eq!(engine.state().decks[0].hot_cues[0].unwrap().seconds, 1.25);
    assert!(!engine.state().decks[0].playing);
    assert!(press(&mut b, &mut engine, 0x6d, 127).is_empty());
    assert!(press(&mut b, &mut engine, 0x6d, 0).is_empty());
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::Seek { seconds: 2. },
            },
        )
        .unwrap();
    mixer.render(&mut [0.; 960]);
    let recall = press(&mut b, &mut engine, 0x6d, 127);
    assert_eq!(
        recall,
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::TriggerHotCue {
                slot: 0,
                play: true,
                loop_length: None
            }
        }]
    );
    apply(&mut engine, recall);
    mixer.render(&mut [0.; 960]);
    assert!(engine.state().decks[0].playing);
    assert!((engine.state().decks[0].position_seconds - 1.26).abs() < 0.002);
}

#[test]
fn physical_pad_addresses_select_distinct_slots_on_each_deck() {
    let (mut engine, mut mixer) = Engine::headless(48000);
    for deck in 0..2 {
        engine
            .load_track(
                deck,
                Arc::new(Track::from_samples("cue", 48000, vec![0.2; 48000 * 4])),
            )
            .unwrap();
    }
    mixer.render(&mut [0.; 960]);
    let mut b = bridge();
    for (deck, first_note) in [(0, 0x6d), (1, 0x71)] {
        for slot in 0..4 {
            let actions = press(&mut b, &mut engine, first_note + slot, 127);
            assert!(
                matches!(&actions[..], [Action::Deck { deck: mapped, action: DeckAction::SetHotCues { .. } }] if *mapped == deck)
            );
            apply(&mut engine, actions);
            assert!(engine.state().decks[usize::from(deck)].hot_cues[usize::from(slot)].is_some());
        }
    }
}

#[test]
fn empty_deck_or_end_of_audio_does_not_create_an_unusable_cue() {
    let (mut engine, mut mixer) = Engine::headless(48000);
    let mut b = bridge();
    assert!(press(&mut b, &mut engine, 0x6d, 127).is_empty());
    press(&mut b, &mut engine, 0x6d, 0);
    engine
        .load_track(
            0,
            Arc::new(Track::from_samples("cue", 48000, vec![0.2; 48000 * 2])),
        )
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::Seek { seconds: 1. },
            },
        )
        .unwrap();
    mixer.render(&mut [0.; 960]);
    assert!(press(&mut b, &mut engine, 0x6d, 127).is_empty());
}
