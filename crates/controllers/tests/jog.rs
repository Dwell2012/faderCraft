use djcraft_controllers::*;
use djcraft_engine::{Engine, Mixer, Track};
use djcraft_protocol::{Action, DeckAction, DeckState, Role, SessionState, TrackInfo};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn profile() -> Profile {
    let mut profile = Profile::from_json(include_str!(
        "../../../profiles/numark-mixtrack-pro-ii.json"
    ))
    .unwrap();
    // Retain coverage for optional touch hardware independently of the Numark.
    for mapping in &mut profile.mappings {
        if let Binding::Jog { config } = &mut mapping.binding {
            config.seconds_per_tick = 0.003; // Generic API calibration fixture.
            config.scratch_enabled = true;
            config.touch = Some(Source::Note {
                channel: 0,
                number: [0x4d, 0x4e][usize::from(config.deck)],
            });
        }
    }
    profile
}
fn bridge() -> Bridge {
    Bridge::new(profile()).unwrap()
}
fn touchless_bridge() -> Bridge {
    let mut p = Profile::from_json(include_str!(
        "../../../profiles/numark-mixtrack-pro-ii.json"
    ))
    .unwrap();
    for mapping in &mut p.mappings {
        if let Binding::Jog { config } = &mut mapping.binding {
            config.touch = None;
            config.seconds_per_tick = 0.003;
        }
    }
    Bridge::new(p).unwrap()
}
fn state(playing: bool) -> SessionState {
    SessionState {
        decks: (0..2)
            .map(|deck| DeckState {
                path: Some(format!("track-{deck}")),
                playing,
                key_lock: true,
                tempo: 1.08,
                position_seconds: 5.,
                track: Some(TrackInfo {
                    title: format!("Track {deck}"),
                    duration_seconds: 10.,
                    loaded_seconds: 10.,
                    fully_loaded: true,
                }),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}
fn event(status: u8, number: u8, value: u8) -> Event {
    Event::parse(&[status, number, value]).unwrap()
}
fn run(
    b: &mut Bridge,
    status: u8,
    number: u8,
    value: u8,
    s: &SessionState,
    at: Instant,
) -> Vec<Action> {
    b.process_at("MixTrack Pro II", event(status, number, value), s, at)
}
fn position(actions: &[Action]) -> f64 {
    actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::Deck {
                action:
                    DeckAction::ScratchTo { seconds }
                    | DeckAction::JogScratch { seconds, .. }
                    | DeckAction::Seek { seconds },
                ..
            } => Some(*seconds),
            _ => None,
        })
        .expect("position action")
}
fn released(actions: &[Action]) -> bool {
    actions.iter().any(|a| {
        matches!(
            a,
            Action::Deck {
                action: DeckAction::Scratch { speed: None },
                ..
            }
        )
    })
}

#[test]
fn scratch_mode_survives_disconnect_and_drives_reconnection_leds() {
    let mut b = bridge();
    let s = state(false);
    let now = Instant::now();
    assert_eq!(b.scratch_modes()[0], Some(true));
    run(&mut b, 0x90, 0x48, 127, &s, now);
    assert_eq!(b.scratch_modes()[0], Some(false));
    b.disconnect("MixTrack Pro II");
    assert_eq!(b.scratch_modes()[0], Some(false));
    assert!(b.jog_feedback("MixTrack Pro II").contains(&[0x90, 0x48, 0]));
    run(
        &mut b,
        0x90,
        0x48,
        127,
        &s,
        now + Duration::from_millis(100),
    );
    assert_eq!(b.scratch_modes()[0], Some(true));
    assert!(
        b.jog_feedback("MixTrack Pro II")
            .contains(&[0x90, 0x48, 100])
    );
}

#[test]
fn contact_pulses_during_motion_do_not_release_seek_or_reanchor_the_scratch() {
    let s = state(true);
    let now = Instant::now();
    for (touch, movement) in [(0x4d, 0x19), (0x4e, 0x18)] {
        let mut b = bridge();
        let mut target = 5.;
        for packet in 0..40 {
            let at = now + Duration::from_millis(packet * 5);
            let down = run(&mut b, 0x90, touch, 127, &s, at);
            if packet > 0 {
                assert!(down.is_empty(), "contact pulse re-grabbed the deck");
            }
            let actions = run(&mut b, 0xb0, movement, 1, &s, at);
            target += 0.003;
            assert!((position(&actions) - target).abs() < 1e-9);
            assert!(!released(&actions));
            assert!(run(&mut b, 0x90, touch, 0, &s, at).is_empty());
            assert!(b.tick_at(&s, at + Duration::from_millis(4)).is_empty());
        }
        assert!(b.tick_at(&s, now + Duration::from_millis(224)).is_empty());
        let release = b.tick_at(&s, now + Duration::from_millis(225));
        assert!(released(&release));
        assert!((position(&release) - target).abs() < 1e-9);
        assert!(b.tick_at(&s, now + Duration::from_secs(1)).is_empty());
    }
}

#[test]
fn touched_wheel_accumulates_packets_without_fresh_audio_state_and_reverses() {
    let mut b = bridge();
    let s = state(true);
    let now = Instant::now();
    assert_eq!(position(&run(&mut b, 0x90, 0x4d, 127, &s, now)), 5.);
    assert!((position(&run(&mut b, 0xb0, 0x19, 2, &s, now)) - 5.006).abs() < 1e-9);
    assert!((position(&run(&mut b, 0xb0, 0x19, 3, &s, now)) - 5.015).abs() < 1e-9);
    assert!((position(&run(&mut b, 0xb0, 0x19, 127, &s, now)) - 5.012).abs() < 1e-9);
    for neutral in [0, 64] {
        assert!(run(&mut b, 0xb0, 0x19, neutral, &s, now).is_empty());
    }
    assert_eq!(
        b.tick_at(&s, now + Duration::from_secs(5)),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::HoldJog
        }]
    );
    assert!(b.tick_at(&s, now + Duration::from_millis(5001)).is_empty()); // Only one stationary hold.
    assert!(run(&mut b, 0x80, 0x4d, 64, &s, now + Duration::from_secs(5)).is_empty());
    let release = b.tick_at(&s, now + Duration::from_millis(5030));
    assert!(released(&release));
    assert_eq!(
        release,
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Scratch { speed: None }
        }]
    );
    assert!(run(&mut b, 0x90, 0x4d, 0, &s, now).is_empty());
}

#[test]
fn untouched_playing_rim_bends_without_editing_tempo_or_key_lock() {
    let mut b = bridge();
    let s = state(true);
    let now = Instant::now();
    for (raw, seconds) in [(1, 0.00025), (127, -0.00025)] {
        assert_eq!(
            run(&mut b, 0xb0, 0x19, raw, &s, now),
            vec![Action::Deck {
                deck: 0,
                action: DeckAction::PitchBend { seconds }
            }]
        );
    }
    assert_eq!(
        b.tick_at(&s, now + Duration::from_secs(1)),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::EndNudge
        }]
    );
}

#[test]
fn scratch_button_toggles_and_disabling_a_held_grab_releases_it() {
    let mut b = bridge();
    let s = state(true);
    let now = Instant::now();
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    assert!(released(&run(&mut b, 0x90, 0x48, 127, &s, now)));
    assert!(run(&mut b, 0x90, 0x48, 127, &s, now).is_empty()); // Debounced.
    assert!(matches!(
        run(&mut b, 0xb0, 0x19, 1, &s, now)[0],
        Action::Deck {
            action: DeckAction::PitchBend { .. },
            ..
        }
    ));
    run(&mut b, 0x90, 0x48, 0, &s, now);
    run(&mut b, 0x90, 0x48, 127, &s, now);
    assert!((position(&run(&mut b, 0xb0, 0x19, 1, &s, now)) - 5.003).abs() < 1e-9);
}

#[test]
fn paused_rim_scrubs_then_releases_after_idle_without_starting_playback() {
    let mut b = touchless_bridge();
    let s = state(false);
    let now = Instant::now();
    run(&mut b, 0x90, 0x48, 127, &s, now);
    run(&mut b, 0x90, 0x48, 0, &s, now);
    assert!((position(&run(&mut b, 0xb0, 0x19, 1, &s, now)) - 5.003).abs() < 1e-9);
    assert!(b.tick_at(&s, now + Duration::from_millis(79)).is_empty());
    let actions = b.tick_at(&s, now + Duration::from_millis(80));
    assert!(released(&actions));
    assert!((position(&actions) - 5.003).abs() < 1e-9);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Deck {
            action: DeckAction::Play,
            ..
        }
    )));
}

#[test]
fn paused_touch_sensing_rim_scratches_in_both_directions_with_scratch_on_or_off() {
    for enabled in [false, true] {
        for (raw, sign) in [(1, 1.), (127, -1.)] {
            let (mut engine, mut mixer) = Engine::headless(8000);
            let samples = (0..32000)
                .flat_map(|i| [(i as f32 * std::f32::consts::TAU * 440. / 8000.).sin() * 0.2; 2])
                .collect();
            engine
                .load_track(0, Arc::new(Track::from_samples("rim", 8000, samples)))
                .unwrap();
            apply(
                &mut engine,
                vec![Action::Deck {
                    deck: 0,
                    action: DeckAction::Seek { seconds: 2. },
                }],
            );
            let mut block = [0.; 160];
            mixer.render(&mut block);
            let mut b = bridge();
            let now = Instant::now();
            if !enabled {
                run(&mut b, 0x90, 0x48, 127, &engine.state(), now);
            }
            let mut audible = false;
            for packet in 0..20 {
                let s = engine.state();
                let actions = run(
                    &mut b,
                    0xb0,
                    0x19,
                    raw,
                    &s,
                    now + Duration::from_millis(packet * 10),
                );
                assert!(actions.iter().any(|action| matches!(
                    action,
                    Action::Deck {
                        action: DeckAction::JogScratch { .. },
                        ..
                    }
                )));
                apply(&mut engine, actions);
                mixer.render(&mut block);
                audible |= block.iter().any(|sample| sample.abs() > 0.01);
            }
            let s = engine.state();
            assert!(audible);
            assert!((s.decks[0].position_seconds - 2.) * sign > 0.03);
            assert!(!s.decks[0].playing);
            let stopped = s.decks[0].position_seconds;
            apply(&mut engine, b.tick_at(&s, now + Duration::from_millis(300)));
            for _ in 0..10 {
                mixer.render(&mut block);
            }
            assert!(!engine.state().decks[0].scratching);
            assert!((engine.state().decks[0].position_seconds - stopped).abs() < 0.0001);
            assert!(block.iter().all(|sample| sample.abs() < 0.00001));
        }
    }
}

#[test]
fn numark_scratch_mode_latches_on_press_and_release_does_not_change_it() {
    for playing in [false, true] {
        let mut b = touchless_bridge();
        let s = state(playing);
        let now = Instant::now();
        assert_eq!(
            b.jog_feedback("MixTrack Pro II"),
            vec![[0x90, 0x48, 0], [0x90, 0x50, 0]]
        );
        let off = run(&mut b, 0xb0, 0x19, 1, &s, now);
        if playing {
            assert!(matches!(
                off[0],
                Action::Deck {
                    action: DeckAction::PitchBend { .. },
                    ..
                }
            ));
        } else {
            assert!((position(&off) - 5.003).abs() < 1e-9);
            b.cancel_deck(0);
        }
        run(&mut b, 0x90, 0x48, 127, &s, now);
        run(&mut b, 0x90, 0x48, 127, &s, now); // Held/repeated down does not toggle again.
        run(&mut b, 0x90, 0x48, 0, &s, now);
        assert_eq!(b.jog_feedback("MixTrack Pro II")[0], [0x90, 0x48, 100]);
        assert!((position(&run(&mut b, 0xb0, 0x19, 1, &s, now)) - 5.003).abs() < 1e-9);
        // Neither touch notes nor playing state gate the latched mode.
        run(&mut b, 0x90, 0x4d, 127, &s, now);
        run(&mut b, 0x90, 0x4d, 0, &s, now);
        assert!(b.tick_at(&s, now + Duration::from_millis(79)).is_empty());
        assert!(released(&b.tick_at(&s, now + Duration::from_millis(80))));
        assert_eq!(b.jog_feedback("MixTrack Pro II")[0][2], 100); // Idle releases audio, not mode.
        let later = now + Duration::from_millis(90);
        run(&mut b, 0x90, 0x48, 127, &s, later);
        run(&mut b, 0x90, 0x48, 0, &s, later);
        assert_eq!(b.jog_feedback("MixTrack Pro II")[0][2], 0);
        let off = run(&mut b, 0xb0, 0x19, 1, &s, later);
        if playing {
            assert!(matches!(
                off[0],
                Action::Deck {
                    action: DeckAction::PitchBend { .. },
                    ..
                }
            ));
        } else {
            assert!(matches!(
                off[0],
                Action::Deck {
                    action: DeckAction::JogScratch { .. },
                    ..
                }
            ));
        }
    }
}

#[test]
fn bundled_numark_rim_bends_in_latched_scratch_mode_and_top_touch_scratches() {
    let mut b = Bridge::new(
        Profile::from_json(include_str!(
            "../../../profiles/numark-mixtrack-pro-ii.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let s = state(true);
    let now = Instant::now();
    for (deck, mode, touch, movement) in [(0, 0x48, 0x4d, 0x19), (1, 0x50, 0x4e, 0x18)] {
        run(&mut b, 0x90, mode, 127, &s, now);
        run(&mut b, 0x90, mode, 0, &s, now);
        let bend = vec![Action::Deck {
            deck,
            action: DeckAction::PitchBend { seconds: 0.00025 },
        }];
        assert_eq!(run(&mut b, 0xb0, movement, 1, &s, now), bend);
        run(&mut b, 0x90, touch, 127, &s, now);
        assert!((position(&run(&mut b, 0xb0, movement, 1, &s, now)) - 5.003).abs() < 1e-9);
        assert_eq!(
            b.tick_at(&s, now + Duration::from_secs(1)),
            vec![Action::Deck {
                deck,
                action: DeckAction::HoldJog
            }]
        );
        assert!(run(&mut b, 0x90, touch, 0, &s, now + Duration::from_secs(1)).is_empty());
        assert!(released(&b.tick_at(&s, now + Duration::from_millis(1030))));
        assert_eq!(
            run(
                &mut b,
                0xb0,
                movement,
                1,
                &s,
                now + Duration::from_millis(1031)
            ),
            bend
        );
        assert_eq!(b.jog_feedback("MixTrack Pro II")[usize::from(deck)][2], 100);
    }
}

#[test]
fn deck_and_port_isolation_and_boundaries_do_not_accumulate_hidden_distance() {
    let mut b = bridge();
    let mut s = state(true);
    let now = Instant::now();
    s.decks[0].position_seconds = 0.;
    assert!(
        b.process_at("Other", event(0x90, 0x4d, 127), &s, now)
            .is_empty()
    );
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    assert_eq!(position(&run(&mut b, 0xb0, 0x19, 65, &s, now)), 0.);
    assert_eq!(position(&run(&mut b, 0xb0, 0x19, 1, &s, now)), 0.003);
    s.decks[1].position_seconds = 10.;
    run(&mut b, 0x90, 0x4e, 127, &s, now);
    assert_eq!(position(&run(&mut b, 0xb0, 0x18, 63, &s, now)), 10.);
    assert!((position(&run(&mut b, 0xb0, 0x18, 127, &s, now)) - 9.997).abs() < 1e-9);
    assert_eq!(b.disconnect("MixTrack Pro II").len(), 2);
    assert!(b.tick_at(&s, now).is_empty());
}

#[test]
fn manual_takeover_or_track_replacement_releases_without_seeking_new_track() {
    let mut b = bridge();
    let mut s = state(true);
    let now = Instant::now();
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    let cancel = b.cancel_deck(0);
    assert_eq!(
        cancel,
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Scratch { speed: None }
        }]
    );
    assert!(run(&mut b, 0x90, 0x4d, 0, &s, now).is_empty());
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    s.decks[0].path = Some("different-track".into());
    assert_eq!(b.tick_at(&s, now), cancel);
    assert!(matches!(
        run(&mut b, 0xb0, 0x19, 1, &s, now)[0],
        Action::Deck {
            action: DeckAction::PitchBend { .. },
            ..
        }
    ));
}

#[test]
fn profile_changes_release_grabs_and_invalid_jog_configuration_is_rejected() {
    let mut b = bridge();
    let s = state(true);
    let now = Instant::now();
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    assert!(released(&b.replace(profile()).unwrap()));
    let mut p = profile();
    let mapping = p.mappings.iter_mut().find(|m| m.id == "jog-0").unwrap();
    let Binding::Jog { config } = &mut mapping.binding else {
        panic!()
    };
    config.encoding = Encoding::Absolute;
    assert!(p.validate().is_err());
}

#[test]
fn jog_direction_can_be_inverted_without_changing_other_controls() {
    let mut p = profile();
    let mapping = p.mappings.iter_mut().find(|m| m.id == "jog-0").unwrap();
    let Binding::Jog { config } = &mut mapping.binding else {
        panic!()
    };
    config.inverted = true;
    let mut b = Bridge::new(p).unwrap();
    let s = state(true);
    let now = Instant::now();
    assert_eq!(
        run(&mut b, 0xb0, 0x19, 1, &s, now),
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::PitchBend { seconds: -0.00025 }
        }]
    );
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    assert!((position(&run(&mut b, 0xb0, 0x19, 1, &s, now)) - 4.997).abs() < 1e-9);
}

#[test]
fn mapped_play_button_takes_over_and_a_later_touch_release_does_not_seek_back() {
    let mut b = bridge();
    let s = state(true);
    let now = Instant::now();
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    assert_eq!(
        run(&mut b, 0x90, 0x3b, 127, &s, now),
        vec![
            Action::Deck {
                deck: 0,
                action: DeckAction::Scratch { speed: None }
            },
            Action::Deck {
                deck: 0,
                action: DeckAction::Pause
            },
        ]
    );
    assert!(run(&mut b, 0x90, 0x4d, 0, &s, now).is_empty());
}

fn render(mixer: &mut Mixer, seconds: f64) -> Vec<f32> {
    let mut samples = vec![0.; (seconds * 48000.) as usize * 2];
    for block in samples.chunks_mut(480 * 2) {
        mixer.render(block);
    }
    samples
}
fn apply(engine: &mut Engine, actions: Vec<Action>) {
    for a in actions {
        engine.apply(Role::Host, a).unwrap();
    }
}

#[test]
fn hardware_jog_actions_are_audible_in_both_directions_and_resume_playing_or_paused() {
    for playing in [false, true] {
        for key_lock in [false, true] {
            let (mut engine, mut mixer) = Engine::headless(48000);
            let ramp: Vec<f32> = (0..48000 * 4)
                .flat_map(|i| [i as f32 / (48000. * 4.); 2])
                .collect();
            engine
                .load_track(0, Arc::new(Track::from_samples("jog", 48000, ramp)))
                .unwrap();
            for action in [
                DeckAction::Seek { seconds: 2. },
                DeckAction::SetKeyLock { on: key_lock },
                DeckAction::SetTempo { ratio: 1.08 },
            ] {
                apply(&mut engine, vec![Action::Deck { deck: 0, action }]);
            }
            if playing {
                apply(
                    &mut engine,
                    vec![Action::Deck {
                        deck: 0,
                        action: DeckAction::Play,
                    }],
                );
            }
            render(&mut mixer, 0.1);
            let mut b = bridge();
            let now = Instant::now();
            let s = engine.state();
            let anchor = s.decks[0].position_seconds;
            apply(&mut engine, run(&mut b, 0x90, 0x4d, 127, &s, now));
            render(&mut mixer, 0.1);
            assert!((engine.state().decks[0].position_seconds - anchor).abs() < 0.0001);
            for (direction, raw, offset) in [(-1., 124, 100), (1., 4, 300)] {
                let start = engine.state().decks[0].position_seconds;
                let mut audible = false;
                for packet in 0..20 {
                    let s = engine.state();
                    let at = now + Duration::from_millis(offset + packet * 10);
                    apply(&mut engine, run(&mut b, 0xb0, 0x19, raw, &s, at));
                    audible |= render(&mut mixer, 0.01).iter().any(|v| v.abs() > 0.01);
                }
                assert!(audible);
                let travel = engine.state().decks[0].position_seconds - start;
                assert!(
                    travel * direction > 0.18,
                    "direction={direction}, travel={travel}"
                );
            }
            let s = engine.state();
            apply(
                &mut engine,
                run(&mut b, 0x90, 0x4d, 0, &s, now + Duration::from_millis(500)),
            );
            apply(&mut engine, b.tick_at(&s, now + Duration::from_millis(580)));
            let release_position = s.decks[0].position_seconds;
            render(&mut mixer, 0.2);
            let local = engine.state().decks[0].clone();
            assert_eq!(local.playing, playing);
            assert_eq!(local.key_lock, key_lock);
            assert_eq!(local.tempo, 1.08);
            if playing {
                assert!(local.position_seconds > anchor + 0.15);
            } else {
                assert!((local.position_seconds - release_position).abs() < 0.0001);
            }
        }
    }
}

#[test]
fn a_complete_touch_burst_before_one_audio_callback_keeps_its_final_position() {
    let (mut engine, mut mixer) = Engine::headless(48000);
    engine
        .load_track(
            0,
            Arc::new(Track::from_samples("burst", 48000, vec![0.1; 48000 * 4])),
        )
        .unwrap();
    apply(
        &mut engine,
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::Seek { seconds: 1. },
        }],
    );
    render(&mut mixer, 0.01);
    let mut b = bridge();
    let s = engine.state();
    let now = Instant::now();
    for (status, number, value) in [
        (0x90, 0x4d, 127),
        (0xb0, 0x19, 10),
        (0xb0, 0x19, 10),
        (0x90, 0x4d, 0),
    ] {
        apply(&mut engine, run(&mut b, status, number, value, &s, now));
    }
    apply(&mut engine, b.tick_at(&s, now + Duration::from_millis(80)));
    render(&mut mixer, 0.1);
    assert!((engine.state().decks[0].position_seconds - 1.06).abs() < 0.0001);
    assert!(!engine.state().decks[0].playing);
}
#[test]
fn paused_platter_free_spin_keeps_scratch_surface_ownership_until_movement_stops() {
    let now = Instant::now();
    for playing in [false] {
        for (touch, movement) in [(0x4d, 0x19), (0x4e, 0x18)] {
            for (raw, sign) in [(1, 1.), (127, -1.)] {
                let mut b = bridge();
                let s = state(playing);
                let mut target = 5.;
                run(&mut b, 0x90, touch, 127, &s, now);
                let first = run(&mut b, 0xb0, movement, raw, &s, now);
                target += sign * 0.003;
                assert!((position(&first) - target).abs() < 1e-9);
                assert!(run(&mut b, 0x90, touch, 0, &s, now + Duration::from_millis(1)).is_empty());
                for packet in 1..=20 {
                    let at = now + Duration::from_millis(packet * 40);
                    assert!(b.tick_at(&s, at - Duration::from_millis(1)).is_empty());
                    let actions = run(&mut b, 0xb0, movement, raw, &s, at);
                    target += sign * 0.003;
                    assert!(!released(&actions));
                    assert!((position(&actions) - target).abs() < 1e-9);
                    assert!(!actions.iter().any(|a| matches!(
                        a,
                        Action::Deck {
                            action: DeckAction::PitchBend { .. },
                            ..
                        }
                    )));
                }
                assert!(b.tick_at(&s, now + Duration::from_millis(879)).is_empty());
                let end = b.tick_at(&s, now + Duration::from_millis(880));
                assert!(released(&end));
                assert!((position(&end) - target).abs() < 1e-9);
                assert!(b.tick_at(&s, now + Duration::from_millis(900)).is_empty());
                if playing {
                    let rim = run(
                        &mut b,
                        0xb0,
                        movement,
                        raw,
                        &s,
                        now + Duration::from_millis(910),
                    );
                    assert!(matches!(
                        rim.as_slice(),
                        [Action::Deck {
                            action: DeckAction::PitchBend { .. },
                            ..
                        }]
                    ));
                }
            }
        }
    }
}
#[test]
fn disabling_scratch_during_free_spin_clears_the_released_contact() {
    let mut b = bridge();
    let s = state(true);
    let now = Instant::now();
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    run(&mut b, 0xb0, 0x19, 127, &s, now);
    run(&mut b, 0x90, 0x4d, 0, &s, now);
    assert!(released(&run(
        &mut b,
        0x90,
        0x48,
        127,
        &s,
        now + Duration::from_millis(5)
    )));
    run(&mut b, 0x90, 0x48, 0, &s, now + Duration::from_millis(6));
    assert!(run(&mut b, 0x90, 0x48, 127, &s, now + Duration::from_millis(7)).is_empty());
    let rim = run(&mut b, 0xb0, 0x19, 1, &s, now + Duration::from_millis(8));
    assert!(matches!(
        rim.as_slice(),
        [Action::Deck {
            action: DeckAction::PitchBend { .. },
            ..
        }]
    ));
}
#[test]
fn paused_released_platter_spin_remains_audible_and_tracks_both_directions() {
    for playing in [false] {
        for key_lock in [false, true] {
            for (raw, sign) in [(1, 1.), (127, -1.)] {
                let (mut engine, mut mixer) = Engine::headless(48000);
                let samples = (0..48000 * 4)
                    .flat_map(|i| {
                        [(i as f32 * std::f32::consts::TAU * 440. / 48000.).sin() * 0.4; 2]
                    })
                    .collect();
                engine
                    .load_track(0, Arc::new(Track::from_samples("spin", 48000, samples)))
                    .unwrap();
                for action in [
                    DeckAction::Seek { seconds: 1. },
                    DeckAction::SetKeyLock { on: key_lock },
                    DeckAction::SetTempo { ratio: 1.08 },
                ] {
                    apply(&mut engine, vec![Action::Deck { deck: 0, action }]);
                }
                if playing {
                    apply(
                        &mut engine,
                        vec![Action::Deck {
                            deck: 0,
                            action: DeckAction::Play,
                        }],
                    );
                }
                render(&mut mixer, 0.1);
                let s = engine.state();
                let anchor = s.decks[0].position_seconds;
                let mut b = bridge();
                let now = Instant::now();
                apply(&mut engine, run(&mut b, 0x90, 0x4d, 127, &s, now));
                apply(&mut engine, run(&mut b, 0xb0, 0x19, raw, &s, now));
                apply(
                    &mut engine,
                    run(&mut b, 0x90, 0x4d, 0, &s, now + Duration::from_millis(1)),
                );
                for packet in 1..=12 {
                    let at = now + Duration::from_millis(packet * 40);
                    let s = engine.state();
                    let actions = run(&mut b, 0xb0, 0x19, raw, &s, at);
                    assert!(!released(&actions));
                    apply(&mut engine, actions);
                    let audio = render(&mut mixer, 0.04);
                    assert!(audio.iter().any(|v| v.abs() > 0.01));
                }
                let s = engine.state();
                assert!((s.decks[0].position_seconds - anchor) * sign > 0.03);
                assert_eq!(s.decks[0].tempo, 1.08);
                assert_eq!(s.decks[0].key_lock, key_lock);
                assert_eq!(s.decks[0].playing, playing);
                let release = b.tick_at(&s, now + Duration::from_millis(560));
                assert!(released(&release));
                apply(&mut engine, release);
                render(&mut mixer, 0.1);
                let end = engine.state();
                let audible = s.decks[0].position_seconds;
                if playing {
                    let advanced = end.decks[0].position_seconds - audible;
                    assert!(advanced > 0.08 && advanced < 0.13);
                } else {
                    assert!((end.decks[0].position_seconds - audible).abs() < 1e-4);
                }
            }
        }
    }
}
#[test]
fn grv6_dense_jog_packets_measure_travel_over_a_short_window() {
    let profile =
        Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json")).unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let s = state(false);
    let now = Instant::now();
    bridge.process_at("DDJ-GRV6", event(0x90, 54, 127), &s, now);
    let travel_per_count = 1.8 / 6200.;
    for (i, ms) in [0, 1, 1, 3, 4, 5, 5, 7, 8].into_iter().enumerate() {
        let actions = bridge.process_at(
            "DDJ-GRV6",
            event(0xb0, 34, 65),
            &s,
            now + Duration::from_millis(ms),
        );
        assert!((position(&actions) - (5. + (i + 1) as f64 * travel_per_count)).abs() < 1e-9);
        if ms >= 4 {
            assert!(matches!(&actions[..], [Action::Deck {
                action: DeckAction::JogScratch { speed, interval_seconds, .. }, ..
            }] if (*speed - travel_per_count / 0.001).abs() < 1e-9 && *interval_seconds == 0.004));
        }
    }
    let reversed = bridge.process_at(
        "DDJ-GRV6",
        event(0xb0, 34, 63),
        &s,
        now + Duration::from_millis(8),
    );
    assert!(matches!(&reversed[..], [Action::Deck {
        action: DeckAction::JogScratch { speed, .. }, ..
    }] if *speed < 0.));
    assert!((position(&reversed) - (5. + 8. * travel_per_count)).abs() < 1e-9);
}

#[test]
fn jog_speed_uses_packet_timing_and_slows_after_surface_release_even_when_replayed_in_a_batch() {
    let mut b = bridge();
    // Paused decks have no motor; all measured reverse speeds remain audible.
    let s = state(false);
    let now = Instant::now();
    run(&mut b, 0x90, 0x4d, 127, &s, now);
    run(&mut b, 0xb0, 0x19, 10, &s, now);
    // These packets can be drained together later; their recorded times, not
    // the UI's loop time, determine the platter velocity.
    for (ms, raw, expected) in [
        (20, 10, 1.5),
        (60, 5, 0.375),
        (100, 123, -0.375),
        (160, 127, -0.05),
    ] {
        if ms == 60 {
            run(&mut b, 0x90, 0x4d, 0, &s, now + Duration::from_millis(21));
        }
        let actions = run(&mut b, 0xb0, 0x19, raw, &s, now + Duration::from_millis(ms));
        assert!(
            matches!(actions.as_slice(),[Action::Deck{action:DeckAction::JogScratch{speed,..},..}]if(*speed-expected).abs()<1e-9)
        );
    }
}

#[test]
fn playing_motor_recovers_from_both_directions_and_ignores_released_inertia() {
    for key_lock in [false, true] {
        for raw in [1, 127] {
            let (mut engine, mut mixer) = Engine::headless(48000);
            engine
                .load_track(
                    0,
                    Arc::new(Track::from_samples("motor", 48000, vec![0.2; 48000 * 20])),
                )
                .unwrap();
            for action in [
                DeckAction::Seek { seconds: 5. },
                DeckAction::SetKeyLock { on: key_lock },
                DeckAction::SetTempo { ratio: 1.08 },
                DeckAction::Play,
            ] {
                apply(&mut engine, vec![Action::Deck { deck: 0, action }]);
            }
            render(&mut mixer, 0.1);
            let mut b = bridge();
            let now = Instant::now();
            let s = engine.state();
            apply(&mut engine, run(&mut b, 0x90, 0x4d, 127, &s, now));
            for packet in 0..10 {
                let s = engine.state();
                apply(
                    &mut engine,
                    run(
                        &mut b,
                        0xb0,
                        0x19,
                        raw,
                        &s,
                        now + Duration::from_millis(packet * 10),
                    ),
                );
                render(&mut mixer, 0.01);
            }
            let s = engine.state();
            apply(
                &mut engine,
                run(&mut b, 0x90, 0x4d, 0, &s, now + Duration::from_millis(100)),
            );
            assert!(!released(&b.tick_at(&s, now + Duration::from_millis(129))));
            render(&mut mixer, 0.03);
            let s = engine.state();
            let position_at_pickup = s.decks[0].position_seconds;
            let pickup = b.tick_at(&s, now + Duration::from_millis(130));
            assert!(released(&pickup));
            assert!(!pickup.iter().any(|action| matches!(
                action,
                Action::Deck {
                    action: DeckAction::Seek { .. },
                    ..
                }
            )));
            apply(&mut engine, pickup);
            for packet in 0..20 {
                let s = engine.state();
                let tail = run(
                    &mut b,
                    0xb0,
                    0x19,
                    raw,
                    &s,
                    now + Duration::from_millis(140 + packet * 10),
                );
                assert!(
                    tail.is_empty(),
                    "released inertia must not fight the motor: {tail:?}"
                );
                render(&mut mixer, 0.01);
            }
            let s = engine.state();
            let travel = s.decks[0].position_seconds - position_at_pickup;
            assert!(
                travel > 0.18 && travel < 0.24,
                "raw={raw}, key_lock={key_lock}, travel={travel}"
            );
            assert!(!s.decks[0].scratching);
            assert!(s.decks[0].playing);
            assert_eq!(s.decks[0].tempo, 1.08);
            assert_eq!(s.decks[0].key_lock, key_lock);
            // Touch immediately wins over motor pickup, even during inertia.
            let grabbed = run(
                &mut b,
                0x90,
                0x4d,
                127,
                &s,
                now + Duration::from_millis(345),
            );
            assert!(!grabbed.is_empty());
            apply(&mut engine, grabbed);
            let reverse = run(
                &mut b,
                0xb0,
                0x19,
                116,
                &s,
                now + Duration::from_millis(350),
            );
            assert!(reverse.iter().any(|action| matches!(action, Action::Deck { action: DeckAction::JogScratch { speed, .. }, .. } if *speed < 0.)));
            apply(&mut engine, reverse);
            render(&mut mixer, 0.01);
            assert!(engine.state().decks[0].scratching);
        }
    }
}

#[test]
fn fast_released_spins_play_out_then_motor_takes_over_as_the_wheel_slows() {
    for key_lock in [false, true] {
        for (fast, slow, sign) in [(20, 1, 1.), (108, 127, -1.)] {
            let (mut engine, mut mixer) = Engine::headless(48000);
            engine
                .load_track(
                    0,
                    Arc::new(Track::from_samples(
                        "spin motor",
                        48000,
                        vec![0.2; 48000 * 20],
                    )),
                )
                .unwrap();
            for action in [
                DeckAction::Seek { seconds: 5. },
                DeckAction::SetKeyLock { on: key_lock },
                DeckAction::SetTempo { ratio: 1.08 },
                DeckAction::Play,
            ] {
                apply(&mut engine, vec![Action::Deck { deck: 0, action }]);
            }
            render(&mut mixer, 0.1);
            let mut b = bridge();
            let now = Instant::now();
            let s = engine.state();
            apply(&mut engine, run(&mut b, 0x90, 0x4d, 127, &s, now));
            apply(&mut engine, run(&mut b, 0xb0, 0x19, fast, &s, now));
            render(&mut mixer, 0.01);
            let s = engine.state();
            apply(
                &mut engine,
                run(&mut b, 0x90, 0x4d, 0, &s, now + Duration::from_millis(1)),
            );
            let start = s.decks[0].position_seconds;
            for packet in 1..=8 {
                let s = engine.state();
                let actions = run(
                    &mut b,
                    0xb0,
                    0x19,
                    fast,
                    &s,
                    now + Duration::from_millis(packet * 40),
                );
                assert!(
                    !released(&actions),
                    "fast released spin must remain audible"
                );
                apply(&mut engine, actions);
                assert!(render(&mut mixer, 0.04).iter().any(|v| v.abs() > 0.01));
            }
            let s = engine.state();
            assert!((s.decks[0].position_seconds - start) * sign > 0.3);
            let pickup = run(
                &mut b,
                0xb0,
                0x19,
                slow,
                &s,
                now + Duration::from_millis(360),
            );
            assert!(released(&pickup), "slowing spin must yield to the motor");
            apply(&mut engine, pickup);
            let start = s.decks[0].position_seconds;
            render(&mut mixer, 0.1);
            let s = engine.state();
            assert!(!s.decks[0].scratching);
            assert!(s.decks[0].position_seconds > start + 0.08);
            // Retouching at slow speed still scratches; motor pickup is only
            // valid after a confirmed release.
            apply(
                &mut engine,
                run(
                    &mut b,
                    0x90,
                    0x4d,
                    127,
                    &s,
                    now + Duration::from_millis(460),
                ),
            );
            let held = run(
                &mut b,
                0xb0,
                0x19,
                slow,
                &s,
                now + Duration::from_millis(470),
            );
            assert!(!released(&held));
            assert!(held.iter().any(|a| matches!(
                a,
                Action::Deck {
                    action: DeckAction::JogScratch { .. },
                    ..
                }
            )));
        }
    }
}

#[test]
fn long_fast_scrub_then_stationary_held_platter_discards_unheard_travel() {
    const RATE: u32 = 8000;
    for (raw, sign) in [(20, 1.), (108, -1.)] {
        let (mut engine, mut mixer) = Engine::headless(RATE);
        engine
            .load_track(
                0,
                Arc::new(Track::from_samples(
                    "long jog",
                    RATE,
                    vec![0.3; RATE as usize * 120 * 2],
                )),
            )
            .unwrap();
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: 0,
                    action: DeckAction::Seek { seconds: 50. },
                },
            )
            .unwrap();
        let mut block = [0.; 80]; // Five milliseconds.
        mixer.render(&mut block);
        let mut bridge = bridge();
        let now = Instant::now();
        let apply = |engine: &mut Engine, actions: Vec<Action>| {
            for a in actions {
                engine.apply(Role::Host, a).unwrap();
            }
        };
        let s = engine.state();
        apply(&mut engine, run(&mut bridge, 0x90, 0x4d, 127, &s, now));
        for packet in 0..300 {
            let s = engine.state();
            apply(
                &mut engine,
                run(
                    &mut bridge,
                    0xb0,
                    0x19,
                    raw,
                    &s,
                    now + Duration::from_millis(packet * 5),
                ),
            );
            mixer.render(&mut block);
        }
        // Continue callbacks after packets stop, keeping contact down throughout.
        for _ in 0..16 {
            mixer.render(&mut block);
        }
        let s = engine.state();
        let stop = bridge.tick_at(&s, now + Duration::from_millis(1575));
        assert!(!released(&stop), "stationary contact must remain grabbed");
        apply(&mut engine, stop);
        mixer.render(&mut block);
        let held = engine.state().decks[0].position_seconds;
        for _ in 0..100 {
            mixer.render(&mut block);
        }
        let after = engine.state().decks[0].position_seconds;
        assert!(
            (after - held).abs() < 1. / f64::from(RATE),
            "held stop kept chasing old travel: sign={sign}, held={held}, after={after}"
        );
        let s = engine.state();
        assert!(bridge.tick_at(&s, now + Duration::from_secs(3)).is_empty());
        let movement = run(
            &mut bridge,
            0xb0,
            0x19,
            if sign > 0. { 1 } else { 127 },
            &s,
            now + Duration::from_secs(3),
        );
        assert!(
            (position(&movement) - (after + sign * 0.003)).abs() < 1e-9,
            "new motion reused the stale target"
        );
    }
}

#[test]
fn released_touch_after_long_scrub_never_seeks_to_unheard_travel() {
    const RATE: u32 = 8000;
    for playing in [false, true] {
        for (raw, sign) in [(20, 1.), (108, -1.)] {
            let (mut engine, mut mixer) = Engine::headless(RATE);
            engine
                .load_track(
                    0,
                    Arc::new(Track::from_samples(
                        "release backlog",
                        RATE,
                        vec![0.3; RATE as usize * 120 * 2],
                    )),
                )
                .unwrap();
            let apply = |engine: &mut Engine, actions: Vec<Action>| {
                for a in actions {
                    engine.apply(Role::Host, a).unwrap();
                }
            };
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: 0,
                        action: DeckAction::Seek { seconds: 50. },
                    },
                )
                .unwrap();
            if playing {
                engine
                    .apply(
                        Role::Host,
                        Action::Deck {
                            deck: 0,
                            action: DeckAction::Play,
                        },
                    )
                    .unwrap();
            }
            let mut out = [0.; 80];
            mixer.render(&mut out);
            let mut b = bridge();
            let now = Instant::now();
            let s = engine.state();
            apply(&mut engine, run(&mut b, 0x90, 0x4d, 127, &s, now));
            for packet in 0..300 {
                let s = engine.state();
                apply(
                    &mut engine,
                    run(
                        &mut b,
                        0xb0,
                        0x19,
                        raw,
                        &s,
                        now + Duration::from_millis(packet * 5),
                    ),
                );
                mixer.render(&mut out);
            }
            let s = engine.state();
            apply(
                &mut engine,
                run(&mut b, 0x90, 0x4d, 0, &s, now + Duration::from_millis(1496)),
            );
            for _ in 0..16 {
                mixer.render(&mut out);
            }
            let before = engine.state().decks[0].position_seconds;
            let s = engine.state();
            assert!(s.decks[0].scratching);
            let release = b.tick_at(&s, now + Duration::from_millis(1575));
            assert!(released(&release));
            assert!(
                !release.iter().any(|a| matches!(
                    a,
                    Action::Deck {
                        action: DeckAction::Seek { .. },
                        ..
                    }
                )),
                "audible scratch release must not seek"
            );
            apply(&mut engine, release);
            for _ in 0..20 {
                mixer.render(&mut out);
            }
            let after = engine.state().decks[0].position_seconds;
            assert!(
                (after - before).abs() < 0.15,
                "release jumped to stale target: playing={playing}, sign={sign}, before={before}, after={after}"
            );
            assert_eq!(engine.state().decks[0].playing, playing);
        }
    }
}
