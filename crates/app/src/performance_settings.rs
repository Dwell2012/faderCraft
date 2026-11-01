//! Machine-local deck setup, independent of songs and controller mapping files.
use djcraft_protocol::{Action, DECKS, DeckAction, EFFECT_SLOTS, EffectState, Role, SessionState};
use serde::{Deserialize, Serialize};
use std::{io, path::Path};

const FILE: &str = "performance-settings.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceSettings {
    pub scratch_modes: [Option<bool>; DECKS],
    pub effects: [[EffectState; EFFECT_SLOTS]; DECKS],
    pub colour_fx: [djcraft_protocol::ColourFxState; DECKS],
    pub stem_effects: [[EffectState; 4]; DECKS],
    pub effect_beats: [f32; DECKS],
    pub cue_quantize: [bool; DECKS],
    pub cue_quantize_window: [f64; DECKS],
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            scratch_modes: [None; DECKS],
            effects: std::array::from_fn(|_| djcraft_protocol::default_effects()),
            colour_fx: [Default::default(); DECKS],
            stem_effects: std::array::from_fn(|_| djcraft_protocol::default_stem_effects()),
            effect_beats: [djcraft_protocol::default_effect_beats(); DECKS],
            cue_quantize: [true; DECKS],
            cue_quantize_window: [0.05; DECKS],
        }
    }
}

impl PerformanceSettings {
    pub fn load(dir: &Path) -> Self {
        let Some(json) = std::fs::read(dir.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        else {
            return Self::default();
        };
        let mut saved: Self = serde_json::from_value(json.clone()).unwrap_or_default();
        if json.get("colour_fx").is_none() {
            for deck in 0..DECKS {
                if saved.effects[deck][0].kind == djcraft_protocol::EffectKind::Filter {
                    saved.colour_fx[deck].position = saved.effects[deck][0].parameter;
                    saved.effects[deck][0] = djcraft_protocol::default_effects()[0];
                }
            }
        }
        saved
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        std::fs::write(dir.join(FILE), bytes)
    }

    pub fn apply_effects(&self, engine: &mut djcraft_engine::Engine) {
        for deck in 0..DECKS {
            let mut colour_fx = self.colour_fx[deck];
            // Restore the combination, but require a fresh gesture to start a
            // standalone noise generator after launching the application.
            if colour_fx.selected & 96 != 0 {
                colour_fx.position = 0.5;
            }
            let _ = engine.apply(
                Role::Host,
                Action::Deck {
                    deck: deck as u8,
                    action: DeckAction::SetColourFx { state: colour_fx },
                },
            );
            let _ = engine.apply(
                Role::Host,
                Action::Deck {
                    deck: deck as u8,
                    action: DeckAction::SetEffectBeats {
                        beats: self.effect_beats[deck],
                    },
                },
            );
            for (slot, effect) in self.effects[deck].iter().copied().enumerate() {
                let _ = engine.apply(
                    Role::Host,
                    Action::Deck {
                        deck: deck as u8,
                        action: DeckAction::SetEffect {
                            slot: slot as u8,
                            effect,
                        },
                    },
                );
            }
            for (stem, effect) in self.stem_effects[deck].iter().copied().enumerate() {
                let _ = engine.apply(
                    Role::Host,
                    Action::Deck {
                        deck: deck as u8,
                        action: DeckAction::SetStemEffect {
                            stem: stem as u8,
                            effect,
                        },
                    },
                );
            }
        }
    }

    pub fn capture(
        &mut self,
        state: Option<&SessionState>,
        modes: Option<[Option<bool>; DECKS]>,
    ) -> bool {
        let previous = self.clone();
        if let Some(state) = state {
            for (deck, state) in state.decks.iter().take(DECKS).enumerate() {
                self.effects[deck] = state.effects;
                self.colour_fx[deck] = state.colour_fx;
                self.stem_effects[deck] = state.stem_effects;
                self.effect_beats[deck] = state.effect_beats;
            }
        }
        if let Some(modes) = modes {
            for (saved, mode) in self.scratch_modes.iter_mut().zip(modes) {
                if mode.is_some() {
                    *saved = mode;
                }
            }
        }
        *self != previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deck_setup_survives_disk_reload_and_restores_effects_without_tracks() {
        let dir = std::env::temp_dir().join(format!("djcraft-performance-{}", std::process::id()));
        let mut saved = PerformanceSettings::default();
        saved.scratch_modes = [Some(true), Some(false), None, Some(true)];
        saved.cue_quantize = [false, true, false, true];
        saved.cue_quantize_window = [0.05, 0.1, 0.2, 0.3];
        saved.colour_fx[1] = djcraft_protocol::ColourFxState {
            selected: 7,
            position: 0.73,
            enabled: true,
        };
        saved.effects[2][1].kind = djcraft_protocol::EffectKind::Echo;
        saved.effects[2][1].enabled = true;
        saved.effects[2][1].feedback = 0.73;
        saved.effect_beats[2] = 2.;
        saved.stem_effects[2][0].kind = djcraft_protocol::EffectKind::Reverb;
        saved.stem_effects[2][0].amount = 0.37;
        for effect in &mut saved.stem_effects[2] {
            effect.beats = 2.;
        }
        for effect in &mut saved.effects[2] {
            effect.beats = 2.;
        }
        saved.save(&dir).unwrap();
        let restored = PerformanceSettings::load(&dir);
        assert_eq!(saved, restored);
        let (mut engine, _) = djcraft_engine::Engine::headless(48000);
        restored.apply_effects(&mut engine);
        let state = engine.state();
        assert_eq!(state.decks[1].colour_fx, saved.colour_fx[1]);
        assert_eq!(state.decks[2].effects, saved.effects[2]);
        assert_eq!(state.decks[2].stem_effects, saved.stem_effects[2]);
        assert_eq!(state.decks[2].effect_beats, 2.);
        assert!(state.decks.iter().all(|deck| deck.track.is_none()));
        let mut captured = restored.clone();
        assert!(!captured.capture(Some(&state), Some([None; DECKS])));
        assert_eq!(captured.scratch_modes, saved.scratch_modes);
        std::fs::write(dir.join(FILE), "{}").unwrap();
        assert_eq!(
            PerformanceSettings::load(&dir),
            PerformanceSettings::default()
        );
        let mut legacy = serde_json::to_value(&saved).unwrap();
        legacy.as_object_mut().unwrap().remove("colour_fx");
        legacy["effects"][0][0] = serde_json::to_value(EffectState {
            kind: djcraft_protocol::EffectKind::Filter,
            parameter: 0.2,
            enabled: true,
            ..Default::default()
        })
        .unwrap();
        std::fs::write(dir.join(FILE), serde_json::to_vec(&legacy).unwrap()).unwrap();
        let migrated = PerformanceSettings::load(&dir);
        assert_eq!(migrated.colour_fx[0].position, 0.2);
        assert_eq!(
            migrated.effects[0][0],
            djcraft_protocol::default_effects()[0]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
