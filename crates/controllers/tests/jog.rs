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
                action: DeckAction::ScratchTo { seconds } | DeckAction::Seek { seconds },
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
    assert!(b.tick_at(&s, now + Duration::from_secs(5)).is_empty()); // Stationary hand holds.
    assert!(run(&mut b, 0x80, 0x4d, 64, &s, now + Duration::from_secs(5)).is_empty());
    let release = b.tick_at(&s, now + Duration::from_millis(5030));
    assert!(released(&release));
    assert!((position(&release) - 5.012).abs() < 1e-9);
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
                    action: DeckAction::ScratchTo { .. },
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
        assert!(b.tick_at(&s, now + Duration::from_secs(1)).is_empty());
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
            let s = engine.state();
            apply(&mut engine, run(&mut b, 0xb0, 0x19, 65, &s, now));
            let reverse = render(&mut mixer, 0.2);
            assert!(reverse.iter().any(|v| v.abs() > 0.01));
            assert!(engine.state().decks[0].position_seconds < anchor - 0.18);
            let s = engine.state();
            apply(&mut engine, run(&mut b, 0xb0, 0x19, 63, &s, now));
            let forward = render(&mut mixer, 0.2);
            assert!(forward.iter().any(|v| v.abs() > 0.01));
            let s = engine.state();
            apply(&mut engine, run(&mut b, 0x90, 0x4d, 0, &s, now));
            apply(&mut engine, b.tick_at(&s, now + Duration::from_millis(30)));
            render(&mut mixer, 0.2);
            let local = engine.state().decks[0].clone();
            assert_eq!(local.playing, playing);
            assert_eq!(local.key_lock, key_lock);
            assert_eq!(local.tempo, 1.08);
            if playing {
                assert!(local.position_seconds > anchor + 0.15);
            } else {
                assert!((local.position_seconds - anchor).abs() < 0.0001);
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
    apply(&mut engine, b.tick_at(&s, now + Duration::from_millis(30)));
    render(&mut mixer, 0.1);
    assert!((engine.state().decks[0].position_seconds - 1.06).abs() < 0.0001);
    assert!(!engine.state().decks[0].playing);
}
