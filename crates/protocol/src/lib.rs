//! djcraft control protocol, version 1.
//!
//! Every user action is an [`Action`] sent to the session host, which checks it
//! against the sender's [`Role`] and applies it to the engine. The host then
//! broadcasts the resulting [`SessionState`]. The desktop UI and remote clients
//! use the same messages.

use serde::{Deserialize, Serialize};

pub mod script;

pub const PROTOCOL_VERSION: u32 = 1;

/// Number of independently playable decks (A, B, C and D).
pub const DECKS: usize = 4;

/// Zero-based deck index (0 = A, 1 = B, 2 = C, 3 = D).
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
            (Role::CoDj, Action::Deck { .. } | Action::Host { .. }) => true,
            (Role::CoDj, Action::Mixer { .. }) => false,
            (Role::Viewer, _) => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum Action {
    Deck {
        deck: DeckId,
        action: DeckAction,
    },
    Mixer {
        action: MixerAction,
    },
    /// UI-owned control request emitted by fcScript, never sent to the audio engine.
    Host {
        deck: DeckId,
        action: HostAction,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadMode {
    Keyboard,
    HotCues,
    Loop,
    Sampler,
    Stems,
    BeatJump,
    Fx,
    BeatLoop,
    LoopRoll,
    Slicer,
    KeyShift,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum HostAction {
    PadMode {
        mode: PadMode,
    },
    PerformancePad {
        mode: PadMode,
        pad: u8,
        alternate: bool,
    },
    ReleasePerformancePad,
    SamplePad {
        slot: u8,
        stop: bool,
    },
    LoopButton {
        button: LoopButton,
    },
    Cue {
        pressed: bool,
    },
    Sync,
    SelectDeck,
    ToggleVinyl,
    LoadSelected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopButton {
    In,
    Out,
    FourBeat,
    Reloop,
    Half,
    Double,
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
    /// Stem source order is vocals, instruments, bass, drums.
    pub fn stem_groups(self) -> [u8; 4] {
        match self {
            Self::Classic | Self::Stems => [0, 1, 2, 3],
            Self::EzRemix => [0, 1, 1, 2],
            Self::Modern => [1, 1, 2, 0],
        }
    }
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
pub const BEAT_FX_BEATS: [f32; 9] = [0.25, 0.5, 0.75, 1., 2., 4., 8., 16., 32.];
/// The four banks of momentary Pad FX, shared by skins and controller scripts.
pub const PAD_EFFECTS: [EffectKind; 16] = [
    EffectKind::Echo,
    EffectKind::Reverb,
    EffectKind::Flanger,
    EffectKind::Cut,
    EffectKind::Spiral,
    EffectKind::BeatRepeat,
    EffectKind::Brake,
    EffectKind::PitchShift,
    EffectKind::Delay,
    EffectKind::PingPong,
    EffectKind::LowCutEcho,
    EffectKind::EchoOut,
    EffectKind::Phaser,
    EffectKind::Chorus,
    EffectKind::Tremolo,
    EffectKind::Bitcrusher,
];
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
    /// Filter resonance, 0..=1: Butterworth Q through a resonant Q of 4.
    /// Defaults to zero for older effect snapshots.
    #[serde(default)]
    pub resonance: f32,
    pub sync: bool,
    /// Snapshot mirror of the deck's shared timing. SetEffectBeats changes it;
    /// SetEffect preserves the deck timing when replacing a slot.
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
            resonance: 0.5,
            sync: true,
            beats: 0.5,
            tails: true,
        }
    }
}

pub fn default_effects() -> [EffectState; EFFECT_SLOTS] {
    std::array::from_fn(|slot| EffectState {
        kind: match slot {
            0 => EffectKind::Echo,
            1 => EffectKind::Reverb,
            2 => EffectKind::Flanger,
            _ => EffectKind::Echo,
        },
        enabled: false,
        amount: 0.5,
        ..EffectState::default()
    })
}

pub fn default_effect_beats() -> f32 {
    0.5
}

pub fn default_stem_effects() -> [EffectState; 4] {
    [EffectState {
        kind: EffectKind::Echo,
        enabled: false,
        ..EffectState::default()
    }; 4]
}

pub const COLOUR_FX_KINDS: [EffectKind; 5] = [
    EffectKind::Filter,
    EffectKind::Echo,
    EffectKind::Reverb,
    EffectKind::Flanger,
    EffectKind::Bitcrusher,
];
pub const COLOUR_FX_LABELS: [&str; 7] = [
    "Filter",
    "Echo",
    "Reverb",
    "Flanger",
    "Bitcrusher",
    "White Noise",
    "Pink Noise",
];

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColourFxState {
    pub position: f32,
    pub selected: u8,
    pub enabled: bool,
}
impl Default for ColourFxState {
    fn default() -> Self {
        Self {
            position: 0.5,
            selected: 1,
            enabled: true,
        }
    }
}
impl ColourFxState {
    pub fn active(self) -> bool {
        self.enabled && self.selected != 0 && (self.position - 0.5).abs() > 0.0001
    }
    pub fn effects(self) -> [EffectState; 5] {
        let travel = ((self.position - 0.5).abs() * 2.).clamp(0., 1.);
        std::array::from_fn(|index| EffectState {
            kind: COLOUR_FX_KINDS[index],
            enabled: self.enabled && self.selected & (1 << index) != 0 && travel > 0.0001,
            amount: if index == 0 { 1. } else { travel * 0.65 },
            parameter: if index == 0 {
                self.position
            } else {
                0.25 + travel * 0.5
            },
            ..EffectState::default()
        })
    }
}

pub const EFFECT_BEATS: [f32; 7] = [0.125, 0.25, 0.5, 1., 2., 4., 8.];

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
    /// Fader scaling only: changing range preserves the current playback ratio.
    SetTempoRange {
        range: TempoFaderRange,
    },
    /// Background analysis result for the current key-locked tempo.
    SetProcessedPeak {
        ratio: f64,
        peak_db: f32,
    },
    SetAutoTrim {
        enabled: bool,
    },
    SetBeatGrid {
        grid: Option<BeatGrid>,
    },
    /// Replace the deck's drum stem with the selected Rhythm Shift pack.
    SetRhythmShift {
        enabled: bool,
    },
    /// Rhythm Shift groove level, with 1.0 at unity.
    SetRhythmGain {
        level: f32,
    },
    /// Trigger one of the pack's four rolls at the next bar boundary.
    TriggerRhythmRoll {
        slot: u8,
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
    /// Retrigger one hot cue with a pitch offset from the unshifted track.
    PlayKeyboardNote {
        slot: u8,
        semitones: u8,
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
    /// Shared beat division for every effect on this deck.
    SetEffectBeats {
        beats: f32,
    },
    SetEffect {
        slot: u8,
        effect: EffectState,
    },
    /// Momentary Pad FX press/release; the engine owns the temporary slot.
    PadFx {
        pad: u8,
        pressed: bool,
    },
    SetStemEffect {
        stem: u8,
        effect: EffectState,
    },
    SetColourFx {
        state: ColourFxState,
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
    /// Eject the loaded track and cancel any load still in progress.
    Unload,
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
    /// Discard unapplied temporary phase nudges without changing tempo.
    EndNudge,
    /// Jog/button phase bend; unused correction expires after 40 ms of audio.
    PitchBend {
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
    SetQuantize {
        enabled: bool,
    },
    /// Keep an inaudible playhead advancing through scratches and loops.
    SetSlip {
        enabled: bool,
    },
    /// End a momentary Slip Hot Cue and return to background playback.
    SlipReturn,
    /// Transpose independently of tempo, from -12 to +12 semitones.
    SetKeyShift {
        semitones: f32,
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
    /// Freeze a stationary touched jog at the audio head, discarding pending travel.
    /// The touch remains grabbed until Scratch { speed: None }.
    HoldJog,
    /// Timestamped physical jog movement: position plus measured source-seconds
    /// per output-second. Interval supplies a packet-sized following buffer.
    JogScratch {
        seconds: f64,
        speed: f64,
        interval_seconds: f64,
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
    ToggleMasterCue,
    ToggleColourFxBypass,
    SetBeatFx {
        state: BeatFxState,
    },
    Sampler {
        action: SamplerAction,
    },
    SetSamplerGain {
        level: f32,
    },
    SetMonitor {
        monitor: MonitorState,
    },
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
    SetBoothGain {
        level: f32,
    },
    SetCrossfader {
        position: f32,
    },
    SetCrossfaderCurve {
        curve: CrossfaderCurve,
    },
    SetCrossfaderRouting {
        routing: CrossfaderRouting,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeatFxTarget {
    Deck(u8),
    Sampler,
    Master,
}

/// One mixer Beat FX processor with a switchable destination.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeatFxState {
    pub target: BeatFxTarget,
    /// An unsupported selector position leaves the processor unassigned.
    pub kind: Option<EffectKind>,
    pub enabled: bool,
    pub amount: f32,
    pub beats: f32,
}

impl Default for BeatFxState {
    fn default() -> Self {
        Self {
            target: BeatFxTarget::Deck(0),
            kind: None,
            enabled: false,
            amount: 0.5,
            beats: 0.5,
        }
    }
}

impl BeatFxState {
    pub fn effect(self, previous: Option<EffectState>) -> Option<EffectState> {
        let kind = self.kind?;
        let mut effect = previous
            .filter(|effect| effect.kind == kind)
            .unwrap_or_else(|| EffectState {
                kind,
                parameter: if kind == EffectKind::Filter { 0. } else { 0.5 },
                ..EffectState::default()
            });
        effect.enabled = self.enabled;
        effect.beats = self.beats;
        if kind == EffectKind::PitchShift {
            effect.parameter = 0.5 + 0.5 * self.amount;
            effect.amount = 1.;
        } else {
            effect.amount = self.amount;
        }
        Some(effect)
    }
}

/// A–D bitmasks for each crossfader side. Neither side bypasses the crossfader;
/// both sides sum their gains, capped at unity.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossfaderRouting {
    pub left: u8,
    pub right: u8,
}

impl Default for CrossfaderRouting {
    fn default() -> Self {
        Self {
            left: 0b0101,
            right: 0b1010,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossfaderCurve {
    #[default]
    Smooth,
    EqualPower,
    Cut,
    Adjustable {
        sharpness: u8,
        power: bool,
        reverse: bool,
        disabled: bool,
    },
}

impl CrossfaderCurve {
    /// Editable equivalents of legacy presets. Legacy audio remains unchanged
    /// until a setting is edited.
    pub fn parameters(self) -> (u8, bool, bool, bool) {
        match self {
            Self::Smooth => (22, false, false, false),
            Self::EqualPower => (22, true, false, false),
            Self::Cut => (58, false, false, false),
            Self::Adjustable {
                sharpness,
                power,
                reverse,
                disabled,
            } => (sharpness.min(100), power, reverse, disabled),
        }
    }
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
    /// Fraction of the overlap held silent before EQ blending starts.
    #[serde(default)]
    pub eq_start: f32,
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

/// Global sampler bus: deliberately independent of every music deck.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SamplerAction {
    /// Capture one deck's selected stems, independently of the master mix.
    Capture {
        deck: DeckId,
        slot: u8,
        start: f64,
        end: f64,
    },
    SelectBank {
        id: String,
    },
    CreateBank {
        name: String,
    },
    RenameBank {
        id: String,
        name: String,
    },
    Load {
        slot: u8,
        path: String,
    },
    Clear {
        slot: u8,
    },
    Play {
        slot: u8,
        play: bool,
        restart: bool,
        repeat: bool,
        level: f32,
    },
    StopAll,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SampleBank {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SamplerState {
    pub banks: Vec<SampleBank>,
    pub selected_bank: String,
    pub names: [String; 16],
    pub loading: [bool; 16],
    pub errors: [Option<String>; 16],
    pub slots: [u8; 16],
    pub error: Option<String>,
}
impl Default for SamplerState {
    fn default() -> Self {
        Self {
            banks: vec![SampleBank {
                id: "default".into(),
                name: "My Samples".into(),
            }],
            selected_bank: "default".into(),
            names: Default::default(),
            loading: [false; 16],
            errors: Default::default(),
            slots: [0; 16],
            error: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub sampler: SamplerState,
    #[serde(default = "unity_gain")]
    pub sampler_gain: f32,
    #[serde(default)]
    pub monitor: MonitorState,
    #[serde(default)]
    pub master_cue_enabled: bool,
    #[serde(default)]
    pub automix: AutomixState,
    pub decks: Vec<DeckState>,
    pub master_gain: f32,
    #[serde(default)]
    pub beat_fx: BeatFxState,
    #[serde(default)]
    pub colour_fx_bypassed: bool,
    #[serde(default = "unity_gain")]
    pub booth_gain: f32,
    pub crossfader: f32,
    pub crossfader_curve: CrossfaderCurve,
    #[serde(default)]
    pub crossfader_routing: CrossfaderRouting,
    pub master_peak: f32,
    pub master_stereo_peak: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonitorState {
    /// Legacy single cue deck, used when cue_decks is absent.
    pub deck: u8,
    /// Explicit cue selection: bits 0–3 select A–D. Zero switches deck cue off.
    /// Absent preserves the single-deck behaviour of older protocol clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cue_decks: Option<u8>,
    /// 0 = cue bus, 1 = master.
    pub mix: f32,
    pub gain: f32,
}
fn unity_gain() -> f32 {
    1.
}
impl Default for MonitorState {
    fn default() -> Self {
        Self {
            deck: 0,
            cue_decks: None,
            mix: 0.,
            gain: 0.5,
        }
    }
}

impl MonitorState {
    pub fn cue_mask(self) -> u8 {
        self.cue_decks.unwrap_or_else(|| {
            if (self.deck as usize) < DECKS {
                1 << self.deck
            } else {
                0
            }
        })
    }

    pub fn cue_enabled(self, deck: usize) -> bool {
        deck < DECKS && self.cue_mask() & (1 << deck) != 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempoFaderRange {
    #[default]
    Low,
    Medium,
    High,
    Wide,
}
impl TempoFaderRange {
    pub fn percent(self) -> u8 {
        match self {
            Self::Low => 8,
            Self::Medium => 16,
            Self::High => 32,
            Self::Wide => 48,
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Wide,
            Self::Wide => Self::Low,
        }
    }
    pub fn extent(self) -> f64 {
        f64::from(self.percent()) / 100.
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeckState {
    /// False means the deck-wide quantize gate is on (including older saved states).
    #[serde(default)]
    pub quantize_disabled: bool,
    #[serde(default)]
    pub slip_enabled: bool,
    #[serde(default)]
    pub rhythm_shift: bool,
    #[serde(default = "default_rhythm_gain")]
    pub rhythm_gain: f32,
    #[serde(default)]
    pub tempo_range: TempoFaderRange,
    /// Negative dB applied by the key-lock peak protector; zero when inactive.
    #[serde(default)]
    pub key_lock_protection_db: f32,
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
    pub scratching: bool,
    #[serde(default)]
    pub hot_cues: [Option<HotCue>; HOT_CUES],
    #[serde(default)]
    pub cue_appearance: [CueAppearance; HOT_CUES],
    #[serde(default)]
    pub effect_tail: bool,
    #[serde(default = "default_effects")]
    pub effects: [EffectState; EFFECT_SLOTS],
    #[serde(default = "default_stem_effects")]
    pub stem_effects: [EffectState; 4],
    #[serde(default)]
    pub colour_fx: ColourFxState,
    #[serde(default = "default_effect_beats")]
    pub effect_beats: f32,
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
    #[serde(default)]
    pub key_shift: f32,
    pub volume: f32,
    pub trim_db: f32,
    pub eq: [f32; 3],
    pub peak: f32,
    pub eq_mode: EqMode,
    pub stem_gains: [f32; 4],
    pub stems_ready: bool,
}

fn default_rhythm_gain() -> f32 {
    1.
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoudnessInfo {
    pub rms_db: f32,
    pub peak_db: f32,
}
impl LoudnessInfo {
    pub fn recommended_trim(self) -> f32 {
        // Normalize the loudest decoded sample, independent of average loudness.
        // Leave a small numerical margin below full scale. Do not limit attenuation:
        // an over-range source may need more than the manual trim's -12 dB.
        (-0.1 - self.peak_db).min(12.)
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
#[cfg(test)]
mod tempo_range_tests {
    use super::*;
    #[test]
    fn legacy_effect_state_gets_shared_half_beat_timing() {
        let mut old = serde_json::to_value(DeckState::default()).unwrap();
        old.as_object_mut().unwrap().remove("effect_beats");
        old.as_object_mut().unwrap().remove("effects");
        let state: DeckState = serde_json::from_value(old).unwrap();
        assert_eq!(state.effect_beats, 0.5);
        assert_eq!(state.effects, default_effects());
        let action = Action::Deck {
            deck: 1,
            action: DeckAction::SetEffectBeats { beats: 0.25 },
        };
        assert_eq!(
            serde_json::from_str::<Action>(&serde_json::to_string(&action).unwrap()).unwrap(),
            action
        );
    }
    #[test]
    fn legacy_deck_state_defaults_to_low_and_range_wire_values_are_bounded() {
        let mut old = serde_json::to_value(DeckState::default()).unwrap();
        old.as_object_mut().unwrap().remove("tempo_range");
        assert_eq!(
            serde_json::from_value::<DeckState>(old)
                .unwrap()
                .tempo_range,
            TempoFaderRange::Low
        );
        assert!(serde_json::from_str::<TempoFaderRange>("\"100\"").is_err());
        let mut range = TempoFaderRange::default();
        for percent in [8, 16, 32, 48] {
            assert_eq!(range.percent(), percent);
            range = range.next();
        }
        assert_eq!(range, TempoFaderRange::Low);
    }
}

#[cfg(test)]
mod resonance_compatibility_tests {
    use super::*;
    #[test]
    fn shaped_sweeps_have_expected_endpoints_and_monotonic_travel() {
        let cutoff = EffectState::filter_cutoff;
        for (position, hz) in [(0., 20.), (0.49, 20_000.), (0.51, 20.), (1., 20_000.)] {
            assert!((cutoff(position) - hz).abs() < 0.01);
        }
        assert!((cutoff(0.245) - 990.).abs() < 10.);
        assert!((cutoff(0.755) - 274.).abs() < 2.);
        for i in 0..1000 {
            let x = i as f32 / 1000.;
            let next = (i + 1) as f32 / 1000.;
            assert!(cutoff(x * 0.49) <= cutoff(next * 0.49));
            assert!(cutoff(0.51 + x * 0.49) <= cutoff(0.51 + next * 0.49));
        }
    }

    #[test]
    fn old_effect_snapshots_default_to_neutral_resonance_and_new_values_round_trip() {
        let old: EffectState =
            serde_json::from_str(r#"{"parameter":0.25,"feedback":0.6}"#).unwrap();
        assert_eq!(old.resonance, 0.);
        assert_eq!(old.feedback, 0.6);
        let state = EffectState {
            resonance: 0.75,
            ..old
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<EffectState>(&json).unwrap(), state);
    }
}

impl EffectState {
    /// The reserved filter is displayed as off at its neutral centre.
    pub fn active_in_slot(&self, _slot: usize) -> bool {
        self.enabled && (self.kind != EffectKind::Filter || (self.parameter - 0.5).abs() > 0.0001)
    }

    /// Nominal cutoff shared by DSP and UI. Separate power curves ease into
    /// each logarithmic sweep from the neutral centre; DSP caps at 0.45 * rate.
    pub fn filter_cutoff(parameter: f32) -> f32 {
        if parameter < 0.5 {
            let travel = ((0.49 - parameter) / 0.49).clamp(0., 1.);
            20_000. * 0.001f32.powf(travel.powf(1.2))
        } else {
            let travel = ((parameter - 0.51) / 0.49).clamp(0., 1.);
            20. * 1000f32.powf(travel.powf(1.4))
        }
    }
}
