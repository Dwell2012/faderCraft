//! Small compiled control language. Execution produces ordinary protocol actions.
use crate::{Action, DECKS, DeckAction, SessionState};

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
    Trim,
    Tempo,
    ColourFx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamDomain {
    Unipolar,
    Bipolar,
}

impl Param {
    pub fn domain(self) -> ParamDomain {
        match self {
            Self::Volume => ParamDomain::Unipolar,
            Self::Eq(_) | Self::Trim | Self::Tempo | Self::ColourFx => ParamDomain::Bipolar,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    Play,
    Pause,
    TogglePlay,
    Unload,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Input,
    Fixed(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Program {
    /// Original 0–1 EQ position syntax, retained for existing bindings.
    LegacyEq { target: Target, band: EqBand },
    SetParam {
        target: Target,
        param: Param,
        value: Value,
    },
    DoAction {
        target: Target,
        action: ButtonAction,
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
                "trim" => Param::Trim,
                "tempo" => Param::Tempo,
                "cfx" => Param::ColourFx,
                _ => return Err("unknown parameter"),
            };
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
                _ => return Err("unknown action"),
            };
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
        if deck >= DECKS {
            return Err("unknown deck");
        }
        Ok(Self::SetParam {
            target: Target::Deck(deck as u8),
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
        let target = self.target();
        let deck = match target {
            Target::Deck(deck) => deck,
            Target::Active | Target::Inactive => {
                return Err("relative target requires invocation context");
            }
        };
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
                let action = match param {
                    Param::Eq(band) => DeckAction::SetEq {
                        band: band.engine_index(),
                        gain: if normalized <= 0.0 {
                            normalized + 1.0
                        } else {
                            10f32.powf(normalized * 6.0 / 20.0).min(2.0)
                        },
                    },
                    Param::Volume => DeckAction::SetVolume {
                        level: normalized * normalized,
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
                };
                action
            }
            Self::DoAction { action, .. } => {
                let action = match action {
                    ButtonAction::Play => DeckAction::Play,
                    ButtonAction::Pause => DeckAction::Pause,
                    ButtonAction::Unload => DeckAction::Unload,
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
        };
        Ok(Dispatch {
            source,
            action: Action::Deck { deck, action },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
