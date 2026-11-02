use std::f32::consts::TAU;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use djcraft_engine::{Engine, Mixer, Track};
use djcraft_protocol::{Action, DeckAction, MixerAction, Rejection, Role};

const RATE: u32 = 48_000;
/// Frames per simulated audio callback.
const CALLBACK: usize = 480;

#[test]
fn slow_continuous_jog_packets_do_not_modulate_volume() {
    let (mut engine, mut mixer) = engine_with(vec![0.4; RATE as usize * 4]);
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 0.5 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::ScratchTo { seconds: 0.5 }))
        .unwrap();
    let mut target = 0.5;
    let mut block = vec![0.; CALLBACK * 2];
    let mut minimum = 1f32;
    for packet in 0..30 {
        target += 0.003;
        engine
            .apply(Role::Host, deck(DeckAction::ScratchTo { seconds: target }))
            .unwrap();
        // A slowly rotated wheel sends one count every 40 ms.
        for _ in 0..4 {
            mixer.render(&mut block);
            if packet > 2 {
                minimum = block.iter().fold(minimum, |a, v| a.min(v.abs()));
            }
        }
    }
    assert!(
        minimum > 0.38,
        "steady motion pumped the gain down to {minimum}"
    );
    for _ in 0..30 {
        mixer.render(&mut block);
    }
    assert!(block.iter().all(|v| v.abs() < 0.001));
}

#[test]
fn stepped_jog_targets_fade_smoothly_to_silence_between_packets() {
    // Constant input exposes discontinuities independently of waveform frequency.
    let (mut engine, mut mixer) = engine_with(vec![0.4; RATE as usize * 4]);
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 0.5 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::ScratchTo { seconds: 0.5 }))
        .unwrap();
    let mut block = vec![0.; CALLBACK * 2];
    let mut previous = 0f32;
    let mut largest_jump = 0f32;
    let mut peak = 0f32;
    for target in [0.503, 0.506, 0.503, 0.500] {
        engine
            .apply(Role::Host, deck(DeckAction::ScratchTo { seconds: target }))
            .unwrap();
        for _ in 0..25 {
            mixer.render(&mut block);
            for frame in block.chunks_exact(2) {
                peak = peak.max(frame[0].abs());
                largest_jump = largest_jump.max((frame[0] - previous).abs());
                previous = frame[0];
            }
        }
        assert!(
            previous.abs() < 0.001,
            "stationary scratch must be silent: {previous}"
        );
        assert!((engine.state().decks[0].position_seconds - target).abs() < 0.0001);
    }
    assert!(peak > 0.05, "movement must remain audible");
    assert!(largest_jump < 0.01, "MIDI-step click: {largest_jump}");
}

fn sine(frequency: f32, seconds: f32, rate: u32) -> Vec<f32> {
    (0..(seconds * rate as f32) as usize)
        .flat_map(|i| {
            let sample = 0.5 * (TAU * frequency * i as f32 / rate as f32).sin();
            [sample, sample]
        })
        .collect()
}

/// A track whose sample value is its own position, from 0.0 to 1.0.
fn ramp(seconds: f32) -> Vec<f32> {
    let frames = (seconds * RATE as f32) as usize;
    (0..frames)
        .flat_map(|i| [i as f32 / frames as f32; 2])
        .collect()
}

fn deck(action: DeckAction) -> Action {
    Action::Deck { deck: 0, action }
}

fn engine_with(samples: Vec<f32>) -> (Engine, Mixer) {
    let (mut engine, mixer) = Engine::headless(RATE);
    engine
        .load_track(0, Arc::new(Track::from_samples("test", RATE, samples)))
        .unwrap();
    (engine, mixer)
}

#[test]
fn stems_preserve_unity_and_remove_only_the_requested_source() {
    use djcraft_protocol::EqMode;
    let count = RATE as usize * 4;
    let track = Arc::new(Track::from_samples("stems", RATE, vec![0.4; count]));
    track
        .attach_stems(std::array::from_fn(|_| vec![0.1; count].into_boxed_slice()))
        .unwrap();
    let (mut engine, mut mixer) = Engine::headless(RATE);
    engine.load_track(0, track).unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEqMode {
                mode: EqMode::Stems,
            }),
        )
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let unity = render(&mut mixer, 0.2);
    assert!((unity.last().unwrap() - 0.4).abs() < 1e-5);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetStemGains {
                gains: [0., 1., 1., 1.],
            }),
        )
        .unwrap();
    let no_vocals = render(&mut mixer, 0.2);
    assert!((no_vocals.last().unwrap() - 0.3).abs() < 1e-5);
    assert!(no_vocals.windows(2).all(|p| (p[1] - p[0]).abs() < 0.002));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetStemGains { gains: [0.; 4] }),
        )
        .unwrap();
    assert!(render(&mut mixer, 0.2).last().unwrap().abs() < 1e-5);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEqMode {
                mode: EqMode::Classic,
            }),
        )
        .unwrap();
    assert!((render(&mut mixer, 0.2).last().unwrap() - 0.4).abs() < 1e-5);
}

#[test]
fn stems_reject_missing_invalid_or_misaligned_sources() {
    use djcraft_protocol::EqMode;
    let (mut engine, _) = engine_with(vec![0.; 100]);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEqMode {
                mode: EqMode::Stems,
            }),
        )
        .unwrap();
    assert!(
        engine
            .apply(
                Role::Host,
                deck(DeckAction::SetStemGains { gains: [0.; 4] })
            )
            .is_err()
    );
    let track = engine.track(0).unwrap();
    assert!(
        track
            .attach_stems(std::array::from_fn(|_| vec![0.; 98].into_boxed_slice()))
            .is_err()
    );
    assert!(
        track
            .attach_stems(std::array::from_fn(
                |_| vec![f32::NAN; 100].into_boxed_slice()
            ))
            .is_err()
    );
    track
        .attach_stems(std::array::from_fn(|_| vec![0.; 100].into_boxed_slice()))
        .unwrap();
    assert!(
        engine
            .apply(
                Role::Host,
                deck(DeckAction::SetStemGains {
                    gains: [f32::NAN; 4]
                })
            )
            .is_err()
    );
    assert!(
        engine
            .apply(Role::Host, deck(DeckAction::SetEq { band: 0, gain: 0. }))
            .is_err()
    );
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetStemGains { gains: [0.; 4] }),
        )
        .unwrap();
    engine
        .load_track(
            0,
            Arc::new(Track::from_samples("next", RATE, vec![0.; 100])),
        )
        .unwrap();
    assert_eq!(engine.state().decks[0].stem_gains, [1.; 4]);
}

#[test]
fn live_load_warning_follows_the_complete_master_signal_route() {
    use djcraft_protocol::{CrossfaderCurve, EqMode};
    // Silent samples deliberately exercise protection during a musical break.
    let (mut engine, mut mixer) = engine_with(vec![0.; RATE as usize * 20]);
    assert!(!engine.deck_has_open_output(0));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.01);
    assert!(engine.deck_has_open_output(0));
    for curve in [
        CrossfaderCurve::Smooth,
        CrossfaderCurve::EqualPower,
        CrossfaderCurve::Cut,
    ] {
        engine
            .apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::SetCrossfaderCurve { curve },
                },
            )
            .unwrap();
        engine
            .apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::SetCrossfader { position: 1. },
                },
            )
            .unwrap();
        assert!(!engine.deck_has_open_output(0));
        engine
            .apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::SetCrossfader { position: 0.5 },
                },
            )
            .unwrap();
        assert!(engine.deck_has_open_output(0));
    }
    engine
        .apply(Role::Host, deck(DeckAction::SetVolume { level: 0. }))
        .unwrap();
    assert!(!engine.deck_has_open_output(0));
    engine
        .apply(Role::Host, deck(DeckAction::SetVolume { level: 1. }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetMasterGain { level: 0. },
            },
        )
        .unwrap();
    assert!(!engine.deck_has_open_output(0));
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetMasterGain { level: 1. },
            },
        )
        .unwrap();
    for band in 0..3 {
        engine
            .apply(Role::Host, deck(DeckAction::SetEq { band, gain: 0. }))
            .unwrap();
    }
    assert!(!engine.deck_has_open_output(0));
    engine
        .apply(Role::Host, deck(DeckAction::SetEq { band: 1, gain: 0.1 }))
        .unwrap();
    assert!(engine.deck_has_open_output(0));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEqMode {
                mode: EqMode::Stems,
            }),
        )
        .unwrap();
    assert!(engine.deck_has_open_output(0)); // Original plays before preparation.
    let track = engine.track(0).unwrap();
    track
        .attach_stems(std::array::from_fn(|_| {
            vec![0.; track.decoded_frames() * 2].into_boxed_slice()
        }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetStemGains { gains: [0.; 4] }),
        )
        .unwrap();
    assert!(!engine.deck_has_open_output(0));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetStemGains {
                gains: [0., 0., 0., 1.],
            }),
        )
        .unwrap();
    assert!(engine.deck_has_open_output(0));
    engine.apply(Role::Host, deck(DeckAction::Pause)).unwrap();
    render(&mut mixer, 0.01);
    assert!(!engine.deck_has_open_output(0));
    assert!(!engine.deck_has_open_output(1));
    assert!(!engine.deck_has_open_output(99));
}

#[test]
fn micro_loops_wrap_at_eighth_and_sixteenth_beats() {
    for beats in [0.125, 0.0625] {
        let (mut engine, mut mixer) = engine_with(ramp(2.));
        let start = 0.5;
        let end = start + beats * 60. / 140.;
        engine
            .apply(Role::Host, deck(DeckAction::SetLoop { start, end }))
            .unwrap();
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        let audio = render(&mut mixer, 0.5);
        assert!(
            audio[32..]
                .iter()
                .all(|sample| *sample >= 0.249 && *sample <= end as f32 / 2. + 0.001)
        );
        let state = engine.state().decks[0].clone();
        assert!(state.playing && (start..end).contains(&state.position_seconds));
        engine
            .apply(Role::Host, deck(DeckAction::ClearLoop))
            .unwrap();
        render(&mut mixer, 0.1);
        assert!(engine.state().decks[0].position_seconds > end);
    }
}

#[test]
fn loops_wrap_audio_and_exit_without_stopping() {
    let (mut engine, mut mixer) = engine_with(ramp(2.));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetLoop {
                start: 0.2,
                end: 0.4,
            }),
        )
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let audio = render(&mut mixer, 0.9);
    // The first 32 frames have a sub-millisecond anti-click gain envelope.
    assert!(audio[32..].iter().all(|v| *v >= 0.099 && *v <= 0.201));
    let state = engine.state().decks[0].clone();
    assert!(state.playing && (0.2..0.4).contains(&state.position_seconds));
    engine
        .apply(Role::Host, deck(DeckAction::ClearLoop))
        .unwrap();
    render(&mut mixer, 0.4);
    assert!(engine.state().decks[0].position_seconds > 0.4);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetLoop {
                start: 0.2,
                end: 0.4,
            }),
        )
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 1. }))
        .unwrap();
    render(&mut mixer, 0.1);
    assert!(engine.state().decks[0].loop_region.is_none());
    assert!(engine.state().decks[0].position_seconds > 1.);
}

#[test]
fn loops_work_with_key_lock_and_reject_invalid_bounds() {
    let (mut engine, mut mixer) = engine_with(sine(440., 2., RATE));
    for (start, end) in [
        (f64::NAN, 1.),
        (0., f64::INFINITY),
        (-1., 1.),
        (1., 0.5),
        (0., 0.001),
        (0., 3.),
    ] {
        assert!(
            engine
                .apply(Role::Host, deck(DeckAction::SetLoop { start, end }))
                .is_err()
        );
    }
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetLoop {
                start: 0.2,
                end: 0.7,
            }),
        )
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.2 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetKeyLock { on: true }))
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let audio = render(&mut mixer, 1.5);
    assert!(audio.iter().all(|v| v.is_finite()));
    assert!((frequency(&audio[RATE as usize / 2..]) - 440.).abs() < 8.);
    let state = engine.state().decks[0].clone();
    assert!(state.playing && (0.2..0.7).contains(&state.position_seconds));
    engine
        .load_track(0, Arc::new(Track::from_samples("new", RATE, vec![0.; 100])))
        .unwrap();
    assert!(engine.state().decks[0].loop_region.is_none());
}

/// Renders `seconds` of audio and returns the left channel.
fn render(mixer: &mut Mixer, seconds: f32) -> Vec<f32> {
    let mut left = Vec::new();
    let mut buffer = vec![0f32; CALLBACK * 2];
    for _ in 0..(seconds * RATE as f32) as usize / CALLBACK {
        mixer.render(&mut buffer);
        left.extend(buffer.chunks(2).map(|frame| frame[0]));
    }
    left
}

fn frequency(samples: &[f32]) -> f32 {
    let crossings: Vec<f32> = samples
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| pair[0] <= 0.0 && pair[1] > 0.0)
        .map(|(i, pair)| i as f32 - pair[0] / (pair[1] - pair[0]))
        .collect();
    if crossings.len() < 2 {
        return 0.;
    }
    (crossings.len() - 1) as f32 * RATE as f32 / (crossings.last().unwrap() - crossings[0])
}

fn position(engine: &mut Engine) -> f64 {
    engine.state().decks[0].position_seconds
}

#[test]
fn plays_forward_at_original_speed() {
    let (mut engine, mut mixer) = engine_with(sine(440.0, 3.0, RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let out = render(&mut mixer, 1.0);

    assert!((position(&mut engine) - 1.0).abs() < 0.01);
    assert!((frequency(&out[RATE as usize / 2..]) - 440.0).abs() < 4.0);
    assert!(engine.state().decks[0].playing);
}

#[test]
fn tempo_without_key_lock_shifts_pitch() {
    let (mut engine, mut mixer) = engine_with(sine(440.0, 4.0, RATE));
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.25 }))
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let out = render(&mut mixer, 2.0);

    assert!((position(&mut engine) - 2.5).abs() < 0.02);
    assert!((frequency(&out[RATE as usize..]) - 550.0).abs() < 5.0);
}

#[test]
fn key_lock_changes_tempo_but_keeps_pitch() {
    let (mut engine, mut mixer) = engine_with(sine(440.0, 4.0, RATE));
    for action in [
        DeckAction::SetTempo { ratio: 1.25 },
        DeckAction::SetKeyLock { on: true },
        DeckAction::Play,
    ] {
        engine.apply(Role::Host, deck(action)).unwrap();
    }
    let out = render(&mut mixer, 2.0);

    // The reported position trails the read position by the stretcher's
    // latency, so allow for that.
    let position = position(&mut engine);
    assert!((2.2..=2.5).contains(&position), "position {position}");
    let pitch = frequency(&out[RATE as usize..]);
    assert!((pitch - 440.0).abs() < 5.0, "pitch {pitch}");
    let peak = out[RATE as usize..]
        .iter()
        .fold(0f32, |p, s| p.max(s.abs()));
    assert!((0.4..=0.6).contains(&peak), "peak {peak}");
}

#[test]
fn key_shift_transposes_without_changing_transport_and_resets() {
    for (lock, ratio, shift) in [(true, 1., 12.), (true, 1.25, -12.), (false, 1.25, 7.)] {
        let (mut engine, mut mixer) = engine_with(sine(440., 8., RATE));
        for action in [
            DeckAction::SetKeyLock { on: lock },
            DeckAction::SetTempo { ratio },
            DeckAction::SetKeyShift { semitones: shift },
            DeckAction::Play,
        ] {
            engine.apply(Role::Host, deck(action)).unwrap();
        }
        let out = render(&mut mixer, 2.);
        let expected = 440. * 2f32.powf(shift / 12.) * if lock { 1. } else { ratio as f32 };
        let pitch = frequency(&out[RATE as usize..]);
        assert!(
            (pitch - expected).abs() < 8.,
            "pitch {pitch}, expected {expected}"
        );
        assert!((position(&mut engine) - 2. * ratio).abs() < 0.03);
        assert_eq!(engine.state().decks[0].key_shift, shift);
        engine
            .apply(Role::Host, deck(DeckAction::SetKeyShift { semitones: 0. }))
            .unwrap();
        let out = render(&mut mixer, 1.);
        let expected = 440. * if lock { 1. } else { ratio as f32 };
        assert!((frequency(&out[RATE as usize / 2..]) - expected).abs() < 8.);
    }
}

#[test]
fn key_shift_rejects_invalid_values() {
    let (mut engine, _) = Engine::headless(RATE);
    for semitones in [f32::NAN, f32::INFINITY, -12.1, 12.1] {
        assert_eq!(
            engine.apply(Role::Host, deck(DeckAction::SetKeyShift { semitones })),
            Err(Rejection::InvalidValue)
        );
    }
}

#[test]
fn changing_key_shift_mid_playback_preserves_the_audible_position() {
    let (mut engine, mut mixer) = engine_with(sine(440., 8., RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 1.);
    for shift in [7., -5., 0.] {
        let before = position(&mut engine);
        engine
            .apply(
                Role::Host,
                deck(DeckAction::SetKeyShift { semitones: shift }),
            )
            .unwrap();
        let audio = render(&mut mixer, 0.5);
        assert!((position(&mut engine) - before - 0.5).abs() < 0.003);
        let expected = 440. * 2f32.powf(shift / 12.);
        assert!((frequency(&audio[RATE as usize / 4..]) - expected).abs() < 8.);
        assert!(engine.state().decks[0].playing);
    }
}

#[test]
fn scratch_plays_in_reverse() {
    let (mut engine, mut mixer) = engine_with(ramp(2.0));
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 1.0 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::Scratch { speed: Some(-1.0) }))
        .unwrap();
    let out = render(&mut mixer, 0.5);

    assert!((position(&mut engine) - 0.5).abs() < 0.01);
    // The ramp reads 0.5 at one second and 0.25 at half a second.
    assert!((out[0] - 0.5).abs() < 0.01);
    assert!((out.last().unwrap() - 0.25).abs() < 0.01);
    assert!(out.windows(2).all(|pair| pair[1] <= pair[0]));
}

#[test]
fn scratch_follows_near_zero_speed() {
    let (mut engine, mut mixer) = engine_with(ramp(2.0));
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 1.0 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::Scratch { speed: Some(0.001) }))
        .unwrap();
    let out = render(&mut mixer, 1.0);

    // One second at a thousandth of normal speed moves one millisecond.
    assert!((position(&mut engine) - 1.001).abs() < 0.0001);
    assert!(out.iter().all(|s| (s - 0.5).abs() < 0.001));

    // Releasing the scratch hands control back to the paused transport.
    engine
        .apply(Role::Host, deck(DeckAction::Scratch { speed: None }))
        .unwrap();
    render(&mut mixer, 0.5);
    let stopped = position(&mut engine);
    render(&mut mixer, 0.5);
    assert_eq!(position(&mut engine), stopped);
}

#[test]
fn stops_at_the_end_of_the_track() {
    let (mut engine, mut mixer) = engine_with(sine(440.0, 0.1, RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let out = render(&mut mixer, 0.5);

    let state = engine.state();
    assert!(!state.decks[0].playing);
    assert!((state.decks[0].position_seconds - 0.1).abs() < 0.001);
    assert!(out[RATE as usize / 4..].iter().all(|s| *s == 0.0));
}

#[test]
fn volume_and_master_gain_scale_the_output() {
    let (mut engine, mut mixer) = engine_with(sine(440.0, 2.0, RATE));
    engine
        .apply(Role::Host, deck(DeckAction::SetVolume { level: 0.5 }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetMasterGain { level: 0.5 },
            },
        )
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let out = render(&mut mixer, 1.0);

    let peak = out[RATE as usize / 2..]
        .iter()
        .fold(0f32, |p, s| p.max(s.abs()));
    assert!((peak - 0.125).abs() < 0.005, "peak {peak}");
}

#[test]
fn rejects_actions_by_role_and_value() {
    let (mut engine, _mixer) = Engine::headless(RATE);
    let master = Action::Mixer {
        action: MixerAction::SetMasterGain { level: 0.5 },
    };

    assert_eq!(
        engine.apply(Role::Viewer, deck(DeckAction::Play)),
        Err(Rejection::NotPermitted)
    );
    assert_eq!(
        engine.apply(Role::CoDj, master.clone()),
        Err(Rejection::NotPermitted)
    );
    assert_eq!(engine.apply(Role::CoDj, deck(DeckAction::Play)), Ok(()));
    assert_eq!(engine.apply(Role::Host, master), Ok(()));
    assert_eq!(
        engine.apply(
            Role::Host,
            Action::Deck {
                deck: 9,
                action: DeckAction::Play
            }
        ),
        Err(Rejection::UnknownDeck)
    );
    assert_eq!(
        engine.apply(Role::Host, deck(DeckAction::SetTempo { ratio: 3.0 })),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(
        engine.apply(
            Role::Host,
            deck(DeckAction::Scratch {
                speed: Some(f64::NAN)
            })
        ),
        Err(Rejection::InvalidValue)
    );
}

/// Writes a mono 16-bit WAV file and returns its path.
fn write_wav(name: &str, rate: u32, samples: &[f32]) -> PathBuf {
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s * i16::MAX as f32) as i16).to_le_bytes())
        .collect();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);

    let path = std::env::temp_dir().join(format!("djcraft-{}-{name}.wav", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

fn wait_for_load(engine: &mut Engine) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.state().decks[0].loading {
        assert!(Instant::now() < deadline, "load timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn loads_a_file_and_resamples_it() {
    let mono: Vec<f32> = sine(440.0, 1.0, 44_100).chunks(2).map(|f| f[0]).collect();
    let path = write_wav("tone", 44_100, &mono);

    let (mut engine, mut mixer) = Engine::headless(RATE);
    let load = DeckAction::Load {
        path: path.to_string_lossy().into_owned(),
    };
    engine.apply(Role::Host, deck(load)).unwrap();
    // No check that it reads as loading here: a small file on a fast
    // machine can be ready before the first look.
    wait_for_load(&mut engine);
    // The deck is handed the track before decoding has necessarily finished.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !engine.state().decks[0].track.as_ref().unwrap().fully_loaded {
        assert!(Instant::now() < deadline, "decode timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::remove_file(&path).unwrap();

    let state = engine.state();
    assert_eq!(state.decks[0].error, None);
    let info = state.decks[0].track.as_ref().unwrap();
    assert!(info.title.ends_with("-tone"));
    assert!((info.duration_seconds - 1.0).abs() < 0.01);
    assert_eq!(info.loaded_seconds, info.duration_seconds);
    let track = engine.track(0).unwrap();
    assert!((track.peak(track.peak_count() / 2) - 0.5).abs() < 0.02);

    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let out = render(&mut mixer, 0.5);
    let pitch = frequency(&out[RATE as usize / 4..]);
    assert!((pitch - 440.0).abs() < 4.0, "resampled pitch {pitch}");
}

#[test]
fn reports_a_failed_load() {
    let (mut engine, _mixer) = Engine::headless(RATE);
    let load = DeckAction::Load {
        path: "/nonexistent/track.mp3".into(),
    };
    engine.apply(Role::Host, deck(load)).unwrap();
    wait_for_load(&mut engine);

    let state = engine.state();
    assert!(state.decks[0].track.is_none());
    assert!(
        state.decks[0]
            .error
            .as_ref()
            .unwrap()
            .contains("cannot open file")
    );
}

#[test]
fn a_track_is_playable_before_it_is_fully_decoded() {
    let mono: Vec<f32> = sine(440.0, 10.0, 44_100).chunks(2).map(|f| f[0]).collect();
    let path = write_wav("long", 44_100, &mono);

    let mut early = None;
    Track::load(&path, RATE, None, |track| {
        // Called from the decoder, part-way through the file.
        assert!(!track.is_complete());
        assert!(track.decoded_frames() > 0);
        assert!(track.decoded_frames() < track.frames());
        assert!((track.duration_seconds() - 10.0).abs() < 0.01);
        early = Some(track);
    })
    .unwrap();
    std::fs::remove_file(&path).unwrap();

    let track = early.unwrap();
    assert!(track.is_complete());
    assert_eq!(track.frames(), 10 * RATE as usize);
    assert_eq!(track.decoded_frames(), track.frames());
    assert_eq!(
        track.peak_count(),
        track.frames().div_ceil(track.frames_per_peak())
    );
}

#[test]
fn playback_waits_for_audio_that_has_not_been_decoded() {
    let mono: Vec<f32> = sine(440.0, 10.0, 44_100).chunks(2).map(|f| f[0]).collect();
    let path = write_wav("stall", 44_100, &mono);
    let (mut engine, mut mixer) = Engine::headless(RATE);

    // Everything inside the callback runs while the decoder is paused after
    // its first packet, so only a fraction of a second exists.
    Track::load(&path, RATE, None, |track| {
        let decoded = track.decoded_seconds();
        assert!(decoded < 1.0);
        engine.load_track(0, track).unwrap();
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        render(&mut mixer, 1.0);

        let state = engine.state();
        assert!(state.decks[0].playing);
        assert!((state.decks[0].position_seconds - decoded).abs() < 0.001);
        assert!(!state.decks[0].track.as_ref().unwrap().fully_loaded);
    })
    .unwrap();
    std::fs::remove_file(&path).unwrap();

    // Once the rest has arrived, playback carries on from where it waited.
    let waited = position(&mut engine);
    render(&mut mixer, 1.0);
    assert!((position(&mut engine) - waited - 1.0).abs() < 0.01);
}

#[test]
fn a_cached_waveform_is_complete_as_soon_as_the_track_opens() {
    let mono: Vec<f32> = sine(440.0, 10.0, 44_100).chunks(2).map(|f| f[0]).collect();
    let path = write_wav("cached", 44_100, &mono);
    let cache = std::env::temp_dir().join(format!("djcraft-cache-{}", std::process::id()));

    // First load: nothing cached, so the waveform grows with the decode.
    let mut first = None;
    Track::load(&path, RATE, Some(&cache), |track| {
        assert!(track.peak_count() < 100);
        first = Some(track);
    })
    .unwrap();
    let first = first.unwrap();
    let peaks = first.peak_count();
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);

    // Second load, under a new name: the whole waveform is there before
    // most of the audio is.
    let moved = path.with_file_name(format!("djcraft-{}-moved.wav", std::process::id()));
    std::fs::rename(&path, &moved).unwrap();
    Track::load(&moved, RATE, Some(&cache), |track| {
        assert!(!track.is_complete());
        assert_eq!(track.peak_count(), peaks);
        assert_eq!(track.frames(), first.frames());
        for i in 0..peaks {
            assert!((track.peak(i) - first.peak(i)).abs() < 0.01);
            for (cached, original) in track.bands(i).into_iter().zip(first.bands(i)) {
                assert!((cached - original).abs() < 0.01);
            }
        }
    })
    .unwrap();

    // A damaged entry is ignored and replaced.
    let entry = std::fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(&entry, b"not a waveform").unwrap();
    let mut rebuilt = None;
    Track::load(&moved, RATE, Some(&cache), |track| {
        assert!(track.peak_count() < 100);
        // Decoding stops if nobody keeps the track.
        rebuilt = Some(track);
    })
    .unwrap();
    assert!(std::fs::metadata(&entry).unwrap().len() > 1000);

    std::fs::remove_file(&moved).unwrap();
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn a_waveform_cached_at_one_rate_serves_another() {
    // Loud for the first half, quiet for the second.
    let mono: Vec<f32> = sine(440.0, 10.0, 44_100)
        .chunks(2)
        .enumerate()
        .map(|(i, f)| if i < 220_500 { f[0] } else { f[0] * 0.25 })
        .collect();
    let path = write_wav("rates", 44_100, &mono);
    let cache = std::env::temp_dir().join(format!("djcraft-rates-{}", std::process::id()));

    // Analysis decodes at a low rate; the decks play at a high one.
    let analysed = Track::decode_cached(&path, 22_050, Some(&cache)).unwrap();
    assert!(analysed.is_complete());
    Track::load(&path, 48_000, Some(&cache), |track| {
        assert!(!track.is_complete());
        let peaks = track.peak_count();
        assert_eq!(peaks, (480_000usize).div_ceil(track.frames_per_peak()));
        assert!(track.frames().abs_diff(480_000) <= 1);
        let (loud, quiet) = (track.peak(peaks / 4), track.peak(peaks * 3 / 4));
        assert!((loud - analysed.peak(analysed.peak_count() / 4)).abs() < 0.01);
        assert!((quiet * 4.0 - loud).abs() < 0.02, "{loud} then {quiet}");
        let bands = track.bands(peaks / 4);
        assert!(bands[1] > bands[0] && bands[1] > bands[2]);
    })
    .unwrap();

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn waveform_bands_follow_frequency_without_stereo_cancellation() {
    for rate in [22_050, 44_100, 48_000] {
        for (dominant, frequency) in [80., 1000., 8000.].into_iter().enumerate() {
            let mut samples = sine(frequency, 0.4, rate);
            // Opposite phase must not make a stereo waveform disappear.
            for frame in samples.chunks_exact_mut(2) {
                frame[1] = -frame[0];
            }
            let track = Track::from_samples("tone", rate, samples);
            let bands = track.bands(track.peak_count() / 2);
            for other in 0..3 {
                if other != dominant {
                    assert!(
                        bands[dominant] > bands[other] * 1.5,
                        "{frequency} Hz at {rate}: {bands:?}"
                    );
                }
            }
        }
    }
    let silent = Track::from_samples("silence", RATE, vec![0.; 514]);
    assert_eq!(silent.peak_count(), 2);
    assert_eq!(silent.bands(1), [0.; 3]);
}

#[test]
fn position_scrubbing_is_audible_bidirectional_and_stops_at_the_hand() {
    let (mut engine, mut mixer) = engine_with(ramp(2.0));
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 1.0 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetKeyLock { on: true }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::ScratchTo { seconds: 0.5 }))
        .unwrap();
    let reverse = render(&mut mixer, 0.3);
    // Compare after the short scratch attack has opened the gain envelope.
    assert!(reverse[1200] > reverse[2400]);
    assert!(reverse.iter().any(|s| s.abs() > 0.1));
    assert!((position(&mut engine) - 0.5).abs() < 0.0001);
    assert!(render(&mut mixer, 0.1).iter().all(|s| *s == 0.));
    engine
        .apply(Role::Host, deck(DeckAction::ScratchTo { seconds: 1.0 }))
        .unwrap();
    let forward = render(&mut mixer, 0.3);
    assert!(forward[1200] < forward[2400]);
    assert!((position(&mut engine) - 1.0).abs() < 0.0001);
    engine
        .apply(Role::Host, deck(DeckAction::Scratch { speed: None }))
        .unwrap();
    assert!(render(&mut mixer, 0.1).iter().all(|s| *s == 0.));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    assert!(position(&mut engine) > 1.09);
    assert!(
        engine
            .apply(
                Role::Host,
                deck(DeckAction::ScratchTo { seconds: f64::NAN })
            )
            .is_err()
    );
}
#[test]
fn mixer_crossfader_trim_meters_and_validation() {
    use djcraft_protocol::CrossfaderCurve;
    let (mut engine, mut mixer) = engine_with(vec![0.1; RATE as usize * 4]);
    engine
        .load_track(
            1,
            Arc::new(Track::from_samples(
                "right",
                RATE,
                vec![0.2; RATE as usize * 4],
            )),
        )
        .unwrap();
    for i in 0..2 {
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: i,
                    action: DeckAction::Play,
                },
            )
            .unwrap();
    }
    for curve in [
        CrossfaderCurve::Smooth,
        CrossfaderCurve::EqualPower,
        CrossfaderCurve::Cut,
    ] {
        engine
            .apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::SetCrossfaderCurve { curve },
                },
            )
            .unwrap();
        for (position, expected) in [(0., 0.1), (1., 0.2)] {
            engine
                .apply(
                    Role::Host,
                    Action::Mixer {
                        action: MixerAction::SetCrossfader { position },
                    },
                )
                .unwrap();
            let output = render(&mut mixer, 0.04);
            assert!((output.last().unwrap() - expected).abs() < 0.001);
        }
    }
    engine
        .apply(Role::Host, deck(DeckAction::SetTrim { db: 12. }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetCrossfader { position: 0. },
            },
        )
        .unwrap();
    let output = render(&mut mixer, 0.1);
    assert!((output.last().unwrap() - 0.3981).abs() < 0.001);
    let state = engine.state();
    assert!(state.decks[0].peak > 0.39 && state.decks[1].peak > 0.19);
    assert!(state.master_peak > 0.39);
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetMasterGain { level: 0. },
            },
        )
        .unwrap();
    assert!(render(&mut mixer, 0.04).last().unwrap().abs() < 1e-6);
    for action in [
        DeckAction::SetTrim { db: f32::NAN },
        DeckAction::SetTrim { db: 13. },
        DeckAction::SetEq { band: 3, gain: 1. },
        DeckAction::SetEq { band: 0, gain: -1. },
    ] {
        assert_eq!(
            engine.apply(Role::Host, deck(action)),
            Err(Rejection::InvalidValue)
        );
    }
}

#[test]
fn master_meter_reports_clipping_before_output_is_clamped() {
    let (mut engine, mut mixer) = engine_with(vec![0.5; RATE as usize * 2]);
    engine
        .apply(Role::Host, deck(DeckAction::SetTrim { db: 12. }))
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let output = render(&mut mixer, 0.1);
    assert!(output.iter().all(|value| value.abs() <= 1.));
    assert!(engine.state().master_peak > 1.9);
}
#[test]
fn master_stereo_meters_keep_channels_separate() {
    let samples = (0..RATE).flat_map(|_| [0.1, 0.4]).collect();
    let (mut engine, mut mixer) = engine_with(samples);
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    let state = engine.state();
    assert!((state.master_stereo_peak[0] - 0.1).abs() < 0.001);
    assert!((state.master_stereo_peak[1] - 0.4).abs() < 0.001);
    assert!((state.master_peak - 0.4).abs() < 0.001);
}

#[test]
fn sync_matches_tempo_and_corrects_phase_gradually() {
    for key_lock in [false, true] {
        let run = |offset: f64| {
            let (mut engine, mut mixer) = engine_with(sine(440., 8., RATE));
            engine
                .apply(Role::Host, deck(DeckAction::SetKeyLock { on: key_lock }))
                .unwrap();
            engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
            render(&mut mixer, 1.);
            let before = position(&mut engine);
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::Sync {
                        ratio: 1.05,
                        offset_seconds: offset,
                    }),
                )
                .unwrap();
            render(&mut mixer, 0.1);
            let early = position(&mut engine);
            assert!(early - before < 0.13, "sync jumped: {}", early - before);
            render(&mut mixer, 5.);
            assert!((engine.state().decks[0].tempo - 1.05).abs() < 1e-9);
            let aligned = position(&mut engine);
            render(&mut mixer, 0.5);
            assert!((position(&mut engine) - aligned - 0.525).abs() < 0.003);
            aligned
        };
        let baseline = run(0.);
        for offset in [-0.2, 0.2] {
            assert!(
                (run(offset) - baseline - offset).abs() < 0.003,
                "key lock {key_lock}"
            );
        }
    }
}

#[test]
fn fine_nudges_accumulate_without_stopping_or_changing_base_tempo() {
    let (mut engine, mut mixer) = engine_with(sine(440., 3., RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.5);
    let before = position(&mut engine);
    for _ in 0..2 {
        engine
            .apply(Role::Host, deck(DeckAction::Nudge { seconds: 0.005 }))
            .unwrap();
    }
    render(&mut mixer, 0.5);
    assert!((position(&mut engine) - before - 0.51).abs() < 0.001);
    let state = engine.state();
    assert!(state.decks[0].playing);
    assert_eq!(state.decks[0].tempo, 1.);
    for action in [
        DeckAction::Nudge { seconds: f64::NAN },
        DeckAction::Sync {
            ratio: 3.,
            offset_seconds: 0.,
        },
        DeckAction::Sync {
            ratio: 1.,
            offset_seconds: 1.,
        },
    ] {
        assert_eq!(
            engine.apply(Role::Host, deck(action)),
            Err(Rejection::InvalidValue)
        );
    }
}

#[test]
fn pitch_bend_expires_on_audio_clock_and_does_not_expire_a_subsequent_sync() {
    for ratio in [0.92, 1., 1.08] {
        for key_lock in [false, true] {
            let (mut engine, mut mixer) = engine_with(sine(220., 8., RATE));
            for action in [
                DeckAction::SetTempo { ratio },
                DeckAction::SetKeyLock { on: key_lock },
                DeckAction::Play,
            ] {
                engine.apply(Role::Host, deck(action)).unwrap();
            }
            render(&mut mixer, 0.2);
            engine
                .apply(Role::Host, deck(DeckAction::PitchBend { seconds: 0.5 }))
                .unwrap();
            // No UI tick or release command: the audio thread must stop the bend.
            render(&mut mixer, 0.1);
            let before = position(&mut engine);
            let out = render(&mut mixer, 0.5);
            assert!((position(&mut engine) - before - ratio * 0.5).abs() < 0.001);
            let expected_pitch = 220. * if key_lock { 1. } else { ratio as f32 };
            assert!((frequency(&out[out.len() / 2..]) - expected_pitch).abs() < 2.);
            assert_eq!(engine.state().decks[0].tempo, ratio);
            engine
                .apply(Role::Host, deck(DeckAction::PitchBend { seconds: 0.5 }))
                .unwrap();
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::Sync {
                        ratio,
                        offset_seconds: 0.16,
                    }),
                )
                .unwrap();
            let before = position(&mut engine);
            render(&mut mixer, 2.5);
            assert!(
                (position(&mut engine) - before - (ratio * 2.5 + 0.16)).abs() < 0.002,
                "jog expiry interrupted Sync at ratio {ratio}, key lock {key_lock}"
            );
        }
    }
}

#[test]
fn loop_roll_tracks_tempo_pause_and_resumes_the_uninterrupted_position() {
    let (mut engine, mut mixer) = engine_with(sine(440., 5., RATE));
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 1. }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.25 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetLoopRoll { enabled: true }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetLoop {
                start: 1.,
                end: 1.25,
            }),
        )
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.8);
    let state = engine.state().decks[0].clone();
    assert!((state.roll_position_seconds.unwrap() - 2.).abs() < 0.002);
    assert!((1.0..1.25).contains(&state.position_seconds));
    engine.apply(Role::Host, deck(DeckAction::Pause)).unwrap();
    render(&mut mixer, 0.2);
    assert!((engine.state().decks[0].roll_position_seconds.unwrap() - 2.).abs() < 0.002);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetLoop {
                start: 1.,
                end: 1.5,
            }),
        )
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::ClearLoop))
        .unwrap();
    render(&mut mixer, 0.02);
    let state = engine.state().decks[0].clone();
    assert!((state.position_seconds - 2.).abs() < 0.01);
    assert!(state.roll_position_seconds.is_none());
}

#[test]
fn future_loop_starts_at_in_point_and_seek_cancels_roll() {
    let (mut engine, mut mixer) = engine_with(ramp(2.));
    engine
        .apply(Role::Host, deck(DeckAction::SetLoopRoll { enabled: true }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetLoop {
                start: 0.7,
                end: 1.2,
            }),
        )
        .unwrap();
    render(&mut mixer, 0.01);
    assert!((engine.state().decks[0].position_seconds - 0.7).abs() < 1e-6);
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 1.5 }))
        .unwrap();
    render(&mut mixer, 0.01);
    engine
        .apply(Role::Host, deck(DeckAction::ClearLoop))
        .unwrap();
    render(&mut mixer, 0.01);
    assert!((engine.state().decks[0].position_seconds - 1.5).abs() < 1e-6);
}

#[test]
fn effects_validate_controls_and_filter_the_selected_deck() {
    use djcraft_protocol::{EffectKind, EffectState};
    let (mut engine, mut mixer) = engine_with(sine(8000., 2., RATE));
    let effect = EffectState {
        kind: EffectKind::Filter,
        enabled: true,
        amount: 1.,
        parameter: 0.1,
        ..EffectState::default()
    };
    assert_eq!(
        engine.apply(Role::Host, deck(DeckAction::SetEffect { slot: 6, effect })),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(
        engine.apply(
            Role::Host,
            deck(DeckAction::SetEffect {
                slot: 1,
                effect: EffectState {
                    amount: f32::NAN,
                    ..effect
                }
            })
        ),
        Err(Rejection::InvalidValue)
    );
    engine
        .apply(Role::Host, deck(DeckAction::SetEffect { slot: 1, effect }))
        .unwrap();
    assert_eq!(engine.state().decks[0].effects[1], effect);
    assert!(!engine.state().decks[1].effects[1].enabled);
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let samples = render(&mut mixer, 0.5);
    assert!(samples[samples.len() / 2..].iter().all(|s| s.abs() < 0.02));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEffect {
                slot: 1,
                effect: EffectState {
                    enabled: false,
                    ..effect
                },
            }),
        )
        .unwrap();
    let samples = render(&mut mixer, 0.5);
    assert!(samples[samples.len() / 2..].iter().any(|s| s.abs() > 0.3));
}

#[test]
fn delayed_effects_keep_live_load_protection_after_eq_kill() {
    use djcraft_protocol::{EffectKind, EffectState};
    let (mut engine, mut mixer) = engine_with(sine(400., 2., RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEffect {
                slot: 1,
                effect: EffectState {
                    kind: EffectKind::Echo,
                    enabled: true,
                    ..EffectState::default()
                },
            }),
        )
        .unwrap();
    render(&mut mixer, 0.2);
    for band in 0..3 {
        engine
            .apply(Role::Host, deck(DeckAction::SetEq { band, gain: 0. }))
            .unwrap();
    }
    assert!(engine.deck_has_open_output(0));
    engine
        .apply(Role::Host, deck(DeckAction::SetVolume { level: 0. }))
        .unwrap();
    assert!(!engine.deck_has_open_output(0));
}

#[test]
fn effects_six_slots_follow_tempo_and_keep_tails_audible_after_pause() {
    use djcraft_protocol::{EffectKind, EffectState};
    let (mut engine, mut mixer) = engine_with(sine(200., 3., RATE));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEffectTempo { bpm: Some(120.) }),
        )
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.5 }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEffect {
                slot: 5,
                effect: EffectState {
                    kind: EffectKind::Echo,
                    enabled: true,
                    sync: true,
                    beats: 0.5,
                    amount: 1.,
                    ..EffectState::default()
                },
            }),
        )
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    engine.apply(Role::Host, deck(DeckAction::Pause)).unwrap();
    render(&mut mixer, 0.03);
    assert!(!engine.state().decks[0].playing);
    assert!(engine.state().decks[0].effect_tail);
    assert!(
        engine.deck_has_open_output(0),
        "delayed output must be protected even before its first repeat"
    );
    let tail = render(&mut mixer, 0.25);
    assert!(tail.iter().any(|x| x.abs() > 0.1));
    engine
        .apply(Role::Host, deck(DeckAction::SetVolume { level: 0. }))
        .unwrap();
    render(&mut mixer, 0.02);
    assert!(render(&mut mixer, 0.2).iter().all(|x| x.abs() < 1e-7));
    assert!(!engine.deck_has_open_output(0));
    engine
        .load_track(
            0,
            Arc::new(Track::from_samples(
                "new",
                RATE,
                vec![0.; RATE as usize * 2],
            )),
        )
        .unwrap();
    render(&mut mixer, 0.02);
    assert!(!engine.state().decks[0].effect_tail);
}

#[test]
fn effect_timing_and_feedback_reject_invalid_protocol_values() {
    use djcraft_protocol::EffectState;
    let (mut engine, _) = Engine::headless(RATE);
    for bpm in [f64::NAN, 0., 301., f64::INFINITY] {
        assert_eq!(
            engine.apply(
                Role::Host,
                deck(DeckAction::SetEffectTempo { bpm: Some(bpm) })
            ),
            Err(Rejection::InvalidValue)
        );
    }
    for effect in [
        EffectState {
            feedback: 0.91,
            ..EffectState::default()
        },
        EffectState {
            feedback: f32::NAN,
            ..EffectState::default()
        },
        EffectState {
            beats: 0.,
            ..EffectState::default()
        },
        EffectState {
            beats: f32::INFINITY,
            ..EffectState::default()
        },
    ] {
        assert_eq!(
            engine.apply(Role::Host, deck(DeckAction::SetEffect { slot: 0, effect })),
            Err(Rejection::InvalidValue)
        );
    }
}

#[test]
fn shared_effect_beats_are_deck_local_preserved_by_slot_edits_and_reach_audio() {
    use djcraft_protocol::{EffectKind, EffectState};
    let (mut engine, mut mixer) = engine_with(vec![0.4; RATE as usize * 12]);
    let state = engine.state();
    for deck in &state.decks {
        assert_eq!(deck.effect_beats, 0.5);
        assert_eq!(
            deck.effects[..3]
                .iter()
                .map(|fx| fx.kind)
                .collect::<Vec<_>>(),
            vec![EffectKind::Echo, EffectKind::Reverb, EffectKind::Flanger]
        );
        assert!(!deck.effects[0].enabled);
        assert!(deck.effects[1..].iter().all(|fx| !fx.enabled));
    }
    for beats in [0., 0.0625, 8.01, f32::NAN, f32::INFINITY] {
        assert_eq!(
            engine.apply(Role::Host, deck(DeckAction::SetEffectBeats { beats })),
            Err(Rejection::InvalidValue)
        );
    }
    assert_eq!(
        engine.apply(Role::Viewer, deck(DeckAction::SetEffectBeats { beats: 1. })),
        Err(Rejection::NotPermitted)
    );
    engine
        .apply(Role::Host, deck(DeckAction::SetEffectBeats { beats: 2. }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEffect {
                slot: 2,
                effect: EffectState {
                    kind: EffectKind::Cut,
                    enabled: true,
                    amount: 1.,
                    beats: 0.25,
                    ..EffectState::default()
                },
            }),
        )
        .unwrap();
    let state = engine.state();
    assert!(state.decks[0].effects.iter().all(|fx| fx.beats == 2.));
    assert_eq!(state.decks[1].effect_beats, 0.5);
    assert!(state.decks[1].effects.iter().all(|fx| fx.beats == 0.5));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEffectTempo { bpm: Some(120.) }),
        )
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.5);
    let transitions = |samples: Vec<f32>| {
        let levels: Vec<bool> = samples
            .chunks_exact(2)
            .map(|frame| frame[0] > 0.15)
            .collect();
        levels.windows(2).filter(|pair| pair[0] != pair[1]).count()
    };
    let slow = transitions(render(&mut mixer, 1.));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetEffectBeats { beats: 0.125 }),
        )
        .unwrap();
    render(&mut mixer, 0.5);
    let fast = transitions(render(&mut mixer, 1.));
    assert!(
        slow <= 4 && fast > 12,
        "shared timing failed to change Cut audio: {slow}, {fast}"
    );
    let state = engine.state();
    assert!(state.decks[0].effects.iter().all(|fx| fx.beats == 0.125));
    assert!(state.decks[0].effects[2].enabled);
    assert_eq!(state.decks[0].effects[2].amount, 1.);
}

#[test]
#[ignore = "manual real-time budget measurement"]
fn effects_render_budget() {
    use djcraft_protocol::{EffectKind as K, EffectState};
    let (mut engine, mut mixer) = Engine::headless(RATE);
    let track = Arc::new(Track::from_samples("budget", RATE, sine(220., 6., RATE)));
    for deck in 0..2 {
        engine.load_track(deck, track.clone()).unwrap();
        for (slot, kind) in [
            K::Filter,
            K::Spiral,
            K::PitchShift,
            K::Phaser,
            K::Chorus,
            K::Flanger,
        ]
        .into_iter()
        .enumerate()
        {
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: deck as u8,
                        action: DeckAction::SetEffect {
                            slot: slot as u8,
                            effect: EffectState {
                                kind,
                                enabled: true,
                                parameter: 0.7,
                                ..EffectState::default()
                            },
                        },
                    },
                )
                .unwrap();
        }
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: deck as u8,
                    action: DeckAction::Play,
                },
            )
            .unwrap();
    }
    let mut block = [0.; CALLBACK * 2];
    let start = Instant::now();
    for _ in 0..500 {
        mixer.render(&mut block);
        assert!(block.iter().all(|x| x.is_finite()));
    }
    println!(
        "12 simultaneous effects: 5 seconds of stereo rendered in {:?}",
        start.elapsed()
    );
}

#[test]
fn hot_cues_trigger_atomically_and_are_cleared_on_new_track() {
    use djcraft_protocol::{HOT_CUES, HotCue};
    let (mut engine, mut mixer) = engine_with(ramp(4.));
    let mut cues = [None; HOT_CUES];
    cues[0] = Some(HotCue {
        seconds: 1.,
        loop_end: None,
    });
    cues[15] = Some(HotCue {
        seconds: 2.,
        loop_end: Some(2.5),
    });
    engine
        .apply(Role::Host, deck(DeckAction::SetHotCues { cues }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::TriggerHotCue {
                slot: 0,
                play: true,
                loop_length: None,
            }),
        )
        .unwrap();
    let audio = render(&mut mixer, 0.1);
    assert!(audio.last().unwrap() > &0.25);
    let state = engine.state().decks[0].clone();
    assert!(state.playing);
    assert!((state.position_seconds - 1.1).abs() < 0.01);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::TriggerHotCue {
                slot: 15,
                play: true,
                loop_length: None,
            }),
        )
        .unwrap();
    render(&mut mixer, 0.7);
    let state = engine.state().decks[0].clone();
    assert!((state.position_seconds - 2.2).abs() < 0.01);
    assert_eq!(state.loop_region, Some([2., 2.5]));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::TriggerHotCue {
                slot: 0,
                play: false,
                loop_length: None,
            }),
        )
        .unwrap();
    render(&mut mixer, 0.1);
    let state = engine.state().decks[0].clone();
    assert!(!state.playing);
    assert!((state.position_seconds - 1.).abs() < 1e-6);
    assert!(state.loop_region.is_none());
    assert_eq!(
        engine.apply(
            Role::Host,
            deck(DeckAction::TriggerHotCue {
                slot: 2,
                play: true,
                loop_length: None
            })
        ),
        Err(Rejection::InvalidValue)
    );
    cues[3] = Some(HotCue {
        seconds: f64::NAN,
        loop_end: None,
    });
    assert_eq!(
        engine.apply(Role::Host, deck(DeckAction::SetHotCues { cues })),
        Err(Rejection::InvalidValue)
    );
    engine
        .load_track(0, Arc::new(Track::from_samples("other", RATE, ramp(1.))))
        .unwrap();
    assert!(engine.state().decks[0].hot_cues.iter().all(Option::is_none));
}

#[test]
fn slicer_roll_restores_uninterrupted_position_and_arm_loop_waits() {
    let (mut engine, mut mixer) = engine_with(ramp(5.));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 1.);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::StartPadLoop {
                start: 0.25,
                end: 0.5,
                roll: true,
            }),
        )
        .unwrap();
    render(&mut mixer, 0.6);
    let state = engine.state().decks[0].clone();
    assert!((0.25..0.5).contains(&state.position_seconds));
    assert!((state.roll_position_seconds.unwrap() - 1.6).abs() < 0.02);
    engine
        .apply(Role::Host, deck(DeckAction::ClearLoop))
        .unwrap();
    render(&mut mixer, 0.01);
    assert!((engine.state().decks[0].position_seconds - 1.61).abs() < 0.02);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::ArmLoop {
                start: 2.,
                end: 2.1,
            }),
        )
        .unwrap();
    render(&mut mixer, 0.1);
    assert!((engine.state().decks[0].position_seconds - 1.71).abs() < 0.02);
    render(&mut mixer, 0.5);
    assert!((2.0..2.1).contains(&engine.state().decks[0].position_seconds));
    engine
        .apply(Role::Host, deck(DeckAction::Jump { seconds: -1. }))
        .unwrap();
    render(&mut mixer, 0.01);
    let state = engine.state().decks[0].clone();
    assert!((1.0..1.12).contains(&state.position_seconds));
    assert!(state.loop_region.is_none());
}

#[test]
fn stem_pads_work_with_classic_eq_and_mute_the_route() {
    let (mut engine, mut mixer) = Engine::headless(RATE);
    let track = Arc::new(Track::from_samples(
        "stems",
        RATE,
        vec![0.4; RATE as usize * 4],
    ));
    track
        .attach_stems(std::array::from_fn(|_| {
            vec![0.1; RATE as usize * 4].into_boxed_slice()
        }))
        .unwrap();
    engine.load_track(0, track).unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetStemGains { gains: [0.; 4] }),
        )
        .unwrap();
    render(&mut mixer, 0.5);
    assert!(render(&mut mixer, 0.2).iter().all(|x| x.abs() < 1e-5));
    // The meter's hold decays; the signal-route check also accounts for closed stems.
    render(&mut mixer, 6.);
    assert!(!engine.deck_has_open_output(0));
}

#[test]
fn sampler_plays_independently_and_survives_deck_replacement() {
    let (mut engine, mut mixer) = engine_with(sine(440., 2., RATE));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::CaptureSample {
                slot: 0,
                start: 0.2,
                end: 0.4,
            }),
        )
        .unwrap();
    engine
        .load_track(
            0,
            Arc::new(Track::from_samples(
                "replacement",
                RATE,
                vec![0.; RATE as usize * 2],
            )),
        )
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::Sample {
                slot: 0,
                play: true,
                restart: true,
                repeat: true,
                level: 0.5,
            }),
        )
        .unwrap();
    let audio = render(&mut mixer, 0.5);
    assert!(audio.iter().any(|s| s.abs() > 0.1));
    assert!(!engine.state().decks[0].playing);
    assert_eq!(engine.state().decks[1].samples[0], 2);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::Sample {
                slot: 0,
                play: false,
                restart: false,
                repeat: false,
                level: 0.5,
            }),
        )
        .unwrap();
    let audio = render(&mut mixer, 0.1);
    assert!(audio[audio.len() - 100..].iter().all(|s| s.abs() < 0.0001));
    engine
        .apply(Role::Host, deck(DeckAction::ClearSample { slot: 0 }))
        .unwrap();
    render(&mut mixer, 0.01);
    assert_eq!(engine.state().decks[0].samples[0], 0);
}

#[test]
fn sampler_one_shot_ends_and_rejects_invalid_capture() {
    let (mut engine, mut mixer) = engine_with(sine(440., 1., RATE));
    for (slot, start, end) in [
        (16, 0., 0.2),
        (0, f64::NAN, 0.2),
        (0, 0.2, 2.),
        (0, 0.3, 0.2),
    ] {
        assert!(
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::CaptureSample { slot, start, end })
                )
                .is_err()
        );
    }
    engine
        .apply(
            Role::Host,
            deck(DeckAction::CaptureSample {
                slot: 0,
                start: 0.,
                end: 0.1,
            }),
        )
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::Sample {
                slot: 0,
                play: true,
                restart: true,
                repeat: false,
                level: 1.,
            }),
        )
        .unwrap();
    let audio = render(&mut mixer, 0.2);
    assert!(audio[..2000].iter().any(|v| v.abs() > 0.1));
    assert!(audio[audio.len() - 100..].iter().all(|v| *v == 0.));
    assert_eq!(engine.state().decks[0].samples[0], 1);
}

#[test]
fn automated_patterns_preserve_transport_and_finish() {
    for pattern in 0..8 {
        let (mut engine, mut mixer) = engine_with(sine(220., 8., RATE));
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        render(&mut mixer, 0.1);
        let start = engine.state().decks[0].position_seconds;
        engine
            .apply(
                Role::Host,
                deck(DeckAction::AutoScratch {
                    pattern: Some(pattern),
                    beat_seconds: 0.25,
                    first_beat: 0.,
                    quantize: false,
                    over: pattern % 2 == 0,
                }),
            )
            .unwrap();
        let audio = render(&mut mixer, 1.2);
        let state = engine.state().decks[0].clone();
        assert!((state.position_seconds - start - 1.2).abs() < 0.002);
        assert!(!state.auto_scratch);
        assert!(audio.iter().all(|v| v.is_finite()));
        assert!(audio.iter().any(|v| v.abs() > 0.1));
    }
}

#[test]
fn automated_scratch_quantizes_and_manual_seek_cancels() {
    let (mut engine, mut mixer) = engine_with(sine(220., 8., RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::AutoScratch {
                pattern: Some(0),
                beat_seconds: 0.5,
                first_beat: 0.,
                quantize: true,
                over: false,
            }),
        )
        .unwrap();
    render(&mut mixer, 2.1);
    assert!(engine.state().decks[0].auto_scratch); // Waiting to next beat extends the four-beat gesture.
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 3. }))
        .unwrap();
    render(&mut mixer, 0.01);
    assert!(!engine.state().decks[0].auto_scratch);
    assert!((engine.state().decks[0].position_seconds - 3.01).abs() < 0.002);
}

#[test]
fn automated_scratch_reverses_audio_and_leaves_paused_transport_parked() {
    let (mut engine, mut mixer) = engine_with(ramp(8.));
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 1. }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::AutoScratch {
                pattern: Some(0),
                beat_seconds: 0.5,
                first_beat: 0.,
                quantize: false,
                over: false,
            }),
        )
        .unwrap();
    let audio = render(&mut mixer, 0.5);
    assert!(
        audio[18000] < audio[14000],
        "return stroke must read backwards"
    );
    assert!(!engine.state().decks[0].playing);
    assert!((engine.state().decks[0].position_seconds - 1.).abs() < 0.001);
    render(&mut mixer, 1.6);
    assert!(!engine.state().decks[0].auto_scratch);
    assert!(render(&mut mixer, 0.1).iter().all(|v| v.abs() < 0.0001));
}

fn wait_sample(engine: &mut Engine, mixer: &mut Mixer, slot: usize) -> djcraft_protocol::DeckState {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = engine.state().decks[0].clone();
        render(mixer, 0.01);
        if !state.sample_loading[slot] {
            return engine.state().decks[0].clone();
        }
        assert!(Instant::now() < deadline, "sample load timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn sample_import_is_async_resampled_and_failed_replacement_preserves_audio() {
    let (mut engine, mut mixer) = Engine::headless(RATE);
    let path = write_wav("sampler-import", 22050, &vec![0.25; 2205]);
    let action = DeckAction::LoadSample {
        slot: 0,
        path: path.to_string_lossy().into_owned(),
    };
    assert_eq!(
        engine.apply(Role::Viewer, deck(action.clone())),
        Err(Rejection::NotPermitted)
    );
    engine.apply(Role::Host, deck(action)).unwrap();
    let state = wait_sample(&mut engine, &mut mixer, 0);
    assert_eq!(state.samples[0], 1);
    assert!(state.sample_errors[0].is_none());
    let name = state.sample_names[0].clone();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::LoadSample {
                slot: 0,
                path: "nonexistent-sampler-file.wav".into(),
            }),
        )
        .unwrap();
    let state = wait_sample(&mut engine, &mut mixer, 0);
    assert!(state.sample_errors[0].is_some());
    assert_eq!(state.sample_names[0], name);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::Sample {
                slot: 0,
                play: true,
                restart: true,
                repeat: false,
                level: 1.,
            }),
        )
        .unwrap();
    let audio = render(&mut mixer, 0.15);
    assert!(audio[1000] > 0.2);
    assert!(audio[6000].abs() < 0.0001);
    assert!(
        engine
            .apply(Role::Host, deck(DeckAction::DefaultSample { slot: 15 }))
            .is_err()
    );
    std::fs::remove_file(path).unwrap();
}
#[test]
fn sample_clear_wins_over_pending_load() {
    let (mut engine, mut mixer) = Engine::headless(RATE);
    engine
        .apply(Role::Host, deck(DeckAction::DefaultSample { slot: 0 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::ClearSample { slot: 0 }))
        .unwrap();
    let state = wait_sample(&mut engine, &mut mixer, 0);
    assert_eq!(state.samples[0], 0);
    assert!(state.sample_names[0].is_empty());
}

fn auto_action(engine: &mut Engine, action: MixerAction) {
    engine.apply(Role::Host, Action::Mixer { action }).unwrap();
}
fn auto_queue(engine: &mut Engine, paths: &[PathBuf]) {
    auto_action(engine, MixerAction::AutomixSmart { enabled: false });
    auto_action(
        engine,
        MixerAction::AutomixEnqueue {
            tracks: paths
                .iter()
                .map(|p| djcraft_protocol::QueueTrack {
                    grid: None,
                    path: p.to_string_lossy().into_owned(),
                    title: p.file_stem().unwrap().to_string_lossy().into_owned(),
                })
                .collect(),
        },
    );
}
fn drive_auto_until(
    engine: &mut Engine,
    mixer: &mut Mixer,
    condition: impl Fn(&djcraft_protocol::SessionState) -> bool,
) -> djcraft_protocol::SessionState {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = engine.state();
        if condition(&state) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "automix stalled: {:?}",
            state.automix
        );
        render(mixer, 0.01);
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn automix_fades_in_audio_callback_and_finishes_queue() {
    let a = write_wav("auto-a", RATE, &vec![0.2; RATE as usize * 2]);
    let b = write_wav("auto-b", RATE, &vec![0.4; RATE as usize * 2]);
    let (mut engine, mut mixer) = Engine::headless(RATE);
    auto_queue(&mut engine, &[a.clone(), b.clone()]);
    auto_action(&mut engine, MixerAction::AutomixFadeTime { seconds: 0.4 });
    auto_action(&mut engine, MixerAction::AutomixStart);
    drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    let audio = render(&mut mixer, 0.5); // No state/UI ticks during the fade.
    assert!(audio.iter().all(|v| v.is_finite()));
    assert!(audio[100] > 0.15 && audio[100] < 0.3);
    assert!(audio[audio.len() - 1] > 0.35);
    let state = engine.state();
    assert_eq!(state.automix.active_deck, Some(1));
    assert_eq!(state.crossfader, 1.);
    assert!(!state.decks[0].playing);
    assert!(state.decks[1].playing);
    assert_eq!(state.decks[1].path.as_deref(), Some(b.to_str().unwrap()));
    drive_auto_until(&mut engine, &mut mixer, |s| !s.automix.enabled);
    std::fs::remove_file(a).unwrap();
    std::fs::remove_file(b).unwrap();
}
#[test]
fn automix_skips_missing_file_and_manual_control_takes_over() {
    let a = write_wav("auto-missing-a", RATE, &vec![0.2; RATE as usize * 3]);
    let b = write_wav("auto-missing-b", RATE, &vec![0.4; RATE as usize * 3]);
    let (mut engine, mut mixer) = Engine::headless(RATE);
    auto_queue(
        &mut engine,
        &[
            PathBuf::from("missing-automix-file.wav"),
            a.clone(),
            b.clone(),
        ],
    );
    auto_action(&mut engine, MixerAction::AutomixFadeTime { seconds: 0.3 });
    auto_action(&mut engine, MixerAction::AutomixStart);
    drive_auto_until(&mut engine, &mut mixer, |s| {
        s.decks[0].playing && s.automix.next.is_some()
    });
    auto_action(&mut engine, MixerAction::SetCrossfader { position: 0.25 });
    render(&mut mixer, 0.02);
    let state = engine.state();
    assert!(!state.automix.enabled);
    assert!(state.decks[0].playing);
    assert_eq!(state.crossfader, 0.25);
    assert_eq!(state.automix.queue.len(), 1); // Preloaded track returns to the queue.
    std::fs::remove_file(a).unwrap();
    std::fs::remove_file(b).unwrap();
}
#[test]
fn automix_queue_order_roles_and_clear_are_validated() {
    let (mut engine, _mixer) = Engine::headless(RATE);
    assert_eq!(
        engine.apply(
            Role::CoDj,
            Action::Mixer {
                action: MixerAction::AutomixStart
            }
        ),
        Err(Rejection::NotPermitted)
    );
    assert!(
        engine
            .apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::AutomixStart
                }
            )
            .is_err()
    );
    auto_queue(
        &mut engine,
        &[
            PathBuf::from("one"),
            PathBuf::from("two"),
            PathBuf::from("three"),
        ],
    );
    auto_action(&mut engine, MixerAction::AutomixMove { from: 2, to: 0 });
    auto_action(&mut engine, MixerAction::AutomixRemove { index: 1 });
    assert_eq!(
        engine
            .state()
            .automix
            .queue
            .iter()
            .map(|t| t.path.as_str())
            .collect::<Vec<_>>(),
        ["three", "two"]
    );
    assert!(
        engine
            .apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::AutomixFadeTime { seconds: f64::NAN }
                }
            )
            .is_err()
    );
    auto_action(&mut engine, MixerAction::AutomixClear);
    assert!(engine.state().automix.queue.is_empty());
}
#[test]
fn automix_adopts_playing_deck_and_clear_discards_preload() {
    let b = write_wav("auto-adopt", RATE, &vec![0.4; RATE as usize * 2]);
    let (mut engine, mut mixer) = engine_with(sine(440., 3., RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    auto_queue(&mut engine, std::slice::from_ref(&b));
    auto_action(&mut engine, MixerAction::AutomixFadeTime { seconds: 0.2 });
    auto_action(&mut engine, MixerAction::AutomixStart);
    let state = drive_auto_until(&mut engine, &mut mixer, |s| s.automix.next.is_some());
    assert!(state.decks[0].position_seconds >= 0.09);
    auto_action(&mut engine, MixerAction::AutomixClear);
    drive_auto_until(&mut engine, &mut mixer, |s| !s.automix.enabled);
    assert!(!engine.state().decks[1].playing);
    std::fs::remove_file(b).unwrap();
}

#[test]
fn automix_short_incoming_track_does_not_cut_outgoing_early() {
    let a = write_wav("auto-short-a", RATE, &vec![0.2; RATE as usize * 2]);
    let b = write_wav("auto-short-b", RATE, &vec![0.4; RATE as usize / 5]);
    let (mut engine, mut mixer) = Engine::headless(RATE);
    auto_queue(&mut engine, &[a.clone(), b.clone()]);
    auto_action(&mut engine, MixerAction::AutomixFadeTime { seconds: 1. });
    auto_action(&mut engine, MixerAction::AutomixStart);
    let state = drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    assert!(state.decks[0].position_seconds >= 1.89);
    drive_auto_until(&mut engine, &mut mixer, |s| !s.automix.enabled);
    std::fs::remove_file(a).unwrap();
    std::fs::remove_file(b).unwrap();
}
#[test]
fn automix_stop_during_fade_leaves_decks_for_manual_control() {
    let a = write_wav("auto-stop-a", RATE, &vec![0.2; RATE as usize * 2]);
    let b = write_wav("auto-stop-b", RATE, &vec![0.4; RATE as usize * 2]);
    let (mut engine, mut mixer) = Engine::headless(RATE);
    auto_queue(&mut engine, &[a.clone(), b.clone()]);
    auto_action(&mut engine, MixerAction::AutomixFadeTime { seconds: 0.6 });
    auto_action(&mut engine, MixerAction::AutomixStart);
    drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    render(&mut mixer, 0.2);
    auto_action(&mut engine, MixerAction::AutomixStop);
    render(&mut mixer, 0.2);
    let state = engine.state();
    assert!(!state.automix.enabled);
    assert!(state.crossfader > 0.2 && state.crossfader < 0.5);
    assert!(state.decks[..2].iter().all(|d| d.playing));
    assert!(state.decks[2..].iter().all(|d| !d.playing));
    std::fs::remove_file(a).unwrap();
    std::fs::remove_file(b).unwrap();
}

fn smart_queue(
    engine: &mut Engine,
    path: &std::path::Path,
    grid: Option<djcraft_protocol::BeatGrid>,
) {
    auto_action(
        engine,
        MixerAction::AutomixEnqueue {
            tracks: vec![djcraft_protocol::QueueTrack {
                path: path.to_string_lossy().into_owned(),
                title: "Incoming".into(),
                grid,
            }],
        },
    );
}
#[test]
fn smart_automix_matches_effective_tempo_and_bar_phase_with_eq_handover() {
    use djcraft_protocol::BeatGrid;
    let path = write_wav("smart-incoming", RATE, &vec![0.2; RATE as usize * 30]);
    let (mut engine, mut mixer) = engine_with(sine(100., 30., RATE));
    let outgoing = BeatGrid {
        bpm: 125.,
        first_beat: 0.13,
        downbeat: 1,
        confidence: 0.9,
    };
    let incoming = BeatGrid {
        bpm: 120.,
        first_beat: 0.27,
        downbeat: 2,
        confidence: 0.9,
    };
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetBeatGrid {
                grid: Some(outgoing),
            }),
        )
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.02 }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 10.37 }))
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    smart_queue(&mut engine, &path, Some(incoming));
    auto_action(&mut engine, MixerAction::AutomixStart);
    auto_action(&mut engine, MixerAction::AutomixNext);
    let state = drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    assert!(state.automix.status.contains("Beatmatched"));
    render(&mut mixer, 0.25);
    let state = engine.state();
    assert!((state.decks[1].tempo - 1.0625).abs() < 1e-8);
    let phase = |deck: usize, g: BeatGrid| {
        ((state.decks[deck].position_seconds - g.first_beat) * g.bpm / 60. - g.downbeat as f64)
            .rem_euclid(4.)
    };
    let delta = (phase(0, outgoing) - phase(1, incoming) + 2.).rem_euclid(4.) - 2.;
    assert!(delta.abs() < 0.005, "phase difference {delta}");
    assert!(state.decks[1].eq[0] < 0.001);
    assert!(state.decks[0].eq[0] > 0.99);
    assert_eq!(state.crossfader, 0.5);
    render(&mut mixer, 2.5);
    let state = engine.state();
    assert!(state.decks[1].eq[2] > state.decks[1].eq[1]);
    assert!(state.decks[1].eq[0] < 0.001);
    render(&mut mixer, 2.);
    let state = engine.state();
    assert!(state.decks[1].eq[0] > 0.99);
    assert!(state.decks[0].eq[0] < 0.001);
    render(&mut mixer, 4.);
    let state = engine.state();
    assert_eq!(state.automix.active_deck, Some(1));
    assert_eq!(state.decks[1].eq, [1.; 3]);
    assert_eq!(state.crossfader, 0.5);
    assert_eq!(state.decks[0].eq, [0.; 3]);
    assert!((state.decks[1].tempo - 1.0625).abs() < 1e-8);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn smart_automix_falls_back_for_distant_tempos_and_stop_preserves_eq() {
    use djcraft_protocol::BeatGrid;
    let path = write_wav("smart-fallback", RATE, &vec![0.2; RATE as usize * 20]);
    let (mut engine, mut mixer) = engine_with(sine(100., 20., RATE));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetBeatGrid {
                grid: Some(BeatGrid {
                    bpm: 120.,
                    first_beat: 0.,
                    downbeat: 0,
                    confidence: 1.,
                }),
            }),
        )
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    smart_queue(
        &mut engine,
        &path,
        Some(BeatGrid {
            bpm: 180.,
            first_beat: 0.,
            downbeat: 0,
            confidence: 1.,
        }),
    );
    auto_action(&mut engine, MixerAction::AutomixStart);
    auto_action(&mut engine, MixerAction::AutomixNext);
    let state = drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    assert!(state.automix.status.contains("EQ fade"));
    render(&mut mixer, 3.);
    let before = engine.state();
    assert_eq!(before.decks[1].tempo, 1.);
    auto_action(&mut engine, MixerAction::AutomixStop);
    render(&mut mixer, 0.2);
    let after = engine.state();
    assert!(!after.automix.enabled);
    assert_eq!(before.decks[0].eq, after.decks[0].eq);
    assert_eq!(before.decks[1].eq, after.decks[1].eq);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn beat_grids_reject_invalid_metadata_without_stopping_automix() {
    use djcraft_protocol::BeatGrid;
    let (mut engine, _mixer) = Engine::headless(RATE);
    for grid in [
        BeatGrid {
            bpm: f64::NAN,
            first_beat: 0.,
            downbeat: 0,
            confidence: 1.,
        },
        BeatGrid {
            bpm: 120.,
            first_beat: 0.,
            downbeat: 4,
            confidence: 1.,
        },
        BeatGrid {
            bpm: 120.,
            first_beat: -1.,
            downbeat: 0,
            confidence: 1.,
        },
    ] {
        assert!(
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::SetBeatGrid { grid: Some(grid) })
                )
                .is_err()
        );
    }
}

#[test]
fn automix_resume_waits_for_idle_deck_and_retains_queue_cursor() {
    let a = write_wav("resume-a", RATE, &vec![0.2; RATE as usize * 10]);
    let b = write_wav("resume-b", RATE, &vec![0.3; RATE as usize * 10]);
    let c = write_wav("resume-c", RATE, &vec![0.4; RATE as usize * 10]);
    let (mut engine, mut mixer) = Engine::headless(RATE);
    auto_queue(&mut engine, &[a.clone(), b.clone(), c.clone()]);
    auto_action(&mut engine, MixerAction::AutomixStart);
    drive_auto_until(&mut engine, &mut mixer, |s| {
        s.automix.next.is_some() && s.decks[0].playing
    });
    auto_action(&mut engine, MixerAction::AutomixNext);
    drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    render(&mut mixer, 0.2);
    auto_action(&mut engine, MixerAction::AutomixStop);
    render(&mut mixer, 0.01);
    let before = engine.state();
    auto_action(&mut engine, MixerAction::AutomixStart);
    render(&mut mixer, 0.05);
    let waiting = engine.state();
    assert!(waiting.automix.enabled);
    assert!(waiting.automix.status.contains("non-playing"));
    assert_eq!(waiting.crossfader, before.crossfader);
    assert_eq!(waiting.automix.queue.len(), 1);
    assert_eq!(waiting.decks[0].path, before.decks[0].path);
    assert_eq!(waiting.decks[1].path, before.decks[1].path);
    engine.apply(Role::Host, deck(DeckAction::Pause)).unwrap();
    render(&mut mixer, 0.01);
    let resumed = drive_auto_until(&mut engine, &mut mixer, |s| s.automix.next.is_some());
    assert!(resumed.decks[1].playing);
    assert!(resumed.decks[1].position_seconds > before.decks[1].position_seconds);
    assert_eq!(resumed.automix.active_deck, Some(1));
    assert_eq!(resumed.automix.next.unwrap().path, c.to_string_lossy());
    assert!(resumed.automix.queue.is_empty());
    auto_action(&mut engine, MixerAction::AutomixStop);
    auto_action(&mut engine, MixerAction::AutomixStop);
    assert_eq!(engine.state().automix.queue.len(), 1);
    for path in [a, b, c] {
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn automix_uses_saved_entry_and_exit_points_without_restarting_playlist() {
    use djcraft_protocol::TrackPreparation;
    let a = write_wav("points-a", RATE, &vec![0.2; RATE as usize * 8]);
    let b = write_wav("points-b", RATE, &vec![0.3; RATE as usize * 8]);
    let (mut engine, mut mixer) = Engine::headless(RATE);
    for path in [&a, &b] {
        auto_action(
            &mut engine,
            MixerAction::SetTrackPreparation {
                path: path.to_string_lossy().into_owned(),
                preparation: TrackPreparation {
                    mix_in: Some(1.),
                    mix_out: Some(3.),
                    ..Default::default()
                },
            },
        );
    }
    auto_queue(&mut engine, &[a.clone(), b.clone()]);
    auto_action(&mut engine, MixerAction::AutomixFadeTime { seconds: 0.5 });
    auto_action(&mut engine, MixerAction::AutomixStart);
    let started = drive_auto_until(&mut engine, &mut mixer, |s| s.decks[0].playing);
    assert!(started.decks[0].position_seconds >= 1.);
    let transition = drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    assert!((transition.decks[0].position_seconds - 3.).abs() < 0.03);
    render(&mut mixer, 0.1);
    let state = engine.state();
    assert!((state.decks[1].position_seconds - 1.1).abs() < 0.03);
    render(&mut mixer, 0.5);
    let state = engine.state();
    assert!(!state.decks[0].playing && state.decks[1].playing);
    for path in [a, b] {
        std::fs::remove_file(path).unwrap();
    }
}
#[test]
fn preparation_validation_and_roles_do_not_disturb_playback() {
    use djcraft_protocol::{PhraseLabel, PhraseMarker, TrackPreparation};
    let (mut engine, mut mixer) = engine_with(sine(440., 2., RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    let mut p = TrackPreparation {
        mix_in: Some(2.),
        mix_out: Some(1.),
        ..Default::default()
    };
    let action = |p| Action::Mixer {
        action: MixerAction::SetTrackPreparation {
            path: "track.wav".into(),
            preparation: p,
        },
    };
    assert!(engine.apply(Role::Host, action(p.clone())).is_err());
    p.mix_out = Some(5.);
    p.transition_bars = Some(7);
    assert!(engine.apply(Role::Host, action(p.clone())).is_err());
    p.transition_bars = Some(16);
    p.bass_swap = Some(f32::NAN);
    assert!(engine.apply(Role::Host, action(p.clone())).is_err());
    p.bass_swap = Some(0.5);
    p.phrases = vec![PhraseMarker {
        seconds: f64::NAN,
        label: PhraseLabel::Intro,
    }];
    assert!(engine.apply(Role::Host, action(p.clone())).is_err());
    p.phrases.clear();
    assert!(engine.apply(Role::Viewer, action(p.clone())).is_err());
    engine.apply(Role::Host, action(p)).unwrap();
    render(&mut mixer, 0.1);
    assert!(engine.state().decks[0].playing);
}

fn phrase_session(name: &str) -> (Engine, Mixer, Vec<PathBuf>) {
    use djcraft_protocol::{BeatGrid, PhraseLabel, PhraseMarker, TrackPreparation};
    let paths = vec![
        write_wav(&format!("{name}-a"), RATE, &vec![0.2; RATE as usize * 30]),
        write_wav(&format!("{name}-b"), RATE, &vec![0.3; RATE as usize * 30]),
    ];
    let (mut engine, mixer) = Engine::headless(RATE);
    let grid = BeatGrid {
        bpm: 120.,
        first_beat: 0.,
        downbeat: 0,
        confidence: 1.,
    };
    for (i, path) in paths.iter().enumerate() {
        let prep = if i == 0 {
            TrackPreparation {
                phrases: vec![
                    PhraseMarker {
                        seconds: 0.,
                        label: PhraseLabel::Intro,
                    },
                    PhraseMarker {
                        seconds: 4.,
                        label: PhraseLabel::Outro,
                    },
                ],
                mix_out: Some(4.),
                transition_bars: Some(8),
                ..Default::default()
            }
        } else {
            TrackPreparation {
                phrases: vec![
                    PhraseMarker {
                        seconds: 2.,
                        label: PhraseLabel::Intro,
                    },
                    PhraseMarker {
                        seconds: 10.,
                        label: PhraseLabel::Drop,
                    },
                ],
                mix_in: Some(2.),
                ..Default::default()
            }
        };
        auto_action(
            &mut engine,
            MixerAction::SetTrackPreparation {
                path: path.to_string_lossy().into_owned(),
                preparation: prep,
            },
        );
        smart_queue(&mut engine, path, Some(grid));
    }
    auto_action(&mut engine, MixerAction::AutomixStart);
    (engine, mixer, paths)
}
#[test]
fn phrase_preview_arms_then_audio_starts_at_boundary_without_ui_polling() {
    let (mut engine, mut mixer, paths) = phrase_session("phrase-timing");
    let preview = drive_auto_until(&mut engine, &mut mixer, |s| {
        s.automix.plan.as_ref().is_some_and(|p| p.phrase_aligned)
    });
    let plan = preview.automix.plan.unwrap();
    assert_eq!(plan.bars, Some(8));
    assert_eq!(plan.outgoing_seconds, 4.);
    assert_eq!(plan.incoming_seconds, 2.);
    assert_eq!(plan.duration_seconds, 16.);
    assert!(!preview.decks[1].playing);
    let armed = drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    assert!(!armed.decks[1].playing && armed.decks[0].position_seconds < 4.);
    render(&mut mixer, 0.05);
    assert!(!engine.state().decks[1].playing);
    // No engine polling across the actual start boundary.
    render(&mut mixer, 0.4);
    let playing = engine.state();
    assert!(playing.decks[1].playing);
    assert!(
        (playing.decks[0].position_seconds - playing.decks[1].position_seconds - 2.).abs() < 0.002
    );
    assert!(playing.decks[1].eq[0] < 0.01);
    render(&mut mixer, (12. - playing.decks[0].position_seconds) as f32);
    let bass = engine.state();
    assert!((bass.decks[1].eq[0] - 0.5).abs() < 0.02);
    render(&mut mixer, 8.1);
    let completed = engine.state();
    assert_eq!(completed.automix.active_deck, Some(1));
    assert!(!completed.decks[0].playing);
    assert!(completed.automix.plan.is_none());
    for p in paths {
        std::fs::remove_file(p).unwrap();
    }
}
#[test]
fn takeover_cancels_armed_phrase_and_requeues_unplayed_track() {
    let (mut engine, mut mixer, paths) = phrase_session("phrase-cancel");
    drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    auto_action(&mut engine, MixerAction::AutomixStop);
    render(&mut mixer, 0.5);
    let stopped = engine.state();
    assert!(!stopped.decks[1].playing);
    assert!(stopped.decks[0].playing);
    assert_eq!(stopped.automix.queue.len(), 1);
    assert!(stopped.automix.plan.is_none());
    auto_action(&mut engine, MixerAction::AutomixStart);
    let next = drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    assert!(
        next.automix
            .plan
            .as_ref()
            .is_some_and(|p| !p.phrase_aligned)
    ); // saved point has passed
    render(&mut mixer, 0.1);
    assert!(engine.state().decks[1].playing);
    for p in paths {
        std::fs::remove_file(p).unwrap();
    }
}

#[test]
fn late_vocal_maps_refresh_preview_without_interrupting_transport() {
    let (mut engine, mut mixer, paths) = phrase_session("vocal-ready");
    let before = drive_auto_until(&mut engine, &mut mixer, |s| {
        s.automix.plan.as_ref().is_some_and(|p| p.phrase_aligned)
    });
    assert!(before.automix.plan.unwrap().vocal_overlap_seconds.is_none());
    for deck in 0..2 {
        let track = engine.track(deck).unwrap().clone();
        assert!(track.vocal_regions().is_none());
        let count = track.decoded_frames() * 2;
        track
            .attach_stems(std::array::from_fn(|_| vec![0.; count].into_boxed_slice()))
            .unwrap();
        assert!(track.set_vocal_regions(vec![[2., 1.]]).is_err());
        track.set_vocal_regions(vec![[0., 30.]]).unwrap();
    }
    let updated = engine.state();
    assert!(updated.automix.enabled && updated.decks[0].playing);
    assert!(!updated.decks[1].playing);
    assert_eq!(
        updated.automix.plan.unwrap().vocal_overlap_seconds,
        Some(16.)
    );
    for p in paths {
        std::fs::remove_file(p).unwrap();
    }
}

#[test]
fn loudness_trim_is_peak_limited_and_manual_override_survives_polling() {
    use djcraft_protocol::LoudnessInfo;
    let info = LoudnessInfo {
        rms_db: -30.,
        peak_db: -2.,
    };
    assert!((info.recommended_trim() - 1.9).abs() < 1e-5);
    assert_eq!(
        LoudnessInfo {
            rms_db: -60.,
            peak_db: -50.
        }
        .recommended_trim(),
        12.
    );
    let (mut engine, mut mixer) = engine_with(sine(440., 2., RATE));
    let track = engine.track(0).unwrap().clone();
    track.set_loudness(Some(info)).unwrap();
    let state = engine.state();
    assert!((state.decks[0].trim_db - 1.9).abs() < 1e-5);
    assert!(state.decks[0].loudness_applied);
    engine
        .apply(Role::Host, deck(DeckAction::SetTrim { db: 2. }))
        .unwrap();
    render(&mut mixer, 0.01);
    let state = engine.state();
    assert_eq!(state.decks[0].trim_db, 2.);
    assert!(!state.decks[0].auto_trim);
    engine
        .apply(Role::Host, deck(DeckAction::SetAutoTrim { enabled: true }))
        .unwrap();
    assert!((engine.state().decks[0].trim_db - 1.9).abs() < 1e-5);
}

#[test]
fn late_loudness_auto_trims_playing_decks_in_four_deck_layout() {
    let (mut engine, mut mixer) = Engine::headless(RATE);
    for deck in 0..4 {
        let track = Arc::new(Track::from_samples(
            format!("deck {deck}"),
            RATE,
            vec![0.4; RATE as usize * 2],
        ));
        engine.load_track(deck, track.clone()).unwrap();
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: deck as u8,
                    action: DeckAction::Play,
                },
            )
            .unwrap();
        render(&mut mixer, 0.01);
        assert_eq!(engine.state().decks[deck].trim_db, 0.);
        track
            .set_loudness(Some(djcraft_protocol::LoudnessInfo {
                rms_db: -10.,
                peak_db: -2.,
            }))
            .unwrap();
        let state = engine.state();
        assert!(state.decks[deck].auto_trim);
        assert!(state.decks[deck].loudness_applied);
        assert!((state.decks[deck].trim_db - 1.9).abs() < 1e-5);
    }
}

#[test]
fn auto_trim_normalizes_the_whole_track_peak_without_startup_clipping() {
    for peak in [0.5f32, 2., 8.] {
        let samples = (0..RATE).flat_map(|_| [peak, -peak]).collect();
        let track = Arc::new(Track::from_samples("peaks", RATE, samples));
        track
            .set_loudness(Some(djcraft_protocol::LoudnessInfo {
                rms_db: -30.,
                peak_db: 20. * (peak * 29. / 27.).log10(),
            }))
            .unwrap();
        let (mut engine, mut mixer) = Engine::headless(RATE);
        engine.load_track(0, track).unwrap();
        let trim = engine.state().decks[0].trim_db;
        assert!((trim - (-0.1 - 20. * (peak * 29. / 27.).log10())).abs() < 1e-5);
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        let out = render(&mut mixer, 1.1);
        let maximum = out.iter().map(|s| s.abs()).fold(0., f32::max);
        let expected = peak * 10f32.powf(trim / 20.);
        assert!(
            maximum < 1. && (maximum - expected).abs() < 1e-5,
            "peak {peak}, output {maximum}, expected {expected}"
        );
    }
}

#[test]
fn key_lock_peak_protection_bounds_processed_audio_at_changed_tempos() {
    let samples: Vec<f32> = (0..RATE * 3)
        .flat_map(|i| {
            let phase = (TAU * 123. * i as f32 / RATE as f32).sin();
            let sample = if phase >= 0. { 0.95 } else { -0.95 };
            [sample, -sample * 0.5]
        })
        .collect();
    let mut detected = false;
    for ratio in [0.92, 1.04, 1.25] {
        let (mut engine, mut mixer) = engine_with(samples.clone());
        engine
            .track(0)
            .unwrap()
            .set_loudness(Some(djcraft_protocol::LoudnessInfo {
                rms_db: -3.,
                peak_db: 20. * (0.95f32 * 1.125).log10(),
            }))
            .unwrap();
        engine.state();
        for action in [
            DeckAction::SetTempo { ratio },
            DeckAction::SetKeyLock { on: true },
            DeckAction::Play,
        ] {
            engine.apply(Role::Host, deck(action)).unwrap();
        }
        for _ in 0..300 {
            let mut block = [0.; CALLBACK * 2];
            mixer.render(&mut block);
            let state = engine.state();
            detected |= state.decks[0].key_lock_protection_db < -0.05;
            assert!(block.iter().all(|s| s.abs() < 1.));
            assert!(state.decks[0].peak < 1. && state.master_peak < 1.);
        }
    }
    assert!(detected, "Fixture must actually exercise gain reduction");
}
#[test]
fn late_loudness_applies_during_play_and_old_track_report_cannot_change_new_track() {
    use djcraft_protocol::LoudnessInfo;
    let (mut engine, mut mixer) = engine_with(sine(440., 3., RATE));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    render(&mut mixer, 0.1);
    let old = engine.track(0).unwrap().clone();
    old.set_loudness(Some(LoudnessInfo {
        rms_db: -10.,
        peak_db: 0.,
    }))
    .unwrap();
    let state = engine.state();
    assert!((state.decks[0].trim_db + 0.1).abs() < 1e-5);
    assert!(state.decks[0].loudness_applied);
    let new = Arc::new(Track::from_samples(
        "new",
        RATE,
        vec![0.; RATE as usize * 2],
    ));
    engine.load_track(0, new).unwrap();
    render(&mut mixer, 0.01);
    let state = engine.state();
    assert_eq!(state.decks[0].trim_db, 0.);
    assert!(state.decks[0].loudness.is_none());
    assert!(
        engine
            .apply(
                Role::Viewer,
                deck(DeckAction::SetAutoTrim { enabled: true })
            )
            .is_err()
    );
}
#[test]
fn automix_keeps_preloaded_loudness_trim_during_transition() {
    let (mut engine, mut mixer, paths) = phrase_session("auto-loudness");
    drive_auto_until(&mut engine, &mut mixer, |s| s.automix.plan.is_some());
    engine
        .track(1)
        .unwrap()
        .set_loudness(Some(djcraft_protocol::LoudnessInfo {
            rms_db: -12.,
            peak_db: -1.,
        }))
        .unwrap();
    assert!((engine.state().decks[1].trim_db - 0.9).abs() < 1e-5);
    auto_action(&mut engine, MixerAction::AutomixNext);
    drive_auto_until(&mut engine, &mut mixer, |s| s.automix.transitioning);
    render(&mut mixer, 0.2);
    let state = engine.state();
    assert!(state.decks[1].playing);
    assert!((state.decks[1].trim_db - 0.9).abs() < 1e-5);
    assert!(state.automix.enabled);
    for p in paths {
        std::fs::remove_file(p).unwrap();
    }
}

#[test]
fn cue_names_and_colours_validate_and_reset_on_track_load() {
    use djcraft_protocol::CueAppearance;
    let (mut engine, mut mixer) = engine_with(sine(220., 2., RATE));
    let style = CueAppearance {
        label: "First beat".into(),
        colour: Some(0xc8ff38),
    };
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetCueAppearance {
                slot: 0,
                appearance: style.clone(),
            }),
        )
        .unwrap();
    assert_eq!(engine.state().decks[0].cue_appearance[0], style);
    for (slot, appearance) in [
        (16, style.clone()),
        (
            0,
            CueAppearance {
                label: "x".repeat(49),
                ..style.clone()
            },
        ),
        (
            0,
            CueAppearance {
                colour: Some(0x1000000),
                ..style.clone()
            },
        ),
    ] {
        assert_eq!(
            engine.apply(
                Role::Host,
                deck(DeckAction::SetCueAppearance { slot, appearance })
            ),
            Err(Rejection::InvalidValue)
        );
    }
    render(&mut mixer, 0.01);
    engine
        .load_track(
            0,
            Arc::new(Track::from_samples("next", RATE, sine(440., 2., RATE))),
        )
        .unwrap();
    assert_eq!(
        engine.state().decks[0].cue_appearance[0],
        CueAppearance::default()
    );
}

#[test]
fn sampler_retrigger_overlaps_and_stop_silences_every_voice() {
    let (mut engine, mut mixer) = engine_with(vec![0.1; RATE as usize * 2]);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::CaptureSample {
                slot: 0,
                start: 0.,
                end: 0.5,
            }),
        )
        .unwrap();
    let trigger = |play, restart| {
        deck(DeckAction::Sample {
            slot: 0,
            play,
            restart,
            repeat: false,
            level: 1.,
        })
    };
    engine.apply(Role::Host, trigger(true, true)).unwrap();
    let first = render(&mut mixer, 0.05);
    let single = first[first.len() - 1];
    assert!(single > 0.05);
    engine.apply(Role::Host, trigger(true, true)).unwrap();
    let second = render(&mut mixer, 0.05);
    assert!((second[second.len() - 1] - 2. * single).abs() < 0.001);
    // Volume changes must not trigger another hit.
    engine.apply(Role::Host, trigger(true, false)).unwrap();
    let unchanged = render(&mut mixer, 0.01);
    assert!((unchanged[unchanged.len() - 1] - 2. * single).abs() < 0.001);
    engine.apply(Role::Host, trigger(false, false)).unwrap();
    let stopped = render(&mut mixer, 0.1);
    assert!(
        stopped[stopped.len() - 100..]
            .iter()
            .all(|v| v.abs() < 0.00001)
    );
    assert_eq!(engine.state().decks[0].samples[0], 1);
    // Rapid hits remain bounded and clearing removes every voice.
    for _ in 0..40 {
        engine.apply(Role::Host, trigger(true, true)).unwrap();
        render(&mut mixer, 0.001);
    }
    engine
        .apply(Role::Host, deck(DeckAction::ClearSample { slot: 0 }))
        .unwrap();
    assert!(render(&mut mixer, 0.01).iter().all(|v| *v == 0.));
}

#[test]
fn cue_preview_returns_silently_without_transport_drift() {
    for key_lock in [false, true] {
        let (mut engine, mut mixer) = engine_with(sine(440., 3., RATE));
        engine
            .apply(Role::Host, deck(DeckAction::SetKeyLock { on: key_lock }))
            .unwrap();
        for _ in 0..20 {
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::CuePreview {
                        seconds: 0.75,
                        play: true,
                    }),
                )
                .unwrap();
            render(&mut mixer, 0.02);
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::CuePreview {
                        seconds: 0.75,
                        play: false,
                    }),
                )
                .unwrap();
            let audio = render(&mut mixer, 0.01);
            let state = engine.state().decks[0].clone();
            assert!(!state.playing);
            assert!((state.position_seconds - 0.75).abs() < 1e-9);
            assert!(audio.iter().all(|s| s.abs() < 1e-6));
        }
    }
}

#[test]
fn repositioning_inside_loop_keeps_bounds_and_resumes_wrapping() {
    use djcraft_protocol::{HOT_CUES, HotCue};
    for method in 0..4 {
        let (mut engine, mut mixer) = engine_with(ramp(4.));
        let mut cues = [None; HOT_CUES];
        cues[0] = Some(HotCue {
            seconds: 1.75,
            loop_end: None,
        });
        engine
            .apply(Role::Host, deck(DeckAction::SetHotCues { cues }))
            .unwrap();
        engine
            .apply(Role::Host, deck(DeckAction::SetLoop { start: 1., end: 2. }))
            .unwrap();
        render(&mut mixer, 0.01);
        match method {
            0 => {
                engine
                    .apply(Role::Host, deck(DeckAction::Seek { seconds: 1.75 }))
                    .unwrap();
            }
            1 => {
                engine
                    .apply(Role::Host, deck(DeckAction::ScratchTo { seconds: 1.75 }))
                    .unwrap();
                render(&mut mixer, 0.01);
                assert_eq!(engine.state().decks[0].loop_region, Some([1., 2.]));
                engine
                    .apply(Role::Host, deck(DeckAction::Scratch { speed: None }))
                    .unwrap();
                engine
                    .apply(Role::Host, deck(DeckAction::Seek { seconds: 1.75 }))
                    .unwrap();
            }
            _ => {
                engine
                    .apply(
                        Role::Host,
                        deck(DeckAction::TriggerHotCue {
                            slot: 0,
                            play: method == 3,
                            loop_length: None,
                        }),
                    )
                    .unwrap();
            }
        }
        assert_eq!(engine.state().decks[0].loop_region, Some([1., 2.]));
        render(&mut mixer, 0.01);
        assert_eq!(engine.state().decks[0].playing, method == 3);
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        render(&mut mixer, 0.5);
        let state = engine.state().decks[0].clone();
        assert_eq!(state.loop_region, Some([1., 2.]));
        assert!(
            (1.2..1.3).contains(&state.position_seconds),
            "method {method}: {}",
            state.position_seconds
        );
    }
}

#[test]
fn outside_reposition_exits_loop_and_saved_loop_cue_replaces_it() {
    use djcraft_protocol::{HOT_CUES, HotCue};
    for action in [
        DeckAction::Seek { seconds: 0.5 },
        DeckAction::Seek { seconds: 2. },
        DeckAction::ScratchTo { seconds: 2.5 },
        DeckAction::TriggerHotCue {
            slot: 0,
            play: false,
            loop_length: None,
        },
    ] {
        let (mut engine, mut mixer) = engine_with(ramp(4.));
        let mut cues = [None; HOT_CUES];
        cues[0] = Some(HotCue {
            seconds: 2.5,
            loop_end: None,
        });
        cues[1] = Some(HotCue {
            seconds: 1.5,
            loop_end: Some(1.8),
        });
        engine
            .apply(Role::Host, deck(DeckAction::SetHotCues { cues }))
            .unwrap();
        engine
            .apply(Role::Host, deck(DeckAction::SetLoop { start: 1., end: 2. }))
            .unwrap();
        render(&mut mixer, 0.01);
        engine.apply(Role::Host, deck(action)).unwrap();
        render(&mut mixer, 0.01);
        assert_eq!(engine.state().decks[0].loop_region, None);
        engine
            .apply(Role::Host, deck(DeckAction::SetLoop { start: 1., end: 2. }))
            .unwrap();
        engine
            .apply(
                Role::Host,
                deck(DeckAction::TriggerHotCue {
                    slot: 1,
                    play: true,
                    loop_length: None,
                }),
            )
            .unwrap();
        render(&mut mixer, 0.5);
        let state = engine.state().decks[0].clone();
        assert_eq!(state.loop_region, Some([1.5, 1.8]));
        assert!((1.5..1.8).contains(&state.position_seconds));
    }
}

#[test]
fn sync_reaches_absolute_target_when_key_lock_changes_processing_mode() {
    for (initial, target) in [(1.0, 1.05), (1.05, 1.0), (0.95, 1.08)] {
        for offset in [-0.15, 0.15] {
            let (mut engine, mut mixer) = engine_with(sine(440., 12., RATE));
            engine
                .apply(Role::Host, deck(DeckAction::SetKeyLock { on: true }))
                .unwrap();
            engine
                .apply(Role::Host, deck(DeckAction::SetTempo { ratio: initial }))
                .unwrap();
            engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
            render(&mut mixer, 2.);
            let before = position(&mut engine);
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::Sync {
                        ratio: target,
                        offset_seconds: offset,
                    }),
                )
                .unwrap();
            render(&mut mixer, 4.);
            let expected = before + 4. * target + offset;
            assert!(
                (position(&mut engine) - expected).abs() < 0.003,
                "{initial} -> {target}, offset {offset}: actual {}, expected {expected}",
                position(&mut engine)
            );
        }
    }
}

#[test]
fn processed_peaks_preserve_track_trim_across_tempo_and_key_lock_changes() {
    use djcraft_protocol::LoudnessInfo;
    let track = Arc::new(Track::from_samples("processed", RATE, sine(440., 4., RATE)));
    track
        .set_loudness(Some(LoudnessInfo {
            rms_db: -10.,
            peak_db: -1.,
        }))
        .unwrap();
    let (mut engine, mut mixer) = Engine::headless(RATE);
    engine.load_track(0, track).unwrap();
    for action in [
        DeckAction::SetKeyLock { on: true },
        DeckAction::SetTempo { ratio: 1.04 },
        DeckAction::Play,
    ] {
        engine.apply(Role::Host, deck(action)).unwrap();
    }
    render(&mut mixer, 0.1);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetProcessedPeak {
                ratio: 1.04,
                peak_db: 5.,
            }),
        )
        .unwrap();
    assert!((engine.state().decks[0].trim_db - 0.9).abs() < 0.001);
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.08 }))
        .unwrap();
    assert!(
        engine
            .apply(
                Role::Host,
                deck(DeckAction::SetProcessedPeak {
                    ratio: 1.04,
                    peak_db: 8.
                })
            )
            .is_err()
    );
    engine
        .apply(Role::Host, deck(DeckAction::SetTrim { db: -3. }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetProcessedPeak {
                ratio: 1.08,
                peak_db: 8.,
            }),
        )
        .unwrap();
    assert_eq!(engine.state().decks[0].trim_db, -3.);
    engine
        .apply(Role::Host, deck(DeckAction::SetAutoTrim { enabled: true }))
        .unwrap();
    assert!((engine.state().decks[0].trim_db - 0.9).abs() < 0.001);
    for ratio in [1.0, 0.96, 1.04, 1.0] {
        engine
            .apply(Role::Host, deck(DeckAction::SetTempo { ratio }))
            .unwrap();
        render(&mut mixer, 0.1);
        assert!((engine.state().decks[0].trim_db - 0.9).abs() < 0.001);
    }
    engine
        .apply(Role::Host, deck(DeckAction::SetKeyLock { on: false }))
        .unwrap();
    assert!((engine.state().decks[0].trim_db - 0.9).abs() < 0.001);
}

#[test]
fn play_starts_at_full_speed_and_key_lock_has_no_empty_startup_buffers() {
    for ratio in [0.8, 1., 1.25] {
        for lock in [false, true] {
            for paused_preparation in [false, true] {
                let (mut engine, mut mixer) = engine_with(sine(440., 5., RATE));
                for action in [
                    DeckAction::SetTempo { ratio },
                    DeckAction::SetKeyLock { on: lock },
                    DeckAction::Seek { seconds: 1. },
                ] {
                    engine.apply(Role::Host, deck(action)).unwrap();
                }
                if paused_preparation {
                    render(&mut mixer, 0.1);
                }
                assert!(
                    (position(&mut engine) - if paused_preparation { 1. } else { 0. }).abs() < 1e-8
                );
                engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
                let mut block = [0.; 256];
                let started = Instant::now();
                mixer.render(&mut block);
                println!(
                    "128-frame start: lock={lock}, tempo={ratio}, prepared={paused_preparation}: {:?}",
                    started.elapsed()
                );
                let peak = block.iter().map(|s| s.abs()).fold(0., f32::max);
                assert!(
                    peak > 0.1,
                    "delayed start: ratio {ratio}, lock {lock}, prepared {paused_preparation}, peak {peak}"
                );
                let expected = 1. + 128. * ratio / RATE as f64;
                let tolerance = if lock && ratio != 1. { 0.002 } else { 0.00001 };
                assert!(
                    (position(&mut engine) - expected).abs() < tolerance,
                    "transport start drift: ratio {ratio}, lock {lock}, actual {} expected {expected}",
                    position(&mut engine)
                );
            }
        }
    }
}

#[test]
fn pause_keeps_brake_ramp_but_resume_reaches_full_speed_immediately() {
    for lock in [false, true] {
        let (mut engine, mut mixer) = engine_with(sine(440., 5., RATE));
        engine
            .apply(Role::Host, deck(DeckAction::SetKeyLock { on: lock }))
            .unwrap();
        engine
            .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.25 }))
            .unwrap();
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        render(&mut mixer, 0.5);
        engine.apply(Role::Host, deck(DeckAction::Pause)).unwrap();
        let brake = render(&mut mixer, 0.1);
        assert!(brake[..480].iter().any(|s| s.abs() > 0.1));
        assert!(brake[2400..].iter().all(|s| s.abs() < 1e-6));
        let stopped = position(&mut engine);
        render(&mut mixer, 0.1);
        assert_eq!(position(&mut engine), stopped);
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        let mut out = [0.; 256];
        mixer.render(&mut out);
        assert!(out.iter().any(|s| s.abs() > 0.1));
        assert!((position(&mut engine) - stopped - 128. * 1.25 / RATE as f64).abs() < 0.002);
    }
}

#[test]
fn key_lock_preroll_keeps_audio_at_the_cue_instead_of_skipping_ahead() {
    let mut samples = sine(440., 0.08, RATE);
    samples.extend(sine(1760., 3., RATE));
    let (mut engine, mut mixer) = engine_with(samples);
    engine
        .apply(Role::Host, deck(DeckAction::SetKeyLock { on: true }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.25 }))
        .unwrap();
    render(&mut mixer, 0.1);
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let onset = render(&mut mixer, 0.04);
    let pitch = frequency(&onset[128..]);
    assert!(
        (pitch - 440.).abs() < 25.,
        "cue audio was skipped: pitch {pitch}"
    );
}

#[test]
fn key_lock_preroll_does_not_cut_audio_from_the_track_end() {
    let mut samples = vec![0.; (RATE as f32 * 0.9) as usize * 2];
    samples.extend(sine(880., 0.1, RATE));
    let (mut engine, mut mixer) = engine_with(samples);
    engine
        .apply(Role::Host, deck(DeckAction::SetKeyLock { on: true }))
        .unwrap();
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.25 }))
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let audio = render(&mut mixer, 1.);
    let late = &audio[(0.75 * RATE as f32) as usize..(0.79 * RATE as f32) as usize];
    assert!(late.iter().any(|s| s.abs() > 0.1), "stretcher tail was cut");
    assert!(!engine.state().decks[0].playing);
}

#[test]
fn leaving_key_lock_reads_from_the_audible_head() {
    for action in [
        DeckAction::SetKeyLock { on: false },
        DeckAction::SetTempo { ratio: 1. },
    ] {
        let unity = matches!(action, DeckAction::SetTempo { .. });
        let (mut engine, mut mixer) = engine_with(sine(440., 5., RATE));
        engine
            .apply(Role::Host, deck(DeckAction::SetKeyLock { on: true }))
            .unwrap();
        engine
            .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.25 }))
            .unwrap();
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        render(&mut mixer, 1.);
        let before = position(&mut engine);
        engine.apply(Role::Host, deck(action)).unwrap();
        render(&mut mixer, 0.1);
        let advance = if unity { 0.1 } else { 0.125 };
        assert!((position(&mut engine) - before - advance).abs() < 0.002);
    }
}

#[test]
fn sampler_from_deck_two_starts_at_exact_capture_frame_with_short_attack() {
    let (mut engine, mut mixer) = Engine::headless(RATE);
    let mut samples = vec![0.; RATE as usize * 2];
    let first = (0.1375 * RATE as f64) as usize;
    samples[first * 2..].fill(0.4);
    engine
        .load_track(1, Arc::new(Track::from_samples("deck two", RATE, samples)))
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 1,
                action: DeckAction::CaptureSample {
                    slot: 0,
                    start: 0.1375,
                    end: 0.6375,
                },
            },
        )
        .unwrap();
    engine
        .apply(
            Role::Host,
            Action::Deck {
                deck: 1,
                action: DeckAction::Sample {
                    slot: 0,
                    play: true,
                    restart: true,
                    repeat: false,
                    level: 1.,
                },
            },
        )
        .unwrap();
    let mut out = [0.; 256];
    mixer.render(&mut out);
    assert!(
        (out[0] - 0.4 / 32.).abs() < 1e-6,
        "sample did not fire at the captured playhead: {}",
        out[0]
    );
    assert!(
        (out[62] - 0.4).abs() < 1e-6,
        "attack took longer than 32 frames"
    );
    assert!((out[254] - 0.4).abs() < 1e-6);
}

#[test]
fn silent_preroll_counts_at_tempo_pauses_and_crosses_into_audio() {
    for lock in [false, true] {
        let (mut engine, mut mixer) = engine_with(sine(440., 3., RATE));
        engine
            .apply(
                Role::Host,
                deck(DeckAction::SetBeatGrid {
                    grid: Some(djcraft_protocol::BeatGrid {
                        bpm: 120.,
                        first_beat: 0.1,
                        downbeat: 0,
                        confidence: 1.,
                    }),
                }),
            )
            .unwrap();
        engine
            .apply(Role::Host, deck(DeckAction::SetKeyLock { on: lock }))
            .unwrap();
        engine
            .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.25 }))
            .unwrap();
        engine
            .apply(Role::Host, deck(DeckAction::Seek { seconds: -100. }))
            .unwrap();
        let mut block = vec![1.; CALLBACK * 2];
        mixer.render(&mut block);
        assert_eq!(engine.state().decks[0].position_seconds, -8.);
        assert!(block.iter().all(|s| *s == 0.));
        engine
            .apply(Role::Host, deck(DeckAction::Seek { seconds: -2. }))
            .unwrap();
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        let mut silence = vec![1.; RATE as usize * 2];
        mixer.render(&mut silence);
        assert!(silence.iter().all(|s| *s == 0.));
        assert!((engine.state().decks[0].position_seconds + 0.75).abs() < 1e-6);
        engine.apply(Role::Host, deck(DeckAction::Pause)).unwrap();
        mixer.render(&mut block);
        assert!((engine.state().decks[0].position_seconds + 0.75).abs() < 1e-6);
        assert!(block.iter().all(|s| *s == 0.));
        engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
        let mut crossing = vec![0.; RATE as usize * 2];
        mixer.render(&mut crossing);
        assert!(
            crossing[..(RATE as usize * 6 / 10 - 1) * 2]
                .iter()
                .all(|s| *s == 0.)
        );
        assert!(
            crossing[RATE as usize * 7 / 10 * 2..]
                .iter()
                .any(|s| s.abs() > 0.01)
        );
        assert!((engine.state().decks[0].position_seconds - 0.5).abs() < 0.02);
        engine
            .apply(
                Role::Host,
                deck(DeckAction::CuePreview {
                    seconds: 0.,
                    play: false,
                }),
            )
            .unwrap();
        mixer.render(&mut block);
        assert_eq!(engine.state().decks[0].position_seconds, 0.);
        assert!(!engine.state().decks[0].playing);
    }
}

#[test]
fn waveform_scrubbing_can_enter_and_leave_negative_timeline() {
    let (mut engine, mut mixer) = engine_with(sine(440., 2., RATE));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetBeatGrid {
                grid: Some(djcraft_protocol::BeatGrid {
                    bpm: 120.,
                    first_beat: 0.2,
                    downbeat: 0,
                    confidence: 1.,
                }),
            }),
        )
        .unwrap();
    let mut block = vec![0.; CALLBACK * 2];
    for (target, expected) in [(-100., -8.), (-1., -1.), (0.5, 0.5), (-2., -2.)] {
        engine
            .apply(Role::Host, deck(DeckAction::ScratchTo { seconds: target }))
            .unwrap();
        mixer.render(&mut block);
        assert!((engine.state().decks[0].position_seconds - expected).abs() < 0.01);
        if target < 0. {
            assert!(block.iter().all(|s| *s == 0.));
        }
    }
    engine
        .apply(Role::Host, deck(DeckAction::Scratch { speed: None }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::CuePreview {
                seconds: -2.,
                play: true,
            }),
        )
        .unwrap();
    mixer.render(&mut block);
    assert!(engine.state().decks[0].position_seconds > -2.);
    assert!(block.iter().all(|s| *s == 0.));
    engine
        .apply(
            Role::Host,
            deck(DeckAction::CuePreview {
                seconds: -2.,
                play: false,
            }),
        )
        .unwrap();
    mixer.render(&mut block);
    assert_eq!(engine.state().decks[0].position_seconds, -2.);
}
#[test]
fn tempo_fader_range_is_per_deck_role_gated_and_preserves_playing_tempo() {
    use djcraft_protocol::TempoFaderRange;
    let (mut engine, mut mixer) = engine_with(sine(440., 2., RATE));
    engine
        .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.2 }))
        .unwrap();
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let mut block = vec![0.; CALLBACK * 2];
    mixer.render(&mut block);
    let action = deck(DeckAction::SetTempoRange {
        range: TempoFaderRange::Wide,
    });
    assert_eq!(
        engine.apply(Role::Viewer, action.clone()),
        Err(Rejection::NotPermitted)
    );
    assert_eq!(engine.state().decks[0].tempo_range, TempoFaderRange::Low);
    engine.apply(Role::CoDj, action).unwrap();
    mixer.render(&mut block);
    let state = engine.state();
    assert_eq!(state.decks[0].tempo_range, TempoFaderRange::Wide);
    assert_eq!(state.decks[0].tempo, 1.2);
    assert!(state.decks[0].playing);
    assert_eq!(state.decks[1].tempo_range, TempoFaderRange::Low);
}
#[test]
fn jog_audio_tracks_measured_speed_and_decays_only_when_the_physical_target_stops() {
    for speed in [-2., -1., -0.25, 0.25, 1., 2.] {
        let (mut engine, mut mixer) = engine_with(vec![0.4; RATE as usize * 16 * 2]);
        engine
            .apply(Role::Host, deck(DeckAction::Seek { seconds: 8. }))
            .unwrap();
        let mut target = 8.;
        let mut block = vec![0.; CALLBACK * 2];
        let mut start = 0.;
        let mut min_gain = 1f32;
        for packet in 0..140 {
            target += speed * 0.01;
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::JogScratch {
                        seconds: target,
                        speed,
                        interval_seconds: 0.01,
                    }),
                )
                .unwrap();
            mixer.render(&mut block);
            if packet == 79 {
                start = engine.state().decks[0].position_seconds;
            }
            if packet >= 80 {
                min_gain = block.iter().fold(min_gain, |a, v| a.min(v.abs()));
            }
        }
        let measured = (engine.state().decks[0].position_seconds - start) / 0.6;
        assert!(
            (measured - speed).abs() < 0.025,
            "platter {speed}, audio {measured}"
        );
        assert!(
            min_gain > 0.38,
            "moving platter pumped the volume: {min_gain}"
        );
        for _ in 0..40 {
            mixer.render(&mut block);
        }
        assert!((engine.state().decks[0].position_seconds - target).abs() < 1e-4);
        assert!(block.iter().all(|v| v.abs() < 0.001));
        for (bad_speed, bad_interval) in
            [(f64::NAN, 0.01), (1., 0.), (1., f64::INFINITY), (9., 0.01)]
        {
            assert_eq!(
                engine.apply(
                    Role::Host,
                    deck(DeckAction::JogScratch {
                        seconds: target,
                        speed: bad_speed,
                        interval_seconds: bad_interval
                    })
                ),
                Err(Rejection::InvalidValue)
            );
        }
    }
}
#[test]
fn jog_audio_follows_a_platter_slowing_through_multiple_speeds() {
    let (mut engine, mut mixer) = engine_with(vec![0.3; RATE as usize * 16 * 2]);
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 5. }))
        .unwrap();
    let mut target = 5.;
    let mut block = vec![0.; CALLBACK * 2];
    for speed in [2., 1., 0.5, 0.25] {
        let mut start = 0.;
        for packet in 0..100 {
            target += speed * 0.01;
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::JogScratch {
                        seconds: target,
                        speed,
                        interval_seconds: 0.01,
                    }),
                )
                .unwrap();
            mixer.render(&mut block);
            if packet == 69 {
                start = engine.state().decks[0].position_seconds;
            }
        }
        let actual = (engine.state().decks[0].position_seconds - start) / 0.3;
        assert!(
            (actual - speed).abs() < 0.025,
            "coasting platter {speed}, audio {actual}"
        );
    }
}

#[test]
fn filter_resonance_is_validated_and_isolated_per_slot_and_deck() {
    use djcraft_protocol::EffectState;
    let (mut engine, _mixer) = Engine::headless(RATE);
    for resonance in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        assert_eq!(
            engine.apply(
                Role::Host,
                deck(DeckAction::SetEffect {
                    slot: 0,
                    effect: EffectState {
                        resonance,
                        ..EffectState::default()
                    }
                })
            ),
            Err(Rejection::InvalidValue)
        );
    }
    let effect = EffectState {
        resonance: 0.8,
        ..EffectState::default()
    };
    engine
        .apply(Role::Host, deck(DeckAction::SetEffect { slot: 0, effect }))
        .unwrap();
    let state = engine.state();
    assert_eq!(state.decks[0].effects[0].resonance, 0.8);
    assert_eq!(state.decks[0].effects[1].resonance, 0.5);
    assert_eq!(state.decks[1].effects[0].resonance, 0.5);
}

#[test]
fn first_effect_slot_is_assignable_and_colour_filter_is_separate() {
    use djcraft_protocol::{ColourFxState, EffectKind, EffectState};
    let (mut engine, mut mixer) = engine_with(sine(1000., 3., RATE));
    assert!(!engine.state().decks[0].colour_fx.active());
    let effect = EffectState {
        kind: EffectKind::Reverb,
        enabled: true,
        ..Default::default()
    };
    engine
        .apply(Role::Host, deck(DeckAction::SetEffect { slot: 0, effect }))
        .unwrap();
    assert_eq!(engine.state().decks[0].effects[0], effect);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetColourFx {
                state: ColourFxState {
                    position: 0.1,
                    ..Default::default()
                },
            }),
        )
        .unwrap();
    assert!(engine.state().decks[0].colour_fx.active());
    assert_eq!(engine.state().decks[0].effects[0], effect);
    render(&mut mixer, 0.1);
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetColourFx {
                state: ColourFxState::default(),
            }),
        )
        .unwrap();
    assert!(!engine.state().decks[0].colour_fx.active());
    assert_eq!(engine.state().decks[0].effects[0], effect);
}
#[test]
fn eject_clears_loaded_track_and_stops_audio() {
    let (mut engine, mut mixer) = engine_with(vec![0.4; RATE as usize * 2]);
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    let mut output = vec![0.; CALLBACK * 2];
    mixer.render(&mut output);
    assert!(output.iter().any(|sample| sample.abs() > 0.01));
    engine.apply(Role::Host, deck(DeckAction::Unload)).unwrap();
    assert!(engine.track(0).is_none());
    assert!(engine.state().decks[0].track.is_none());
    mixer.render(&mut output);
    assert!(output.iter().all(|sample| sample.abs() < 1e-6));
    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
    mixer.render(&mut output);
    assert!(output.iter().all(|sample| sample.abs() < 1e-6));
}

fn sampler_action(action: djcraft_protocol::SamplerAction) -> Action {
    Action::Mixer {
        action: MixerAction::Sampler { action },
    }
}
fn wait_bank(engine: &mut Engine, mixer: &mut Mixer) -> djcraft_protocol::SamplerState {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = engine.state().sampler;
        mixer.render(&mut [0.; CALLBACK * 2]);
        if !state.loading.iter().any(|v| *v) {
            return engine.state().sampler;
        }
        assert!(Instant::now() < deadline, "bank load timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn named_sampler_banks_preserve_legacy_samples_isolate_writes_and_restore_selection() {
    use djcraft_protocol::SamplerAction as S;
    let root = std::env::temp_dir().join(format!(
        "fadercraft-bank-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (mut engine, mut mixer) = Engine::headless(RATE);
    engine.set_sampler_dir(root.clone());
    let original = wait_bank(&mut engine, &mut mixer);
    assert_eq!(original.names[0], "Kick");
    assert_eq!(original.selected_bank, "default");
    assert_eq!(
        engine.apply(
            Role::Viewer,
            sampler_action(S::CreateBank {
                name: "Drops".into()
            })
        ),
        Err(Rejection::NotPermitted)
    );
    engine
        .apply(
            Role::Host,
            sampler_action(S::CreateBank {
                name: "Drops".into(),
            }),
        )
        .unwrap();
    let empty = wait_bank(&mut engine, &mut mixer);
    let id = empty.selected_bank.clone();
    assert!(empty.slots.iter().all(|v| *v == 0));
    assert!(empty.names.iter().all(String::is_empty));
    assert_eq!(
        engine.apply(
            Role::Host,
            sampler_action(S::CreateBank {
                name: "drops".into()
            })
        ),
        Err(Rejection::InvalidValue)
    );
    let path = write_wav("bank-isolation", 24000, &vec![0.2; 2400]);
    engine
        .apply(
            Role::Host,
            sampler_action(S::Load {
                slot: 0,
                path: path.to_string_lossy().into_owned(),
            }),
        )
        .unwrap();
    // Switch before the import completes: old work must save to its original bank.
    engine
        .apply(
            Role::Host,
            sampler_action(S::SelectBank {
                id: "default".into(),
            }),
        )
        .unwrap();
    assert_eq!(wait_bank(&mut engine, &mut mixer).names[0], "Kick");
    engine
        .apply(Role::Host, sampler_action(S::SelectBank { id: id.clone() }))
        .unwrap();
    let loaded = wait_bank(&mut engine, &mut mixer);
    assert!(loaded.names[0].contains("bank-isolation"));
    assert_eq!(loaded.slots[0], 1);
    engine
        .apply(
            Role::Host,
            sampler_action(S::RenameBank {
                id: id.clone(),
                name: "Vocal Drops".into(),
            }),
        )
        .unwrap();
    engine
        .apply(Role::Host, sampler_action(S::Clear { slot: 0 }))
        .unwrap();
    assert_eq!(wait_bank(&mut engine, &mut mixer).slots[0], 0);
    drop(engine);
    drop(mixer);
    let (mut engine, mut mixer) = Engine::headless(RATE);
    engine.set_sampler_dir(root.clone());
    let restored = wait_bank(&mut engine, &mut mixer);
    assert_eq!(restored.selected_bank, id);
    assert!(restored.banks.iter().any(|b| b.name == "Vocal Drops"));
    assert_eq!(restored.slots[0], 0);
    drop(engine);
    drop(mixer);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn standalone_sampler_ignores_deck_faders_crossfader_and_transport() {
    use djcraft_protocol::SamplerAction as S;
    let (mut engine, mut mixer) = Engine::headless(RATE);
    let path = write_wav("sampler-bus", 24000, &vec![0.25; 2400]);
    engine
        .apply(
            Role::Host,
            sampler_action(S::Load {
                slot: 0,
                path: path.to_string_lossy().into_owned(),
            }),
        )
        .unwrap();
    wait_bank(&mut engine, &mut mixer);
    for deck in 0..4 {
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck,
                    action: DeckAction::SetVolume { level: 0. },
                },
            )
            .unwrap();
    }
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetCrossfader { position: 1. },
            },
        )
        .unwrap();
    engine
        .apply(
            Role::Host,
            sampler_action(S::Play {
                slot: 0,
                play: true,
                restart: true,
                repeat: true,
                level: 1.,
            }),
        )
        .unwrap();
    let mut block = [0.; CALLBACK * 2];
    for _ in 0..3 {
        mixer.render(&mut block);
    }
    let full: f32 = block.iter().map(|v| v.abs()).sum::<f32>() / block.len() as f32;
    assert!(full > 0.1, "sampler was affected by deck routing: {full}");
    engine
        .apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::SetSamplerGain { level: 0.5 },
            },
        )
        .unwrap();
    for _ in 0..3 {
        mixer.render(&mut block);
    }
    let half: f32 = block.iter().map(|v| v.abs()).sum::<f32>() / block.len() as f32;
    assert!((half / full - 0.5).abs() < 0.02);
    engine
        .apply(Role::Host, sampler_action(S::StopAll))
        .unwrap();
    for _ in 0..4 {
        mixer.render(&mut block);
    }
    assert!(block.iter().all(|v| v.abs() < 0.001));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn bundled_cc0_banks_decode_and_user_edits_survive_restart() {
    use djcraft_protocol::SamplerAction as S;
    let root = std::env::temp_dir().join(format!(
        "fadercraft-starter-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (mut engine, mut mixer) = Engine::headless(RATE);
    engine.set_sampler_dir(root.clone());
    let original = wait_bank(&mut engine, &mut mixer);
    assert_eq!(original.names[0], "Kick");
    let mut selected = String::new();
    for name in ["MS-20 Drums", "Synth FX"] {
        let id = original
            .banks
            .iter()
            .find(|b| b.name == name)
            .expect("missing starter bank")
            .id
            .clone();
        engine
            .apply(Role::Host, sampler_action(S::SelectBank { id: id.clone() }))
            .unwrap();
        let bank = wait_bank(&mut engine, &mut mixer);
        assert!(
            bank.errors.iter().all(Option::is_none),
            "{name}: {:?}",
            bank.errors
        );
        assert!(
            bank.slots.iter().all(|v| *v == 1),
            "{name}: {:?}",
            bank.slots
        );
        assert!(bank.names.iter().all(|n| !n.is_empty()));
        selected = id;
    }
    engine
        .apply(Role::Host, sampler_action(S::Clear { slot: 0 }))
        .unwrap();
    wait_bank(&mut engine, &mut mixer);
    drop(engine);
    drop(mixer);
    let (mut engine, mut mixer) = Engine::headless(RATE);
    engine.set_sampler_dir(root.clone());
    let restored = wait_bank(&mut engine, &mut mixer);
    assert_eq!(restored.selected_bank, selected);
    assert_eq!(restored.banks.len(), original.banks.len());
    assert_eq!(
        restored.slots[0], 0,
        "starter seeding overwrote a user's cleared slot"
    );
    assert!(root.join("CC0-1.0.txt").exists());
    drop(engine);
    drop(mixer);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn deck_stem_capture_freezes_selected_sources_and_survives_reload() {
    use djcraft_protocol::SamplerAction as S;
    for persisted in [false, true] {
        for gains in [[1., 0., 0., 0.], [1., 0., 0., 1.], [0.5, 0., 0., 1.]] {
            let root = std::env::temp_dir().join(format!(
                "fadercraft-stem-capture-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let (mut engine, mut mixer) = Engine::headless(RATE);
            if persisted {
                engine.set_sampler_dir(root.clone());
                wait_bank(&mut engine, &mut mixer);
            }
            let frames = RATE as usize;
            let stereo = |left, right| {
                (0..frames)
                    .flat_map(|_| [left, right])
                    .collect::<Vec<f32>>()
            };
            let source = Arc::new(Track::from_samples("Target", RATE, stereo(0.53, 0.70)));
            source
                .attach_stems([
                    stereo(0.08, 0.16).into_boxed_slice(),
                    stereo(0.1, 0.12).into_boxed_slice(),
                    stereo(0.15, 0.17).into_boxed_slice(),
                    stereo(0.2, 0.25).into_boxed_slice(),
                ])
                .unwrap();
            engine.load_track(2, source).unwrap();
            engine
                .load_track(
                    0,
                    Arc::new(Track::from_samples("Other deck", RATE, stereo(0.4, 0.4))),
                )
                .unwrap();
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: 0,
                        action: DeckAction::Play,
                    },
                )
                .unwrap();
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: 2,
                        action: DeckAction::SetStemGains { gains },
                    },
                )
                .unwrap();
            engine
                .apply(
                    Role::Host,
                    sampler_action(S::Capture {
                        deck: 2,
                        slot: 15,
                        start: 0.1,
                        end: 0.6,
                    }),
                )
                .unwrap();
            // Changing the deck while the request is queued must not change the capture.
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: 2,
                        action: DeckAction::SetStemGains { gains: [1.; 4] },
                    },
                )
                .unwrap();
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: 2,
                        action: DeckAction::Unload,
                    },
                )
                .unwrap();
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: 0,
                        action: DeckAction::Pause,
                    },
                )
                .unwrap();
            if persisted {
                wait_bank(&mut engine, &mut mixer);
            }
            let mut out = [0.; CALLBACK * 2];
            for _ in 0..100 {
                mixer.render(&mut out);
            }
            engine
                .apply(
                    Role::Host,
                    sampler_action(S::Play {
                        slot: 15,
                        play: true,
                        restart: true,
                        repeat: true,
                        level: 1.,
                    }),
                )
                .unwrap();
            for _ in 0..3 {
                mixer.render(&mut out);
            }
            let expected = [
                0.08 * gains[0] + 0.2 * gains[3],
                0.16 * gains[0] + 0.25 * gains[3],
            ];
            for frame in out.chunks_exact(2) {
                for c in 0..2 {
                    assert!(
                        (frame[c] - expected[c]).abs() < 0.0002,
                        "persisted={persisted}, gains={gains:?}, got={frame:?}, expected={expected:?}"
                    );
                }
            }
            assert!(engine.state().sampler.names[15].contains("Vocals"));
            drop(engine);
            drop(mixer);
            if persisted {
                let (mut engine, mut mixer) = Engine::headless(RATE);
                engine.set_sampler_dir(root.clone());
                wait_bank(&mut engine, &mut mixer);
                engine
                    .apply(
                        Role::Host,
                        sampler_action(S::Play {
                            slot: 15,
                            play: true,
                            restart: true,
                            repeat: true,
                            level: 1.,
                        }),
                    )
                    .unwrap();
                for _ in 0..3 {
                    mixer.render(&mut out);
                }
                assert!((out[100] - expected[0]).abs() < 0.0002);
                assert!((out[101] - expected[1]).abs() < 0.0002);
                drop(engine);
                drop(mixer);
                std::fs::remove_dir_all(root).unwrap();
            }
        }
    }
}
#[test]
fn sampler_capture_rejects_unprepared_silent_and_invalid_sources() {
    use djcraft_protocol::SamplerAction as S;
    let (mut engine, _mixer) = Engine::headless(RATE);
    let request = || {
        sampler_action(S::Capture {
            deck: 0,
            slot: 0,
            start: 0.,
            end: 0.2,
        })
    };
    assert_eq!(
        engine.apply(Role::Viewer, request()),
        Err(Rejection::NotPermitted)
    );
    assert_eq!(
        engine.apply(Role::Host, request()),
        Err(Rejection::InvalidValue)
    );
    let track = Arc::new(Track::from_samples(
        "plain",
        RATE,
        vec![0.1; RATE as usize * 2],
    ));
    engine.load_track(0, track.clone()).unwrap();
    assert_eq!(
        engine.apply(Role::Host, request()),
        Err(Rejection::InvalidValue)
    );
    track
        .attach_stems(std::array::from_fn(|_| {
            vec![0.025; RATE as usize * 2].into_boxed_slice()
        }))
        .unwrap();
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetStemGains { gains: [0.; 4] }),
        )
        .unwrap();
    assert_eq!(
        engine.apply(Role::Host, request()),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(
        engine.apply(
            Role::Host,
            sampler_action(S::Capture {
                deck: 4,
                slot: 0,
                start: 0.,
                end: 0.2
            })
        ),
        Err(Rejection::UnknownDeck)
    );
    engine
        .apply(
            Role::Host,
            deck(DeckAction::SetStemGains { gains: [1.; 4] }),
        )
        .unwrap();
    assert_eq!(
        engine.apply(
            Role::Host,
            sampler_action(S::Capture {
                deck: 0,
                slot: 16,
                start: 0.,
                end: 0.2
            })
        ),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(
        engine.apply(
            Role::Host,
            sampler_action(S::Capture {
                deck: 0,
                slot: 0,
                start: 0.,
                end: 2.
            })
        ),
        Err(Rejection::InvalidValue)
    );
}

#[test]
fn stationary_jog_holds_audio_head_without_target_catchup_and_releases_cleanly() {
    for playing in [false, true] {
        for key_lock in [false, true] {
            for velocity in [-4., 4.] {
                let (mut engine, mut mixer) = engine_with(vec![0.3; RATE as usize * 20 * 2]);
                engine
                    .apply(Role::Host, deck(DeckAction::Seek { seconds: 10. }))
                    .unwrap();
                engine
                    .apply(Role::Host, deck(DeckAction::SetKeyLock { on: key_lock }))
                    .unwrap();
                engine
                    .apply(Role::Host, deck(DeckAction::SetTempo { ratio: 1.08 }))
                    .unwrap();
                if playing {
                    engine.apply(Role::Host, deck(DeckAction::Play)).unwrap();
                }
                engine
                    .apply(
                        Role::Host,
                        deck(DeckAction::JogScratch {
                            seconds: 10. + velocity,
                            speed: velocity,
                            interval_seconds: 0.01,
                        }),
                    )
                    .unwrap();
                let mut out = [0.; CALLBACK * 2];
                for _ in 0..5 {
                    mixer.render(&mut out);
                }
                let before = engine.state().decks[0].position_seconds;
                engine.apply(Role::Host, deck(DeckAction::HoldJog)).unwrap();
                mixer.render(&mut out);
                let held = engine.state().decks[0].clone();
                assert!(held.scratching);
                assert_eq!(held.playing, playing);
                assert!((held.position_seconds - before).abs() < 1. / RATE as f64);
                for _ in 0..100 {
                    mixer.render(&mut out);
                }
                assert!(
                    (engine.state().decks[0].position_seconds - before).abs() < 1. / RATE as f64
                );
                assert!(out.iter().all(|v| v.abs() < 0.0001));
                engine
                    .apply(Role::Host, deck(DeckAction::Scratch { speed: None }))
                    .unwrap();
                for _ in 0..10 {
                    mixer.render(&mut out);
                }
                let released = engine.state().decks[0].clone();
                assert!(!released.scratching);
                if playing {
                    assert!(
                        released.position_seconds > before
                            && released.position_seconds < before + 0.12
                    );
                } else {
                    assert!((released.position_seconds - before).abs() < 1. / RATE as f64);
                }
            }
        }
    }
}

#[test]
fn long_jog_backlog_does_not_rapidly_catch_up_after_last_packet() {
    for sign in [-1., 1.] {
        let (mut engine, mut mixer) = engine_with(vec![0.3; RATE as usize * 60 * 2]);
        engine
            .apply(Role::Host, deck(DeckAction::Seek { seconds: 30. }))
            .unwrap();
        let mut target = 30.;
        let mut out = [0.; 480]; // 5 ms.
        for _ in 0..300 {
            target += sign * 0.06; // Travel faster than the bounded audio speed.
            engine
                .apply(
                    Role::Host,
                    deck(DeckAction::JogScratch {
                        seconds: target,
                        speed: sign * 4.,
                        interval_seconds: 0.005,
                    }),
                )
                .unwrap();
            mixer.render(&mut out);
        }
        // No controller idle/release action: the follower must stop on its own.
        for _ in 0..10 {
            mixer.render(&mut out);
        }
        let stopped = engine.state().decks[0].position_seconds;
        assert!(
            out.iter().all(|v| v.abs() < 0.001),
            "stale target kept audio moving after packets stopped: sign={sign}"
        );
        for _ in 0..80 {
            mixer.render(&mut out);
        }
        assert!(
            (engine.state().decks[0].position_seconds - stopped).abs() < 1. / RATE as f64,
            "audio rapidly chased old accumulated travel"
        );
    }
}
#[test]
fn jog_reversal_follows_new_velocity_even_when_old_target_is_far_ahead() {
    let (mut engine, mut mixer) = engine_with(vec![0.3; RATE as usize * 60 * 2]);
    engine
        .apply(Role::Host, deck(DeckAction::Seek { seconds: 10. }))
        .unwrap();
    let mut target = 10.;
    let mut out = [0.; 480];
    for _ in 0..200 {
        target += 0.06;
        engine
            .apply(
                Role::Host,
                deck(DeckAction::JogScratch {
                    seconds: target,
                    speed: 4.,
                    interval_seconds: 0.005,
                }),
            )
            .unwrap();
        mixer.render(&mut out);
    }
    let before = engine.state().decks[0].position_seconds;
    for _ in 0..10 {
        target -= 0.003;
        engine
            .apply(
                Role::Host,
                deck(DeckAction::JogScratch {
                    seconds: target,
                    speed: -0.6,
                    interval_seconds: 0.005,
                }),
            )
            .unwrap();
        mixer.render(&mut out);
    }
    assert!(
        engine.state().decks[0].position_seconds < before,
        "reversed platter still moved toward old forward target"
    );
}

#[test]
fn returning_tempo_to_zero_does_not_raise_audio_level_after_peak_report() {
    let (mut engine, mut mixer) = engine_with(sine(440., 12., RATE));
    engine
        .track(0)
        .unwrap()
        .set_loudness(Some(djcraft_protocol::LoudnessInfo {
            rms_db: -10.,
            peak_db: -1.,
        }))
        .unwrap();
    engine.state();
    for action in [
        DeckAction::SetKeyLock { on: true },
        DeckAction::SetTempo { ratio: 1.04 },
        DeckAction::Play,
        DeckAction::SetProcessedPeak {
            ratio: 1.04,
            peak_db: 5.,
        },
    ] {
        engine.apply(Role::Host, deck(action)).unwrap();
    }
    let mut levels = Vec::new();
    for ratio in [1.04, 1., 0.96, 1.] {
        engine
            .apply(Role::Host, deck(DeckAction::SetTempo { ratio }))
            .unwrap();
        engine.state();
        render(&mut mixer, 0.4);
        let mut energy = 0f64;
        let mut block = [0.; CALLBACK * 2];
        for _ in 0..50 {
            mixer.render(&mut block);
            energy += block.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
        }
        levels.push(energy.sqrt());
    }
    for pair in levels.windows(2) {
        let db = 20. * (pair[1] / pair[0]).log10();
        assert!(
            db.abs() < 0.5,
            "tempo transition changed level by {db} dB: {levels:?}"
        );
    }
}
