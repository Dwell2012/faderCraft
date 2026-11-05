//! Small compiled control language. Execution produces ordinary protocol actions.
use crate::{Action, DECKS, DeckAction, MixerAction, SessionState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlSource {
    Skin,
    Custom,
    Controller,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dispatch {
    pub source: ControlSource,
    pub action: Action,
}

/// The host supplies relative deck selection at invocation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Deck(u8),
    Active,
    Inactive,
    Mixer,
}

#[derive(Clone, Copy, Debug)]
pub struct InvocationContext<'a> {
    pub source: ControlSource,
    pub state: Option<&'a SessionState>,
    pub active_deck: Option<u8>,
    /// Number of decks shown by the active layout (2 or 4).
    pub visible_decks: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqBand {
    Low,
    Mid,
    High,
}

impl EqBand {
    fn engine_index(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Mid => 1,
            Self::High => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    Eq(EqBand),
    Volume,
    RhythmGain,
    Trim,
    Tempo,
    ColourFx,
    Eq4,
    MasterGain,
    BoothGain,
    Crossfader,
    CrossfaderSharpness,
    SamplerGain,
    HeadphoneGain,
    HeadphoneMix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamDomain {
    Unipolar,
    Bipolar,
}

impl Param {
    pub fn domain(self) -> ParamDomain {
        match self {
            Self::Volume
            | Self::RhythmGain
            | Self::MasterGain
            | Self::BoothGain
            | Self::Crossfader
            | Self::CrossfaderSharpness
            | Self::SamplerGain
            | Self::HeadphoneGain
            | Self::HeadphoneMix => ParamDomain::Unipolar,
            Self::Eq(_) | Self::Eq4 | Self::Trim | Self::Tempo | Self::ColourFx => {
                ParamDomain::Bipolar
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    Play,
    Pause,
    TogglePlay,
    Unload,
    KeyLockOn,
    KeyLockOff,
    ToggleKeyLock,
    QuantizeOn,
    QuantizeOff,
    ToggleQuantize,
    ToggleSlip,
    HotCue(u8),
    ReleaseHotCue,
    ToggleColourFxSelection(u8),
    ToggleCrossfaderLeft,
    ToggleCrossfaderRight,
    ToggleHeadphoneCue,
    SetEqMode(u8),
    CrossfaderSmooth,
    CrossfaderEqualPower,
    CrossfaderCut,
    ToggleCrossfaderPower,
    ToggleCrossfaderReverse,
    ToggleCrossfaderDisabled,
    CycleTempoRange,
    ToggleRhythmShift,
    RhythmShiftOn,
    RhythmShiftOff,
    RhythmRoll(u8),
}

impl ButtonAction {
    fn is_mixer(self) -> bool {
        matches!(
            self,
            Self::CrossfaderSmooth
                | Self::CrossfaderEqualPower
                | Self::CrossfaderCut
                | Self::ToggleCrossfaderPower
                | Self::ToggleCrossfaderReverse
                | Self::ToggleCrossfaderDisabled
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Input,
    Fixed(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Program {
    /// Original 0–1 EQ position syntax, retained for existing bindings.
    LegacyEq {
        target: Target,
        band: EqBand,
    },
    SetParam {
        target: Target,
        param: Param,
        value: Value,
    },
    DoAction {
        target: Target,
        action: ButtonAction,
    },
    SetCurve {
        curve: crate::CrossfaderCurve,
    },
    SetRouting {
        routing: crate::CrossfaderRouting,
    },
}

fn deck_id(name: &str) -> Result<Target, &'static str> {
    match name {
        "deck_a" | "A" | "a" => Ok(Target::Deck(0)),
        "deck_b" | "B" | "b" => Ok(Target::Deck(1)),
        "deck_c" | "C" | "c" => Ok(Target::Deck(2)),
        "deck_d" | "D" | "d" => Ok(Target::Deck(3)),
        "active" => Ok(Target::Active),
        "inactive" => Ok(Target::Inactive),
        "mixer" => Ok(Target::Mixer),
        _ => Err("unknown deck target"),
    }
}

fn quoted(word: &str) -> Result<&str, &'static str> {
    word.strip_prefix('"')
        .and_then(|word| word.strip_suffix('"'))
        .filter(|word| !word.contains('"'))
        .ok_or("expected a quoted name")
}

fn args<'a>(source: &'a str, name: &str, count: usize) -> Result<Vec<&'a str>, &'static str> {
    let inside = source
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or("invalid function call")?;
    let parts: Vec<_> = inside.split(',').map(str::trim).collect();
    if parts.len() != count || parts.iter().any(|part| part.is_empty()) {
        return Err("wrong number of arguments");
    }
    Ok(parts)
}

impl Program {
    pub fn compile(source: &str) -> Result<Self, &'static str> {
        let source = source.trim();
        if source.starts_with("set_param(") {
            let parts = args(source, "set_param", 3)?;
            let target = deck_id(quoted(parts[0])?)?;
            let param = match quoted(parts[1])? {
                "eq_low" => Param::Eq(EqBand::Low),
                "eq_mid" => Param::Eq(EqBand::Mid),
                "eq_high" => Param::Eq(EqBand::High),
                "volume" => Param::Volume,
                "rhythm_gain" => Param::RhythmGain,
                "trim" => Param::Trim,
                "tempo" => Param::Tempo,
                "cfx" => Param::ColourFx,
                "eq_4" => Param::Eq4,
                "master_gain" => Param::MasterGain,
                "booth_gain" => Param::BoothGain,
                "crossfader" => Param::Crossfader,
                "crossfader_sharpness" => Param::CrossfaderSharpness,
                "sampler_gain" => Param::SamplerGain,
                "headphone_gain" => Param::HeadphoneGain,
                "headphone_mix" => Param::HeadphoneMix,
                _ => return Err("unknown parameter"),
            };
            if (target == Target::Mixer) != param.is_mixer() {
                return Err("parameter target does not match its scope");
            }
            let value = if parts[2] == "$value" {
                Value::Input
            } else {
                Value::Fixed(parts[2].parse().map_err(|_| "invalid numeric value")?)
            };
            return Ok(Self::SetParam {
                target,
                param,
                value,
            });
        }
        if source.starts_with("do_action(") {
            let parts = args(source, "do_action", 2)?;
            let target = deck_id(quoted(parts[0])?)?;
            let action = match quoted(parts[1])? {
                "play" => ButtonAction::Play,
                "pause" => ButtonAction::Pause,
                "toggle_play" => ButtonAction::TogglePlay,
                "unload" => ButtonAction::Unload,
                "key_lock_on" => ButtonAction::KeyLockOn,
                "key_lock_off" => ButtonAction::KeyLockOff,
                "toggle_key_lock" => ButtonAction::ToggleKeyLock,
                "quantize_on" => ButtonAction::QuantizeOn,
                "quantize_off" => ButtonAction::QuantizeOff,
                "toggle_quantize" => ButtonAction::ToggleQuantize,
                "toggle_slip" => ButtonAction::ToggleSlip,
                "release_hot_cue" => ButtonAction::ReleaseHotCue,
                "toggle_crossfader_left" => ButtonAction::ToggleCrossfaderLeft,
                "toggle_crossfader_right" => ButtonAction::ToggleCrossfaderRight,
                "toggle_headphone_cue" => ButtonAction::ToggleHeadphoneCue,
                "eq_classic" => ButtonAction::SetEqMode(0),
                "eq_ezremix" => ButtonAction::SetEqMode(1),
                "eq_modern" => ButtonAction::SetEqMode(2),
                "eq_stems" => ButtonAction::SetEqMode(3),
                "crossfader_smooth" => ButtonAction::CrossfaderSmooth,
                "crossfader_equal_power" => ButtonAction::CrossfaderEqualPower,
                "crossfader_cut" => ButtonAction::CrossfaderCut,
                "toggle_crossfader_power" => ButtonAction::ToggleCrossfaderPower,
                "toggle_crossfader_reverse" => ButtonAction::ToggleCrossfaderReverse,
                "toggle_crossfader_disabled" => ButtonAction::ToggleCrossfaderDisabled,
                "toggle_cfx_1" => ButtonAction::ToggleColourFxSelection(0),
                "toggle_cfx_2" => ButtonAction::ToggleColourFxSelection(1),
                "toggle_cfx_3" => ButtonAction::ToggleColourFxSelection(2),
                "toggle_cfx_4" => ButtonAction::ToggleColourFxSelection(3),
                "toggle_cfx_5" => ButtonAction::ToggleColourFxSelection(4),
                "toggle_cfx_6" => ButtonAction::ToggleColourFxSelection(5),
                "toggle_cfx_7" => ButtonAction::ToggleColourFxSelection(6),
                "cycle_tempo_range" => ButtonAction::CycleTempoRange,
                "toggle_rhythm_shift" => ButtonAction::ToggleRhythmShift,
                "rhythm_shift_on" => ButtonAction::RhythmShiftOn,
                "rhythm_shift_off" => ButtonAction::RhythmShiftOff,
                "rhythm_roll_1" => ButtonAction::RhythmRoll(0),
                "rhythm_roll_2" => ButtonAction::RhythmRoll(1),
                "rhythm_roll_3" => ButtonAction::RhythmRoll(2),
                "rhythm_roll_4" => ButtonAction::RhythmRoll(3),
                name if name.starts_with("hot_cue_") => {
                    let slot: u8 = name[8..].parse().map_err(|_| "invalid hot cue slot")?;
                    if !(1..=16).contains(&slot) {
                        return Err("invalid hot cue slot");
                    }
                    ButtonAction::HotCue(slot - 1)
                }
                _ => return Err("unknown action"),
            };
            let mixer_action = matches!(
                action,
                ButtonAction::CrossfaderSmooth
                    | ButtonAction::CrossfaderEqualPower
                    | ButtonAction::CrossfaderCut
                    | ButtonAction::ToggleCrossfaderPower
                    | ButtonAction::ToggleCrossfaderReverse
                    | ButtonAction::ToggleCrossfaderDisabled
            );
            if (target == Target::Mixer) != mixer_action {
                return Err("deck action requires a deck target");
            }
            return Ok(Self::DoAction { target, action });
        }
        // Preserve the initial spelling while mixer mappings migrate.
        let mut words = source.split_whitespace();
        if words.next() != Some("set_eq") {
            return Err("unknown command");
        }
        let target = deck_id(words.next().ok_or("missing deck")?)?;
        let band = match words.next() {
            Some("low") => EqBand::Low,
            Some("mid") => EqBand::Mid,
            Some("high") => EqBand::High,
            _ => return Err("expected low, mid or high EQ band"),
        };
        if words.next() != Some("$value") || words.next().is_some() {
            return Err("expected $value and no other arguments");
        }
        Ok(Self::LegacyEq { target, band })
    }

    pub fn set_param(deck: usize, param: Param) -> Result<Self, &'static str> {
        if deck >= DECKS || param.is_mixer() {
            return Err("unknown deck");
        }
        Ok(Self::SetParam {
            target: Target::Deck(deck as u8),
            param,
            value: Value::Input,
        })
    }

    pub fn set_curve(curve: crate::CrossfaderCurve) -> Self {
        Self::SetCurve { curve }
    }

    pub fn set_routing(routing: crate::CrossfaderRouting) -> Self {
        Self::SetRouting { routing }
    }

    pub fn set_mixer_param(param: Param) -> Result<Self, &'static str> {
        if !param.is_mixer() {
            return Err("expected mixer parameter");
        }
        Ok(Self::SetParam {
            target: Target::Mixer,
            param,
            value: Value::Input,
        })
    }

    pub fn set_eq(deck: usize, band: EqBand) -> Result<Self, &'static str> {
        if deck >= DECKS {
            return Err("unknown deck");
        }
        Ok(Self::LegacyEq {
            target: Target::Deck(deck as u8),
            band,
        })
    }

    pub fn do_action(deck: usize, action: ButtonAction) -> Result<Self, &'static str> {
        if deck >= DECKS {
            return Err("unknown deck");
        }
        Ok(Self::DoAction {
            target: Target::Deck(deck as u8),
            action,
        })
    }

    /// Controller profiles may bind absolute CCs only to parameter programs.
    pub fn is_parameter(self) -> bool {
        matches!(self, Self::SetParam { .. } | Self::LegacyEq { .. })
    }

    pub fn uses_relative_target(self) -> bool {
        matches!(self.target(), Target::Active | Target::Inactive)
    }

    pub fn input_domain(self) -> ParamDomain {
        match self {
            Self::SetParam { param, .. } => param.domain(),
            Self::LegacyEq { .. }
            | Self::DoAction { .. }
            | Self::SetCurve { .. }
            | Self::SetRouting { .. } => ParamDomain::Unipolar,
        }
    }

    /// Resolve a relative target when the control is invoked. `inactive`
    /// dispatches once to every visible deck except the active deck.
    pub fn run_in(
        self,
        context: InvocationContext<'_>,
        input: f32,
    ) -> Result<Vec<Dispatch>, &'static str> {
        if context.visible_decks == 0 || context.visible_decks > DECKS {
            return Err("invalid visible deck count");
        }
        let target = self.target();
        let active = context.active_deck.map(usize::from);
        if matches!(target, Target::Active | Target::Inactive)
            && !active.is_some_and(|deck| deck < context.visible_decks)
        {
            return Err("active deck is not visible");
        }
        let decks: Vec<u8> = match target {
            Target::Deck(deck) => vec![deck],
            Target::Active => vec![active.unwrap() as u8],
            Target::Inactive => (0..context.visible_decks)
                .filter(|deck| Some(*deck) != active)
                .map(|deck| deck as u8)
                .collect(),
            Target::Mixer => {
                return Ok(vec![self.run_with_state(
                    context.source,
                    input,
                    context.state,
                )?]);
            }
        };
        decks
            .into_iter()
            .map(|deck| {
                self.with_deck(deck)
                    .run_with_state(context.source, input, context.state)
            })
            .collect()
    }

    fn target(self) -> Target {
        match self {
            Self::LegacyEq { target, .. }
            | Self::SetParam { target, .. }
            | Self::DoAction { target, .. } => target,
            Self::SetCurve { .. } | Self::SetRouting { .. } => Target::Mixer,
        }
    }

    fn with_deck(self, deck: u8) -> Self {
        let target = Target::Deck(deck);
        match self {
            Self::LegacyEq { band, .. } => Self::LegacyEq { target, band },
            Self::SetParam { param, value, .. } => Self::SetParam {
                target,
                param,
                value,
            },
            Self::DoAction { action, .. } => Self::DoAction { target, action },
            Self::SetCurve { .. } | Self::SetRouting { .. } => self,
        }
    }

    pub fn run(self, source: ControlSource, input: f32) -> Result<Dispatch, &'static str> {
        self.run_with_state(source, input, None)
    }

    pub fn run_with_state(
        self,
        source: ControlSource,
        input: f32,
        state: Option<&SessionState>,
    ) -> Result<Dispatch, &'static str> {
        if let Self::SetCurve { curve } = self {
            return Ok(Dispatch {
                source,
                action: Action::Mixer {
                    action: MixerAction::SetCrossfaderCurve { curve },
                },
            });
        }
        if let Self::SetRouting { routing } = self {
            return Ok(Dispatch {
                source,
                action: Action::Mixer {
                    action: MixerAction::SetCrossfaderRouting { routing },
                },
            });
        }
        let target = self.target();
        match self {
            Self::SetParam { param, .. } if (target == Target::Mixer) != param.is_mixer() => {
                return Err("parameter target does not match its scope");
            }
            Self::LegacyEq { .. } if target == Target::Mixer => {
                return Err("deck action requires a deck target");
            }
            Self::DoAction { action, .. } if (target == Target::Mixer) != action.is_mixer() => {
                return Err("action target does not match its scope");
            }
            _ => {}
        }
        let deck = match target {
            Target::Deck(deck) => deck,
            Target::Mixer => 0,
            Target::Active | Target::Inactive => {
                return Err("relative target requires invocation context");
            }
        };
        if let Self::DoAction { action, .. } = self
            && action.is_mixer()
        {
            let curve = match action {
                ButtonAction::CrossfaderSmooth => crate::CrossfaderCurve::Smooth,
                ButtonAction::CrossfaderEqualPower => crate::CrossfaderCurve::EqualPower,
                ButtonAction::CrossfaderCut => crate::CrossfaderCurve::Cut,
                _ => {
                    let current = state
                        .ok_or("crossfader curve requires session state")?
                        .crossfader_curve;
                    let (sharpness, mut power, mut reverse, mut disabled) = current.parameters();
                    match action {
                        ButtonAction::ToggleCrossfaderPower => power = !power,
                        ButtonAction::ToggleCrossfaderReverse => reverse = !reverse,
                        ButtonAction::ToggleCrossfaderDisabled => disabled = !disabled,
                        _ => unreachable!(),
                    }
                    crate::CrossfaderCurve::Adjustable {
                        sharpness,
                        power,
                        reverse,
                        disabled,
                    }
                }
            };
            return Ok(Dispatch {
                source,
                action: Action::Mixer {
                    action: MixerAction::SetCrossfaderCurve { curve },
                },
            });
        }
        if let Self::DoAction {
            action: ButtonAction::ToggleCrossfaderLeft | ButtonAction::ToggleCrossfaderRight,
            ..
        } = self
        {
            let state = state.ok_or("crossfader routing requires session state")?;
            let mut routing = state.crossfader_routing;
            let bit = 1u8 << deck;
            if matches!(
                self,
                Self::DoAction {
                    action: ButtonAction::ToggleCrossfaderLeft,
                    ..
                }
            ) {
                routing.left ^= bit;
            } else {
                routing.right ^= bit;
            }
            return Ok(Dispatch {
                source,
                action: Action::Mixer {
                    action: MixerAction::SetCrossfaderRouting { routing },
                },
            });
        }
        if let Self::DoAction {
            action: ButtonAction::ToggleHeadphoneCue,
            ..
        } = self
        {
            let state = state.ok_or("headphone cue requires session state")?;
            let mut monitor = state.monitor;
            monitor.cue_decks = Some(monitor.cue_mask() ^ (1 << deck));
            return Ok(Dispatch {
                source,
                action: Action::Mixer {
                    action: MixerAction::SetMonitor { monitor },
                },
            });
        }
        let action = match self {
            Self::LegacyEq { band, .. } => {
                if !input.is_finite() || !(0.0..=1.0).contains(&input) {
                    return Err("legacy EQ position must be between 0 and 1");
                }
                return Self::SetParam {
                    target,
                    param: Param::Eq(band),
                    value: Value::Fixed(input * 2.0 - 1.0),
                }
                .run_with_state(source, input, state);
            }
            Self::SetParam { param, value, .. } => {
                let normalized = match value {
                    Value::Input => input,
                    Value::Fixed(value) => value,
                };
                let valid = normalized.is_finite()
                    && match param.domain() {
                        ParamDomain::Unipolar => (0.0..=1.0).contains(&normalized),
                        ParamDomain::Bipolar => (-1.0..=1.0).contains(&normalized),
                    };
                if !valid {
                    return Err("parameter value is outside its normalized domain");
                }
                if param.is_mixer() {
                    let mixer = match param {
                        Param::MasterGain => MixerAction::SetMasterGain {
                            level: normalized * normalized,
                        },
                        Param::BoothGain => MixerAction::SetBoothGain {
                            level: normalized * normalized,
                        },
                        Param::Crossfader => MixerAction::SetCrossfader {
                            position: normalized,
                        },
                        Param::CrossfaderSharpness => {
                            let current = state
                                .ok_or("crossfader sharpness requires session state")?
                                .crossfader_curve;
                            let (_, power, reverse, disabled) = current.parameters();
                            MixerAction::SetCrossfaderCurve {
                                curve: crate::CrossfaderCurve::Adjustable {
                                    sharpness: (normalized * 100.).round() as u8,
                                    power,
                                    reverse,
                                    disabled,
                                },
                            }
                        }
                        Param::SamplerGain => MixerAction::SetSamplerGain {
                            level: normalized * normalized,
                        },
                        Param::HeadphoneGain | Param::HeadphoneMix => {
                            let mut monitor = state
                                .ok_or("monitor parameter requires session state")?
                                .monitor;
                            if param == Param::HeadphoneGain {
                                monitor.gain = normalized * normalized;
                            } else {
                                monitor.mix = normalized;
                            }
                            MixerAction::SetMonitor { monitor }
                        }
                        _ => unreachable!(),
                    };
                    return Ok(Dispatch {
                        source,
                        action: Action::Mixer { action: mixer },
                    });
                }
                let action = match param {
                    Param::Eq(band) => DeckAction::SetEq {
                        band: band.engine_index(),
                        gain: if normalized <= 0.0 {
                            normalized + 1.0
                        } else {
                            10f32.powf(normalized * 6.0 / 20.0).min(2.0)
                        },
                    },
                    Param::Eq4 => DeckAction::SetEq {
                        band: 3,
                        gain: if normalized <= 0.0 {
                            normalized + 1.0
                        } else {
                            10f32.powf(normalized * 6.0 / 20.0).min(2.0)
                        },
                    },
                    Param::Volume => DeckAction::SetVolume {
                        level: normalized * normalized,
                    },
                    Param::RhythmGain => DeckAction::SetRhythmGain {
                        level: normalized * 2.,
                    },
                    Param::Trim => DeckAction::SetTrim {
                        db: normalized * 12.0,
                    },
                    Param::Tempo => {
                        let state = state.ok_or("tempo requires session state")?;
                        let range = state
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?
                            .tempo_range
                            .extent();
                        DeckAction::SetTempo {
                            ratio: 1.0 + f64::from(normalized) * range,
                        }
                    }
                    Param::ColourFx => {
                        let state = state.ok_or("cfx requires session state")?;
                        let mut fx = state
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?
                            .colour_fx;
                        fx.position = (normalized + 1.0) / 2.0;
                        fx.enabled = true;
                        DeckAction::SetColourFx { state: fx }
                    }
                    _ => unreachable!("mixer parameter handled above"),
                };
                action
            }
            Self::DoAction { action, .. } => {
                let action = match action {
                    ButtonAction::Play => DeckAction::Play,
                    ButtonAction::Pause => DeckAction::Pause,
                    ButtonAction::Unload => DeckAction::Unload,
                    ButtonAction::KeyLockOn => DeckAction::SetKeyLock { on: true },
                    ButtonAction::KeyLockOff => DeckAction::SetKeyLock { on: false },
                    ButtonAction::QuantizeOn => DeckAction::SetQuantize { enabled: true },
                    ButtonAction::QuantizeOff => DeckAction::SetQuantize { enabled: false },
                    ButtonAction::ToggleSlip => {
                        let deck = state
                            .ok_or("toggle_slip requires session state")?
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?;
                        DeckAction::SetSlip {
                            enabled: !deck.slip_enabled,
                        }
                    }
                    ButtonAction::ReleaseHotCue => DeckAction::SlipReturn,
                    ButtonAction::ToggleColourFxSelection(index) => {
                        let deck_state = state
                            .ok_or("CFX selection requires session state")?
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?;
                        let mut fx = deck_state.colour_fx;
                        fx.selected ^= 1 << index;
                        DeckAction::SetColourFx { state: fx }
                    }
                    ButtonAction::ToggleCrossfaderLeft | ButtonAction::ToggleCrossfaderRight => {
                        unreachable!("handled above")
                    }
                    ButtonAction::ToggleHeadphoneCue => unreachable!("handled above"),
                    ButtonAction::CrossfaderSmooth
                    | ButtonAction::CrossfaderEqualPower
                    | ButtonAction::CrossfaderCut
                    | ButtonAction::ToggleCrossfaderPower
                    | ButtonAction::ToggleCrossfaderReverse
                    | ButtonAction::ToggleCrossfaderDisabled => unreachable!("handled above"),
                    ButtonAction::SetEqMode(index) => DeckAction::SetEqMode {
                        mode: match index {
                            0 => crate::EqMode::Classic,
                            1 => crate::EqMode::EzRemix,
                            2 => crate::EqMode::Modern,
                            3 => crate::EqMode::Stems,
                            _ => return Err("invalid EQ mode"),
                        },
                    },
                    ButtonAction::HotCue(slot) => {
                        let deck_state = state
                            .ok_or("hot_cue requires session state")?
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?;
                        if deck_state.hot_cues[slot as usize].is_some() {
                            DeckAction::TriggerHotCue {
                                slot,
                                play: true,
                                loop_length: None,
                            }
                        } else {
                            let track = deck_state
                                .track
                                .as_ref()
                                .ok_or("hot_cue requires a track")?;
                            let seconds = deck_state
                                .loop_region
                                .map_or(deck_state.position_seconds.max(0.), |region| region[0]);
                            if !seconds.is_finite() || seconds >= track.loaded_seconds {
                                return Err("hot cue position is outside loaded audio");
                            }
                            let mut cues = deck_state.hot_cues;
                            cues[slot as usize] = Some(crate::HotCue {
                                seconds,
                                loop_end: deck_state.loop_region.map(|region| region[1]),
                            });
                            DeckAction::SetHotCues { cues }
                        }
                    }
                    ButtonAction::ToggleQuantize => {
                        let deck = state
                            .ok_or("toggle_quantize requires session state")?
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?;
                        DeckAction::SetQuantize {
                            enabled: deck.quantize_disabled,
                        }
                    }
                    ButtonAction::ToggleKeyLock => {
                        let deck = state
                            .ok_or("toggle_key_lock requires session state")?
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?;
                        DeckAction::SetKeyLock { on: !deck.key_lock }
                    }
                    ButtonAction::CycleTempoRange => {
                        let deck = state
                            .ok_or("cycle_tempo_range requires session state")?
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?;
                        DeckAction::SetTempoRange {
                            range: deck.tempo_range.next(),
                        }
                    }
                    ButtonAction::RhythmRoll(slot) => DeckAction::TriggerRhythmRoll { slot },
                    ButtonAction::RhythmShiftOn => DeckAction::SetRhythmShift { enabled: true },
                    ButtonAction::RhythmShiftOff => DeckAction::SetRhythmShift { enabled: false },
                    ButtonAction::ToggleRhythmShift => {
                        let state = state.ok_or("toggle_rhythm_shift requires session state")?;
                        let enabled = state
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?
                            .rhythm_shift;
                        DeckAction::SetRhythmShift { enabled: !enabled }
                    }
                    ButtonAction::TogglePlay => {
                        let state = state.ok_or("toggle_play requires session state")?;
                        if state
                            .decks
                            .get(deck as usize)
                            .ok_or("deck missing from state")?
                            .playing
                        {
                            DeckAction::Pause
                        } else {
                            DeckAction::Play
                        }
                    }
                };
                action
            }
            Self::SetCurve { .. } | Self::SetRouting { .. } => unreachable!("handled above"),
        };
        Ok(Dispatch {
            source,
            action: Action::Deck { deck, action },
        })
    }
}

impl Param {
    fn is_mixer(self) -> bool {
        matches!(
            self,
            Self::MasterGain
                | Self::BoothGain
                | Self::Crossfader
                | Self::CrossfaderSharpness
                | Self::SamplerGain
                | Self::HeadphoneGain
                | Self::HeadphoneMix
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfader_curve_presets_and_sharpness_are_scriptable() {
        let mut state = SessionState::default();
        let preset = Program::compile("do_action(\"mixer\", \"crossfader_cut\")").unwrap();
        assert_eq!(
            preset.run(ControlSource::Controller, 0.).unwrap().action,
            Action::Mixer {
                action: MixerAction::SetCrossfaderCurve {
                    curve: crate::CrossfaderCurve::Cut
                }
            }
        );
        state.crossfader_curve = crate::CrossfaderCurve::EqualPower;
        let sharpness =
            Program::compile("set_param(\"mixer\", \"crossfader_sharpness\", 0.7)").unwrap();
        assert_eq!(
            sharpness
                .run_with_state(ControlSource::Controller, 0., Some(&state))
                .unwrap()
                .action,
            Action::Mixer {
                action: MixerAction::SetCrossfaderCurve {
                    curve: crate::CrossfaderCurve::Adjustable {
                        sharpness: 70,
                        power: true,
                        reverse: false,
                        disabled: false
                    }
                }
            }
        );
        let reverse =
            Program::compile("do_action(\"mixer\", \"toggle_crossfader_reverse\")").unwrap();
        assert_eq!(
            reverse
                .run_with_state(ControlSource::Controller, 0., Some(&state))
                .unwrap()
                .action,
            Action::Mixer {
                action: MixerAction::SetCrossfaderCurve {
                    curve: crate::CrossfaderCurve::Adjustable {
                        sharpness: 22,
                        power: true,
                        reverse: true,
                        disabled: false
                    }
                }
            }
        );
        assert_eq!(
            Program::set_curve(crate::CrossfaderCurve::Smooth)
                .run(ControlSource::Skin, 0.)
                .unwrap()
                .action,
            Action::Mixer {
                action: MixerAction::SetCrossfaderCurve {
                    curve: crate::CrossfaderCurve::Smooth
                }
            }
        );
    }

    #[test]
    fn mixer_extensions_resolve_from_shared_session_state() {
        let mut state = SessionState::default();
        state.decks = vec![crate::DeckState::default(); DECKS];
        state.decks[0].eq_mode = crate::EqMode::EzRemix;
        state.decks[0].stem_gains = [1., 1., 1., 1.];
        let stems = Program::compile("set_param(\"deck_a\", \"eq_mid\", -0.5)").unwrap();
        assert_eq!(
            stems
                .run_with_state(ControlSource::Skin, 0., Some(&state))
                .unwrap()
                .action,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetEq { band: 1, gain: 0.5 }
            }
        );
        assert_eq!(
            Program::compile("set_param(\"deck_a\", \"eq_4\", -1)")
                .unwrap()
                .run(ControlSource::Skin, 0.)
                .unwrap()
                .action,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetEq { band: 3, gain: 0. }
            }
        );
        let cfx = Program::compile("do_action(\"deck_a\", \"toggle_cfx_2\")").unwrap();
        let Action::Deck {
            action: DeckAction::SetColourFx { state: fx },
            ..
        } = cfx
            .run_with_state(ControlSource::Controller, 0., Some(&state))
            .unwrap()
            .action
        else {
            panic!("expected CFX selection")
        };
        assert_eq!(fx.selected, state.decks[0].colour_fx.selected ^ 0b10);
        let routing =
            Program::compile("do_action(\"deck_a\", \"toggle_crossfader_right\")").unwrap();
        assert_eq!(
            routing
                .run_with_state(ControlSource::Skin, 0., Some(&state))
                .unwrap()
                .action,
            Action::Mixer {
                action: MixerAction::SetCrossfaderRouting {
                    routing: crate::CrossfaderRouting {
                        left: state.crossfader_routing.left,
                        right: state.crossfader_routing.right ^ 1,
                    }
                }
            }
        );
        let cue = Program::compile("do_action(\"deck_a\", \"toggle_headphone_cue\")").unwrap();
        let Action::Mixer {
            action: MixerAction::SetMonitor { monitor },
        } = cue
            .run_with_state(ControlSource::Controller, 0., Some(&state))
            .unwrap()
            .action
        else {
            panic!("expected monitor action")
        };
        assert_eq!(monitor.cue_mask(), state.monitor.cue_mask() ^ 1);
        assert_eq!(
            Program::compile("do_action(\"deck_a\", \"eq_stems\")")
                .unwrap()
                .run(ControlSource::Skin, 0.)
                .unwrap()
                .action,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetEqMode {
                    mode: crate::EqMode::Stems
                }
            }
        );
    }

    #[test]
    fn deck_setup_buttons_require_state_only_for_relative_changes() {
        for command in ["toggle_key_lock", "toggle_quantize", "cycle_tempo_range"] {
            let program =
                Program::compile(&format!("do_action(\"deck_d\", \"{command}\")")).unwrap();
            assert!(program.run(ControlSource::Skin, 0.).is_err());
            assert!(
                program
                    .run_with_state(ControlSource::Skin, 0., Some(&SessionState::default()))
                    .is_err()
            );
            assert!(Program::compile(&format!("do_action(\"mixer\", \"{command}\")")).is_err());
        }
        for (command, on) in [("key_lock_on", true), ("key_lock_off", false)] {
            let program =
                Program::compile(&format!("do_action(\"deck_d\", \"{command}\")")).unwrap();
            assert_eq!(
                program.run(ControlSource::Custom, 0.).unwrap(),
                Dispatch {
                    source: ControlSource::Custom,
                    action: Action::Deck {
                        deck: 3,
                        action: DeckAction::SetKeyLock { on }
                    },
                }
            );
        }
        for (command, enabled) in [("quantize_on", true), ("quantize_off", false)] {
            let program =
                Program::compile(&format!("do_action(\"deck_d\", \"{command}\")")).unwrap();
            assert_eq!(
                program.run(ControlSource::Custom, 0.).unwrap().action,
                Action::Deck {
                    deck: 3,
                    action: DeckAction::SetQuantize { enabled },
                }
            );
        }
    }

    #[test]
    fn rhythm_shift_actions_dispatch_to_the_selected_deck() {
        let mut state = SessionState::default();
        state.decks = vec![crate::DeckState::default(); DECKS];
        state.decks[2].rhythm_shift = true;
        let toggle = Program::compile("do_action(\"deck_c\", \"toggle_rhythm_shift\")").unwrap();
        assert_eq!(
            toggle
                .run_with_state(ControlSource::Skin, 0., Some(&state))
                .unwrap()
                .action,
            Action::Deck {
                deck: 2,
                action: DeckAction::SetRhythmShift { enabled: false }
            }
        );
        let roll = Program::compile("do_action(\"deck_c\", \"rhythm_roll_4\")").unwrap();
        assert_eq!(
            roll.run_with_state(ControlSource::Controller, 0., Some(&state))
                .unwrap()
                .action,
            Action::Deck {
                deck: 2,
                action: DeckAction::TriggerRhythmRoll { slot: 3 }
            }
        );
        let gain = Program::compile("set_param(\"deck_c\", \"rhythm_gain\", $value)").unwrap();
        assert_eq!(
            gain.run(ControlSource::Controller, 0.5).unwrap().action,
            Action::Deck {
                deck: 2,
                action: DeckAction::SetRhythmGain { level: 1. }
            }
        );
    }

    #[test]
    fn parameter_commands_preserve_existing_mixer_curves() {
        for deck in 0..DECKS {
            let letter = ["A", "B", "C", "D"][deck];
            for (name, band, index) in [
                ("low", EqBand::Low, 0),
                ("mid", EqBand::Mid, 1),
                ("high", EqBand::High, 2),
            ] {
                let old = Program::compile(&format!("set_eq {letter} {name} $value")).unwrap();
                let new = Program::compile(&format!(
                    "set_param(\"deck_{}\", \"eq_{name}\", $value)",
                    letter.to_lowercase()
                ))
                .unwrap();
                assert_eq!(old, Program::set_eq(deck, band).unwrap());
                assert_eq!(new, Program::set_param(deck, Param::Eq(band)).unwrap());
                for (position, gain) in [(-1.0, 0.0), (0.0, 1.0), (1.0, 10f32.powf(6.0 / 20.0))] {
                    assert_eq!(
                        new.run(ControlSource::Skin, position).unwrap().action,
                        Action::Deck {
                            deck: deck as u8,
                            action: DeckAction::SetEq { band: index, gain }
                        }
                    );
                    assert_eq!(
                        old.run(ControlSource::Skin, (position + 1.0) / 2.0)
                            .unwrap()
                            .action,
                        new.run(ControlSource::Skin, position).unwrap().action
                    );
                }
            }
        }
        let volume = Program::compile("set_param(\"deck_a\", \"volume\", $value)").unwrap();
        assert_eq!(
            volume.run(ControlSource::Controller, 0.5).unwrap().action,
            Action::Deck {
                deck: 0,
                action: DeckAction::SetVolume { level: 0.25 }
            }
        );
        let trim = Program::compile("set_param(\"deck_d\", \"trim\", 0.5)").unwrap();
        assert_eq!(
            trim.run(ControlSource::Custom, f32::NAN).unwrap().action,
            Action::Deck {
                deck: 3,
                action: DeckAction::SetTrim { db: 6.0 }
            }
        );
    }

    #[test]
    fn button_commands_and_stateful_parameters() {
        let mut state = SessionState::default();
        state.decks = vec![crate::DeckState::default(); DECKS];
        let toggle = Program::compile("do_action(\"deck_b\", \"toggle_play\")").unwrap();
        assert!(toggle.run(ControlSource::Controller, 0.0).is_err());
        assert_eq!(
            toggle
                .run_with_state(ControlSource::Controller, 0.0, Some(&state))
                .unwrap()
                .action,
            Action::Deck {
                deck: 1,
                action: DeckAction::Play
            }
        );
        state.decks[1].playing = true;
        assert_eq!(
            toggle
                .run_with_state(ControlSource::Controller, 0.0, Some(&state))
                .unwrap()
                .action,
            Action::Deck {
                deck: 1,
                action: DeckAction::Pause
            }
        );
        let tempo = Program::compile("set_param(\"deck_b\", \"tempo\", $value)").unwrap();
        assert_eq!(
            tempo
                .run_with_state(ControlSource::Skin, 0.0, Some(&state))
                .unwrap()
                .action,
            Action::Deck {
                deck: 1,
                action: DeckAction::SetTempo { ratio: 1.0 }
            }
        );
        let cfx = Program::compile("set_param(\"deck_b\", \"cfx\", -0.5)").unwrap();
        assert!(
            matches!(cfx.run_with_state(ControlSource::Skin, 0.0, Some(&state)).unwrap().action,
            Action::Deck { deck: 1, action: DeckAction::SetColourFx { state: fx } } if fx.position == 0.25 && fx.enabled)
        );
    }

    #[test]
    fn bipolar_extremes_project_to_each_control_curve() {
        let mut state = SessionState::default();
        state.decks = vec![crate::DeckState::default(); DECKS];
        for (input, db, tempo, cfx_position) in [
            (-1.0, -12.0, 0.92, 0.0),
            (0.0, 0.0, 1.0, 0.5),
            (1.0, 12.0, 1.08, 1.0),
        ] {
            let trim = Program::set_param(0, Param::Trim).unwrap();
            assert_eq!(
                trim.run(ControlSource::Skin, input).unwrap().action,
                Action::Deck {
                    deck: 0,
                    action: DeckAction::SetTrim { db }
                }
            );
            let tempo_program = Program::set_param(0, Param::Tempo).unwrap();
            let tempo_action = tempo_program
                .run_with_state(ControlSource::Skin, input, Some(&state))
                .unwrap()
                .action;
            assert!(
                matches!(tempo_action, Action::Deck { deck: 0, action: DeckAction::SetTempo { ratio } } if (ratio - tempo).abs() < 1e-6)
            );
            let fx = Program::set_param(0, Param::ColourFx).unwrap();
            assert!(
                matches!(fx.run_with_state(ControlSource::Skin, input, Some(&state)).unwrap().action,
                Action::Deck { deck: 0, action: DeckAction::SetColourFx { state: fx } } if fx.position == cfx_position)
            );
        }
    }

    #[test]
    fn bad_commands_and_values_are_rejected() {
        for source in [
            "set_param(\"deck_e\", \"eq_high\", 0.5)",
            "set_param(\"deck_a\", \"unknown\", 0.5)",
            "set_param(\"deck_a\", \"trim\", nope)",
            "do_action(\"deck_a\", \"explode\")",
            "do_action(\"deck_a\", \"play\", 1)",
            "set_param(\"deck_a\", \"master_gain\", 0.5)",
            "set_param(\"mixer\", \"eq_high\", 0.5)",
            "do_action(\"mixer\", \"play\")",
        ] {
            assert!(Program::compile(source).is_err(), "{source}");
        }
        let program = Program::set_param(0, Param::Volume).unwrap();
        assert!(program.run(ControlSource::Skin, f32::NAN).is_err());
        assert!(program.run(ControlSource::Skin, 1.1).is_err());
        let bipolar = Program::set_param(0, Param::Trim).unwrap();
        assert!(bipolar.run(ControlSource::Skin, -1.1).is_err());
        assert!(bipolar.run(ControlSource::Skin, 0.0).is_ok());
        assert_eq!(Param::Volume.domain(), ParamDomain::Unipolar);
        assert_eq!(Param::Trim.domain(), ParamDomain::Bipolar);
        assert!(Program::set_param(DECKS, Param::Trim).is_err());
    }

    #[test]
    fn mixer_parameters_preserve_skin_curves_and_monitor_state() {
        let state = SessionState::default();
        for (name, expected) in [
            ("master_gain", MixerAction::SetMasterGain { level: 0.25 }),
            ("booth_gain", MixerAction::SetBoothGain { level: 0.25 }),
            ("crossfader", MixerAction::SetCrossfader { position: 0.5 }),
            ("sampler_gain", MixerAction::SetSamplerGain { level: 0.25 }),
        ] {
            let program =
                Program::compile(&format!("set_param(\"mixer\", \"{name}\", $value)")).unwrap();
            assert_eq!(
                program.run(ControlSource::Controller, 0.5).unwrap(),
                Dispatch {
                    source: ControlSource::Controller,
                    action: Action::Mixer { action: expected },
                }
            );
        }
        for (name, gain, mix) in [
            ("headphone_gain", 0.25, state.monitor.mix),
            ("headphone_mix", state.monitor.gain, 0.5),
        ] {
            let program =
                Program::compile(&format!("set_param(\"mixer\", \"{name}\", $value)")).unwrap();
            let dispatch = program
                .run_with_state(ControlSource::Skin, 0.5, Some(&state))
                .unwrap();
            assert_eq!(
                dispatch.action,
                Action::Mixer {
                    action: MixerAction::SetMonitor {
                        monitor: crate::MonitorState {
                            gain,
                            mix,
                            ..state.monitor
                        }
                    },
                }
            );
        }
        assert!(Program::set_mixer_param(Param::MasterGain).is_ok());
        assert!(Program::set_mixer_param(Param::Volume).is_err());
    }

    #[test]
    fn relative_targets_resolve_at_invocation_for_two_or_four_decks() {
        let mut state = SessionState::default();
        state.decks = vec![crate::DeckState::default(); DECKS];
        state.decks[1].playing = true;
        let active = Program::compile("do_action(\"active\", \"toggle_play\")").unwrap();
        let inactive = Program::compile("do_action(\"inactive\", \"play\")").unwrap();
        for (visible_decks, expected) in [(2, vec![0]), (4, vec![0, 2, 3])] {
            let context = InvocationContext {
                source: ControlSource::Skin,
                state: Some(&state),
                active_deck: Some(1),
                visible_decks,
            };
            assert_eq!(
                active.run_in(context, 0.0).unwrap(),
                vec![Dispatch {
                    source: ControlSource::Skin,
                    action: Action::Deck {
                        deck: 1,
                        action: DeckAction::Pause
                    },
                }]
            );
            let targets: Vec<_> = inactive
                .run_in(context, 0.0)
                .unwrap()
                .into_iter()
                .map(|dispatch| match dispatch.action {
                    Action::Deck {
                        deck,
                        action: DeckAction::Play,
                    } => deck,
                    _ => panic!("unexpected action"),
                })
                .collect();
            assert_eq!(targets, expected);
        }
        let missing = InvocationContext {
            source: ControlSource::Skin,
            state: Some(&state),
            active_deck: None,
            visible_decks: 4,
        };
        assert!(active.run_in(missing, 0.0).is_err());
        assert!(inactive.run_in(missing, 0.0).is_err());
        assert!(active.run(ControlSource::Skin, 0.0).is_err());
    }
}
