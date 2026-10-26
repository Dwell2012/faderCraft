use djcraft_engine::{Engine, Track};
use djcraft_protocol::{Action, DeckAction, MixerAction, MonitorState, Rejection, Role};
use std::sync::Arc;

fn deck(engine: &mut Engine, index: u8, action: DeckAction) {
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: index,
                action,
            },
        )
        .unwrap();
}
fn monitor(engine: &mut Engine, value: MonitorState) -> Result<(), Rejection> {
    engine.apply(
        Role::Host,
        Action::Mixer {
            action: MixerAction::SetMonitor { monitor: value },
        },
    )
}

#[test]
fn independent_cue_toggles_sum_only_selected_decks_and_allow_all_off() {
    let (mut engine, mut mixer) = Engine::headless(48000);
    let levels = [0.05, 0.1, 0.15, 0.2];
    for (index, level) in levels.into_iter().enumerate() {
        engine
            .load_track(
                index,
                Arc::new(Track::from_samples("cue", 48000, vec![level; 48000])),
            )
            .unwrap();
        deck(&mut engine, index as u8, DeckAction::Play);
    }
    let mut master = [0.; 2048];
    let mut cue = [0.; 2048];
    let gains = djcraft_engine::crossfader_gains(0.5, Default::default());
    let master_expected = (levels[0] + levels[2]) * gains[0] + (levels[1] + levels[3]) * gains[1];
    for mask in [0, 1, 5, 13, 0, 10, 15, 2, 0] {
        monitor(
            &mut engine,
            MonitorState {
                cue_decks: Some(mask),
                gain: 1.,
                ..Default::default()
            },
        )
        .unwrap();
        mixer.render_with_monitor(&mut master, Some(&mut cue));
        mixer.render_with_monitor(&mut master, Some(&mut cue));
        let expected: f32 = levels
            .iter()
            .enumerate()
            .filter(|(deck, _)| mask & (1 << deck) != 0)
            .map(|(_, level)| level)
            .sum();
        assert!(
            cue[1024..]
                .iter()
                .all(|sample| (*sample - expected).abs() < 1e-4),
            "cue mask {mask}: {} vs {expected}",
            cue[2046]
        );
        assert!((master[2046] - master_expected).abs() < 1e-4);
        assert!(cue.windows(2).all(|s| (s[1] - s[0]).abs() < 0.01));
    }
    let before = engine.state().monitor;
    assert_eq!(
        monitor(
            &mut engine,
            MonitorState {
                cue_decks: Some(0b10000),
                ..before
            }
        ),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(engine.state().monitor, before);
}

#[test]
fn cue_is_independent_of_faders_and_switches_decks_without_advancing_twice() {
    let (mut engine, mut mixer) = Engine::headless(48000);
    for (index, level) in [(0, 0.2), (1, 0.6)] {
        engine
            .load_track(
                index,
                Arc::new(Track::from_samples("cue", 48000, vec![level; 48000])),
            )
            .unwrap();
        deck(
            &mut engine,
            index as u8,
            DeckAction::SetVolume { level: 0. },
        );
        deck(&mut engine, index as u8, DeckAction::Play);
    }
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetMasterGain { level: 0. },
            },
        )
        .unwrap();
    let mut master = [0.; 2048];
    let mut cue = [0.; 2048];
    mixer.render_with_monitor(&mut master, Some(&mut cue));
    mixer.render_with_monitor(&mut master, Some(&mut cue));
    assert!(master.iter().all(|x| x.abs() < 1e-6));
    assert!((cue[2046] - 0.1).abs() < 1e-4);
    let before_switch = engine.state().decks[0].position_seconds;
    monitor(
        &mut engine,
        MonitorState {
            deck: 1,
            ..Default::default()
        },
    )
    .unwrap();
    mixer.render_with_monitor(&mut master, Some(&mut cue));
    assert!((cue[2046] - 0.3).abs() < 1e-4);
    assert!(cue.chunks_exact(2).all(|f| (f[0] - f[1]).abs() < 1e-6));
    let position = engine.state().decks[0].position_seconds;
    // Initial play uses a speed ramp; measure a settled block, not its startup offset.
    assert!(
        (position - before_switch - 1024. / 48000.).abs() < 1e-4,
        "position {position}"
    );
    monitor(
        &mut engine,
        MonitorState {
            mix: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    mixer.render_with_monitor(&mut master, Some(&mut cue));
    assert!(cue[1024..].iter().all(|x| x.abs() < 1e-6));
}

#[test]
fn headphone_blend_and_level_leave_master_unchanged() {
    let (mut engine, mut mixer) = Engine::headless(48000);
    for (index, level) in [(0, 0.2), (1, 0.6)] {
        engine
            .load_track(
                index,
                Arc::new(Track::from_samples("tone", 48000, vec![level; 48000])),
            )
            .unwrap();
        deck(&mut engine, index as u8, DeckAction::Play);
    }
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetCrossfader { position: 1. },
            },
        )
        .unwrap();
    let mut master = [0.; 2048];
    let mut cue = [0.; 2048];
    for (mix, gain, expected) in [
        (0., 1., 0.2),
        (0.25, 1., 0.5),
        (0.5, 1., 0.8),
        (0.75, 1., 0.7),
        (1., 1., 0.6),
        (1., 0.25, 0.15),
        (0., 0., 0.),
    ] {
        monitor(
            &mut engine,
            MonitorState {
                deck: 0,
                mix,
                gain,
                ..Default::default()
            },
        )
        .unwrap();
        mixer.render_with_monitor(&mut master, Some(&mut cue));
        assert!(
            (cue[2046] - expected).abs() < 1e-4,
            "mix {mix}, gain {gain}: {}",
            cue[2046]
        );
        assert!((master[2046] - 0.6).abs() < 1e-4);
        assert!(cue.windows(2).all(|pair| (pair[1] - pair[0]).abs() < 0.01));
    }
}

#[test]
fn sampler_bus_volume_scales_all_samples_without_muting_decks() {
    let (mut engine, mut mixer) = Engine::headless(48000);
    engine
        .load_track(
            0,
            Arc::new(Track::from_samples("sample", 48000, vec![0.2; 48000])),
        )
        .unwrap();
    deck(
        &mut engine,
        0,
        DeckAction::CaptureSample {
            slot: 0,
            start: 0.1,
            end: 0.3,
        },
    );
    deck(
        &mut engine,
        0,
        DeckAction::Sample {
            slot: 0,
            play: true,
            restart: true,
            repeat: true,
            level: 1.,
        },
    );
    let mut out = [0.; 2048];
    for (level, expected) in [(1., 0.2), (0.25, 0.05), (0., 0.)] {
        engine
            .apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::SetSamplerGain { level },
                },
            )
            .unwrap();
        mixer.render(&mut out);
        assert!((out[2046] - expected).abs() < 1e-4);
        assert!(out.windows(2).all(|p| (p[1] - p[0]).abs() < 0.01));
    }
    deck(&mut engine, 0, DeckAction::Play);
    mixer.render(&mut out);
    assert!((out[2046] - 0.2).abs() < 1e-4);
    for level in [f32::NAN, -1., 2.] {
        assert_eq!(
            engine.apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::SetSamplerGain { level }
                }
            ),
            Err(Rejection::InvalidValue)
        );
    }
}

#[test]
fn invalid_monitor_values_and_remote_mixer_control_are_rejected() {
    let (mut engine, _) = Engine::headless(48000);
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert_eq!(
            monitor(
                &mut engine,
                MonitorState {
                    mix: value,
                    ..Default::default()
                }
            ),
            Err(Rejection::InvalidValue)
        );
        assert_eq!(
            monitor(
                &mut engine,
                MonitorState {
                    gain: value,
                    ..Default::default()
                }
            ),
            Err(Rejection::InvalidValue)
        );
    }
    assert_eq!(
        monitor(
            &mut engine,
            MonitorState {
                deck: djcraft_protocol::DECKS as u8,
                ..Default::default()
            }
        ),
        Err(Rejection::UnknownDeck)
    );
    assert_eq!(
        engine.apply(
            Role::Viewer,
            Action::Mixer {
                action: MixerAction::SetMonitor {
                    monitor: Default::default()
                }
            }
        ),
        Err(Rejection::NotPermitted)
    );
}
