use djcraft_controllers::*;
use djcraft_protocol::{Action, DeckAction, DeckState, MixerAction, Role, SessionState};

#[test]
fn bundled_numark_profile_loads_with_pickup_and_dispatches_play() {
    let profile = Profile::from_json(include_str!(
        "../../../profiles/numark-mixtrack-pro-ii.json"
    ))
    .unwrap();
    assert_eq!(profile.mappings.len(), 140);
    for mapping in &profile.mappings {
        if let Binding::Parameter {
            encoding, pickup, ..
        } = mapping.binding
        {
            if encoding == Encoding::Absolute {
                assert!(pickup);
            } else {
                assert!(!pickup);
            }
        }
    }
    let mut bridge = Bridge::new(profile).unwrap();
    let play = Event::parse(&[0x90, 0x3b, 127]).unwrap();
    assert_eq!(
        bridge.process("MixTrack Pro II", play, &state()),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Play
        }]
    );
}

fn state() -> SessionState {
    SessionState {
        decks: vec![DeckState {
            volume: 0.5,
            tempo: 1.,
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn custom_mappings_control_extra_decks_independently() {
    let (mut engine, _) = djcraft_engine::Engine::headless(48000);
    for target in [2, 3] {
        let mut bridge = Bridge::new(profile(Binding::Parameter {
            parameter: Parameter::Volume { deck: target },
            min: 0.,
            max: 1.,
            center: None,
            step: 0.1,
            encoding: Encoding::Absolute,
            pickup: false,
        }))
        .unwrap();
        let actions = bridge.process("Numark", event(0), &engine.state());
        assert_eq!(
            actions,
            vec![Action::Deck {
                deck: target,
                action: DeckAction::SetVolume { level: 0. },
            }]
        );
        for action in actions {
            engine.apply(Role::Host, action).unwrap();
        }
        let state = engine.state();
        assert_eq!(state.decks[target as usize].volume, 0.);
        assert_eq!(state.decks[0].volume, 1.);
        assert_eq!(state.decks[1].volume, 1.);
    }
}
fn profile(binding: Binding) -> Profile {
    Profile {
        version: PROFILE_VERSION,
        name: "Test".into(),
        mappings: vec![Mapping {
            id: "test".into(),
            port: "Numark".into(),
            source: Source::Cc {
                channel: 0,
                number: 22,
            },
            modifiers: Vec::new(),
            binding,
        }],
    }
}
fn parameter(encoding: Encoding, pickup: bool) -> Binding {
    Binding::Parameter {
        parameter: Parameter::Volume { deck: 0 },
        min: 0.,
        max: 1.,
        center: None,
        step: 0.1,
        encoding,
        pickup,
    }
}
fn event(value: u16) -> Event {
    Event {
        source: Source::Cc {
            channel: 0,
            number: 22,
        },
        value,
    }
}
fn volume(value: f32) -> Action {
    Action::Deck {
        deck: 0,
        action: DeckAction::SetVolume { level: value },
    }
}

#[test]
fn parser_normalizes_release_and_keeps_channel_and_14_bit_pitch() {
    assert_eq!(Event::parse(&[0x81, 59, 64]), Event::parse(&[0x91, 59, 0]));
    assert_eq!(
        Event::parse(&[0xe3, 0, 64]),
        Some(Event {
            source: Source::PitchBend { channel: 3 },
            value: 8192
        })
    );
    for bytes in [
        vec![],
        vec![0x90, 1],
        vec![0x90, 128, 1],
        vec![0xf0, 1, 2],
        vec![0xa0, 1, 2],
        vec![0x90, 1, 1, 1],
    ] {
        assert_eq!(Event::parse(&bytes), None);
    }
}

#[test]
fn learn_ignores_note_releases_but_accepts_zero_faders() {
    assert_eq!(Event::parse(&[0x90, 59, 0]).unwrap().learn(), None);
    assert_eq!(
        Event::parse(&[0x90, 59, 127]).unwrap().learn(),
        Some(Source::Note {
            channel: 0,
            number: 59
        })
    );
    assert_eq!(event(0).learn(), Some(event(0).source));
}

#[test]
fn ports_and_channels_are_isolated() {
    let mut bridge = Bridge::new(profile(parameter(Encoding::Absolute, false))).unwrap();
    assert!(bridge.process("Other", event(127), &state()).is_empty());
    let other = Event {
        source: Source::Cc {
            channel: 1,
            number: 22,
        },
        value: 127,
    };
    assert!(bridge.process("Numark", other, &state()).is_empty());
    assert_eq!(
        bridge.process("Numark", event(127), &state()),
        vec![volume(1.)]
    );
}

#[test]
fn absolute_endpoints_and_inverted_ranges() {
    let mut p = profile(parameter(Encoding::Absolute, false));
    if let Binding::Parameter { min, max, .. } = &mut p.mappings[0].binding {
        *min = 1.;
        *max = 0.;
    }
    let mut bridge = Bridge::new(p).unwrap();
    assert_eq!(
        bridge.process("Numark", event(0), &state()),
        vec![volume(1.)]
    );
    assert_eq!(
        bridge.process("Numark", event(127), &state()),
        vec![volume(0.)]
    );
    assert!(bridge.process("Numark", event(128), &state()).is_empty());
}

#[test]
fn pickup_waits_for_crossing_and_rearms_after_external_changes() {
    let mut bridge = Bridge::new(profile(parameter(Encoding::Absolute, true))).unwrap();
    let mut s = state();
    assert!(bridge.process("Numark", event(0), &s).is_empty());
    assert!(!bridge.process("Numark", event(80), &s).is_empty());
    s.decks[0].volume = 80. / 127.;
    assert!(!bridge.process("Numark", event(90), &s).is_empty());
    s.decks[0].volume = 0.1;
    assert!(bridge.process("Numark", event(100), &s).is_empty());
    assert!(!bridge.process("Numark", event(0), &s).is_empty());
}

#[test]
fn physical_fader_below_app_full_volume_cannot_change_it_until_pickup() {
    let mut bridge = Bridge::new(profile(parameter(Encoding::Absolute, true))).unwrap();
    let mut s = state();
    s.decks[0].volume = 1.;
    for raw in 0..127 {
        assert!(
            bridge.process("Numark", event(raw), &s).is_empty(),
            "acquired early at {raw}"
        );
    }
    assert_eq!(bridge.process("Numark", event(127), &s), vec![volume(1.)]);
    assert_eq!(
        bridge.process("Numark", event(100), &s),
        vec![volume(100. / 127.)]
    );
    // A software change re-arms the physical fader at its new destination.
    s.decks[0].volume = 0.;
    for raw in (1..100).rev() {
        assert!(bridge.process("Numark", event(raw), &s).is_empty());
    }
    assert_eq!(bridge.process("Numark", event(0), &s), vec![volume(0.)]);
}

#[test]
fn relative_encoders_use_device_encoding_and_clamp() {
    let mut bridge = Bridge::new(profile(parameter(Encoding::TwosComplement, false))).unwrap();
    assert_eq!(
        bridge.process("Numark", event(127), &state()),
        vec![volume(0.4)]
    );
    assert_eq!(
        bridge.process("Numark", event(1), &state()),
        vec![volume(0.6)]
    );
    assert!(bridge.process("Numark", event(64), &state()).is_empty());
    assert_eq!(
        bridge.process("Numark", event(63), &state()),
        vec![volume(1.)]
    );
    let mut bridge = Bridge::new(profile(parameter(Encoding::Offset64, false))).unwrap();
    assert_eq!(
        bridge.process("Numark", event(63), &state()),
        vec![volume(0.4)]
    );
    assert_eq!(
        bridge.process("Numark", event(65), &state()),
        vec![volume(0.6)]
    );
}

#[test]
fn buttons_debounce_and_release_on_disconnect() {
    let press = Action::Deck {
        deck: 0,
        action: DeckAction::Scratch { speed: Some(0.) },
    };
    let release = Action::Deck {
        deck: 0,
        action: DeckAction::Scratch { speed: None },
    };
    let mut bridge = Bridge::new(profile(Binding::Button {
        press: press.clone(),
        release: Some(release.clone()),
    }))
    .unwrap();
    assert_eq!(
        bridge.process("Numark", event(127), &state()),
        vec![press.clone()]
    );
    assert!(bridge.process("Numark", event(127), &state()).is_empty());
    assert_eq!(
        bridge.process("Numark", event(0), &state()),
        vec![release.clone()]
    );
    assert!(bridge.process("Numark", event(0), &state()).is_empty());
    bridge.process("Numark", event(127), &state());
    assert_eq!(bridge.disconnect("Numark"), vec![release]);
    assert_eq!(bridge.process("Numark", event(127), &state()), vec![press]);
}

#[test]
fn toggles_follow_host_state_and_ignore_unknown_decks() {
    let mut bridge = Bridge::new(profile(Binding::TogglePlay { deck: 0 })).unwrap();
    assert_eq!(
        bridge.process("Numark", event(127), &state()),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Play
        }]
    );
    bridge.process("Numark", event(0), &state());
    let mut s = state();
    s.decks[0].playing = true;
    assert_eq!(
        bridge.process("Numark", event(127), &s),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Pause
        }]
    );
    let mut bridge = Bridge::new(profile(Binding::TogglePlay { deck: 255 })).unwrap();
    assert!(bridge.process("Numark", event(127), &s).is_empty());
}

#[test]
fn json_roundtrip_and_validation_preserve_old_profile_on_error() {
    let p = profile(parameter(Encoding::Absolute, true));
    assert_eq!(Profile::from_json(&p.to_json().unwrap()).unwrap(), p);
    let mut bridge = Bridge::new(p.clone()).unwrap();
    let mut bad = p.clone();
    bad.version += 1;
    assert!(bridge.replace(bad).is_err());
    assert_eq!(bridge.profile(), &p);
    let mut bad = p.clone();
    bad.mappings.push(bad.mappings[0].clone());
    assert!(bad.validate().is_err());
    let mut bad = p;
    bad.mappings[0].binding = parameter(Encoding::Offset64, true);
    assert!(bad.validate().is_err());
}

#[test]
fn mapped_actions_use_engine_validation_and_roles_without_audio_device() {
    let (mut engine, _mixer) = djcraft_engine::Engine::headless(48000);
    let mut bridge = Bridge::new(profile(parameter(Encoding::Absolute, false))).unwrap();
    let action = bridge
        .process("Numark", event(0), &engine.state())
        .pop()
        .unwrap();
    assert!(engine.apply(Role::Viewer, action.clone()).is_err());
    engine.apply(Role::Host, action).unwrap();
    assert_eq!(engine.state().decks[0].volume, 0.);
    let mut bridge = Bridge::new(profile(Binding::Button {
        press: Action::Mixer {
            action: MixerAction::SetMasterGain { level: f32::MAX },
        },
        release: None,
    }))
    .unwrap();
    let action = bridge
        .process("Numark", event(127), &engine.state())
        .pop()
        .unwrap();
    assert!(engine.apply(Role::Host, action).is_err());
}

#[test]
fn shift_layers_keep_release_after_modifier_is_released_first() {
    let shift = Source::Note {
        channel: 0,
        number: 97,
    };
    let press = volume(0.);
    let release = volume(0.5);
    let mut p = profile(Binding::Button {
        press: press.clone(),
        release: Some(release.clone()),
    });
    p.mappings[0].modifiers.push(Modifier {
        source: shift,
        pressed: true,
    });
    let mut bridge = Bridge::new(p).unwrap();
    assert!(bridge.process("Numark", event(127), &state()).is_empty());
    bridge.process(
        "Numark",
        Event {
            source: shift,
            value: 127,
        },
        &state(),
    );
    assert_eq!(bridge.process("Numark", event(127), &state()), vec![press]);
    bridge.process(
        "Numark",
        Event {
            source: shift,
            value: 0,
        },
        &state(),
    );
    assert_eq!(bridge.process("Numark", event(0), &state()), vec![release]);
}

#[test]
fn learning_releases_held_actions_and_preserves_other_bindings() {
    let release = volume(0.5);
    let p = profile(Binding::Button {
        press: volume(0.),
        release: Some(release.clone()),
    });
    let mut bridge = Bridge::new(p.clone()).unwrap();
    bridge.process("Numark", event(127), &state());
    let note = Event::parse(&[0x92, 44, 127]).unwrap();
    assert_eq!(
        bridge.learn("test", "Other", note).unwrap(),
        Some(vec![release])
    );
    assert_eq!(bridge.profile().mappings[0].source, note.source);
    assert_eq!(bridge.profile().mappings[0].binding, p.mappings[0].binding);
    assert_eq!(
        bridge
            .learn("test", "Other", Event { value: 0, ..note })
            .unwrap(),
        None
    );
    assert!(bridge.learn("missing", "Other", note).is_err());
}

#[test]
fn exact_center_and_14_bit_endpoints() {
    let mut p = profile(parameter(Encoding::Absolute, false));
    if let Binding::Parameter { center, .. } = &mut p.mappings[0].binding {
        *center = Some(0.5);
    }
    let mut bridge = Bridge::new(p.clone()).unwrap();
    assert_eq!(
        bridge.process("Numark", event(64), &state()),
        vec![volume(0.5)]
    );
    p.mappings[0].source = Source::PitchBend { channel: 0 };
    let mut bridge = Bridge::new(p).unwrap();
    for (raw, expected) in [(0, 0.), (8192, 0.5), (16383, 1.)] {
        let pitch = Event {
            source: Source::PitchBend { channel: 0 },
            value: raw,
        };
        assert_eq!(
            bridge.process("Numark", pitch, &state()),
            vec![volume(expected)]
        );
    }
}

#[test]
fn bundled_pitch_fader_matches_app_extremes_and_exact_neutral_with_pickup() {
    let profile = Profile::from_json(include_str!(
        "../../../profiles/numark-mixtrack-pro-ii.json"
    ))
    .unwrap();
    for (deck, cc) in [(0, 0x0d), (1, 0x0e)] {
        for (raw, ratio) in [
            (0, djcraft_engine::MAX_TEMPO),
            (64, 1.),
            (127, djcraft_engine::MIN_TEMPO),
        ] {
            let mut bridge = Bridge::new(profile.clone()).unwrap();
            let mut s = state();
            s.decks = vec![s.decks[0].clone(); 2];
            s.decks[usize::from(deck)].tempo = ratio;
            let event = Event::parse(&[0xb0, cc, raw]).unwrap();
            assert_eq!(
                bridge.process("MixTrack Pro II", event, &s),
                vec![Action::Deck {
                    deck,
                    action: DeckAction::SetTempo { ratio }
                }]
            );
        }
    }
}
