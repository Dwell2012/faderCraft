//! Generate an editable starter profile from the official MIDI addresses.
//! Jog calibration is provisional; LED/init and app-only browser commands remain pending.
use djcraft_controllers::*;

fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: mixtrack_profile <new-file.json> [exact MIDI port name]")?;
    let port = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "MixTrack Pro II".into());
    let mut profile = Profile {
        version: PROFILE_VERSION,
        name: "MixTrack Pro II starter (hardware validation pending)".into(),
        mappings: Vec::new(),
    };
    for deck in 0..2 {
        let d = usize::from(deck);
        for (name, note, press, release) in [
            ("sync", [0x40, 0x47][d], HostCommand::Sync { deck }, None),
            (
                "cue",
                [0x33, 0x3c][d],
                HostCommand::Cue {
                    deck,
                    pressed: true,
                },
                Some(HostCommand::Cue {
                    deck,
                    pressed: false,
                }),
            ),
            (
                "headphone-cue",
                [0x51, 0x52][d],
                HostCommand::ToggleHeadphoneCue { deck },
                None,
            ),
            (
                "stutter",
                [0x4a, 0x4c][d],
                HostCommand::Stutter { deck },
                None,
            ),
        ] {
            profile.mappings.push(Mapping {
                id: format!("{name}-{deck}"),
                port: port.clone(),
                source: Source::Note {
                    channel: 0,
                    number: note,
                },
                modifiers: vec![],
                binding: Binding::HostButton {
                    led: matches!(
                        &press,
                        HostCommand::SelectDeck { .. } | HostCommand::ToggleHeadphoneCue { .. }
                    )
                    .then_some(Source::Note {
                        channel: 0,
                        number: note,
                    }),
                    press,
                    release,
                },
            });
        }
        let shift = Source::Note {
            channel: 0,
            number: [0x61, 0x62][d],
        };
        for slot in 0..3 {
            profile.mappings.push(Mapping {
                id: format!("fx-parameter-{deck}-{slot}"),
                port: port.clone(),
                source: Source::Cc {
                    channel: 0,
                    number: 0x1b + deck * 3 + slot,
                },
                modifiers: vec![],
                binding: Binding::Parameter {
                    parameter: Parameter::EffectParameter { deck, slot },
                    min: 0.,
                    max: 1.,
                    center: None,
                    encoding: Encoding::TwosComplement,
                    step: 0.01,
                    pickup: false,
                },
            });
            profile.mappings.push(Mapping {
                id: format!("fx-{deck}-{slot}"),
                port: port.clone(),
                source: Source::Note {
                    channel: 0,
                    number: 0x59 + deck * 4 + slot,
                },
                modifiers: vec![],
                binding: Binding::ToggleEffect { deck, slot },
            });
        }
        for pad in 0..4 {
            for alternate in [false, true] {
                for (name, note, press) in [
                    (
                        "loop",
                        [[0x53, 0x54, 0x55, 0x63], [0x56, 0x57, 0x58, 0x64]][d][usize::from(pad)],
                        HostCommand::LoopPad {
                            deck,
                            pad,
                            alternate,
                        },
                    ),
                    (
                        "sample",
                        0x65 + deck * 4 + pad,
                        HostCommand::SamplePad {
                            deck,
                            slot: pad,
                            stop: alternate,
                        },
                    ),
                ] {
                    profile.mappings.push(Mapping {
                        id: format!("{name}-{deck}-{pad}-{alternate}"),
                        port: port.clone(),
                        source: Source::Note {
                            channel: 0,
                            number: note,
                        },
                        modifiers: vec![Modifier {
                            source: shift,
                            pressed: alternate,
                        }],
                        binding: Binding::HostButton {
                            press,
                            release: None,
                            led: None,
                        },
                    });
                }
            }
        }
        for (name, note, forward) in [
            ("pitch-minus", [0x43, 0x45][d], false),
            ("pitch-plus", [0x44, 0x46][d], true),
        ] {
            profile.mappings.push(Mapping {
                id: format!("{name}-{deck}"),
                port: port.clone(),
                source: Source::Note {
                    channel: 0,
                    number: note,
                },
                modifiers: vec![Modifier {
                    source: shift,
                    pressed: false,
                }],
                binding: Binding::Bend {
                    deck,
                    forward,
                    amount: 0.03,
                },
            });
        }
        profile.mappings.push(Mapping {
            id: format!("shift-key-lock-{deck}"),
            port: port.clone(),
            source: Source::Note {
                channel: 0,
                number: [0x43, 0x45][d],
            },
            modifiers: vec![Modifier {
                source: shift,
                pressed: true,
            }],
            binding: Binding::ToggleKeyLock { deck },
        });
        profile.mappings.push(Mapping {
            id: format!("jog-{deck}"),
            port: port.clone(),
            modifiers: vec![],
            source: Source::Cc {
                channel: 0,
                number: [0x19, 0x18][usize::from(deck)],
            },
            binding: Binding::Jog {
                config: JogConfig {
                    deck,
                    touch: Some(Source::Note {
                        channel: 0,
                        number: [0x4d, 0x4e][usize::from(deck)],
                    }),
                    scratch_toggle: Some(Source::Note {
                        channel: 0,
                        number: [0x48, 0x50][usize::from(deck)],
                    }),
                    scratch_led: Some(Source::Note {
                        channel: 0,
                        number: [0x48, 0x50][usize::from(deck)],
                    }),
                    encoding: Encoding::TwosComplement,
                    seconds_per_tick: 0.003,
                    bend_seconds_per_tick: 0.00025,
                    scratch_enabled: false,
                    inverted: false,
                },
            },
        });
        profile.mappings.push(Mapping {
            id: format!("play-{deck}"),
            port: port.clone(),
            source: Source::Note {
                channel: 0,
                number: [0x3b, 0x42][usize::from(deck)],
            },
            modifiers: vec![],
            binding: Binding::TogglePlay { deck },
        });
        for (name, cc, parameter, min, max, center) in [
            (
                "volume",
                [0x16, 0x07],
                Parameter::Volume { deck },
                0.,
                1.,
                None,
            ),
            (
                "pitch",
                [0x0d, 0x0e],
                Parameter::RangedTempo { deck },
                1.08,
                0.92,
                Some(1.),
            ),
            (
                "high",
                [0x10, 0x11],
                Parameter::Eq { deck, band: 2 },
                0.,
                2.,
                Some(1.),
            ),
            (
                "mid",
                [0x12, 0x13],
                Parameter::Eq { deck, band: 1 },
                0.,
                2.,
                Some(1.),
            ),
            (
                "low",
                [0x14, 0x15],
                Parameter::Eq { deck, band: 0 },
                0.,
                2.,
                Some(1.),
            ),
        ] {
            profile.mappings.push(Mapping {
                id: format!("{name}-{deck}"),
                port: port.clone(),
                source: Source::Cc {
                    channel: 0,
                    number: cc[usize::from(deck)],
                },
                modifiers: vec![],
                binding: Binding::Parameter {
                    parameter,
                    min,
                    max,
                    center,
                    encoding: Encoding::Absolute,
                    step: 0.01,
                    pickup: true,
                },
            });
        }
        for slot in 0..4 {
            profile.mappings.push(Mapping {
                id: format!("hotcue-{deck}-{slot}"),
                port: port.clone(),
                source: Source::Note {
                    channel: 0,
                    number: 0x6d + deck * 4 + slot,
                },
                modifiers: vec![],
                binding: Binding::HotCue { deck, slot },
            });
        }
    }
    for (name, cc, parameter, min, max) in [
        ("master", 0x17, Parameter::Master, 0., 1.),
        ("crossfader", 0x0a, Parameter::Crossfader, 1., 0.),
        ("headphone-level", 0x0b, Parameter::HeadphoneGain, 0., 1.),
        ("headphone-mix", 0x0c, Parameter::HeadphoneMix, 0., 1.),
    ] {
        profile.mappings.push(Mapping {
            id: name.into(),
            port: port.clone(),
            source: Source::Cc {
                channel: 0,
                number: cc,
            },
            modifiers: vec![],
            binding: Binding::Parameter {
                parameter,
                min,
                max,
                center: None,
                encoding: Encoding::Absolute,
                step: 0.01,
                pickup: true,
            },
        });
    }
    for deck in 0..2u8 {
        let shift = Source::Note {
            channel: 0,
            number: 0x61 + deck,
        };
        for m in &mut profile.mappings {
            if matches!(m.binding, Binding::ToggleEffect { deck: d, .. } if d == deck)
                || m.id == format!("hotcue-{deck}-3")
            {
                m.modifiers = vec![Modifier {
                    source: shift,
                    pressed: false,
                }];
            }
        }
        let mut add = |id: String, number, binding, modifiers| {
            profile.mappings.push(Mapping {
                id,
                port: port.clone(),
                source: Source::Note { channel: 0, number },
                binding,
                modifiers,
            })
        };
        for (slot, beats) in [1, 2, 4, 16].into_iter().enumerate() {
            add(
                format!("shift-auto-loop-{deck}-{beats}"),
                0x59 + deck * 4 + slot as u8,
                Binding::HostButton {
                    press: HostCommand::AutoLoop { deck, beats },
                    release: None,
                    led: None,
                },
                vec![Modifier {
                    source: shift,
                    pressed: true,
                }],
            );
        }
        add(
            format!("cue-delete-mode-{deck}"),
            0x70 + deck * 4,
            Binding::HotCueDeleteMode { deck },
            vec![Modifier {
                source: shift,
                pressed: true,
            }],
        );
        for (name, number, feedback) in [
            (
                "play",
                [0x3b, 0x42][usize::from(deck)],
                LedFeedback::Playing { deck },
            ),
            (
                "stutter",
                [0x4a, 0x4c][usize::from(deck)],
                LedFeedback::Playing { deck },
            ),
            (
                "cue",
                [0x33, 0x3c][usize::from(deck)],
                LedFeedback::Cue { deck },
            ),
            ("pitch", 0x28 + deck, LedFeedback::PitchCenter { deck }),
            (
                "sync",
                [0x40, 0x47][usize::from(deck)],
                LedFeedback::Pressed { deck },
            ),
        ] {
            add(
                format!("led-{name}-{deck}"),
                number,
                Binding::Led { feedback },
                vec![],
            );
        }
        for slot in 0..4u8 {
            for (name, number, feedback) in [
                (
                    "loop",
                    [[0x53, 0x54, 0x55, 0x63], [0x56, 0x57, 0x58, 0x64]][usize::from(deck)]
                        [usize::from(slot)],
                    if slot == 0 {
                        LedFeedback::LoopIn { deck }
                    } else {
                        LedFeedback::Loop { deck }
                    },
                ),
                (
                    "hotcue",
                    0x6d + deck * 4 + slot,
                    LedFeedback::HotCue { deck, slot },
                ),
                (
                    "sample",
                    0x65 + deck * 4 + slot,
                    LedFeedback::Sample { deck, slot },
                ),
            ] {
                add(
                    format!("led-{name}-{deck}-{slot}"),
                    number,
                    Binding::Led { feedback },
                    vec![],
                );
            }
        }
        for slot in 0..3u8 {
            add(
                format!("led-fx-{deck}-{slot}"),
                0x59 + deck * 4 + slot,
                Binding::Led {
                    feedback: LedFeedback::Effect { deck, slot },
                },
                vec![],
            );
        }
    }
    profile.mappings.push(Mapping {
        id: "browse".into(),
        port: port.clone(),
        source: Source::Cc {
            channel: 0,
            number: 0x1a,
        },
        modifiers: vec![],
        binding: Binding::Browse {
            encoding: Encoding::TwosComplement,
        },
    });
    for (id, number, press) in [
        ("browse-back", 0x77, HostCommand::BrowseBack),
        ("load-a", 0x4b, HostCommand::LoadSelected { deck: 0 }),
        ("load-b", 0x34, HostCommand::LoadSelected { deck: 1 }),
    ] {
        profile.mappings.push(Mapping {
            id: id.into(),
            port: port.clone(),
            source: Source::Note { channel: 0, number },
            modifiers: vec![],
            binding: Binding::HostButton {
                press,
                release: None,
                led: None,
            },
        });
    }
    for (id, number, folders) in [("led-file", 0x4b, false), ("led-folder", 0x34, true)] {
        profile.mappings.push(Mapping {
            id: id.into(),
            port: port.clone(),
            source: Source::Note { channel: 0, number },
            modifiers: vec![],
            binding: Binding::BrowserLed { folders },
        });
    }
    for deck in 0..2u8 {
        profile.mappings.push(Mapping {
            id: format!("pitch-range-{deck}"),
            port: port.clone(),
            source: Source::Note {
                channel: 0,
                number: [0x44, 0x46][usize::from(deck)],
            },
            modifiers: vec![Modifier {
                source: Source::Note {
                    channel: 0,
                    number: 0x61 + deck,
                },
                pressed: true,
            }],
            binding: Binding::CycleTempoRange { deck },
        });
    }
    for deck in 0..2u8 {
        profile.mappings.push(Mapping {
            id: format!("fx-beats-{deck}"),
            port: port.clone(),
            source: Source::Cc {
                channel: 0,
                number: 0x21 + deck,
            },
            modifiers: vec![],
            binding: Binding::EffectBeats {
                deck,
                encoding: Encoding::TwosComplement,
            },
        });
    }
    let json = profile.to_json()?;
    // Creating a starter must never overwrite someone's custom map.
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())
}
