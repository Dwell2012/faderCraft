//! djcraft audio engine. No UI code lives here: the engine takes
//! [`Action`]s from the control protocol and reports a [`SessionState`].

mod automix;
mod cache;
mod effects;
mod eq;
mod mixer;
mod output;
mod peak_guard;
mod processed_peak;
pub use processed_peak::measure_key_lock_peak;
mod sampler;
mod spectrum;
mod track;
mod transition;
#[cfg(windows)]
mod wasapi_output;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use djcraft_protocol::{
    Action, CrossfaderCurve, CrossfaderRouting, DeckAction, DeckState, EqMode, MixerAction,
    Rejection, Role, SessionState, TrackInfo,
};

use mixer::{Command, Shared};

pub use mixer::crossfader_gains;
pub use mixer::cue_mix_gains;
pub use mixer::routed_crossfader_gain;
pub use mixer::{DECKS, MAX_SCRATCH_SPEED, MAX_TEMPO, MIN_TEMPO, Mixer};
pub use output::OutputRouting;
pub use output::output_devices;
pub use track::{CHANNELS, DecodeError, Track};

const COMMAND_QUEUE: usize = 1024;
const GARBAGE_QUEUE: usize = 64;

#[derive(Debug)]
pub enum EngineError {
    NoOutputDevice,
    Device(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::NoOutputDevice => f.write_str("no audio output device"),
            EngineError::Device(message) => write!(f, "audio device error: {message}"),
        }
    }
}

impl std::error::Error for EngineError {}

struct DeckControl {
    tempo_range: djcraft_protocol::TempoFaderRange,
    auto_trim: bool,
    loudness_applied: bool,
    processed_peak: Option<(f64, f32)>,
    trim_refresh_live: bool,
    grid: Option<djcraft_protocol::BeatGrid>,
    last_load: Option<u64>,
    path: Option<String>,
    requested_path: Option<String>,
    hot_cues: [Option<djcraft_protocol::HotCue>; djcraft_protocol::HOT_CUES],
    cue_appearance: [djcraft_protocol::CueAppearance; djcraft_protocol::HOT_CUES],
    effects: [djcraft_protocol::EffectState; djcraft_protocol::EFFECT_SLOTS],
    effect_beats: f32,
    loop_roll: bool,
    loop_region: Option<[f64; 2]>,
    track: Option<Arc<Track>>,
    /// Identifies the load in progress, so a superseded one is discarded.
    loading: Option<u64>,
    error: Option<String>,
    tempo: f64,
    key_lock: bool,
    key_shift: f32,
    volume: f32,
    trim_db: f32,
    eq: [f32; 3],
    eq_mode: EqMode,
    stem_gains: [f32; 4],
}

impl DeckControl {
    fn loudness(&self) -> Option<djcraft_protocol::LoudnessInfo> {
        let mut report = self.track.as_ref()?.loudness()??;
        if self.key_shift == 0. && self.key_lock && (self.tempo - 1.).abs() > 1e-3 {
            if let Some((ratio, peak)) = self.processed_peak {
                if ratio == self.tempo {
                    report.peak_db = peak;
                }
            }
        }
        Some(report)
    }
}

struct Loaded {
    deck: usize,
    load_id: u64,
    result: Result<Arc<Track>, DecodeError>,
}

/// The control side of the engine. Lives on the UI thread; the matching
/// [`Mixer`] runs on the audio thread.
pub struct Engine {
    preparations: std::collections::BTreeMap<String, djcraft_protocol::TrackPreparation>,
    automix: automix::Automix,
    sample_worker: Option<sampler::Worker>,
    sample_revision: [u64; 16],
    sample_names: [String; 16],
    sample_loading: [bool; 16],
    sample_errors: [Option<String>; 16],
    sample_pending: Option<sampler::Ready>,
    sample_rate: u32,
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Arc<Track>>,
    shared: Arc<Shared>,
    decks: [DeckControl; DECKS],
    master_gain: f32,
    sampler_gain: f32,
    monitor: djcraft_protocol::MonitorState,
    crossfader: f32,
    crossfader_curve: CrossfaderCurve,
    crossfader_routing: CrossfaderRouting,
    next_load_id: u64,
    cache_dir: Option<PathBuf>,
    loaded_tx: mpsc::Sender<Loaded>,
    loaded_rx: mpsc::Receiver<Loaded>,
    output_description: String,
    _stream: Option<cpal::Stream>,
    #[cfg(windows)]
    exclusive_stream: Option<wasapi_output::Stream>,
}

impl Engine {
    /// Starts the engine on the default audio output device.
    pub fn new() -> Result<Self, EngineError> {
        Self::with_output(false, None, 0)
    }
    /// Opens an explicit backend/device. Changes are applied when the app restarts.
    pub fn with_output(
        asio: bool,
        name: Option<&str>,
        buffer_frames: u32,
    ) -> Result<Self, EngineError> {
        Self::with_routing(asio, name, buffer_frames, OutputRouting::default())
    }

    pub fn with_routing(
        asio: bool,
        name: Option<&str>,
        buffer_frames: u32,
        routing: OutputRouting,
    ) -> Result<Self, EngineError> {
        use cpal::traits::DeviceTrait;
        let (device, config) = output::device(asio, name, routing)?;
        let description = device
            .description()
            .map(|d| d.name().to_owned())
            .unwrap_or_default();
        let (mut engine, mixer) = Self::headless(config.sample_rate());
        engine.output_description = format!(
            "{} · {} · {} Hz · {}",
            if asio { "ASIO" } else { "System audio" },
            description,
            config.sample_rate(),
            if buffer_frames == 0 {
                "driver buffer".to_string()
            } else {
                format!("{buffer_frames} frames (requested)")
            }
        );
        engine.output_description.push_str(&format!(
            " · master {}/{} · cue {}",
            routing.master + 1,
            routing.master + 2,
            routing
                .cue
                .map(|c| format!("{}/{}", c + 1, c + 2))
                .unwrap_or_else(|| "off".into())
        ));
        engine._stream = Some(output::start(
            &device,
            config,
            mixer,
            buffer_frames,
            routing,
        )?);
        Ok(engine)
    }
    pub fn with_wasapi_exclusive(
        name: Option<&str>,
        buffer_frames: u32,
    ) -> Result<Self, EngineError> {
        #[cfg(windows)]
        {
            wasapi_output::open(name, buffer_frames)
        }
        #[cfg(not(windows))]
        {
            let _ = (name, buffer_frames);
            Err(EngineError::Device("WASAPI requires Windows".into()))
        }
    }
    pub fn output_description(&self) -> &str {
        #[cfg(windows)]
        if self.exclusive_stream.as_ref().is_some_and(|s| s.failed()) {
            return "WASAPI exclusive stopped: device disconnected or output failed. Restart audio by restarting DJCraft.";
        }
        &self.output_description
    }

    /// Creates an engine with no audio device. The caller drives the returned
    /// [`Mixer`] itself, for offline rendering and tests.
    pub fn headless(sample_rate: u32) -> (Self, Mixer) {
        let (commands, command_rx) = rtrb::RingBuffer::new(COMMAND_QUEUE);
        let (garbage_tx, garbage) = rtrb::RingBuffer::new(GARBAGE_QUEUE);
        let shared = Arc::new(Shared::default());
        let mixer = Mixer::new(sample_rate, command_rx, garbage_tx, shared.clone());
        let (loaded_tx, loaded_rx) = mpsc::channel();
        let engine = Self {
            preparations: Default::default(),
            automix: automix::Automix::default(),
            sample_worker: None,
            sample_revision: [0; 16],
            sample_names: std::array::from_fn(|_| String::new()),
            sample_loading: [false; 16],
            sample_errors: std::array::from_fn(|_| None),
            sample_pending: None,
            sample_rate,
            commands,
            garbage,
            shared,
            decks: std::array::from_fn(|_| DeckControl {
                auto_trim: true,
                loudness_applied: false,
                processed_peak: None,
                trim_refresh_live: false,
                grid: None,
                last_load: None,
                path: None,
                requested_path: None,
                track: None,
                loading: None,
                error: None,
                tempo: 1.0,
                key_lock: false,
                tempo_range: Default::default(),
                key_shift: 0.,
                volume: 1.0,
                trim_db: 0.,
                eq: [1.; 3],
                eq_mode: EqMode::Classic,
                loop_region: None,
                loop_roll: false,
                hot_cues: [None; djcraft_protocol::HOT_CUES],
                cue_appearance: Default::default(),
                effects: djcraft_protocol::default_effects(),
                effect_beats: djcraft_protocol::default_effect_beats(),
                stem_gains: [1.; 4],
            }),
            master_gain: 1.0,
            sampler_gain: 1.,
            monitor: Default::default(),
            crossfader: 0.5,
            crossfader_curve: CrossfaderCurve::default(),
            crossfader_routing: CrossfaderRouting::default(),
            next_load_id: 0,
            cache_dir: None,
            loaded_tx,
            loaded_rx,
            output_description: "Offline".into(),
            _stream: None,
            #[cfg(windows)]
            exclusive_stream: None,
        };
        (engine, mixer)
    }

    /// Sets where waveforms are cached between loads. Off by default.
    pub fn set_cache_dir(&mut self, dir: impl Into<PathBuf>) {
        self.cache_dir = Some(dir.into());
    }

    /// Restore the private sample bank, seeding absent slots with original defaults.
    pub fn set_sampler_dir(&mut self, dir: impl Into<PathBuf>) {
        if self.sample_worker.is_some() {
            return;
        }
        self.sample_worker = Some(sampler::Worker::new(self.sample_rate, Some(dir.into())));
        for slot in 0..16 {
            let _ = self.request_sample(slot, sampler::Source::Restore);
        }
    }
    fn request_sample(&mut self, slot: usize, source: sampler::Source) -> Result<(), Rejection> {
        if slot >= 16 {
            return Err(Rejection::InvalidValue);
        }
        let worker = self
            .sample_worker
            .get_or_insert_with(|| sampler::Worker::new(self.sample_rate, None));
        let revision = self.sample_revision[slot] + 1;
        worker
            .tx
            .try_send(sampler::Request {
                slot,
                revision,
                source,
            })
            .map_err(|_| Rejection::Busy)?;
        self.sample_revision[slot] = revision;
        self.sample_loading[slot] = true;
        self.sample_errors[slot] = None;
        Ok(())
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The track loaded on a deck, for drawing its waveform.
    pub fn track(&self, deck: usize) -> Option<&Arc<Track>> {
        self.decks.get(deck)?.track.as_ref()
    }

    /// Whether replacing this playing deck could interrupt the master output.
    /// Use the signal route, not an instantaneous meter: quiet beats and breaks
    /// must not bypass a live-deck warning. Fully closed controls are safe.
    pub fn deck_has_open_output(&mut self, deck: usize) -> bool {
        let state = self.state();
        let Some(channel) = state.decks.get(deck) else {
            return false;
        };
        (channel.playing || channel.effect_tail || channel.peak > 0.00001)
            && channel.track.is_some()
            && channel.volume > 0.
            && state.master_gain > 0.
            && mixer::routed_crossfader_gain(
                deck,
                state.crossfader_routing,
                mixer::crossfader_gains(state.crossfader, state.crossfader_curve),
            ) > 1e-6
            && if channel.effect_tail
                || channel.peak > 0.00001
                || channel
                    .effects
                    .iter()
                    .any(|fx| fx.enabled && fx.amount > 0. && fx.kind.has_tail())
            {
                true
            } else if channel.eq_mode == EqMode::Classic {
                channel.eq.iter().any(|gain| *gain > 0.)
                    && (!channel.stems_ready || channel.stem_gains.iter().any(|gain| *gain > 0.))
            } else {
                !channel.stems_ready || channel.stem_gains.iter().any(|gain| *gain > 0.)
            }
    }

    /// Loads a track onto a deck. It may still be decoding.
    pub fn load_track(&mut self, deck: usize, track: Arc<Track>) -> Result<(), Rejection> {
        if deck >= DECKS {
            return Err(Rejection::UnknownDeck);
        }
        if self.commands.slots() < 2 {
            return Err(Rejection::Busy);
        }
        self.send(Command::Load {
            deck,
            track: track.clone(),
        })?;
        self.send(Command::Trim { deck, gain: 1. })?;
        let control = &mut self.decks[deck];
        control.auto_trim = true;
        control.loudness_applied = false;
        control.processed_peak = None;
        control.trim_refresh_live = false;
        control.trim_db = 0.;
        control.grid = None;
        control.track = Some(track);
        control.path = None;
        control.last_load = None;
        control.hot_cues = [None; djcraft_protocol::HOT_CUES];
        control.cue_appearance = Default::default();
        control.loop_region = None;
        control.stem_gains = [1.; 4];
        control.loading = None;
        control.error = None;
        Ok(())
    }

    /// Applies an action on behalf of a participant with the given role.
    pub fn apply(&mut self, role: Role, action: Action) -> Result<(), Rejection> {
        if !role.allows(&action) {
            return Err(Rejection::NotPermitted);
        }
        match action {
            Action::Mixer {
                action: MixerAction::SetTrackPreparation { path, preparation },
            } => {
                if path.is_empty() || !preparation.valid() {
                    return Err(Rejection::InvalidValue);
                }
                self.preparations.insert(path, preparation);
                if !self.automix.state.transitioning {
                    self.automix.state.plan = None;
                }
                Ok(())
            }
            Action::Mixer {
                action:
                    action @ (MixerAction::AutomixSmart { .. }
                    | MixerAction::AutomixEnqueue { .. }
                    | MixerAction::AutomixRemove { .. }
                    | MixerAction::AutomixMove { .. }
                    | MixerAction::AutomixClear
                    | MixerAction::AutomixStart
                    | MixerAction::AutomixStop
                    | MixerAction::AutomixNext
                    | MixerAction::AutomixFadeTime { .. }),
            } => self.automix_action(action),
            Action::Deck { deck, action } => {
                let deck = deck as usize;
                if deck >= DECKS {
                    return Err(Rejection::UnknownDeck);
                }
                if self.automix.state.enabled
                    && !(self.automix.waiting_for_deck && matches!(action, DeckAction::Pause))
                    && !matches!(
                        action,
                        DeckAction::SetBeatGrid { .. }
                            | DeckAction::SetTempoRange { .. }
                            | DeckAction::EndNudge
                            | DeckAction::SetHotCues { .. }
                            | DeckAction::SetCueAppearance { .. }
                            | DeckAction::SetEffectTempo { .. }
                            | DeckAction::LoadSample { .. }
                            | DeckAction::DefaultSample { .. }
                            | DeckAction::CaptureSample { .. }
                            | DeckAction::Sample { .. }
                            | DeckAction::ClearSample { .. }
                    )
                {
                    self.stop_automix("Manual deck control · automix stopped")?;
                }
                self.apply_deck(deck, action)
            }
            Action::Mixer {
                action: MixerAction::SetCrossfader { position },
            } => {
                let position = unit_level(position)?;
                if self.automix.state.enabled {
                    self.stop_automix("Manual crossfader control · automix stopped")?;
                }
                self.send(Command::Crossfader { position })?;
                self.crossfader = position;
                Ok(())
            }
            Action::Mixer {
                action: MixerAction::SetCrossfaderRouting { routing },
            } => {
                if (routing.left | routing.right) & !0b1111 != 0 {
                    return Err(Rejection::InvalidValue);
                }
                if self.automix.state.enabled {
                    self.stop_automix("Manual crossfader routing · automix stopped")?;
                }
                self.send(Command::CrossfaderRouting { routing })?;
                self.crossfader_routing = routing;
                Ok(())
            }
            Action::Mixer {
                action: MixerAction::SetCrossfaderCurve { curve },
            } => {
                if self.automix.state.enabled {
                    self.stop_automix("Manual curve control · automix stopped")?;
                }
                self.send(Command::CrossfaderCurve { curve })?;
                self.crossfader_curve = curve;
                Ok(())
            }
            Action::Mixer {
                action: MixerAction::SetMasterGain { level },
            } => {
                let level = unit_level(level)?;
                self.send(Command::MasterGain { level })?;
                self.master_gain = level;
                Ok(())
            }
            Action::Mixer {
                action: MixerAction::SetSamplerGain { level },
            } => {
                let level = unit_level(level)?;
                self.send(Command::SamplerGain { level })?;
                self.sampler_gain = level;
                Ok(())
            }
            Action::Mixer {
                action: MixerAction::SetMonitor { monitor },
            } => {
                if monitor.deck as usize >= DECKS {
                    return Err(Rejection::UnknownDeck);
                }
                if monitor
                    .cue_decks
                    .is_some_and(|mask| mask & !((1 << DECKS) - 1) != 0)
                {
                    return Err(Rejection::InvalidValue);
                }
                unit_level(monitor.mix)?;
                unit_level(monitor.gain)?;
                self.send(Command::Monitor { monitor })?;
                self.monitor = monitor;
                Ok(())
            }
        }
    }

    fn apply_deck(&mut self, deck: usize, action: DeckAction) -> Result<(), Rejection> {
        match action {
            DeckAction::SetBeatGrid { grid } => {
                if grid.is_some_and(|g| !g.valid()) {
                    return Err(Rejection::InvalidValue);
                }
                if self.decks[deck].grid != grid && !self.automix.state.transitioning {
                    self.automix.state.plan = None;
                }
                self.decks[deck].grid = grid;
                Ok(())
            }
            DeckAction::LoadSample { slot, path } => {
                self.request_sample(slot as usize, sampler::Source::File(path.into()))
            }
            DeckAction::DefaultSample { slot } => {
                if slot as usize >= sampler::DEFAULTS.len() {
                    return Err(Rejection::InvalidValue);
                }
                self.request_sample(slot as usize, sampler::Source::Default)
            }
            DeckAction::CaptureSample { slot, start, end } => {
                let track = self.decks[deck]
                    .track
                    .as_ref()
                    .ok_or(Rejection::InvalidValue)?;
                if slot >= 16
                    || !start.is_finite()
                    || !end.is_finite()
                    || start < 0.
                    || end - start < 0.01
                    || end - start > 120.
                    || end > track.decoded_seconds()
                {
                    return Err(Rejection::InvalidValue);
                }
                if self.sample_worker.is_some() {
                    return self.request_sample(
                        slot as usize,
                        sampler::Source::Capture {
                            track: track.clone(),
                            start,
                            end,
                        },
                    );
                }
                self.sample_names[slot as usize] = track.title.clone();
                self.send(Command::CaptureSample {
                    slot: slot as usize,
                    track: track.clone(),
                    start: start * self.sample_rate as f64,
                    end: end * self.sample_rate as f64,
                })
            }
            DeckAction::Sample {
                slot,
                play,
                restart,
                repeat,
                level,
            } => {
                if slot >= 16 || !level.is_finite() || !(0. ..=1.).contains(&level) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Sample {
                    slot: slot as usize,
                    play,
                    restart,
                    repeat,
                    level,
                })
            }
            DeckAction::ClearSample { slot } => {
                if slot >= 16 {
                    return Err(Rejection::InvalidValue);
                }
                if self.sample_worker.is_some() {
                    return self.request_sample(slot as usize, sampler::Source::Clear);
                }
                self.sample_names[slot as usize].clear();
                self.send(Command::ClearSample {
                    slot: slot as usize,
                })
            }
            DeckAction::AutoScratch {
                pattern,
                beat_seconds,
                first_beat,
                quantize,
                over,
            } => {
                if pattern.is_some_and(|p| p >= 8)
                    || !beat_seconds.is_finite()
                    || !(0.1..=3.).contains(&beat_seconds)
                    || !first_beat.is_finite()
                {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::AutoScratch {
                    deck,
                    pattern,
                    beat: beat_seconds * self.sample_rate as f64,
                    first: first_beat * self.sample_rate as f64,
                    quantize,
                    over,
                })
            }
            DeckAction::SetCueAppearance { slot, appearance } => {
                if slot as usize >= djcraft_protocol::HOT_CUES
                    || appearance.label.chars().count() > 48
                    || appearance.label.chars().any(char::is_control)
                    || appearance.colour.is_some_and(|c| c > 0xffffff)
                {
                    return Err(Rejection::InvalidValue);
                }
                self.decks[deck].cue_appearance[slot as usize] = appearance;
                Ok(())
            }
            DeckAction::SetHotCues { cues } => {
                if cues.iter().flatten().any(|cue| {
                    !cue.seconds.is_finite()
                        || cue.seconds < 0.
                        || cue
                            .loop_end
                            .is_some_and(|end| !end.is_finite() || end - cue.seconds < 0.01)
                }) {
                    return Err(Rejection::InvalidValue);
                }
                self.decks[deck].hot_cues = cues;
                Ok(())
            }
            DeckAction::CuePreview { seconds, play } => {
                let available = self.decks[deck]
                    .track
                    .as_ref()
                    .map_or(0., |t| t.decoded_seconds());
                let minimum = self.decks[deck].grid.map_or(0., |g| -16. * 60. / g.bpm);
                if !seconds.is_finite() || seconds < minimum || seconds > available {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Cue {
                    deck,
                    frame: seconds * self.sample_rate as f64,
                    end: None,
                    play,
                    keep_loop: false,
                })?;
                self.decks[deck].loop_region = None;
                Ok(())
            }
            DeckAction::TriggerHotCue {
                slot,
                play,
                loop_length,
            } => {
                let cue = self.decks[deck]
                    .hot_cues
                    .get(slot as usize)
                    .copied()
                    .flatten()
                    .ok_or(Rejection::InvalidValue)?;
                let end = loop_length.map(|len| cue.seconds + len).or(cue.loop_end);
                let available = self.decks[deck]
                    .track
                    .as_ref()
                    .map_or(0., |t| t.decoded_seconds());
                if cue.seconds >= available
                    || end.is_some_and(|end| {
                        !end.is_finite() || end - cue.seconds < 0.01 || end > available
                    })
                {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Cue {
                    deck,
                    frame: cue.seconds * self.sample_rate as f64,
                    end: end.map(|end| end * self.sample_rate as f64),
                    play,
                    keep_loop: true,
                })?;
                self.decks[deck].loop_region = end
                    .map(|end| [cue.seconds, end])
                    .or_else(|| mixer::loop_at(self.decks[deck].loop_region, cue.seconds));
                Ok(())
            }
            DeckAction::Jump { seconds } => {
                if !seconds.is_finite() {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Jump {
                    deck,
                    frames: seconds * self.sample_rate as f64,
                })?;
                self.decks[deck].loop_region = None;
                Ok(())
            }
            DeckAction::StartPadLoop { start, end, roll } => {
                let available = self.decks[deck]
                    .track
                    .as_ref()
                    .map_or(0., |t| t.decoded_seconds());
                if !start.is_finite()
                    || !end.is_finite()
                    || start < 0.
                    || end - start < 0.01
                    || end > available
                {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::PadLoop {
                    deck,
                    start: start * self.sample_rate as f64,
                    end: end * self.sample_rate as f64,
                    roll,
                })?;
                self.decks[deck].loop_region = Some([start, end]);
                self.decks[deck].loop_roll = roll;
                Ok(())
            }
            DeckAction::ArmLoop { start, end } => {
                let available = self.decks[deck]
                    .track
                    .as_ref()
                    .map_or(0., |t| t.decoded_seconds());
                if !start.is_finite()
                    || !end.is_finite()
                    || start < 0.
                    || end - start < 0.01
                    || end > available
                {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::ArmLoop {
                    deck,
                    start: start * self.sample_rate as f64,
                    end: end * self.sample_rate as f64,
                })?;
                self.decks[deck].loop_region = Some([start, end]);
                self.decks[deck].loop_roll = false;
                Ok(())
            }
            DeckAction::SetLoop { start, end } => {
                let available = self.decks[deck]
                    .track
                    .as_ref()
                    .map_or(0., |t| t.decoded_frames() as f64 / self.sample_rate as f64);
                if !start.is_finite()
                    || !end.is_finite()
                    || start < 0.
                    || end - start < 0.01
                    || end > available
                {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Loop {
                    deck,
                    region: Some([
                        start * self.sample_rate as f64,
                        end * self.sample_rate as f64,
                    ]),
                })?;
                self.decks[deck].loop_region = Some([start, end]);
                Ok(())
            }
            DeckAction::SetEffectTempo { bpm } => {
                if bpm.is_some_and(|bpm| !bpm.is_finite() || !(40.0..=300.0).contains(&bpm)) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::EffectTempo { deck, bpm })
            }
            DeckAction::SetEffectBeats { beats } => {
                if !beats.is_finite() || !(0.125..=8.).contains(&beats) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::EffectBeats { deck, beats })?;
                self.decks[deck].effect_beats = beats;
                for effect in &mut self.decks[deck].effects {
                    effect.beats = beats;
                }
                Ok(())
            }
            DeckAction::SetEffect { slot, mut effect } => {
                if slot as usize >= djcraft_protocol::EFFECT_SLOTS
                    || !effect.feedback.is_finite()
                    || !(0.0..=0.9).contains(&effect.feedback)
                    || !effect.beats.is_finite()
                    || !(0.125..=8.0).contains(&effect.beats)
                    || !effect.amount.is_finite()
                    || !effect.parameter.is_finite()
                    || !(0.0..=1.0).contains(&effect.amount)
                    || !(0.0..=1.0).contains(&effect.parameter)
                {
                    return Err(Rejection::InvalidValue);
                }
                effect.beats = self.decks[deck].effect_beats;
                self.send(Command::Effect {
                    deck,
                    slot: slot as usize,
                    effect,
                })?;
                self.decks[deck].effects[slot as usize] = effect;
                Ok(())
            }
            DeckAction::SetLoopRoll { enabled } => {
                self.send(Command::LoopRoll { deck, enabled })?;
                self.decks[deck].loop_roll = enabled;
                Ok(())
            }
            DeckAction::ClearLoop => {
                self.send(Command::Loop { deck, region: None })?;
                self.decks[deck].loop_region = None;
                Ok(())
            }
            DeckAction::Load { path } => {
                self.decks[deck].requested_path = Some(path.clone());
                let load_id = self.next_load_id;
                self.next_load_id += 1;
                self.decks[deck].loading = Some(load_id);
                self.decks[deck].last_load = Some(load_id);
                self.decks[deck].error = None;
                let sample_rate = self.sample_rate;
                let loaded = self.loaded_tx.clone();
                let cache_dir = self.cache_dir.clone();
                std::thread::spawn(move || {
                    let send = |result| {
                        let _ = loaded.send(Loaded {
                            deck,
                            load_id,
                            result,
                        });
                    };
                    // The track is handed over as soon as it is playable;
                    // decoding carries on in this thread.
                    let result = Track::load(
                        &PathBuf::from(path),
                        sample_rate,
                        cache_dir.as_deref(),
                        |track| send(Ok(track)),
                    );
                    if let Err(error) = result {
                        send(Err(error));
                    }
                });
                Ok(())
            }
            DeckAction::Play => self.send(Command::Play { deck }),
            DeckAction::Pause => self.send(Command::Pause { deck }),
            DeckAction::Seek { seconds } => {
                if !seconds.is_finite() {
                    return Err(Rejection::InvalidValue);
                }
                let minimum = self.decks[deck].grid.map_or(0., |g| -16. * 60. / g.bpm);
                let seconds = seconds.max(minimum);
                let frame = seconds * self.sample_rate as f64;
                self.send(Command::Seek { deck, frame })?;
                self.decks[deck].loop_region =
                    mixer::loop_at(self.decks[deck].loop_region, seconds);
                Ok(())
            }
            DeckAction::SetTempo { ratio } => {
                if !(MIN_TEMPO..=MAX_TEMPO).contains(&ratio) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Tempo { deck, ratio })?;
                if self.decks[deck].tempo != ratio {
                    self.decks[deck].processed_peak = None;
                    self.decks[deck].trim_refresh_live = true;
                    self.decks[deck].loudness_applied = false;
                }
                self.decks[deck].tempo = ratio;
                Ok(())
            }
            DeckAction::Nudge { seconds } => {
                if !(-0.5..=0.5).contains(&seconds) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Nudge {
                    deck,
                    frames: seconds * self.sample_rate as f64,
                    transient: false,
                })
            }
            DeckAction::PitchBend { seconds } => {
                if !(-0.5..=0.5).contains(&seconds) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Nudge {
                    deck,
                    frames: seconds * self.sample_rate as f64,
                    transient: true,
                })
            }
            DeckAction::EndNudge => self.send(Command::EndNudge { deck }),
            DeckAction::Sync {
                ratio,
                offset_seconds,
            } => {
                if !(MIN_TEMPO..=MAX_TEMPO).contains(&ratio)
                    || !(-0.5..=0.5).contains(&offset_seconds)
                {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Sync {
                    deck,
                    ratio,
                    frames: offset_seconds * self.sample_rate as f64,
                })?;
                if self.decks[deck].tempo != ratio {
                    self.decks[deck].processed_peak = None;
                    self.decks[deck].trim_refresh_live = true;
                    self.decks[deck].loudness_applied = false;
                }
                self.decks[deck].tempo = ratio;
                Ok(())
            }
            DeckAction::SetKeyShift { semitones } => {
                if !(-12.0..=12.0).contains(&semitones) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::KeyShift { deck, semitones })?;
                self.decks[deck].key_shift = semitones;
                self.decks[deck].processed_peak = None;
                Ok(())
            }
            DeckAction::SetTempoRange { range } => {
                self.decks[deck].tempo_range = range;
                Ok(())
            }
            DeckAction::SetKeyLock { on } => {
                self.send(Command::KeyLock { deck, on })?;
                if self.decks[deck].key_lock != on {
                    self.decks[deck].processed_peak = None;
                    self.decks[deck].trim_refresh_live = true;
                    self.decks[deck].loudness_applied = false;
                }
                self.decks[deck].key_lock = on;
                Ok(())
            }
            DeckAction::SetVolume { level } => {
                let level = unit_level(level)?;
                self.send(Command::Volume { deck, level })?;
                self.decks[deck].volume = level;
                Ok(())
            }
            DeckAction::SetEqMode { mode } => {
                self.send(Command::EqMode {
                    deck,
                    mode,
                    eq: self.decks[deck].eq,
                })?;
                self.decks[deck].eq_mode = mode;
                self.decks[deck].stem_gains = [1.; 4];
                Ok(())
            }
            DeckAction::SetStemGains { gains } => {
                if gains.iter().any(|gain| !(0.0..=2.0).contains(gain)) {
                    return Err(Rejection::InvalidValue);
                }
                if !self.decks[deck]
                    .track
                    .as_ref()
                    .is_some_and(|track| track.stems_ready())
                {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::StemGains { deck, gains })?;
                self.decks[deck].stem_gains = gains;
                Ok(())
            }
            DeckAction::SetProcessedPeak { ratio, peak_db } => {
                if !self.decks[deck].key_lock
                    || ratio != self.decks[deck].tempo
                    || !peak_db.is_finite()
                    || !(-100.0..=40.0).contains(&peak_db)
                {
                    return Err(Rejection::InvalidValue);
                }
                self.decks[deck].processed_peak = Some((ratio, peak_db));
                if self.decks[deck].auto_trim {
                    if let Some(report) = self.decks[deck].loudness() {
                        let db = report.recommended_trim();
                        self.send(Command::Trim {
                            deck,
                            gain: 10f32.powf(db / 20.),
                        })?;
                        self.decks[deck].trim_db = db;
                        self.decks[deck].loudness_applied = true;
                    }
                }
                Ok(())
            }
            DeckAction::SetAutoTrim { enabled } => {
                if enabled {
                    if let Some(report) = self.decks[deck].loudness() {
                        let db = report.recommended_trim();
                        self.send(Command::Trim {
                            deck,
                            gain: 10f32.powf(db / 20.),
                        })?;
                        self.decks[deck].trim_db = db;
                        self.decks[deck].loudness_applied = true;
                    }
                }
                self.decks[deck].auto_trim = enabled;
                Ok(())
            }
            DeckAction::SetTrim { db } => {
                if !(-12.0..=12.0).contains(&db) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Trim {
                    deck,
                    gain: 10f32.powf(db / 20.),
                })?;
                self.decks[deck].trim_db = db;
                self.decks[deck].auto_trim = false;
                Ok(())
            }
            DeckAction::SetEq { band, gain } => {
                if self.decks[deck].eq_mode != EqMode::Classic {
                    return Err(Rejection::InvalidValue);
                }
                if band >= 3 || !(0.0..=2.0).contains(&gain) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Eq {
                    deck,
                    band: band as usize,
                    gain,
                })?;
                self.decks[deck].eq[band as usize] = gain;
                Ok(())
            }
            DeckAction::ScratchTo { seconds } => {
                if !seconds.is_finite() {
                    return Err(Rejection::InvalidValue);
                }
                let minimum = self.decks[deck].grid.map_or(0., |g| -16. * 60. / g.bpm);
                let seconds = seconds.max(minimum);
                self.send(Command::ScratchTo {
                    deck,
                    frame: seconds * self.sample_rate as f64,
                    velocity: None,
                })?;
                self.decks[deck].loop_region =
                    mixer::loop_at(self.decks[deck].loop_region, seconds);
                Ok(())
            }
            DeckAction::JogScratch {
                seconds,
                speed,
                interval_seconds,
            } => {
                if !seconds.is_finite()
                    || !speed.is_finite()
                    || !(-MAX_SCRATCH_SPEED..=MAX_SCRATCH_SPEED).contains(&speed)
                    || !(0.001..=0.08).contains(&interval_seconds)
                {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::ScratchTo {
                    deck,
                    frame: seconds.max(0.) * self.sample_rate as f64,
                    velocity: Some((
                        speed,
                        (interval_seconds * 1.5).max(0.015) * self.sample_rate as f64,
                    )),
                })?;
                self.decks[deck].loop_region =
                    mixer::loop_at(self.decks[deck].loop_region, seconds);
                Ok(())
            }
            DeckAction::Scratch { speed } => {
                if speed.is_some_and(|s| !(-MAX_SCRATCH_SPEED..=MAX_SCRATCH_SPEED).contains(&s)) {
                    return Err(Rejection::InvalidValue);
                }
                self.send(Command::Scratch { deck, speed })
            }
        }
    }

    fn send(&mut self, command: Command) -> Result<(), Rejection> {
        self.commands.push(command).map_err(|_| Rejection::Busy)
    }

    /// Finishes pending loads and frees tracks the audio thread has let go
    /// of. Called by [`Engine::state`].
    fn poll(&mut self) {
        while self.garbage.pop().is_ok() {}
        loop {
            let ready = self.sample_pending.take().or_else(|| {
                self.sample_worker
                    .as_ref()
                    .and_then(|w| w.rx.try_recv().ok())
            });
            let Some(ready) = ready else {
                break;
            };
            let slot = ready.slot;
            if self.sample_revision[slot] != ready.revision {
                continue;
            }
            if self.commands.slots() == 0 {
                self.sample_pending = Some(ready);
                break;
            }
            self.sample_loading[slot] = false;
            match ready.result {
                Ok(sample) => {
                    let command = if let Some((track, name)) = sample {
                        self.sample_names[slot] = name;
                        Command::CaptureSample {
                            slot,
                            start: 0.,
                            end: track.decoded_frames() as f64,
                            track,
                        }
                    } else {
                        self.sample_names[slot].clear();
                        Command::ClearSample { slot }
                    };
                    let _ = self.send(command);
                }
                Err(error) => self.sample_errors[slot] = Some(error),
            }
        }
        while let Ok(loaded) = self.loaded_rx.try_recv() {
            if self.decks[loaded.deck].loading != Some(loaded.load_id)
                && !(self.decks[loaded.deck].last_load == Some(loaded.load_id)
                    && loaded.result.is_err())
            {
                continue;
            }
            let outcome = match loaded.result {
                Ok(track) => self
                    .load_track(loaded.deck, track)
                    .map_err(|_| "engine busy".to_string()),
                Err(error) => Err(error.to_string()),
            };
            let control = &mut self.decks[loaded.deck];
            control.loading = None;
            if outcome.is_ok() {
                control.last_load = Some(loaded.load_id);
                control.path = control.requested_path.take();
            }
            control.error = outcome.err();
        }
    }

    /// The current state, as broadcast to every participant.
    pub fn state(&mut self) -> SessionState {
        self.poll();
        for deck in 0..DECKS {
            if self.decks[deck].auto_trim
                && !self.decks[deck].loudness_applied
                && (!self.shared.decks[deck].playing()
                    || (self.decks[deck].trim_refresh_live
                        && (!self.decks[deck].key_lock
                            || (self.decks[deck].tempo - 1.).abs() <= 1e-3)))
            {
                if let Some(report) = self.decks[deck].loudness() {
                    let db = report.recommended_trim();
                    if self
                        .send(Command::Trim {
                            deck,
                            gain: 10f32.powf(db / 20.),
                        })
                        .is_ok()
                    {
                        self.decks[deck].trim_db = db;
                        self.decks[deck].loudness_applied = true;
                    }
                }
            }
        }
        self.tick_automix();
        let sample_rate = self.sample_rate as f64;
        SessionState {
            automix: self.automix.state.clone(),
            decks: self
                .decks
                .iter()
                .zip(&self.shared.decks)
                .map(|(control, shared)| DeckState {
                    key_lock_protection_db: shared.protection_db(),
                    loudness: control.loudness(),
                    auto_trim: control.auto_trim,
                    loudness_applied: control.loudness_applied,
                    path: control.path.clone(),
                    samples: self.shared.sample_states(),
                    sample_names: self.sample_names.clone(),
                    sample_loading: self.sample_loading,
                    sample_errors: self.sample_errors.clone(),
                    auto_scratch: shared.auto_scratch(),
                    loop_region: control.loop_region,
                    loop_roll: control.loop_roll,
                    effects: control.effects,
                    effect_beats: control.effect_beats,
                    hot_cues: control.hot_cues,
                    cue_appearance: control.cue_appearance.clone(),
                    effect_tail: shared.effect_tail(),
                    roll_position_seconds: if control.loop_roll && control.loop_region.is_some() {
                        shared.roll_position().map(|p| p / sample_rate)
                    } else {
                        None
                    },
                    track: control.track.as_ref().map(|track| TrackInfo {
                        title: track.title.clone(),
                        duration_seconds: track.duration_seconds(),
                        loaded_seconds: track.decoded_seconds(),
                        fully_loaded: track.is_complete(),
                    }),
                    loading: control.loading.is_some(),
                    error: control.error.clone(),
                    playing: shared.playing(),
                    position_seconds: shared.position() / sample_rate,
                    tempo: control.tempo,
                    key_lock: control.key_lock,
                    tempo_range: control.tempo_range,
                    key_shift: control.key_shift,
                    volume: control.volume,
                    trim_db: control.trim_db,
                    eq: control.eq,
                    peak: shared.peak(),
                    eq_mode: control.eq_mode,
                    stem_gains: control.stem_gains,
                    stems_ready: control
                        .track
                        .as_ref()
                        .is_some_and(|track| track.stems_ready()),
                })
                .collect(),
            master_gain: self.master_gain,
            sampler_gain: self.sampler_gain,
            monitor: self.monitor,
            crossfader: self.crossfader,
            crossfader_curve: self.crossfader_curve,
            crossfader_routing: self.crossfader_routing,
            master_peak: self.shared.master_peak(),
            master_stereo_peak: self.shared.master_stereo_peak(),
        }
    }
}

fn unit_level(level: f32) -> Result<f32, Rejection> {
    if (0.0..=1.0).contains(&level) {
        Ok(level)
    } else {
        Err(Rejection::InvalidValue)
    }
}
