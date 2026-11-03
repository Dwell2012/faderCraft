use djcraft_controllers::{
    Binding, Bridge, Event, HostCommand, Mapping, PROFILE_VERSION, Profile, Source,
};
use djcraft_protocol::{Action, DeckAction, MixerAction, SessionState};

fn profile(mappings: Vec<Mapping>) -> Profile {
    Profile {
        version: PROFILE_VERSION,
        name: "Offline script test".into(),
        mappings,
    }
}

fn mapping(id: &str, source: Source, program: &str, release: Option<&str>) -> Mapping {
    Mapping {
        id: id.into(),
        port: "GRV6".into(),
        source,
        modifiers: vec![],
        binding: Binding::Script {
            program: program.into(),
            release: release.map(str::to_owned),
        },
    }
}

#[test]
fn scripts_run_from_offline_note_and_cc_mappings() {
    let mut bridge = Bridge::new(profile(vec![
        mapping(
            "play",
            Source::Note {
                channel: 0,
                number: 11,
            },
            "do_action(\"deck_a\", \"toggle_play\")",
            None,
        ),
        mapping(
            "crossfader",
            Source::Cc {
                channel: 6,
                number: 31,
            },
            "set_param(\"mixer\", \"crossfader\", $value)",
            None,
        ),
        mapping(
            "eq",
            Source::Cc {
                channel: 0,
                number: 8,
            },
            "set_param(\"deck_a\", \"eq_high\", $value)",
            None,
        ),
    ]))
    .unwrap();
    let mut state = SessionState::default();
    state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
    assert_eq!(
        bridge.process(
            "GRV6",
            Event {
                source: Source::Note {
                    channel: 0,
                    number: 11
                },
                value: 127
            },
            &state
        ),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Play
        }]
    );
    assert!(
        bridge
            .process(
                "GRV6",
                Event {
                    source: Source::Note {
                        channel: 0,
                        number: 11
                    },
                    value: 0
                },
                &state
            )
            .is_empty()
    );
    assert_eq!(
        bridge.process(
            "GRV6",
            Event {
                source: Source::Cc {
                    channel: 6,
                    number: 31
                },
                value: 127
            },
            &state
        ),
        vec![Action::Mixer {
            action: MixerAction::SetCrossfader { position: 1.0 }
        }]
    );
    assert_eq!(
        bridge.process(
            "GRV6",
            Event {
                source: Source::Cc {
                    channel: 0,
                    number: 8
                },
                value: 0
            },
            &state
        ),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::SetEq { band: 2, gain: 0.0 }
        }]
    );
}

#[test]
fn invalid_script_profile_fails_before_hardware_connection() {
    for (source, program) in [
        (
            Source::Cc {
                channel: 0,
                number: 1,
            },
            "do_action(\"deck_a\", \"play\")",
        ),
        (
            Source::Note {
                channel: 0,
                number: 1,
            },
            "set_param(\"active\", \"volume\", 1)",
        ),
        (
            Source::Cc {
                channel: 0,
                number: 1,
            },
            "set_param(\"mixer\", \"bad\", $value)",
        ),
    ] {
        assert!(Bridge::new(profile(vec![mapping("bad", source, program, None)])).is_err());
    }
}

#[test]
fn note_release_can_run_a_separate_script() {
    let mut bridge = Bridge::new(profile(vec![mapping(
        "held",
        Source::Note {
            channel: 0,
            number: 12,
        },
        "do_action(\"deck_a\", \"play\")",
        Some("do_action(\"deck_a\", \"pause\")"),
    )]))
    .unwrap();
    let state = SessionState::default();
    let source = Source::Note {
        channel: 0,
        number: 12,
    };
    assert_eq!(
        bridge.process("GRV6", Event { source, value: 127 }, &state),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Play
        }]
    );
    assert_eq!(
        bridge.process("GRV6", Event { source, value: 0 }, &state),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Pause
        }]
    );
}

#[test]
fn fourteen_bit_tempo_waits_for_both_halves_and_preserves_center() {
    let mut bridge = Bridge::new(profile(vec![mapping(
        "tempo",
        Source::Cc14 {
            channel: 0,
            msb: 0,
            lsb: 32,
        },
        "set_param(\"deck_a\", \"tempo\", $value)",
        None,
    )]))
    .unwrap();
    let mut state = SessionState::default();
    state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
    let cc = |number, value| Event {
        source: Source::Cc { channel: 0, number },
        value,
    };
    assert!(bridge.process("GRV6", cc(0, 64), &state).is_empty());
    assert_eq!(
        bridge.process("GRV6", cc(32, 0), &state),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::SetTempo { ratio: 1.0 }
        }]
    );
    assert!(bridge.process("GRV6", cc(32, 127), &state).is_empty());
    assert_eq!(
        bridge.process("GRV6", cc(0, 127), &state),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::SetTempo { ratio: 1.08 }
        }]
    );
    assert!(bridge.process("GRV6", cc(0, 0), &state).is_empty());
    assert_eq!(
        bridge.process("GRV6", cc(32, 0), &state),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::SetTempo { ratio: 0.92 }
        }]
    );
}

#[test]
fn bundled_grv6_profile_compiles_and_routes_each_deck() {
    let profile = Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json"))
        .expect("starter profile should validate without hardware");
    assert_eq!(profile.mappings.len(), 85);
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState::default();
    state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
    for deck in 0..4 {
        let source = Source::Note {
            channel: deck,
            number: 11,
        };
        assert_eq!(
            bridge.process("DDJ-GRV6", Event { source, value: 127 }, &state),
            vec![Action::Deck {
                deck,
                action: DeckAction::Play
            }]
        );
    }
    for deck in 0..4 {
        bridge.process(
            "DDJ-GRV6",
            Event {
                source: Source::Note {
                    channel: 6,
                    number: 70 + deck,
                },
                value: 127,
            },
            &state,
        );
        assert_eq!(
            bridge.take_commands(),
            vec![HostCommand::LoadSelected { deck }]
        );
    }
    bridge.process(
        "DDJ-GRV6",
        Event {
            source: Source::Cc {
                channel: 6,
                number: 64,
            },
            value: 127,
        },
        &state,
    );
    assert_eq!(
        bridge.take_commands(),
        vec![HostCommand::Browse { steps: -1 }]
    );
}
