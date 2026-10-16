//! djcraft control protocol, version 1.
//!
//! Every user action is an [`Action`] sent to the session host, which checks it
//! against the sender's [`Role`] and applies it to the engine. The host then
//! broadcasts the resulting [`SessionState`]. The desktop UI and remote clients
//! use the same messages.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

/// Zero-based deck index.
pub type DeckId = u8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Owns the session and renders the audio.
    Host,
    /// Can control decks, but not the session or the master output.
    CoDj,
    /// Receives state only.
    Viewer,
}

impl Role {
    pub fn allows(self, action: &Action) -> bool {
        match (self, action) {
            (Role::Host, _) => true,
            (Role::CoDj, Action::Deck { .. }) => true,
            (Role::CoDj, Action::Mixer { .. }) => false,
            (Role::Viewer, _) => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum Action {
    Deck { deck: DeckId, action: DeckAction },
    Mixer { action: MixerAction },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum EqMode {
    #[default]
    Classic,
    EzRemix,
    Modern,
    Stems,
}
impl EqMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic EQ",
            Self::EzRemix => "EZRemix",
            Self::Modern => "Modern EQ",
            Self::Stems => "Stems",
        }
    }
}

pub const EFFECT_SLOTS: usize = 6;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectKind {
    #[default]
    Filter,
    Echo,
    Flanger,
    Cut,
    Reverb,
    Delay,
    PingPong,
    LowCutEcho,
    EchoOut,
    Phaser,
    Chorus,
    Tremolo,
    AutoPan,
    Distortion,
    Bitcrusher,
    Noise,
    PitchShift,
    Spiral,
    BeatRepeat,
    Brake,
}
impl EffectKind {
    pub const ALL: [Self; 20] = [
        Self::Filter,
        Self::Echo,
        Self::Flanger,
        Self::Cut,
        Self::Reverb,
        Self::Delay,
        Self::PingPong,
        Self::LowCutEcho,
        Self::EchoOut,
        Self::Phaser,
        Self::Chorus,
        Self::Tremolo,
        Self::AutoPan,
        Self::Distortion,
        Self::Bitcrusher,
        Self::Noise,
        Self::PitchShift,
        Self::Spiral,
        Self::BeatRepeat,
        Self::Brake,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Filter => "Filter",
            Self::Echo => "Echo",
            Self::Flanger => "Flanger",
            Self::Cut => "Cut",
            Self::Reverb => "Reverb",
            Self::Delay => "Delay",
            Self::PingPong => "Ping-pong Echo",
            Self::LowCutEcho => "Low-cut Echo",
            Self::EchoOut => "Echo Out",
            Self::Phaser => "Phaser",
            Self::Chorus => "Chorus",
            Self::Tremolo => "Tremolo",
            Self::AutoPan => "Auto-pan",
            Self::Distortion => "Distortion",
            Self::Bitcrusher => "Bitcrusher",
            Self::Noise => "Noise",
            Self::PitchShift => "Pitch Shift",
            Self::Spiral => "Spiral",
            Self::BeatRepeat => "Beat Repeat",
            Self::Brake => "Brake",
        }
    }
    pub fn has_tail(self) -> bool {
        matches!(
            self,
            Self::Echo
                | Self::Delay
                | Self::PingPong
                | Self::LowCutEcho
                | Self::EchoOut
                | Self::Reverb
                | Self::Spiral
        )
    }
    pub fn timed(self) -> bool {
        matches!(
            self,
            Self::Echo
                | Self::Delay
                | Self::PingPong
                | Self::LowCutEcho
                | Self::EchoOut
                | Self::Flanger
                | Self::Cut
                | Self::Phaser
                | Self::Chorus
                | Self::Tremolo
                | Self::AutoPan
                | Self::Spiral
                | Self::BeatRepeat
                | Self::Brake
        )
    }
    pub fn feedback_control(self) -> bool {
        self.has_tail() || matches!(self, Self::Flanger | Self::Phaser)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectState {
    pub kind: EffectKind,
    pub enabled: bool,
    pub amount: f32,
    pub parameter: f32,
    pub feedback: f32,
    pub sync: bool,
    pub beats: f32,
    pub tails: bool,
}
impl Default for EffectState {
    fn default() -> Self {
        Self {
            kind: EffectKind::Filter,
            enabled: false,
            amount: 0.5,
            parameter: 0.5,
            feedback: 0.45,
            sync: true,
            beats: 0.5,
            tails: true,
        }
    }
}

pub const HOT_CUES: usize = 16;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CueAppearance {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub colour: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HotCue {
    pub seconds: f64,
    pub loop_end: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DeckAction {
    SetAutoTrim {
        enabled: bool,
    },
    SetBeatGrid {
        grid: Option<BeatGrid>,
    },
    LoadSample {
        slot: u8,
        path: String,
    },
    DefaultSample {
        slot: u8,
    },
    CaptureSample {
        slot: u8,
        start: f64,
        end: f64,
    },
    Sample {
        slot: u8,
        play: bool,
        restart: bool,
        repeat: bool,
        level: f32,
    },
    ClearSample {
        slot: u8,
    },
    AutoScratch {
        pattern: Option<u8>,
        beat_seconds: f64,
        first_beat: f64,
        quantize: bool,
        over: bool,
    },
    ArmLoop {
        start: f64,
        end: f64,
    },
    SetCueAppearance {
        slot: u8,
        appearance: CueAppearance,
    },
    SetHotCues {
        cues: [Option<HotCue>; HOT_CUES],
    },
    /// Atomically position the transport and start/stop a temporary cue preview.
    CuePreview {
        seconds: f64,
        play: bool,
    },
    TriggerHotCue {
        slot: u8,
        play: bool,
        loop_length: Option<f64>,
    },
    Jump {
        seconds: f64,
    },
    StartPadLoop {
        start: f64,
        end: f64,
        roll: bool,
    },
    /// Original-track BPM for effect timing; None uses manual timing.
    SetEffectTempo {
        bpm: Option<f64>,
    },
    SetEffect {
        slot: u8,
        effect: EffectState,
    },
    /// Loop bounds in original-track seconds, independent of playback tempo.
    SetLoop {
        start: f64,
        end: f64,
    },
    ClearLoop,
    SetLoopRoll {
        enabled: bool,
    },
    /// Load a track from a path on the host.
    Load {
        path: String,
    },
    Play,
    Pause,
    Seek {
        seconds: f64,
    },
    /// Playback speed as a ratio of the original tempo; 1.0 is unchanged.
    SetTempo {
        ratio: f64,
    },
    /// Small audible phase correction without stopping or seeking.
    Nudge {
        seconds: f64,
    },
    /// Set matched tempo and smoothly correct track position by this offset.
    Sync {
        ratio: f64,
        offset_seconds: f64,
    },
    /// Keep the original pitch when the tempo changes.
    SetKeyLock {
        on: bool,
    },
    SetVolume {
        level: f32,
    },
    SetEqMode {
        mode: EqMode,
    },
    /// Vocals, instruments, bass, drums. Unity preserves the original mix.
    SetStemGains {
        gains: [f32; 4],
    },
    SetTrim {
        db: f32,
    },
    /// Band order: low, mid, high. Gain 0 kills, 1 is neutral, 2 is +6 dB.
    SetEq {
        band: u8,
        gain: f32,
    },
    /// Scrub towards a track position with audible, bidirectional varispeed.
    /// Release with `Scratch { speed: None }`.
    ScratchTo {
        seconds: f64,
    },
    /// Drive playback from an external speed signal (jog wheel, timecode).
    /// `Some(speed)` overrides the transport, with 1.0 as normal forward
    /// speed and negative values as reverse. `None` hands control back.
    Scratch {
        speed: Option<f64>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MixerAction {
    SetTrackPreparation {
        path: String,
        preparation: TrackPreparation,
    },
    AutomixSmart {
        enabled: bool,
    },
    AutomixEnqueue {
        tracks: Vec<QueueTrack>,
    },
    AutomixRemove {
        index: usize,
    },
    AutomixMove {
        from: usize,
        to: usize,
    },
    AutomixClear,
    AutomixStart,
    AutomixStop,
    AutomixNext,
    AutomixFadeTime {
        seconds: f64,
    },
    SetMasterGain {
        level: f32,
    },
    SetCrossfader {
        position: f32,
    },
    SetCrossfaderCurve {
        curve: CrossfaderCurve,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossfaderCurve {
    #[default]
    Smooth,
    EqualPower,
    Cut,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientMessage {
    pub version: u32,
    /// Chosen by the client; echoed back if the action is rejected.
    pub seq: u64,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    State { state: SessionState },
    Rejected { seq: u64, reason: Rejection },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    NotPermitted,
    UnknownDeck,
    InvalidValue,
    /// The engine could not take the action right now; retry.
    Busy,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeatGrid {
    pub bpm: f64,
    pub first_beat: f64,
    pub downbeat: u8,
    pub confidence: f64,
}
impl BeatGrid {
    pub fn valid(self) -> bool {
        self.bpm.is_finite()
            && (40. ..=300.).contains(&self.bpm)
            && self.first_beat.is_finite()
            && self.first_beat >= 0.
            && self.downbeat < 4
            && self.confidence.is_finite()
            && (0. ..=1.).contains(&self.confidence)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PhraseLabel {
    Intro,
    Build,
    Breakdown,
    Drop,
    Outro,
    Section,
}
impl PhraseLabel {
    pub fn text(self) -> &'static str {
        match self {
            Self::Intro => "Intro",
            Self::Build => "Build",
            Self::Breakdown => "Breakdown",
            Self::Drop => "Drop",
            Self::Outro => "Outro",
            Self::Section => "Section",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhraseMarker {
    pub seconds: f64,
    pub label: PhraseLabel,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackPreparation {
    pub phrases: Vec<PhraseMarker>,
    pub mix_in: Option<f64>,
    /// Start of the outgoing transition, not the end of playback.
    pub mix_out: Option<f64>,
    pub transition_bars: Option<u8>,
    /// Fraction of the transition at which bass changes decks.
    pub bass_swap: Option<f32>,
}
impl TrackPreparation {
    pub fn valid(&self) -> bool {
        let time = |t: f64| t.is_finite() && t >= 0.;
        self.phrases.len() <= 128
            && self.phrases.iter().all(|p| time(p.seconds))
            && self.phrases.windows(2).all(|p| p[0].seconds < p[1].seconds)
            && self.mix_in.is_none_or(time)
            && self.mix_out.is_none_or(time)
            && !matches!((self.mix_in, self.mix_out), (Some(a), Some(b)) if a >= b)
            && self
                .transition_bars
                .is_none_or(|n| [8, 16, 32].contains(&n))
            && self
                .bass_swap
                .is_none_or(|v| v.is_finite() && (0.1..=0.9).contains(&v))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueueTrack {
    #[serde(default)]
    pub grid: Option<BeatGrid>,
    pub path: String,
    pub title: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomixPlan {
    #[serde(default)]
    pub vocal_overlap_seconds: Option<f64>,
    #[serde(default)]
    pub vocal_note: String,
    pub outgoing_seconds: f64,
    pub incoming_seconds: f64,
    pub duration_seconds: f64,
    pub bars: Option<u8>,
    pub bass_swap: f32,
    pub phrase_aligned: bool,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomixState {
    #[serde(default)]
    pub plan: Option<AutomixPlan>,
    #[serde(default = "smart_default")]
    pub smart: bool,
    pub enabled: bool,
    pub queue: Vec<QueueTrack>,
    pub next: Option<QueueTrack>,
    pub active_deck: Option<usize>,
    pub fade_seconds: f64,
    pub transitioning: bool,
    pub status: String,
}
fn smart_default() -> bool {
    true
}
impl Default for AutomixState {
    fn default() -> Self {
        Self {
            plan: None,
            smart: true,
            enabled: false,
            queue: Vec::new(),
            next: None,
            active_deck: None,
            fade_seconds: 8.,
            transitioning: false,
            status: "Ready · drop tracks here or queue a playlist".into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub automix: AutomixState,
    pub decks: Vec<DeckState>,
    pub master_gain: f32,
    pub crossfader: f32,
    pub crossfader_curve: CrossfaderCurve,
    pub master_peak: f32,
    pub master_stereo_peak: [f32; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeckState {
    #[serde(default)]
    pub loudness: Option<LoudnessInfo>,
    #[serde(default)]
    pub auto_trim: bool,
    #[serde(default)]
    pub loudness_applied: bool,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sample_names: [String; 16],
    #[serde(default)]
    pub sample_loading: [bool; 16],
    #[serde(default)]
    pub sample_errors: [Option<String>; 16],
    #[serde(default)]
    pub samples: [u8; 16],
    #[serde(default)]
    pub auto_scratch: bool,
    #[serde(default)]
    pub hot_cues: [Option<HotCue>; HOT_CUES],
    #[serde(default)]
    pub cue_appearance: [CueAppearance; HOT_CUES],
    #[serde(default)]
    pub effect_tail: bool,
    #[serde(default)]
    pub effects: [EffectState; EFFECT_SLOTS],
    pub loop_region: Option<[f64; 2]>,
    pub loop_roll: bool,
    pub roll_position_seconds: Option<f64>,
    pub track: Option<TrackInfo>,
    /// A load has started but nothing is playable yet.
    pub loading: bool,
    /// Set when the last load failed.
    pub error: Option<String>,
    pub playing: bool,
    pub position_seconds: f64,
    pub tempo: f64,
    pub key_lock: bool,
    pub volume: f32,
    pub trim_db: f32,
    pub eq: [f32; 3],
    pub peak: f32,
    pub eq_mode: EqMode,
    pub stem_gains: [f32; 4],
    pub stems_ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoudnessInfo {
    pub rms_db: f32,
    pub peak_db: f32,
}
impl LoudnessInfo {
    pub fn recommended_trim(self) -> f32 {
        (-20. - self.rms_db)
            .min(-6. - self.peak_db)
            .clamp(-12., 12.)
    }
    pub fn valid(self) -> bool {
        self.rms_db.is_finite()
            && self.peak_db.is_finite()
            && (-100. ..=40.).contains(&self.rms_db)
            && (-100. ..=40.).contains(&self.peak_db)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackInfo {
    pub title: String,
    /// An estimate until the track is fully loaded.
    pub duration_seconds: f64,
    /// How much of the track has been decoded and can be played.
    pub loaded_seconds: f64,
    pub fully_loaded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play() -> Action {
        Action::Deck {
            deck: 0,
            action: DeckAction::Play,
        }
    }

    fn master() -> Action {
        Action::Mixer {
            action: MixerAction::SetMasterGain { level: 0.5 },
        }
    }

    #[test]
    fn roles_gate_actions() {
        assert!(Role::Host.allows(&play()));
        assert!(Role::Host.allows(&master()));
        assert!(Role::CoDj.allows(&play()));
        assert!(!Role::CoDj.allows(&master()));
        assert!(!Role::Viewer.allows(&play()));
        assert!(!Role::Viewer.allows(&master()));
    }

    #[test]
    fn client_message_wire_format() {
        let message = ClientMessage {
            version: PROTOCOL_VERSION,
            seq: 7,
            action: Action::Deck {
                deck: 1,
                action: DeckAction::Scratch { speed: Some(-0.5) },
            },
        };
        let json = serde_json::to_string(&message).unwrap();
        assert_eq!(
            json,
            r#"{"version":1,"seq":7,"action":{"target":"deck","deck":1,"action":{"op":"scratch","speed":-0.5}}}"#
        );
        assert_eq!(
            serde_json::from_str::<ClientMessage>(&json).unwrap(),
            message
        );
    }

    #[test]
    fn server_message_round_trips() {
        let message = ServerMessage::State {
            state: SessionState {
                decks: vec![DeckState {
                    track: Some(TrackInfo {
                        title: "a".into(),
                        duration_seconds: 1.5,
                        loaded_seconds: 1.5,
                        fully_loaded: true,
                    }),
                    tempo: 1.0,
                    ..Default::default()
                }],
                master_gain: 1.0,
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&message).unwrap();
        assert_eq!(
            serde_json::from_str::<ServerMessage>(&json).unwrap(),
            message
        );
    }
}
