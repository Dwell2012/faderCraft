use djcraft_controllers::{Bridge, Event, HostCommand, Profile};
use djcraft_protocol::{Action, DeckAction, DeckState, SessionState};
use std::time::{Duration, Instant};

fn bridge() -> Bridge {
    Bridge::new(
        Profile::from_json(include_str!(
            "../../../profiles/numark-mixtrack-pro-ii.json"
        ))
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn fx_encoder_follows_the_first_ui_control_for_every_effect_kind() {
    use djcraft_protocol::{EffectKind, Role};
    let (mut engine, _mixer) = djcraft_engine::Engine::headless(48000);
    let mut b = bridge();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetEffectBeats { beats: 0.25 },
            },
        )
        .unwrap();
    for kind in EffectKind::ALL {
        let slot = if kind == EffectKind::Filter { 0 } else { 1 };
        let mut effect = djcraft_protocol::EffectState {
            kind,
            enabled: true,
            parameter: 0.5,
            amount: 0.5,
            sync: true,
            beats: 0.25,
            ..Default::default()
        };
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: 0,
                    action: DeckAction::SetEffect { slot, effect },
                },
            )
            .unwrap();
        for action in b.process(
            "MixTrack Pro II",
            Event::parse(&[0xb0, 0x1b + slot, 1]).unwrap(),
            &engine.state(),
        ) {
            engine.apply(Role::Host, action).unwrap();
        }
        if kind == EffectKind::Filter {
            effect.parameter = 0.51;
            effect.amount = 1.;
        } else {
            effect.amount = 0.51;
        }
        assert_eq!(
            engine.state().decks[0].effects[slot as usize],
            effect,
            "{kind:?}"
        );
    }
}

#[test]
fn fx_encoders_adjust_only_their_slot_and_preserve_effect_settings() {
    let (mut engine, _mixer) = djcraft_engine::Engine::headless(48000);
    let mut b = bridge();
    for deck in 0..2u8 {
        for slot in 1..3u8 {
            let mut effect = engine.state().decks[usize::from(deck)].effects[usize::from(slot)];
            effect.kind = djcraft_protocol::EffectKind::Echo;
            effect.enabled = true;
            effect.amount = 0.5;
            effect.parameter = 0.37;
            effect.feedback = 0.61;
            engine
                .apply(
                    djcraft_protocol::Role::Host,
                    Action::Deck {
                        deck,
                        action: DeckAction::SetEffect { slot, effect },
                    },
                )
                .unwrap();
            for (raw, expected) in [(1, 0.51f32), (127, 0.5), (63, 1.), (65, 0.37)] {
                let state = engine.state();
                let actions = b.process(
                    "MixTrack Pro II",
                    Event::parse(&[0xb0, 0x1b + deck * 3 + slot, raw]).unwrap(),
                    &state,
                );
                assert_eq!(actions.len(), 1);
                for action in actions {
                    engine.apply(djcraft_protocol::Role::Host, action).unwrap();
                }
                let actual = engine.state().decks[usize::from(deck)].effects[usize::from(slot)];
                assert!((actual.amount - expected).abs() < 1e-6);
                effect.amount = actual.amount;
                assert_eq!(actual, effect);
            }
            for raw in [0, 64] {
                assert!(
                    b.process(
                        "MixTrack Pro II",
                        Event::parse(&[0xb0, 0x1b + deck * 3 + slot, raw]).unwrap(),
                        &engine.state()
                    )
                    .is_empty()
                );
            }
        }
    }
    assert!(
        b.process(
            "other",
            Event::parse(&[0xb0, 0x1b, 1]).unwrap(),
            &engine.state()
        )
        .is_empty()
    );
    let mut profile = b.profile().clone();
    for m in &mut profile.mappings {
        if let djcraft_controllers::Binding::Parameter {
            parameter: djcraft_controllers::Parameter::EffectParameter { slot, .. },
            ..
        } = &mut m.binding
        {
            *slot = 99;
        }
    }
    assert!(profile.validate().is_err());
}

#[test]
fn loop_and_sample_modes_dispatch_independently_on_each_deck_with_shift() {
    let s = state(false);
    let now = Instant::now();
    for deck in 0..2u8 {
        for alternate in [false, true] {
            let mut b = bridge();
            if alternate {
                button(&mut b, 0x61 + deck, 127, &s, now);
            }
            for pad in 0..4u8 {
                let loop_note = [[0x53, 0x54, 0x55, 0x63], [0x56, 0x57, 0x58, 0x64]]
                    [usize::from(deck)][usize::from(pad)];
                for (note, command) in [
                    (
                        loop_note,
                        HostCommand::LoopPad {
                            deck,
                            pad,
                            alternate,
                        },
                    ),
                    (
                        0x65 + deck * 4 + pad,
                        HostCommand::SamplePad {
                            deck,
                            slot: pad,
                            stop: alternate,
                        },
                    ),
                ] {
                    assert!(button(&mut b, note, 127, &s, now).is_empty());
                    assert_eq!(b.take_commands(), vec![command]);
                    button(&mut b, note, 127, &s, now);
                    assert!(b.take_commands().is_empty());
                    button(&mut b, note, 0, &s, now);
                    assert!(b.take_commands().is_empty());
                }
            }
        }
    }
}

#[test]
fn fx_pads_toggle_existing_slot_settings_through_the_engine_and_leave_tap_free() {
    use djcraft_protocol::{EffectKind, Role};
    let (mut engine, _mixer) = djcraft_engine::Engine::headless(48000);
    let now = Instant::now();
    let mut b = bridge();
    for deck in 0..2u8 {
        for slot in 0..3u8 {
            let note = 0x59 + deck * 4 + slot;
            let mut effect = engine.state().decks[usize::from(deck)].effects[usize::from(slot)];
            effect.kind = if slot == 0 {
                EffectKind::Filter
            } else {
                EffectKind::Echo
            };
            effect.amount = 0.37;
            effect.parameter = 0.2;
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck,
                        action: DeckAction::SetEffect { slot, effect },
                    },
                )
                .unwrap();
            for enabled in [true, false] {
                let s = engine.state();
                let actions = button(&mut b, note, 127, &s, now);
                assert_eq!(actions.len(), 1);
                for action in actions {
                    engine.apply(Role::Host, action).unwrap();
                }
                let actual = engine.state().decks[usize::from(deck)].effects[usize::from(slot)];
                effect.enabled = enabled;
                assert_eq!(actual.active_in_slot(slot as usize), enabled);
                assert_eq!(actual, effect);
                assert!(button(&mut b, note, 127, &engine.state(), now).is_empty());
                assert!(button(&mut b, note, 0, &engine.state(), now).is_empty());
            }
        }
        assert!(button(&mut b, 0x5c + deck * 4, 127, &engine.state(), now).is_empty());
        assert!(b.take_commands().is_empty());
    }
}

#[test]
fn headphone_leds_follow_actual_monitor_selection_and_stay_on_after_release() {
    let mut b = bridge();
    let s = state(false);
    let now = Instant::now();
    assert!(b.feedback("other port", 0).is_empty());
    for deck in 0..2 {
        let feedback = b.feedback("MixTrack Pro II", deck);
        assert!(feedback.contains(&[0x90, 0x51, if deck == 0 { 100 } else { 0 }]));
        assert!(feedback.contains(&[0x90, 0x52, if deck == 1 { 100 } else { 0 }]));
    }
    let before = b.feedback("MixTrack Pro II", 0);
    button(&mut b, 0x52, 127, &s, now);
    assert_eq!(b.feedback("MixTrack Pro II", 0), before);
    button(&mut b, 0x52, 0, &s, now);
    assert!(
        b.feedback("MixTrack Pro II", 1)
            .contains(&[0x90, 0x52, 100])
    );
    let mut profile = b.profile().clone();
    for m in &mut profile.mappings {
        if let djcraft_controllers::Binding::HostButton { led: Some(led), .. } = &mut m.binding {
            *led = djcraft_controllers::Source::Cc {
                channel: 0,
                number: 81,
            };
        }
    }
    assert!(profile.validate().is_err());
}

#[test]
fn headphone_leds_support_multiple_cues_and_all_off() {
    let b = bridge();
    for mask in [0, 1, 2, 3] {
        let feedback = b.feedback_monitor(
            "MixTrack Pro II",
            djcraft_protocol::MonitorState {
                cue_decks: Some(mask),
                ..Default::default()
            },
        );
        assert!(feedback.contains(&[0x90, 0x51, if mask & 1 != 0 { 100 } else { 0 }]));
        assert!(feedback.contains(&[0x90, 0x52, if mask & 2 != 0 { 100 } else { 0 }]));
    }
}
fn state(playing: bool) -> SessionState {
    SessionState {
        decks: vec![
            DeckState {
                playing,
                tempo: 1.08,
                key_lock: true,
                ..Default::default()
            };
            2
        ],
        ..Default::default()
    }
}
fn button(b: &mut Bridge, note: u8, value: u8, state: &SessionState, now: Instant) -> Vec<Action> {
    b.process_at(
        "MixTrack Pro II",
        Event::parse(&[0x90, note, value]).unwrap(),
        state,
        now,
    )
}

#[test]
fn official_sync_headphone_and_stutter_buttons_resolve_to_correct_decks() {
    let mut b = bridge();
    let s = state(false);
    let now = Instant::now();
    for (note, command) in [
        (0x40, HostCommand::Sync { deck: 0 }),
        (0x47, HostCommand::Sync { deck: 1 }),
        (0x51, HostCommand::ToggleHeadphoneCue { deck: 0 }),
        (0x52, HostCommand::ToggleHeadphoneCue { deck: 1 }),
        (0x4a, HostCommand::Stutter { deck: 0 }),
        (0x4c, HostCommand::Stutter { deck: 1 }),
    ] {
        assert!(button(&mut b, note, 127, &s, now).is_empty());
        assert_eq!(b.take_commands(), vec![command]);
        button(&mut b, note, 127, &s, now);
        assert!(b.take_commands().is_empty());
        button(&mut b, note, 0, &s, now);
        assert!(b.take_commands().is_empty());
    }
}

#[test]
fn cue_hold_release_and_disconnect_cleanup_are_delivered_once() {
    let mut b = bridge();
    let s = state(false);
    let now = Instant::now();
    for (note, deck) in [(0x33, 0), (0x3c, 1)] {
        button(&mut b, note, 127, &s, now);
        assert_eq!(
            b.take_commands(),
            vec![HostCommand::Cue {
                deck,
                pressed: true
            }]
        );
        button(&mut b, note, 0, &s, now);
        assert_eq!(
            b.take_commands(),
            vec![HostCommand::Cue {
                deck,
                pressed: false
            }]
        );
    }
    button(&mut b, 0x33, 127, &s, now);
    b.take_commands();
    b.disconnect("MixTrack Pro II");
    assert_eq!(
        b.take_commands(),
        vec![HostCommand::Cue {
            deck: 0,
            pressed: false
        }]
    );
    b.disconnect("MixTrack Pro II");
    assert!(b.take_commands().is_empty());
}

#[test]
fn pitch_buttons_repeat_while_held_and_stop_without_rewriting_tempo() {
    let s = state(true);
    let now = Instant::now();
    for (note, deck, sign) in [(0x43, 0, -1.), (0x44, 0, 1.), (0x45, 1, -1.), (0x46, 1, 1.)] {
        let mut b = bridge();
        let expected = vec![Action::Deck {
            deck,
            action: DeckAction::PitchBend {
                seconds: sign * 0.0006,
            },
        }];
        assert_eq!(button(&mut b, note, 127, &s, now), expected);
        assert!(button(&mut b, note, 127, &s, now + Duration::from_millis(10)).is_empty());
        assert_eq!(b.tick_at(&s, now + Duration::from_millis(20)), expected);
        button(&mut b, note, 0, &s, now + Duration::from_millis(21));
        assert!(b.tick_at(&s, now + Duration::from_secs(2)).is_empty());
    }
}

#[test]
fn paused_bend_does_not_queue_a_jump_and_shift_minus_toggles_key_lock_only() {
    let mut b = bridge();
    let mut s = state(false);
    let now = Instant::now();
    assert!(button(&mut b, 0x44, 127, &s, now).is_empty());
    assert!(b.tick_at(&s, now + Duration::from_secs(5)).is_empty());
    s.decks[0].playing = true;
    assert_eq!(
        b.tick_at(&s, now + Duration::from_millis(5020)),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::PitchBend { seconds: 0.0006 }
        }]
    );
    b.disconnect("MixTrack Pro II");
    assert!(b.tick_at(&s, now + Duration::from_secs(6)).is_empty());
    button(&mut b, 0x61, 127, &s, now);
    assert_eq!(
        button(&mut b, 0x43, 127, &s, now),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::SetKeyLock { on: false }
        }]
    );
    assert!(b.tick_at(&s, now + Duration::from_secs(1)).is_empty());
}
