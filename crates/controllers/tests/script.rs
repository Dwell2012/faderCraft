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

#[test]
fn grv6_jog_sources_share_deck_state_and_lamps_follow_session() {
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState {
        decks: vec![djcraft_protocol::DeckState::default(); 4],
        ..Default::default()
    };
    let deck = 2;
    state.decks[deck].track = Some(djcraft_protocol::TrackInfo {
        title: "Jog test".into(),
        duration_seconds: 30.,
        loaded_seconds: 30.,
        fully_loaded: true,
    });
    state.decks[deck].position_seconds = 10.;
    state.decks[deck].playing = true;
    state.monitor.cue_decks = Some(1 << deck);
    let lights = bridge.state_feedback("DDJ-GRV6", &state, 0, &[]);
    for light in [
        [0x92, 11, 127],
        [0x92, 84, 127],
        [0x9f, 2, 127],
        [0x9f, 0, 0],
    ] {
        assert!(lights.contains(&light), "missing {light:02x?}");
    }
    state.decks[deck].position_seconds = 0.;
    let lights = bridge.state_feedback("DDJ-GRV6", &state, 0, &[]);
    assert!(lights.contains(&[0xbf, 2, 1]));
    assert!(lights.contains(&[0xbf, 0, 0]));
    state.decks[deck].position_seconds = 0.9;
    assert!(
        bridge
            .state_feedback("DDJ-GRV6", &state, 0, &[])
            .contains(&[0xbf, 2, 37])
    );
    state.decks[deck].playing = false;
    assert!(
        bridge
            .state_feedback("DDJ-GRV6", &state, 0, &[])
            .contains(&[0xbf, 2, 0])
    );
    state.decks[deck].playing = true;
    state.decks[deck].position_seconds = 10.;
    let now = std::time::Instant::now();
    let event = |source, value| Event { source, value };
    let note = Source::Note {
        channel: 2,
        number: 54,
    };
    let cc = |number| Source::Cc { channel: 2, number };
    for number in [33, 35] {
        let actions = bridge.process_at("DDJ-GRV6", event(cc(number), 65), &state, now);
        assert!(
            matches!(&actions[..], [Action::Deck { deck: 2, action: DeckAction::PitchBend { seconds } }] if *seconds > 0.)
        );
    }
    bridge.process_at("DDJ-GRV6", event(note, 127), &state, now);
    for (number, expected) in [(34, 10. + 1.8 / 6200.), (33, 10. + 3.6 / 6200.)] {
        let actions = bridge.process_at("DDJ-GRV6", event(cc(number), 65), &state, now);
        assert!(
            matches!(&actions[..], [Action::Deck { deck: 2, action: DeckAction::JogScratch { seconds, .. } }] if (*seconds - expected).abs() < 1e-9)
        );
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
fn grv6_beat_fx_remembers_selection_routes_depth_and_drives_lamps() {
    use djcraft_protocol::{BeatFxTarget, EffectKind};
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState::default();
    let send = |bridge: &mut Bridge, state: &mut SessionState, source, value| {
        let actions = bridge.process("DDJ-GRV6", Event { source, value }, state);
        if let [
            Action::Mixer {
                action: MixerAction::SetBeatFx { state: fx },
            },
        ] = actions.as_slice()
        {
            state.beat_fx = *fx;
        }
        actions.len()
    };
    let note = |number| Source::Note { channel: 4, number };
    state.decks = vec![djcraft_protocol::DeckState::default(); 4];
    state.decks[0].effects[0].kind = EffectKind::Reverb;
    assert_eq!(send(&mut bridge, &mut state, note(71), 127), 1);
    assert_eq!(state.beat_fx.kind, Some(EffectKind::Reverb));
    assert!(state.beat_fx.enabled);
    send(&mut bridge, &mut state, note(71), 0);
    send(&mut bridge, &mut state, note(71), 127);
    send(&mut bridge, &mut state, note(71), 0);
    assert_eq!(send(&mut bridge, &mut state, note(33), 127), 1);
    assert_eq!(state.beat_fx.kind, Some(EffectKind::Echo));
    assert!(!state.beat_fx.enabled);
    for (number, target) in [
        (16, BeatFxTarget::Deck(0)),
        (17, BeatFxTarget::Deck(1)),
        (18, BeatFxTarget::Deck(2)),
        (19, BeatFxTarget::Deck(3)),
        (20, BeatFxTarget::Master),
        (22, BeatFxTarget::Sampler),
    ] {
        assert_eq!(send(&mut bridge, &mut state, note(number), 127), 1);
        assert_eq!(state.beat_fx.target, target);
    }
    assert_eq!(send(&mut bridge, &mut state, note(71), 127), 1);
    assert_eq!(send(&mut bridge, &mut state, note(71), 127), 0);
    assert_eq!(send(&mut bridge, &mut state, note(71), 0), 0);
    assert!(state.beat_fx.enabled);
    let cc = |number| Source::Cc { channel: 4, number };
    assert_eq!(send(&mut bridge, &mut state, cc(2), 127), 0);
    assert_eq!(send(&mut bridge, &mut state, cc(34), 127), 1);
    assert_eq!(state.beat_fx.amount, 1.);
    assert_eq!(send(&mut bridge, &mut state, note(75), 127), 1);
    assert_eq!(state.beat_fx.beats, 0.75);
    let lamps = bridge.state_feedback("DDJ-GRV6", &state, 0, &[]);
    assert!(lamps.contains(&[0x94, 0x47, 127]));
    assert!(lamps.contains(&[0xb4, 0x64, 0x21]));
    assert_eq!(send(&mut bridge, &mut state, note(44), 127), 1);
    assert_eq!(state.beat_fx.kind, None);
    assert!(!state.beat_fx.enabled);
    send(&mut bridge, &mut state, note(71), 127);
    assert!(!state.beat_fx.enabled);
    assert_eq!(send(&mut bridge, &mut state, note(33), 127), 1);
    assert_eq!(state.beat_fx.kind, Some(EffectKind::Echo));
}

#[test]
fn grv6_pad_banks_follow_each_deck_and_shift_layer() {
    use djcraft_controllers::PadMode;
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let state = SessionState::default();
    for deck in 0..4 {
        for alternate in [false, true] {
            let channel = 7 + deck * 2 + u8::from(alternate);
            for (number, mode, expected) in [
                (
                    18,
                    PadMode::Stems,
                    HostCommand::PerformancePad {
                        deck,
                        mode: PadMode::Stems,
                        pad: 2,
                        alternate,
                    },
                ),
                (
                    34,
                    PadMode::BeatJump,
                    HostCommand::PerformancePad {
                        deck,
                        mode: PadMode::BeatJump,
                        pad: 2,
                        alternate,
                    },
                ),
                (
                    66,
                    PadMode::Keyboard,
                    HostCommand::PerformancePad {
                        deck,
                        mode: PadMode::Keyboard,
                        pad: 2,
                        alternate,
                    },
                ),
                (
                    50,
                    PadMode::Sampler,
                    HostCommand::SamplePad {
                        deck,
                        slot: (deck % 2) * 8 + 2,
                        stop: alternate,
                    },
                ),
            ] {
                let source = Source::Note { channel, number };
                bridge.process("DDJ-GRV6", Event { source, value: 127 }, &state);
                assert_eq!(bridge.take_commands(), vec![expected]);
                assert_eq!(bridge.take_pad_modes()[usize::from(deck)], Some(mode));
                bridge.process("DDJ-GRV6", Event { source, value: 0 }, &state);
                assert!(bridge.take_commands().is_empty());
            }
        }
    }
}

#[test]
fn bundled_grv6_profile_compiles_and_routes_each_deck() {
    let profile = Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json"))
        .expect("starter profile should validate without hardware");
    assert_eq!(profile.mappings.len(), 597);
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState::default();
    state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
    for deck in 0..4 {
        state.decks[usize::from(deck)].colour_fx.selected = 1 << deck;
        for (msb, lsb, position) in [(0, 0, 0.), (64, 0, 0.5), (127, 127, 1.)] {
            assert!(
                bridge
                    .process(
                        "DDJ-GRV6",
                        Event {
                            source: Source::Cc {
                                channel: 6,
                                number: 23 + deck
                            },
                            value: msb
                        },
                        &state
                    )
                    .is_empty()
            );
            let actions = bridge.process(
                "DDJ-GRV6",
                Event {
                    source: Source::Cc {
                        channel: 6,
                        number: 55 + deck,
                    },
                    value: lsb,
                },
                &state,
            );
            assert!(
                matches!(&actions[..], [Action::Deck { deck: target, action: DeckAction::SetColourFx { state: fx } }] if *target == deck && fx.position == position && fx.selected == 1 << deck && fx.enabled)
            );
        }
    }
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
fn grv6_dedicated_loop_buttons_keep_the_current_pad_page() {
    use djcraft_controllers::LoopButton;
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState::default();
    state.decks = vec![djcraft_protocol::DeckState::default(); 4];
    state.decks[2].loop_region = Some([1., 2.]);
    for deck in 0..4 {
        for (number, button) in [
            (16, LoopButton::In),
            (17, LoopButton::Out),
            (20, LoopButton::FourBeat),
            (77, LoopButton::Reloop),
            (81, LoopButton::Half),
            (83, LoopButton::Double),
        ] {
            let source = Source::Note {
                channel: deck,
                number,
            };
            bridge.process("DDJ-GRV6", Event { source, value: 127 }, &state);
            assert_eq!(
                bridge.take_commands(),
                vec![HostCommand::LoopButton { deck, button }]
            );
            assert!(bridge.take_pad_modes().iter().all(Option::is_none));
            bridge.process("DDJ-GRV6", Event { source, value: 0 }, &state);
            assert!(bridge.take_commands().is_empty());
        }
    }
    let leds = bridge.state_feedback("DDJ-GRV6", &state, 0, &[Some(1.), None, None, None]);
    assert!(leds.contains(&[0x90, 16, 127]));
    assert!(leds.contains(&[0x90, 17, 0]));
    assert!(leds.contains(&[0x92, 17, 127]));
}

#[test]
fn grv6_master_cue_toggles_once_per_press() {
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let state = SessionState::default();
    let source = Source::Note {
        channel: 6,
        number: 99,
    };
    for (value, pressed) in [(127, true), (127, false), (0, false), (127, true)] {
        let actions = bridge.process("DDJ-GRV6", Event { source, value }, &state);
        if pressed {
            assert_eq!(
                actions,
                vec![Action::Mixer {
                    action: MixerAction::ToggleMasterCue
                }]
            );
        } else {
            assert!(actions.is_empty());
        }
    }
}

#[test]
fn grv6_colour_fx_button_toggles_global_bypass_and_lamp() {
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState::default();
    let source = Source::Note {
        channel: 6,
        number: 0,
    };
    assert!(
        bridge
            .state_feedback("DDJ-GRV6", &state, 0, &[])
            .contains(&[0x96, 0, 127])
    );
    assert_eq!(
        bridge.process("DDJ-GRV6", Event { source, value: 127 }, &state),
        vec![Action::Mixer {
            action: MixerAction::ToggleColourFxBypass
        }]
    );
    state.colour_fx_bypassed = true;
    assert!(
        bridge
            .state_feedback("DDJ-GRV6", &state, 0, &[])
            .contains(&[0x96, 0, 0])
    );
    assert!(
        bridge
            .process("DDJ-GRV6", Event { source, value: 0 }, &state)
            .is_empty()
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
        (105, PadMode::Keyboard),
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
fn grv6_slip_and_vinyl_lamps_follow_each_decks_mode() {
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let mut state = SessionState {
        decks: vec![djcraft_protocol::DeckState::default(); 4],
        ..Default::default()
    };
    for deck in 0..4u8 {
        let status = 0x90 | deck;
        assert!(
            bridge
                .state_feedback("DDJ-GRV6", &state, 0, &[])
                .contains(&[status, 64, 0])
        );
        state.decks[usize::from(deck)].slip_enabled = true;
        assert!(
            bridge
                .state_feedback("DDJ-GRV6", &state, 0, &[])
                .contains(&[status, 64, 127])
        );
        for (enabled, velocity) in [(false, 0), (true, 127)] {
            bridge.set_vinyl_mode(deck, enabled);
            assert_eq!(bridge.scratch_modes()[usize::from(deck)], Some(enabled));
            let lights = bridge.jog_feedback("DDJ-GRV6");
            assert!(lights.contains(&[status, 23, velocity]));
            assert!(lights.contains(&[0x90 | (deck ^ 1), 23, 127]));
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
                .contains(&[0x90 | deck, 53, 127])
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
        let led = [0x90 | (7 + deck * 2), slot, 127];
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
