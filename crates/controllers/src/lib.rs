//! User-owned controller mappings. All processing happens on the control thread.
//! The bridge produces protocol actions; the host must apply them under its Role.
use djcraft_protocol::{Action, DeckAction, MixerAction, SessionState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;

mod jog;
pub use jog::JogConfig;

#[cfg(feature = "native")]
pub mod native;

pub const PROFILE_VERSION: u32 = 1;

/// Client intents resolved using the desktop's existing cue/grid/selection
/// logic. Hosts must turn these into normal protocol Actions under their Role.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum HostCommand {
    Browse { steps: i32 },
    BrowseBack,
    LoadSelected { deck: u8 },
    Sync { deck: u8 },
    Cue { deck: u8, pressed: bool },
    SelectDeck { deck: u8 },
    ToggleHeadphoneCue { deck: u8 },
    Stutter { deck: u8 },
    AutoLoop { deck: u8, beats: u8 },
    LoopPad { deck: u8, pad: u8, alternate: bool },
    SamplePad { deck: u8, slot: u8, stop: bool },
}
impl HostCommand {
    pub fn deck(&self) -> u8 {
        match *self {
            Self::Browse { .. } | Self::BrowseBack => 0,
            Self::LoadSelected { deck } => deck,
            Self::Sync { deck }
            | Self::Cue { deck, .. }
            | Self::SelectDeck { deck }
            | Self::ToggleHeadphoneCue { deck }
            | Self::Stutter { deck }
            | Self::AutoLoop { deck, .. }
            | Self::LoopPad { deck, .. }
            | Self::SamplePad { deck, .. } => deck,
        }
    }
}

/// Zero-based MIDI channel and controller/note number. Note-off and zero-velocity
/// note-on share one source, so learning cannot lose a button's release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Note { channel: u8, number: u8 },
    Cc { channel: u8, number: u8 },
    PitchBend { channel: u8 },
}

impl Source {
    fn valid(self) -> bool {
        match self {
            Self::Note { channel, number } | Self::Cc { channel, number } => {
                channel < 16 && number < 128
            }
            Self::PitchBend { channel } => channel < 16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub source: Source,
    pub value: u16,
}

impl Event {
    /// Parse one complete MIDI 1 channel message. Ignore system traffic,
    /// unsupported messages and malformed data (including stray status bytes).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let [status, a, b] = *bytes else { return None };
        if a > 127 || b > 127 {
            return None;
        }
        let channel = status & 15;
        let (source, value) = match status & 0xf0 {
            0x80 => (Source::Note { channel, number: a }, 0),
            0x90 => (Source::Note { channel, number: a }, u16::from(b)),
            0xb0 => (Source::Cc { channel, number: a }, u16::from(b)),
            0xe0 => (
                Source::PitchBend { channel },
                u16::from(a) | (u16::from(b) << 7),
            ),
            _ => return None,
        };
        Some(Self { source, value })
    }

    /// Capture only actuated controls, not button releases.
    pub fn learn(self) -> Option<Source> {
        if matches!(self.source, Source::Note { .. }) && self.value == 0 {
            None
        } else {
            Some(self.source)
        }
    }
}

/// A discoverable, typed catalogue for continuous controls. Fixed Action bindings
/// below expose all other protocol operations without adding controller-specific
/// branches to the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "control", rename_all = "snake_case")]
pub enum Parameter {
    RangedTempo {
        deck: u8,
    },
    Volume {
        deck: u8,
    },
    Tempo {
        deck: u8,
    },
    Trim {
        deck: u8,
    },
    Eq {
        deck: u8,
        band: u8,
    },
    /// The first UI effect control: Filter cutoff, otherwise wet/dry Mix.
    EffectParameter {
        deck: u8,
        slot: u8,
    },
    Master,
    Crossfader,
    Sampler,
    HeadphoneGain,
    HeadphoneMix,
}

impl Parameter {
    pub fn value(self, state: &SessionState) -> Option<f64> {
        let deck = |id: u8| state.decks.get(usize::from(id));
        Some(match self {
            Self::Volume { deck: id } => f64::from(deck(id)?.volume),
            Self::Tempo { deck: id } | Self::RangedTempo { deck: id } => deck(id)?.tempo,
            Self::Trim { deck: id } => f64::from(deck(id)?.trim_db),
            Self::Eq { deck: id, band } => f64::from(*deck(id)?.eq.get(usize::from(band))?),
            Self::EffectParameter { deck: id, slot } => {
                let effect = deck(id)?.effects.get(usize::from(slot))?;
                f64::from(if effect.kind == djcraft_protocol::EffectKind::Filter {
                    effect.parameter
                } else {
                    effect.amount
                })
            }
            Self::Master => f64::from(state.master_gain),
            Self::Crossfader => f64::from(state.crossfader),
            Self::Sampler => f64::from(state.sampler_gain),
            Self::HeadphoneGain => f64::from(state.monitor.gain),
            Self::HeadphoneMix => f64::from(state.monitor.mix),
        })
    }

    fn action(self, value: f64, state: &SessionState) -> Action {
        let v = value as f32;
        let (deck, action) = match self {
            Self::Volume { deck } => (deck, DeckAction::SetVolume { level: v }),
            Self::Tempo { deck } | Self::RangedTempo { deck } => {
                (deck, DeckAction::SetTempo { ratio: value })
            }
            Self::Trim { deck } => (deck, DeckAction::SetTrim { db: v }),
            Self::Eq { deck, band } => (deck, DeckAction::SetEq { band, gain: v }),
            Self::EffectParameter { deck, slot } => {
                let mut effect = state.decks[usize::from(deck)].effects[usize::from(slot)];
                if effect.kind == djcraft_protocol::EffectKind::Filter {
                    effect.parameter = v;
                    effect.amount = 1.;
                } else {
                    effect.amount = v;
                }
                (deck, DeckAction::SetEffect { slot, effect })
            }
            other => {
                return Action::Mixer {
                    action: match other {
                        Self::Master => MixerAction::SetMasterGain { level: v },
                        Self::Crossfader => MixerAction::SetCrossfader { position: v },
                        Self::Sampler => MixerAction::SetSamplerGain { level: v },
                        Self::HeadphoneGain | Self::HeadphoneMix => {
                            let mut monitor = state.monitor;
                            if other == Self::HeadphoneGain {
                                monitor.gain = v;
                            } else {
                                monitor.mix = v;
                            }
                            MixerAction::SetMonitor { monitor }
                        }
                        _ => unreachable!(),
                    },
                };
            }
        };
        Action::Deck { deck, action }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Absolute,
    /// 1..63 positive; 65..127 negative; 0 and 64 neutral (Numark).
    TwosComplement,
    /// 64 is neutral (RX3).
    Offset64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Binding {
    EffectBeats {
        deck: u8,
        encoding: Encoding,
    },
    CycleTempoRange {
        deck: u8,
    },
    Browse {
        encoding: Encoding,
    },
    BrowserLed {
        folders: bool,
    },
    Led {
        feedback: LedFeedback,
    },
    HotCueDeleteMode {
        deck: u8,
    },
    ToggleEffect {
        deck: u8,
        slot: u8,
    },
    HostButton {
        press: HostCommand,
        release: Option<HostCommand>,
        #[serde(default)]
        led: Option<Source>,
    },
    /// A held pitch bend, expressed as small timed phase nudges. It never
    /// rewrites the tempo fader, so no stale tempo is restored on release.
    Bend {
        deck: u8,
        forward: bool,
        amount: f64,
    },
    /// Set an empty cue at the current playhead; otherwise recall and play it.
    HotCue {
        deck: u8,
        slot: u8,
    },
    Jog {
        config: JogConfig,
    },
    /// Press and release may each dispatch a different protocol action.
    Button {
        press: Action,
        release: Option<Action>,
    },
    TogglePlay {
        deck: u8,
    },
    ToggleKeyLock {
        deck: u8,
    },
    /// min/max are engine units and may be reversed to invert an axis.
    /// step is in engine units per relative tick. Pickup is absolute-only.
    Parameter {
        parameter: Parameter,
        min: f64,
        max: f64,
        /// Optional exact engine value at raw MIDI centre (64 or 8192).
        #[serde(default)]
        center: Option<f64>,
        encoding: Encoding,
        step: f64,
        pickup: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    pub id: String,
    /// Exact input port name: identical CCs on other devices cannot interfere.
    pub port: String,
    pub source: Source,
    /// Same-port held notes, for Shift layers. Empty means unconditional.
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    pub binding: Binding,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    pub source: Source,
    pub pressed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LedFeedback {
    Pressed { deck: u8 },
    LoopIn { deck: u8 },
    Playing { deck: u8 },
    Cue { deck: u8 },
    PitchCenter { deck: u8 },
    Loop { deck: u8 },
    HotCue { deck: u8, slot: u8 },
    Sample { deck: u8, slot: u8 },
    Effect { deck: u8, slot: u8 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    pub name: String,
    pub mappings: Vec<Mapping>,
}

impl Profile {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let profile: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != PROFILE_VERSION {
            return Err("unsupported controller profile version".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for m in &self.mappings {
            if m.id.is_empty() || m.port.is_empty() || !ids.insert(&m.id) {
                return Err(
                    "mapping IDs must be unique and nonempty; ports must be nonempty".into(),
                );
            }
            let valid_source = match m.source {
                Source::Note { channel, number } | Source::Cc { channel, number } => {
                    channel < 16 && number < 128
                }
                Source::PitchBend { channel } => channel < 16,
            };
            if matches!(
                m.binding,
                Binding::Browse {
                    encoding: Encoding::Absolute
                }
            ) || matches!(m.binding, Binding::Browse { .. })
                && !matches!(m.source, Source::Cc { .. })
                || matches!(m.binding, Binding::BrowserLed { .. })
                    && !matches!(m.source, Source::Note { .. })
            {
                return Err(format!("{}: invalid browser source/encoding", m.id));
            }
            if let Binding::Led { feedback } = &m.binding {
                let valid_slot = match feedback {
                    LedFeedback::HotCue { slot, .. } => {
                        usize::from(*slot) < djcraft_protocol::HOT_CUES
                    }
                    LedFeedback::Sample { slot, .. } => *slot < 16,
                    LedFeedback::Effect { slot, .. } => {
                        usize::from(*slot) < djcraft_protocol::EFFECT_SLOTS
                    }
                    _ => true,
                };
                if !matches!(m.source, Source::Note { .. })
                    || !valid_slot
                    || !m.modifiers.is_empty()
                {
                    return Err(format!(
                        "{}: LED requires a note, valid slot and no modifiers",
                        m.id
                    ));
                }
            }
            if !valid_source
                || m.modifiers
                    .iter()
                    .any(|v| !v.source.valid() || !matches!(v.source, Source::Note { .. }))
            {
                return Err(format!("{}: invalid MIDI address", m.id));
            }
            if let Binding::HostButton {
                press,
                led: Some(led),
                ..
            } = &m.binding
                && (!led.valid()
                    || !matches!(led, Source::Note { .. })
                    || !matches!(
                        press,
                        HostCommand::SelectDeck { .. } | HostCommand::ToggleHeadphoneCue { .. }
                    ))
            {
                return Err(format!(
                    "{}: LED requires a note and deck selection or headphone cue command",
                    m.id
                ));
            }
            if matches!(m.binding, Binding::CycleTempoRange { .. })
                && !matches!(m.source, Source::Note { .. })
            {
                return Err(format!("{}: range cycle requires a note", m.id));
            }
            if let Binding::EffectBeats { deck, encoding } = m.binding
                && (usize::from(deck) >= djcraft_protocol::DECKS
                    || encoding == Encoding::Absolute
                    || !matches!(m.source, Source::Cc { .. }))
            {
                return Err(format!(
                    "{}: effect beats requires a valid deck and relative CC",
                    m.id
                ));
            }
            if let Binding::ToggleEffect { slot, .. } = m.binding
                && (usize::from(slot) >= djcraft_protocol::EFFECT_SLOTS
                    || !matches!(m.source, Source::Note { .. }))
            {
                return Err(format!(
                    "{}: effect requires a valid slot and note source",
                    m.id
                ));
            }
            if let Binding::HostButton { press, release, .. } = &m.binding {
                for command in std::iter::once(press).chain(release.iter()) {
                    if matches!(command, HostCommand::LoopPad { pad, .. } if *pad >= 4)
                        || matches!(command, HostCommand::AutoLoop { beats, .. } if ![1,2,4,16].contains(beats))
                        || matches!(command, HostCommand::SamplePad { slot, .. } if *slot >= 16)
                    {
                        return Err(format!("{}: invalid pad index", m.id));
                    }
                }
            }
            if let Binding::Jog { config } = &m.binding {
                if !m.modifiers.is_empty() {
                    return Err(format!(
                        "{}: jog mode uses its dedicated Scratch button, not mapping modifiers",
                        m.id
                    ));
                }
                config
                    .validate(m.source)
                    .map_err(|e| format!("{}: {e}", m.id))?;
            }
            if let Binding::HotCue { slot, .. } = m.binding
                && (usize::from(slot) >= djcraft_protocol::HOT_CUES
                    || !matches!(m.source, Source::Note { .. }))
            {
                return Err(format!(
                    "{}: hot cues require a valid slot and note source",
                    m.id
                ));
            }
            if let Binding::Bend { amount, .. } = m.binding
                && (!amount.is_finite()
                    || !(0.001..=0.08).contains(&amount)
                    || !matches!(m.source, Source::Note { .. }))
            {
                return Err(format!(
                    "{}: bend requires a note and amount in 0.001..=0.08",
                    m.id
                ));
            }
            if let Binding::Parameter {
                parameter,
                min,
                max,
                center,
                step,
                encoding,
                pickup,
            } = m.binding
            {
                if !min.is_finite()
                    || !max.is_finite()
                    || min == max
                    || !step.is_finite()
                    || step <= 0.
                    || min.abs() > f64::from(f32::MAX)
                    || max.abs() > f64::from(f32::MAX)
                    || center
                        .is_some_and(|v| !v.is_finite() || v < min.min(max) || v > min.max(max))
                    || (pickup && encoding != Encoding::Absolute)
                    || (encoding != Encoding::Absolute && !matches!(m.source, Source::Cc { .. }))
                    || matches!(parameter, Parameter::Eq { band, .. } if band >= 3)
                    || matches!(parameter, Parameter::RangedTempo { .. })
                        && encoding != Encoding::Absolute
                    || matches!(parameter, Parameter::EffectParameter { slot, .. } if usize::from(slot) >= djcraft_protocol::EFFECT_SLOTS)
                {
                    return Err(format!("{}: invalid parameter range/encoding", m.id));
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Memory {
    startup_snapshot: Option<Instant>,
    tempo_range: Option<djcraft_protocol::TempoFaderRange>,
    down: bool,
    previous: Option<f64>,
    acquired: bool,
    last_sent: Option<f64>,
    bend_at: Option<Instant>,
}

pub struct Bridge {
    tempo_bpms: Vec<Option<f64>>,
    profile: Profile,
    memory: BTreeMap<String, Memory>,
    held: BTreeMap<(String, Source), bool>,
    jogs: BTreeMap<String, jog::Jog>,
    commands: Vec<HostCommand>,
    delete_modes: std::collections::BTreeSet<(String, u8)>,
}

impl Bridge {
    pub fn new(profile: Profile) -> Result<Self, String> {
        profile.validate()?;
        Ok(Self {
            tempo_bpms: Vec::new(),
            profile,
            memory: BTreeMap::new(),
            held: BTreeMap::new(),
            jogs: BTreeMap::new(),
            commands: Vec::new(),
            delete_modes: Default::default(),
        })
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Arm one absolute reading per mapping after a verified hardware dump
    /// request. Playback always retains normal soft takeover.
    pub fn prepare_startup_snapshot(&mut self, port: &str, now: Instant) {
        for mapping in &self.profile.mappings {
            if mapping.port == port
                && matches!(
                    mapping.binding,
                    Binding::Parameter {
                        encoding: Encoding::Absolute,
                        ..
                    }
                )
            {
                self.memory
                    .entry(mapping.id.clone())
                    .or_default()
                    .startup_snapshot = Some(now + std::time::Duration::from_secs(1));
            }
        }
    }

    /// Analysed base BPMs supplied by the host; unknown tracks disable snapping.
    pub fn set_tempo_bpms(&mut self, bpms: &[Option<f64>]) {
        self.tempo_bpms.clear();
        self.tempo_bpms.extend_from_slice(bpms);
    }

    /// Drain after process/tick/disconnect/replace, including cleanup releases.
    pub fn take_commands(&mut self) -> Vec<HostCommand> {
        std::mem::take(&mut self.commands)
    }

    /// State-only output bindings. Hosts supply an elapsed clock for blinking.
    pub fn state_feedback(
        &self,
        port: &str,
        state: &SessionState,
        elapsed_ms: u64,
        loop_in: &[Option<f64>],
    ) -> Vec<[u8; 3]> {
        let blink = elapsed_ms / 250 % 2 == 0;
        self.profile
            .mappings
            .iter()
            .filter_map(|m| {
                if m.port != port {
                    return None;
                }
                let Binding::Led { feedback } = &m.binding else {
                    return None;
                };
                let Source::Note { channel, number } = m.source else {
                    return None;
                };
                let (deck, slot) = match *feedback {
                    LedFeedback::Pressed { deck }
                    | LedFeedback::LoopIn { deck }
                    | LedFeedback::Playing { deck }
                    | LedFeedback::Cue { deck }
                    | LedFeedback::PitchCenter { deck }
                    | LedFeedback::Loop { deck } => (deck, 0),
                    LedFeedback::HotCue { deck, slot }
                    | LedFeedback::Sample { deck, slot }
                    | LedFeedback::Effect { deck, slot } => (deck, usize::from(slot)),
                };
                let on = state
                    .decks
                    .get(usize::from(deck))
                    .is_some_and(|s| match feedback {
                        LedFeedback::Pressed { .. } => self
                            .held
                            .get(&(port.to_owned(), m.source))
                            .copied()
                            .unwrap_or(false),
                        LedFeedback::LoopIn { .. } => {
                            s.loop_region.is_some()
                                || loop_in.get(usize::from(deck)).is_some_and(Option::is_some)
                        }
                        LedFeedback::Playing { .. } => s.track.is_some() && (s.playing || blink),
                        LedFeedback::Cue { .. } => {
                            s.track.is_some()
                                && (self
                                    .held
                                    .get(&(port.to_owned(), m.source))
                                    .copied()
                                    .unwrap_or(false)
                                    || !s.playing && blink)
                        }
                        LedFeedback::PitchCenter { .. } => (s.tempo - 1.).abs() < 0.001,
                        LedFeedback::Loop { .. } => s.loop_region.is_some(),
                        LedFeedback::HotCue { .. } => {
                            let populated = s.hot_cues.get(slot).is_some_and(Option::is_some);
                            if self.delete_modes.contains(&(port.to_owned(), deck)) {
                                populated && blink
                            } else {
                                populated
                            }
                        }
                        LedFeedback::Sample { .. } => match s.samples.get(slot) {
                            Some(2) => blink,
                            Some(1) => true,
                            _ => false,
                        },
                        LedFeedback::Effect { .. } => s
                            .effects
                            .get(slot)
                            .is_some_and(|fx| fx.active_in_slot(slot)),
                    });
                Some([0x90 | channel, number, if on { 100 } else { 0 }])
            })
            .collect()
    }

    /// Explicit profile-owned Scratch LEDs. Send again on button release when
    /// the device's local momentary lamp would otherwise hide the latched mode.
    pub fn jog_feedback(&self, port: &str) -> Vec<[u8; 3]> {
        self.profile
            .mappings
            .iter()
            .filter_map(|mapping| {
                if mapping.port != port {
                    return None;
                }
                let Binding::Jog { config } = &mapping.binding else {
                    return None;
                };
                let Some(Source::Note { channel, number }) = config.scratch_led else {
                    return None;
                };
                let enabled = self
                    .jogs
                    .get(&mapping.id)
                    .map_or(config.scratch_enabled, |j| j.enabled());
                Some([0x90 | channel, number, if enabled { 100 } else { 0 }])
            })
            .collect()
    }

    /// LEDs follow actual monitor selection, including changes from other clients.
    pub fn feedback(&self, port: &str, monitor_deck: u8) -> Vec<[u8; 3]> {
        self.feedback_monitor(
            port,
            djcraft_protocol::MonitorState {
                deck: monitor_deck,
                ..Default::default()
            },
        )
    }

    pub fn feedback_monitor(
        &self,
        port: &str,
        monitor: djcraft_protocol::MonitorState,
    ) -> Vec<[u8; 3]> {
        let mut messages = self.jog_feedback(port);
        for mapping in &self.profile.mappings {
            if mapping.port != port {
                continue;
            }
            if let Binding::HostButton {
                press: HostCommand::SelectDeck { deck } | HostCommand::ToggleHeadphoneCue { deck },
                led: Some(Source::Note { channel, number }),
                ..
            } = &mapping.binding
            {
                messages.push([
                    0x90 | channel,
                    *number,
                    if monitor.cue_enabled(*deck as usize) {
                        100
                    } else {
                        0
                    },
                ]);
            }
        }
        messages
    }

    /// Atomic replacement for editors/Learn: validation failure keeps the old map.
    /// Returns releases for held buttons. Apply these before activating the new
    /// map in the host, so editing a held scratch/Cue binding cannot stick it.
    pub fn replace(&mut self, profile: Profile) -> Result<Vec<Action>, String> {
        profile.validate()?;
        let ports: std::collections::BTreeSet<_> = self
            .profile
            .mappings
            .iter()
            .map(|m| m.port.clone())
            .collect();
        let releases = ports
            .into_iter()
            .flat_map(|port| self.disconnect(&port))
            .collect();
        self.profile = profile;
        self.memory.clear();
        self.held.clear();
        self.jogs.clear();
        Ok(releases)
    }

    /// Learn the next actuated source for an existing binding; releases are
    /// ignored. The editor chooses the target/encoding, not the MIDI message.
    pub fn learn(
        &mut self,
        id: &str,
        port: &str,
        event: Event,
    ) -> Result<Option<Vec<Action>>, String> {
        let Some(source) = event.learn() else {
            return Ok(None);
        };
        let mut profile = self.profile.clone();
        let mapping = profile
            .mappings
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or("unknown mapping ID")?;
        mapping.port = port.to_owned();
        mapping.source = source;
        self.replace(profile).map(Some)
    }

    /// Reset pickup and button edges after disconnect/reconnect. Hosts must also
    /// release any held momentary actions before closing a connection.
    pub fn disconnect(&mut self, port: &str) -> Vec<Action> {
        self.delete_modes.retain(|(p, _)| p != port);
        self.held.retain(|(name, _), _| name != port);
        let mut releases = Vec::new();
        for mapping in &self.profile.mappings {
            if mapping.port == port {
                if let Binding::Jog { config } = &mapping.binding
                    && let Some(mut jog) = self.jogs.remove(&mapping.id)
                {
                    releases.extend(jog.cancel(config.deck));
                }
                if let Some(memory) = self.memory.remove(&mapping.id)
                    && memory.down
                {
                    match &mapping.binding {
                        Binding::Button {
                            release: Some(action),
                            ..
                        } => releases.push(action.clone()),
                        Binding::HostButton {
                            release: Some(command),
                            ..
                        } => self.commands.push(command.clone()),
                        Binding::Bend { deck, .. } => releases.push(Action::Deck {
                            deck: *deck,
                            action: DeckAction::EndNudge,
                        }),
                        _ => {}
                    }
                }
            }
        }
        releases
    }

    pub fn process(&mut self, port: &str, event: Event, state: &SessionState) -> Vec<Action> {
        self.process_at(port, event, state, Instant::now())
    }

    /// Deterministic monotonic-clock entry point for hosts and tests.
    pub fn process_at(
        &mut self,
        port: &str,
        event: Event,
        state: &SessionState,
        now: Instant,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut takeovers = std::collections::BTreeSet::new();
        if event.value
            > match event.source {
                Source::PitchBend { .. } => 16383,
                _ => 127,
            }
        {
            return actions;
        }
        if matches!(event.source, Source::Note { .. }) {
            self.held
                .insert((port.to_owned(), event.source), event.value != 0);
        }
        for mapping in &self.profile.mappings {
            if mapping.port == port
                && let Binding::Jog { config } = &mapping.binding
            {
                // Jog touch and mode are companion sources of this one mapping.
                if mapping.source == event.source
                    || config.touch == Some(event.source)
                    || config.scratch_toggle == Some(event.source)
                {
                    let jog = self
                        .jogs
                        .entry(mapping.id.clone())
                        .or_insert_with(|| jog::Jog::new(config));
                    actions.extend(jog.process(config, mapping.source, event, state, now));
                }
                continue;
            }
            if mapping.port != port || mapping.source != event.source {
                continue;
            }
            let memory = self.memory.entry(mapping.id.clone()).or_default();
            let modifiers_match = mapping.modifiers.iter().all(|m| {
                self.held
                    .get(&(port.to_owned(), m.source))
                    .copied()
                    .unwrap_or(false)
                    == m.pressed
            });
            // A held action's release must still arrive if Shift was released first.
            if !modifiers_match && !(memory.down && event.value == 0) {
                continue;
            }
            let down = event.value != 0;
            let rising = down && !memory.down;
            let falling = !down && memory.down;
            memory.down = down;
            let action = match &mapping.binding {
                Binding::EffectBeats { deck, encoding } => {
                    let raw = i32::from(event.value);
                    let steps = match encoding {
                        Encoding::TwosComplement if raw == 64 => 0,
                        Encoding::TwosComplement if raw > 63 => raw - 128,
                        Encoding::TwosComplement => raw,
                        Encoding::Offset64 => raw - 64,
                        Encoding::Absolute => 0,
                    };
                    if steps == 0 {
                        continue;
                    }
                    state.decks.get(usize::from(*deck)).map(|s| {
                        let beats = djcraft_protocol::EFFECT_BEATS;
                        let index = beats
                            .iter()
                            .position(|b| *b >= s.effect_beats)
                            .unwrap_or(beats.len() - 1) as i32;
                        Action::Deck {
                            deck: *deck,
                            action: DeckAction::SetEffectBeats {
                                beats: beats
                                    [(index + steps).clamp(0, beats.len() as i32 - 1) as usize],
                            },
                        }
                    })
                }
                Binding::CycleTempoRange { deck } if rising => {
                    state.decks.get(usize::from(*deck)).map(|s| Action::Deck {
                        deck: *deck,
                        action: DeckAction::SetTempoRange {
                            range: s.tempo_range.next(),
                        },
                    })
                }
                Binding::Browse { encoding } => {
                    let raw = i32::from(event.value);
                    let steps = match encoding {
                        Encoding::TwosComplement if raw == 64 => 0,
                        Encoding::TwosComplement if raw > 63 => raw - 128,
                        Encoding::TwosComplement => raw,
                        Encoding::Offset64 => raw - 64,
                        Encoding::Absolute => 0,
                    };
                    if steps != 0 {
                        self.commands.push(HostCommand::Browse { steps });
                    }
                    None
                }
                Binding::HotCueDeleteMode { deck } if rising => {
                    let key = (port.to_owned(), *deck);
                    if !self.delete_modes.remove(&key) {
                        self.delete_modes.insert(key);
                    }
                    None
                }
                Binding::ToggleEffect { deck, slot } if rising => {
                    state.decks.get(usize::from(*deck)).map(|s| {
                        let mut effect = s.effects[usize::from(*slot)];
                        effect.enabled = *slot != 0 && !effect.enabled;
                        Action::Deck {
                            deck: *deck,
                            action: DeckAction::SetEffect {
                                slot: *slot,
                                effect,
                            },
                        }
                    })
                }
                Binding::HostButton { press, release, .. } => {
                    let command = if rising {
                        Some(press.clone())
                    } else if falling {
                        release.clone()
                    } else {
                        None
                    };
                    if let Some(command) = command {
                        if !matches!(
                            command,
                            HostCommand::SelectDeck { .. }
                                | HostCommand::ToggleHeadphoneCue { .. }
                                | HostCommand::Browse { .. }
                                | HostCommand::BrowseBack
                                | HostCommand::LoadSelected { .. }
                        ) {
                            takeovers.insert(command.deck());
                        }
                        self.commands.push(command);
                    }
                    None
                }
                Binding::Bend {
                    deck,
                    forward,
                    amount,
                } => {
                    if rising || falling {
                        memory.bend_at = Some(now);
                    }
                    if rising
                        && state
                            .decks
                            .get(usize::from(*deck))
                            .is_some_and(|s| s.playing)
                    {
                        Some(Action::Deck {
                            deck: *deck,
                            action: DeckAction::PitchBend {
                                seconds: amount * 0.02 * if *forward { 1. } else { -1. },
                            },
                        })
                    } else if falling {
                        Some(Action::Deck {
                            deck: *deck,
                            action: DeckAction::EndNudge,
                        })
                    } else {
                        None
                    }
                }
                Binding::HotCue { deck, slot } if rising => {
                    state.decks.get(usize::from(*deck)).and_then(|local| {
                        let cue = *local.hot_cues.get(usize::from(*slot))?;
                        let action = if self.delete_modes.contains(&(port.to_owned(), *deck)) {
                            let mut cues = local.hot_cues;
                            cues[usize::from(*slot)] = None;
                            DeckAction::SetHotCues { cues }
                        } else if cue.is_some() {
                            DeckAction::TriggerHotCue {
                                slot: *slot,
                                play: true,
                                loop_length: None,
                            }
                        } else {
                            let track = local.track.as_ref()?;
                            let seconds = local
                                .loop_region
                                .map_or(local.position_seconds.max(0.), |r| r[0]);
                            if !seconds.is_finite() || seconds >= track.loaded_seconds {
                                return None;
                            }
                            let mut cues = local.hot_cues;
                            cues[usize::from(*slot)] = Some(djcraft_protocol::HotCue {
                                seconds,
                                loop_end: local.loop_region.map(|r| r[1]),
                            });
                            DeckAction::SetHotCues { cues }
                        };
                        Some(Action::Deck {
                            deck: *deck,
                            action,
                        })
                    })
                }
                Binding::Button { press, release } => {
                    if rising {
                        Some(press.clone())
                    } else if falling {
                        release.clone()
                    } else {
                        None
                    }
                }
                Binding::TogglePlay { deck } if rising => {
                    state.decks.get(usize::from(*deck)).map(|s| Action::Deck {
                        deck: *deck,
                        action: if s.playing {
                            DeckAction::Pause
                        } else {
                            DeckAction::Play
                        },
                    })
                }
                Binding::ToggleKeyLock { deck } if rising => {
                    state.decks.get(usize::from(*deck)).map(|s| Action::Deck {
                        deck: *deck,
                        action: DeckAction::SetKeyLock { on: !s.key_lock },
                    })
                }
                Binding::Parameter {
                    parameter,
                    min,
                    max,
                    center,
                    encoding,
                    step,
                    pickup,
                } => {
                    let Some(current) = parameter.value(state).filter(|v| v.is_finite()) else {
                        continue;
                    };
                    let ranged = if let Parameter::RangedTempo { deck } = parameter {
                        let range = state.decks[usize::from(*deck)].tempo_range;
                        if memory.tempo_range != Some(range) {
                            memory.acquired = false;
                            memory.previous = None;
                            memory.last_sent = None;
                            memory.tempo_range = Some(range);
                        }
                        let extent = range.extent();
                        Some(if min > max {
                            (1. + extent, 1. - extent)
                        } else {
                            (1. - extent, 1. + extent)
                        })
                    } else {
                        None
                    };
                    let (min, max) = ranged.as_ref().map_or((min, max), |(a, b)| (a, b));
                    let neutral = Some(1.);
                    let center = if ranged.is_some() { &neutral } else { center };
                    let value = if *encoding == Encoding::Absolute {
                        let full = if matches!(event.source, Source::PitchBend { .. }) {
                            16383.
                        } else {
                            127.
                        };
                        let raw = f64::from(event.value);
                        let half = (full + 1.) / 2.;
                        let value = if let Some(center) = center {
                            if raw <= half {
                                min + (center - min) * raw / half
                            } else {
                                center + (max - center) * (raw - half) / (full - half)
                            }
                        } else {
                            min + (max - min) * raw / full
                        };
                        if *pickup {
                            let adopt_startup = memory
                                .startup_snapshot
                                .take()
                                .is_some_and(|until| now <= until)
                                && state.decks.iter().all(|deck| !deck.playing);
                            // UI/other controller changes re-arm pickup.
                            // Half a hardware step accommodates quantization
                            // without taking over a full step before an endpoint.
                            let resolution = if let Some(center) = center {
                                ((center - min).abs() / half)
                                    .max((max - center).abs() / (full - half))
                            } else {
                                (max - min).abs() / full
                            };
                            let tolerance = resolution * 0.5 + 1e-7;
                            if memory
                                .last_sent
                                .is_some_and(|v| (v - current).abs() > tolerance)
                            {
                                memory.acquired = false;
                                memory.previous = None;
                                memory.last_sent = None;
                            }
                            let crossed = memory
                                .previous
                                .is_some_and(|old| (old - current) * (value - current) <= 0.);
                            memory.previous = Some(value);
                            memory.acquired |=
                                adopt_startup || crossed || (value - current).abs() <= tolerance;
                            if !memory.acquired {
                                continue;
                            }
                        }
                        // Assign the nearest physical fader position to the exact
                        // opposite-deck tempo, without changing pickup detection.
                        if let Parameter::RangedTempo { deck } = parameter {
                            let deck = usize::from(*deck);
                            let opposite = deck ^ 1;
                            let target = self
                                .tempo_bpms
                                .get(deck)
                                .copied()
                                .flatten()
                                .zip(self.tempo_bpms.get(opposite).copied().flatten())
                                .zip(state.decks.get(opposite))
                                .and_then(|((own, other), reference)| {
                                    (own.is_finite() && own > 0. && other.is_finite() && other > 0.)
                                        .then_some(other * reference.tempo / own)
                                });
                            if let Some(target) = target.filter(|target| {
                                target.is_finite()
                                    && *target >= min.min(*max)
                                    && *target <= min.max(*max)
                            }) {
                                // Round in hardware coordinates so exactly one
                                // position wins, including near the neutral point.
                                let center = center.unwrap_or((min + max) / 2.);
                                let target_raw = if (target - min) * (target - center) <= 0. {
                                    (target - min) / (center - min) * half
                                } else {
                                    half + (target - center) / (max - center) * (full - half)
                                };
                                if raw == target_raw.round() {
                                    target
                                } else {
                                    value
                                }
                            } else {
                                value
                            }
                        } else {
                            value
                        }
                    } else {
                        let raw = i32::from(event.value);
                        let delta = match encoding {
                            Encoding::TwosComplement if raw == 64 => 0,
                            Encoding::TwosComplement if raw > 63 => raw - 128,
                            Encoding::TwosComplement => raw,
                            Encoding::Offset64 => raw - 64,
                            Encoding::Absolute => unreachable!(),
                        };
                        if delta == 0 {
                            continue;
                        }
                        current + f64::from(delta) * step
                    }
                    .clamp(min.min(*max), min.max(*max));
                    memory.last_sent = Some(value);
                    Some(parameter.action(value, state))
                }
                _ => None,
            };
            if let Some(action) = action {
                if let Action::Deck {
                    deck,
                    action:
                        DeckAction::Load { .. }
                        | DeckAction::Play
                        | DeckAction::Pause
                        | DeckAction::Seek { .. }
                        | DeckAction::Jump { .. }
                        | DeckAction::TriggerHotCue { .. }
                        | DeckAction::CuePreview { .. }
                        | DeckAction::AutoScratch { .. }
                        | DeckAction::Sync { .. }
                        | DeckAction::Scratch { .. }
                        | DeckAction::ScratchTo { .. }
                        | DeckAction::JogScratch { .. },
                } = &action
                {
                    takeovers.insert(*deck);
                }
                actions.push(action);
            }
        }
        let mut releases: Vec<_> = takeovers
            .into_iter()
            .flat_map(|deck| self.cancel_deck(deck))
            .collect();
        releases.extend(actions);
        releases
    }

    /// Release transient paused scrubs after motion stops; also cancel grabs
    /// when their track disappears or is replaced. Call even with no MIDI input.
    pub fn tick(&mut self, state: &SessionState) -> Vec<Action> {
        self.tick_at(state, Instant::now())
    }

    pub fn tick_at(&mut self, state: &SessionState, now: Instant) -> Vec<Action> {
        let mut actions = Vec::new();
        for mapping in &self.profile.mappings {
            if let Binding::Bend {
                deck,
                forward,
                amount,
            } = mapping.binding
                && let Some(memory) = self.memory.get_mut(&mapping.id)
                && memory.down
            {
                let elapsed = memory
                    .bend_at
                    .map_or(0., |last| now.saturating_duration_since(last).as_secs_f64());
                if elapsed >= 0.02 {
                    memory.bend_at = Some(now);
                    if state
                        .decks
                        .get(usize::from(deck))
                        .is_some_and(|s| s.playing)
                    {
                        actions.push(Action::Deck {
                            deck,
                            action: DeckAction::PitchBend {
                                seconds: elapsed.min(0.05)
                                    * amount
                                    * if forward { 1. } else { -1. },
                            },
                        });
                    }
                }
            }
            if let Binding::Jog { config } = &mapping.binding
                && let Some(jog) = self.jogs.get_mut(&mapping.id)
            {
                actions.extend(jog.tick(config, state, now));
            }
        }
        actions
    }

    /// Manual transport/load actions take over from controller grabs.
    pub fn cancel_deck(&mut self, deck: u8) -> Vec<Action> {
        let mut actions = Vec::new();
        for mapping in &self.profile.mappings {
            if matches!(mapping.binding, Binding::Bend { deck: target, .. } if target == deck)
                && let Some(memory) = self.memory.get_mut(&mapping.id)
                && memory.down
            {
                memory.down = false;
                memory.bend_at = None;
                actions.push(Action::Deck {
                    deck,
                    action: DeckAction::EndNudge,
                });
            }
            if let Binding::Jog { config } = &mapping.binding
                && config.deck == deck
                && let Some(jog) = self.jogs.get_mut(&mapping.id)
            {
                actions.extend(jog.cancel(deck));
            }
        }
        actions
    }
}
