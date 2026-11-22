use djcraft_controllers::{Bridge, Event, Profile, Source};
use djcraft_protocol::{Action, DeckAction, SessionState};

const PORT: &str = "DDJ-1000:DDJ-1000 MIDI 1";

fn profile() -> Profile {
    Profile::from_json(include_str!("../../../profiles/pioneer-ddj-1000.json"))
        .expect("starter profile should validate without hardware")
}

#[test]
fn bundled_ddj1000_profile_validates_and_targets_the_stable_port_name() {
    let profile = profile();
    assert_eq!(profile.mappings.len(), 307);
    assert!(profile.mappings.iter().all(|m| m.port == PORT));
}

#[test]
fn ddj1000_play_buttons_reach_their_own_decks() {
    let mut bridge = Bridge::new(profile()).unwrap();
    let state = SessionState {
        decks: vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS],
        ..Default::default()
    };
    for deck in 0..4 {
        let source = Source::Note {
            channel: deck,
            number: 11,
        };
        assert_eq!(
            bridge.process(PORT, Event { source, value: 127 }, &state),
            vec![Action::Deck {
                deck,
                action: DeckAction::Play
            }]
        );
    }
}

#[test]
fn ddj1000_leds_use_the_documented_full_velocity() {
    let mut bridge = Bridge::new(profile()).unwrap();
    let mut state = SessionState {
        decks: vec![djcraft_protocol::DeckState::default(); 4],
        ..Default::default()
    };
    let deck = 2;
    state.decks[deck].track = Some(djcraft_protocol::TrackInfo {
        title: "LED test".into(),
        duration_seconds: 30.,
        loaded_seconds: 30.,
        fully_loaded: true,
    });
    state.decks[deck].playing = true;
    let lights = bridge.state_feedback(PORT, &state, 0, &[]);
    assert!(lights.contains(&[0x92, 11, 127]), "play lamp on deck 3");
}
