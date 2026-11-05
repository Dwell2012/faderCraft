use djcraft_controllers::{
    Binding, Bridge, Event, HostCommand, Mapping, PROFILE_VERSION, PadMode, Profile, Source,
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
fn legacy_three_button_curve_controller_maps_to_script_presets() {
    let mappings = [
        "crossfader_smooth",
        "crossfader_equal_power",
        "crossfader_cut",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, preset)| {
        mapping(
            &format!("curve-{index}"),
            Source::Note {
                channel: 0,
                number: 40 + index as u8,
            },
            &format!("do_action(\"mixer\", \"{preset}\")"),
            None,
        )
    })
    .collect();
    let mut bridge = Bridge::new(profile(mappings)).unwrap();
    let state = SessionState::default();
    for (index, curve) in [
        djcraft_protocol::CrossfaderCurve::Smooth,
        djcraft_protocol::CrossfaderCurve::EqualPower,
        djcraft_protocol::CrossfaderCurve::Cut,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            bridge.process(
                "GRV6",
                Event {
                    source: Source::Note {
                        channel: 0,
                        number: 40 + index as u8
                    },
                    value: 127
                },
                &state
            ),
            vec![Action::Mixer {
                action: MixerAction::SetCrossfaderCurve { curve }
            }]
        );
    }
}

#[test]
fn deck_setup_script_buttons_run_once_per_press_and_read_latest_state() {
    let source = Source::Note {
        channel: 0,
        number: 12,
    };
    for (command, first, second) in [
        (
            "toggle_key_lock",
            DeckAction::SetKeyLock { on: true },
            DeckAction::SetKeyLock { on: false },
        ),
        (
            "toggle_quantize",
            DeckAction::SetQuantize { enabled: false },
            DeckAction::SetQuantize { enabled: true },
        ),
        (
            "cycle_tempo_range",
            DeckAction::SetTempoRange {
                range: djcraft_protocol::TempoFaderRange::Medium,
            },
            DeckAction::SetTempoRange {
                range: djcraft_protocol::TempoFaderRange::High,
            },
        ),
    ] {
        let mut bridge = Bridge::new(profile(vec![mapping(
            "setup",
            source,
            &format!("do_action(\"deck_b\", \"{command}\")"),
            None,
        )]))
        .unwrap();
        let mut state = SessionState::default();
        state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
        state.decks[1].key_lock = false;
        for expected in [first, second] {
            assert_eq!(
                bridge.process("GRV6", Event { source, value: 127 }, &state),
                vec![Action::Deck {
                    deck: 1,
                    action: expected.clone()
                }]
            );
            match expected {
                DeckAction::SetKeyLock { on } => state.decks[1].key_lock = on,
                DeckAction::SetQuantize { enabled } => state.decks[1].quantize_disabled = !enabled,
                DeckAction::SetTempoRange { range } => state.decks[1].tempo_range = range,
                _ => unreachable!(),
            }
            assert!(
                bridge
                    .process("GRV6", Event { source, value: 127 }, &state)
                    .is_empty()
            );
            assert!(
                bridge
                    .process("GRV6", Event { source, value: 0 }, &state)
                    .is_empty()
            );
        }
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
    assert_eq!(profile.mappings.len(), 218);
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
        let source = Source::Note {
            channel: deck,
            number: 114,
        };
        assert!(
            bridge
                .process("DDJ-GRV6", Event { source, value: 127 }, &state)
                .is_empty()
        );
        assert_eq!(
            bridge.take_commands(),
            vec![HostCommand::SelectDeck { deck }]
        );
        assert!(
            bridge
                .process("DDJ-GRV6", Event { source, value: 0 }, &state)
                .is_empty()
        );
        assert!(bridge.take_commands().is_empty());
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

#[test]
fn grv6_documented_mixer_knobs_dispatch_to_their_own_controls() {
    let profile = Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json"))
        .expect("GRV6 profile should validate without hardware");
    let mut bridge = Bridge::new(profile).unwrap();
    let state = SessionState::default();
    let mut turn_knob = |msb, lsb| {
        assert!(
            bridge
                .process(
                    "DDJ-GRV6",
                    Event {
                        source: Source::Cc {
                            channel: 6,
                            number: msb,
                        },
                        value: 127,
                    },
                    &state,
                )
                .is_empty()
        );
        bridge.process(
            "DDJ-GRV6",
            Event {
                source: Source::Cc {
                    channel: 6,
                    number: lsb,
                },
                value: 127,
            },
            &state,
        )
    };
    assert_eq!(
        turn_knob(8, 40),
        vec![Action::Mixer {
            action: MixerAction::SetMasterGain { level: 1.0 }
        }]
    );
    let headphone_level = turn_knob(13, 45);
    assert!(matches!(
        headphone_level.as_slice(),
        [Action::Mixer { action: MixerAction::SetMonitor { monitor } }]
            if monitor.gain == 1.0 && monitor.mix == state.monitor.mix
    ));
    let headphone_mix = turn_knob(12, 44);
    assert!(matches!(
        headphone_mix.as_slice(),
        [Action::Mixer { action: MixerAction::SetMonitor { monitor } }]
            if monitor.mix == 1.0 && monitor.gain == state.monitor.gain
    ));
    assert_eq!(
        turn_knob(9, 41),
        vec![Action::Mixer {
            action: MixerAction::SetBoothGain { level: 1.0 }
        }]
    );
}

#[test]
fn grv6_mode_buttons_select_each_decks_visible_pad_page() {
    let profile = Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json"))
        .expect("GRV6 mode mappings should validate without hardware");
    let mut bridge = Bridge::new(profile).unwrap();
    let state = SessionState::default();
    for (number, mode) in [
        (27, PadMode::HotCues),
        (30, PadMode::Stems),
        (32, PadMode::BeatJump),
        (34, PadMode::Sampler),
    ] {
        for deck in 0..4 {
            let source = Source::Note {
                channel: deck,
                number,
            };
            assert!(
                bridge
                    .process("DDJ-GRV6", Event { source, value: 127 }, &state)
                    .is_empty()
            );
            let modes = bridge.take_pad_modes();
            assert_eq!(modes[usize::from(deck)], Some(mode));
            assert_eq!(modes.iter().flatten().count(), 1);
            assert!(
                bridge
                    .process("DDJ-GRV6", Event { source, value: 0 }, &state)
                    .is_empty()
            );
            assert!(bridge.take_pad_modes().iter().all(Option::is_none));
        }
    }
}

#[test]
fn grv6_quantize_buttons_toggle_each_deck_and_report_led_state() {
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState::default();
    state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
    for deck in 0..4u8 {
        let source = Source::Note {
            channel: deck,
            number: 53,
        };
        assert_eq!(
            bridge.process("DDJ-GRV6", Event { source, value: 127 }, &state),
            vec![Action::Deck {
                deck,
                action: DeckAction::SetQuantize { enabled: false },
            }]
        );
        bridge.process("DDJ-GRV6", Event { source, value: 0 }, &state);
        assert!(
            bridge
                .state_feedback("DDJ-GRV6", &state, 0, &[None; 4])
                .contains(&[0x90 | deck, 53, 100])
        );
        state.decks[deck as usize].quantize_disabled = true;
        assert!(
            bridge
                .state_feedback("DDJ-GRV6", &state, 0, &[None; 4])
                .contains(&[0x90 | deck, 53, 0])
        );
    }
}

#[test]
fn grv6_hot_cue_pads_recall_delete_and_follow_saved_led_state() {
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState::default();
    state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
    for deck in 0..4u8 {
        let slot = 7u8;
        state.decks[deck as usize].hot_cues[slot as usize] = Some(djcraft_protocol::HotCue {
            seconds: 1.,
            loop_end: None,
        });
        let normal = Source::Note {
            channel: 7 + deck * 2,
            number: slot,
        };
        let shifted = Source::Note {
            channel: 8 + deck * 2,
            number: slot,
        };
        assert_eq!(
            bridge.process(
                "DDJ-GRV6",
                Event {
                    source: normal,
                    value: 127
                },
                &state
            ),
            vec![Action::Deck {
                deck,
                action: DeckAction::TriggerHotCue {
                    slot,
                    play: true,
                    loop_length: None,
                },
            }]
        );
        bridge.process(
            "DDJ-GRV6",
            Event {
                source: normal,
                value: 0,
            },
            &state,
        );
        let led = [0x90 | (7 + deck * 2), slot, 100];
        assert!(
            bridge
                .state_feedback("DDJ-GRV6", &state, 0, &[None; 4])
                .contains(&led)
        );
        let mut cleared = state.decks[deck as usize].hot_cues;
        cleared[slot as usize] = None;
        assert_eq!(
            bridge.process(
                "DDJ-GRV6",
                Event {
                    source: shifted,
                    value: 127
                },
                &state
            ),
            vec![Action::Deck {
                deck,
                action: DeckAction::SetHotCues { cues: cleared },
            }]
        );
        state.decks[deck as usize].hot_cues = cleared;
        assert!(
            bridge
                .state_feedback("DDJ-GRV6", &state, 0, &[None; 4])
                .contains(&[led[0], led[1], 0])
        );
    }
}

#[test]
fn grv6_slip_hot_cue_returns_on_release() {
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState::default();
    state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
    for deck in 0..4u8 {
        let slip = Source::Note {
            channel: deck,
            number: 64,
        };
        bridge.process(
            "DDJ-GRV6",
            Event {
                source: slip,
                value: 127,
            },
            &state,
        );
        assert_eq!(
            bridge.take_commands(),
            vec![HostCommand::ToggleSlip { deck }]
        );
        bridge.process(
            "DDJ-GRV6",
            Event {
                source: slip,
                value: 0,
            },
            &state,
        );
        state.decks[deck as usize].slip_enabled = true;
        state.decks[deck as usize].hot_cues[0] = Some(djcraft_protocol::HotCue {
            seconds: 1.,
            loop_end: None,
        });
        let pad = Source::Note {
            channel: 7 + deck * 2,
            number: 0,
        };
        assert_eq!(
            bridge.process(
                "DDJ-GRV6",
                Event {
                    source: pad,
                    value: 127
                },
                &state
            ),
            vec![Action::Deck {
                deck,
                action: DeckAction::TriggerHotCue {
                    slot: 0,
                    play: true,
                    loop_length: None
                }
            }]
        );
        assert_eq!(
            bridge.process(
                "DDJ-GRV6",
                Event {
                    source: pad,
                    value: 0
                },
                &state
            ),
            vec![Action::Deck {
                deck,
                action: DeckAction::SlipReturn
            }]
        );
    }
}
