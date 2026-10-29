use crate::{Encoding, Event, Source};
use djcraft_protocol::{Action, DeckAction, DeckState, SessionState};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const SCRUB_IDLE: Duration = Duration::from_millis(80);
const BEND_IDLE: Duration = Duration::from_millis(40);
// Some controllers pulse the contact note between successive wheel packets.
const CONTACT_RELEASE: Duration = Duration::from_millis(30);

/// One relative movement source plus its touch and optional Scratch-mode button.
/// Distances are source-track seconds per tick, independent of tempo or pitch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JogConfig {
    pub deck: u8,
    #[serde(default)]
    pub touch: Option<Source>,
    pub scratch_toggle: Option<Source>,
    #[serde(default)]
    pub scratch_led: Option<Source>,
    pub encoding: Encoding,
    pub seconds_per_tick: f64,
    pub bend_seconds_per_tick: f64,
    pub scratch_enabled: bool,
    #[serde(default)]
    pub inverted: bool,
}

impl JogConfig {
    pub(crate) fn validate(&self, movement: Source) -> Result<(), &'static str> {
        if !matches!(movement, Source::Cc { .. }) || self.encoding == Encoding::Absolute {
            return Err("jog movement requires a relative CC encoding");
        }
        if self
            .touch
            .is_some_and(|s| !s.valid() || !matches!(s, Source::Note { .. }))
            || self.scratch_toggle.is_some_and(|s| {
                !s.valid() || !matches!(s, Source::Note { .. }) || Some(s) == self.touch
            })
        {
            return Err("jog touch/mode require distinct valid note sources");
        }
        if !self.seconds_per_tick.is_finite()
            || !(0.000001..=0.1).contains(&self.seconds_per_tick)
            || !self.bend_seconds_per_tick.is_finite()
            || !(0.000001..=0.01).contains(&self.bend_seconds_per_tick)
        {
            return Err("invalid jog distance or bend sensitivity");
        }
        if self
            .scratch_led
            .is_some_and(|s| !s.valid() || !matches!(s, Source::Note { .. }))
        {
            return Err("jog Scratch LED requires a valid note source");
        }
        Ok(())
    }
}

pub(crate) struct Jog {
    enabled: bool,
    touched: bool,
    contact_release: Option<Instant>,
    mode_down: bool,
    active: bool,
    transient: bool,
    target: Option<f64>,
    last_motion: Option<Instant>,
    motion_interval: f64,
    track: Option<(Option<String>, String)>,
    bending: bool,
    bend_at: Option<Instant>,
}

impl Jog {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
    pub(crate) fn new(config: &JogConfig) -> Self {
        Self {
            enabled: config.scratch_enabled,
            touched: false,
            contact_release: None,
            mode_down: false,
            active: false,
            transient: false,
            target: None,
            last_motion: None,
            motion_interval: 0.01,
            track: None,
            bending: false,
            bend_at: None,
        }
    }

    fn action(deck: u8, action: DeckAction) -> Action {
        Action::Deck { deck, action }
    }

    fn loaded<'a>(config: &JogConfig, state: &'a SessionState) -> Option<&'a DeckState> {
        let local = state.decks.get(usize::from(config.deck))?;
        let track = local.track.as_ref()?;
        if !local.position_seconds.is_finite()
            || !track.loaded_seconds.is_finite()
            || track.loaded_seconds <= 0.
        {
            return None;
        }
        Some(local)
    }

    fn identity(local: &DeckState) -> (Option<String>, String) {
        (
            local.path.clone(),
            local
                .track
                .as_ref()
                .map(|t| t.title.clone())
                .unwrap_or_default(),
        )
    }

    fn grab(&mut self, config: &JogConfig, local: &DeckState) -> Action {
        self.bending = false;
        self.bend_at = None;
        self.track = Some(Self::identity(local));
        self.target = Some(
            local
                .position_seconds
                .clamp(0., local.track.as_ref().unwrap().loaded_seconds),
        );
        self.active = true;
        self.transient = false;
        Self::action(
            config.deck,
            DeckAction::ScratchTo {
                seconds: self.target.unwrap(),
            },
        )
    }

    fn finish(&mut self, deck: u8, settle: bool) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.active {
            actions.push(Self::action(deck, DeckAction::Scratch { speed: None }));
            // A whole touch/motion/release burst can fit between audio callbacks.
            // Keep its final position even if the audio thread has not caught up.
            if settle && let Some(seconds) = self.target {
                actions.push(Self::action(deck, DeckAction::Seek { seconds }));
            }
        }
        self.active = false;
        self.contact_release = None;
        self.transient = false;
        self.target = None;
        self.last_motion = None;
        self.motion_interval = 0.01;
        actions
    }

    pub(crate) fn cancel(&mut self, deck: u8) -> Vec<Action> {
        self.touched = false;
        self.contact_release = None;
        self.track = None;
        let mut actions = self.end_bend(deck);
        actions.extend(self.finish(deck, false));
        actions
    }

    fn end_bend(&mut self, deck: u8) -> Vec<Action> {
        let actions = if self.bending {
            vec![Self::action(deck, DeckAction::EndNudge)]
        } else {
            Vec::new()
        };
        self.bending = false;
        self.bend_at = None;
        actions
    }

    pub(crate) fn tick(
        &mut self,
        config: &JogConfig,
        state: &SessionState,
        now: Instant,
    ) -> Vec<Action> {
        let Some(local) = Self::loaded(config, state) else {
            return self.cancel(config.deck);
        };
        if self
            .track
            .as_ref()
            .is_some_and(|id| *id != Self::identity(local))
        {
            return self.cancel(config.deck);
        }
        // The last scratch-surface contact owns a still-moving platter. Use
        // actual wheel packets for free spin; do not invent velocity after idle.
        if self
            .contact_release
            .is_some_and(|last| now.saturating_duration_since(last) >= CONTACT_RELEASE)
            && self
                .last_motion
                .is_none_or(|last| now.saturating_duration_since(last) >= SCRUB_IDLE)
        {
            self.contact_release = None;
            self.touched = false;
            return self.finish(config.deck, true);
        }
        if self.bending
            && (!local.playing
                || self
                    .bend_at
                    .is_some_and(|last| now.saturating_duration_since(last) >= BEND_IDLE))
        {
            return self.end_bend(config.deck);
        }
        if self.active
            && self.transient
            && (self
                .last_motion
                .is_some_and(|last| now.saturating_duration_since(last) >= SCRUB_IDLE))
        {
            return self.finish(config.deck, true);
        }
        Vec::new()
    }

    pub(crate) fn process(
        &mut self,
        config: &JogConfig,
        movement: Source,
        event: Event,
        state: &SessionState,
        now: Instant,
    ) -> Vec<Action> {
        let mut actions = self.tick(config, state, now);
        if config.scratch_toggle == Some(event.source) {
            let down = event.value != 0;
            if down && !self.mode_down {
                self.enabled = !self.enabled;
                if !self.enabled {
                    if self.contact_release.is_some() {
                        self.touched = false;
                    }
                    actions.extend(self.finish(config.deck, true));
                } else if self.touched
                    && let Some(local) = Self::loaded(config, state)
                {
                    actions.push(self.grab(config, local));
                }
            }
            self.mode_down = down;
            return actions;
        }
        let Some(local) = Self::loaded(config, state) else {
            return actions;
        };
        let limit = local.track.as_ref().unwrap().loaded_seconds;
        if Some(event.source) == config.touch {
            let down = event.value != 0;
            if down {
                self.contact_release = None;
            }
            if down && !self.touched && self.enabled {
                actions.push(self.grab(config, local));
            } else if !down && self.touched && self.active {
                self.contact_release = Some(now);
                return actions;
            }
            self.touched = down;
            return actions;
        }
        if event.source != movement {
            return actions;
        }
        let raw = i32::from(event.value);
        let mut ticks = match config.encoding {
            Encoding::TwosComplement if raw == 64 => 0,
            Encoding::TwosComplement if raw > 63 => raw - 128,
            Encoding::TwosComplement => raw,
            Encoding::Offset64 => raw - 64,
            Encoding::Absolute => return actions,
        };
        if config.inverted {
            ticks = -ticks;
        }
        if ticks == 0 {
            return actions;
        }
        let vinyl = self.enabled && (config.touch.is_none() || self.touched);
        if vinyl || (!self.enabled && !local.playing) {
            self.bending = false;
            self.bend_at = None;
            let seconds = (self.target.unwrap_or(local.position_seconds)
                + f64::from(ticks) * config.seconds_per_tick)
                .clamp(0., limit);
            self.track = Some(Self::identity(local));
            self.target = Some(seconds);
            self.active = true;
            self.transient = !vinyl || config.touch.is_none();
            let interval = self
                .last_motion
                .map(|last| now.saturating_duration_since(last).as_secs_f64())
                .filter(|dt| *dt >= 0.001)
                .unwrap_or(self.motion_interval)
                .clamp(0.001, 0.08);
            self.motion_interval = interval;
            self.last_motion = Some(now);
            let speed = (f64::from(ticks) * config.seconds_per_tick / interval).clamp(-4., 4.);
            actions.push(Self::action(
                config.deck,
                DeckAction::JogScratch {
                    seconds,
                    speed,
                    interval_seconds: interval,
                },
            ));
        } else if local.playing {
            self.bending = true;
            self.bend_at = Some(now);
            // Engine Nudge applies a bounded temporary phase correction, leaving
            // the tempo fader and key-lock setting unchanged.
            actions.push(Self::action(
                config.deck,
                DeckAction::PitchBend {
                    seconds: f64::from(ticks) * config.bend_seconds_per_tick,
                },
            ));
        }
        actions
    }
}
