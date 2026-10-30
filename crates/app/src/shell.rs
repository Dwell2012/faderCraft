use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use djcraft_engine::{DECKS, Engine, MAX_TEMPO, MIN_TEMPO, Track};
use djcraft_library::TempoResult;
use djcraft_protocol::{Action, CrossfaderCurve, DeckAction, DeckState, EqMode, MixerAction, Role};
#[path = "automix_controls.rs"]
mod automix_controls;
#[path = "deck_header.rs"]
mod deck_header;
#[path = "effect_controls.rs"]
mod effect_controls;
#[path = "loop_controls.rs"]
mod loop_controls;
#[path = "mixer_controls.rs"]
mod mixer_controls;
#[path = "pad_controls.rs"]
mod pad_controls;
#[path = "preparation_controls.rs"]
mod preparation_controls;
#[path = "pro_layout.rs"]
mod pro_layout;
#[path = "stem_controls.rs"]
mod stem_controls;
#[path = "suggestion_controls.rs"]
mod suggestion_controls;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable, TitleBar,
    button::{Button, ButtonRounded, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::library_view::{DraggedTrack, LibraryEvent, LibraryView};
use crate::settings::{Settings, StartTab, data_dir};
use crate::settings_view::{SettingsEvent, SettingsView};
use crate::waveform::{VISIBLE_SECONDS, overview, waveform};

const TOP_BAR_HEIGHT: Pixels = px(52.);
const OVERVIEW_HEIGHT: f32 = 44.;

pub(crate) fn deck_label(deck: usize) -> &'static str {
    ["A", "B", "C", "D"][deck]
}

fn deck_colour(deck: usize) -> Rgba {
    rgb(crate::waveform::deck_palette(deck)[1])
}

/// Visual clock follows audio with gradual corrections between buffer updates.
#[derive(Default, Clone, Copy)]
struct DisplayPosition(Option<f64>);

impl DisplayPosition {
    fn update(&mut self, raw: f64, playing: bool, tempo: f64, elapsed: f64) -> f64 {
        let next = match self.0 {
            Some(previous) if playing && elapsed < 0.2 => {
                let predicted = previous + elapsed * tempo;
                let error = raw - predicted;
                if error.abs() > 0.15 {
                    raw // Seek, load or another discontinuity: never animate across it.
                } else {
                    let corrected = predicted + error * (1. - (-elapsed / 0.12).exp());
                    corrected.clamp(raw - 0.05, raw + 0.05)
                }
            }
            _ => raw,
        };
        self.0 = Some(next);
        next
    }
}

#[derive(Clone, Copy)]
struct WaveformDrag {
    deck: usize,
    origin_x: f32,
    origin_seconds: f64,
    seconds_per_pixel: f64,
    duration: f64,
    minimum: f64,
    position: f64,
    resume: bool,
    vinyl: bool,
}

impl WaveformDrag {
    fn at(&self, x: f32) -> f64 {
        (self.origin_seconds + (x - self.origin_x) as f64 * self.seconds_per_pixel)
            .clamp(self.minimum, self.duration)
    }
}

/// Track uninterrupted transport playback independently of seek position or output gain.
#[derive(Default)]
struct SyncPlayback {
    path: Option<String>,
    started: Option<Instant>,
}

impl SyncPlayback {
    fn update(&mut self, path: Option<&str>, playing: bool, now: Instant) {
        if self.path.as_deref() != path {
            self.path = path.map(str::to_owned);
            self.started = None;
        }
        if playing && path.is_some() {
            self.started.get_or_insert(now);
        } else {
            self.started = None;
        }
    }
}

fn select_sync_reference(
    deck: usize,
    visible: usize,
    grids: &[Option<TempoResult>],
    playback: &[SyncPlayback; DECKS],
) -> Option<usize> {
    if visible > 2
        && let Some(other) = (0..visible)
            .filter(|&other| other != deck && grids[other].is_some())
            .filter_map(|other| playback[other].started.map(|started| (other, started)))
            .min_by_key(|&(other, started)| (started, other))
            .map(|(other, _)| other)
    {
        return Some(other);
    }
    std::iter::once(deck ^ 1)
        .chain(0..visible)
        .find(|&other| other < visible && other != deck && grids[other].is_some())
}

/// Accumulate listening time, never seek distance or pitch-adjusted time.
#[derive(Default)]
struct HistoryTracker {
    path: Option<String>,
    seconds: f64,
    recorded: bool,
}
impl HistoryTracker {
    fn update(&mut self, path: Option<&str>, audible: bool, elapsed: f64) -> bool {
        if self.path.as_deref() != path {
            *self = Self {
                path: path.map(str::to_owned),
                ..Self::default()
            };
            return false;
        }
        if audible && path.is_some() && !self.recorded {
            self.seconds += elapsed.clamp(0., 0.25);
        }
        self.seconds >= 10. && !self.recorded
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Library,
    Live,
    Radio,
}

pub struct Shell {
    stems_starting: bool,
    play_history: [HistoryTracker; DECKS],
    sync_playback: [SyncPlayback; DECKS],
    deck_focus: FocusHandle,
    active_deck: usize,
    monitor: djcraft_protocol::MonitorState,
    focus_loaded_deck: bool,
    cue_preview: Option<(usize, f64, Option<String>)>,
    cue_anchor: [Option<(f64, Option<String>)>; DECKS],
    pitch_drag: Option<usize>,
    pitch_areas: [Rc<Cell<Bounds<Pixels>>>; DECKS],
    automix: djcraft_protocol::AutomixState,
    saved_queue: Vec<djcraft_protocol::QueueTrack>,
    automix_notice: String,
    tab: Tab,
    library: Entity<LibraryView>,
    /// The track browser under the decks.
    browser: Entity<LibraryView>,
    settings: Settings,
    /// The settings dialog, while it is open.
    settings_view: Option<Entity<SettingsView>>,
    /// Set when the dialog has just opened and should take keyboard focus.
    focus_settings: bool,
    engine: Option<Engine>,
    controllers: Option<crate::controllers::Controllers>,
    engine_error: Option<SharedString>,
    /// The file last loaded onto each deck.
    deck_paths: Vec<Option<PathBuf>>,
    /// The beat grid of each deck's track, when it is a library track that
    /// has been analysed.
    grids: Vec<Option<TempoResult>>,
    deck_keys: Vec<Option<djcraft_library::Camelot>>,
    deck_modes: Vec<Option<djcraft_library::ModalKeyResult>>,
    deck_metadata: Vec<Option<(String, String)>>,
    deck_artwork: Vec<Option<Arc<Image>>>,
    artwork_paths: Vec<Option<PathBuf>>,
    /// Where each deck's whole-track waveform is on screen, for seeking.
    overview_areas: Vec<Rc<Cell<Bounds<Pixels>>>>,
    waveform_areas: Vec<Rc<Cell<Bounds<Pixels>>>>,
    drag: Option<WaveformDrag>,
    display_positions: [DisplayPosition; DECKS],
    last_frame: Instant,
    overview_cache: [Rc<std::cell::RefCell<crate::waveform::OverviewCache>>; DECKS],
    pending_seeks: [Option<(f64, Instant)>; DECKS],
    mixer_values: [f32; mixer_controls::CONTROL_COUNT],
    mixer_areas: Vec<Rc<Cell<Bounds<Pixels>>>>,
    mixer_drag: Option<(usize, f32, f32)>,
    mixer_peaks: [f32; DECKS + 2],
    mixer_curve: CrossfaderCurve,
    crossfader_routing: djcraft_protocol::CrossfaderRouting,
    eq_mode: EqMode,
    stem_waveforms: [bool; DECKS],
    deck_eq_modes: [EqMode; DECKS],
    stem_tracks: [Option<std::sync::Arc<djcraft_engine::Track>>; DECKS],
    stem_busy: [bool; DECKS],
    stem_progress: [std::sync::Arc<std::sync::atomic::AtomicU8>; DECKS],
    stem_status: [String; DECKS],
    wheel_scrub: Option<(usize, f64, bool, Instant)>,
    tempo_returns: [Option<(f64, Instant)>; DECKS],
    /// The deck a track was loaded onto last.
    last_loaded: Option<usize>,
    pending_load: Option<(usize, String)>,
    pads: [pad_controls::Pads; DECKS],
    pad_cues: crate::pad_store::CueStore,
    cue_appearances: crate::pad_store::AppearanceStore,
    pad_tracks: [Option<Arc<Track>>; DECKS],
    deck_panel: [usize; DECKS],
    deck_tools_open: bool,
    deck_tools_deck: usize,
    deck_tools_from: f32,
    deck_tools_changed: Instant,
    effect_view: [usize; DECKS],
    effect_selected: [usize; DECKS],
    effect_details: [Option<usize>; DECKS],
    effect_drag: Option<(usize, usize, u8, f32, f32)>,
    loop_size: [i32; DECKS],
    loop_options: [loop_controls::LoopOptions; DECKS],
    loop_view: [usize; DECKS],
    loop_last: [Option<[f64; 2]>; DECKS],
    loudness_tracks: [Option<(Arc<Track>, f64, bool)>; DECKS],
    preparations: crate::preparation::Preparations,
    preparation_pending: std::collections::BTreeSet<String>,
    preparation_selected: [Option<usize>; DECKS],
    preparation_drag: Option<(usize, String, preparation_controls::Marker)>,
    preparation_notice: String,
    saved_loops: crate::loop_store::SavedLoops,
    loop_recording: [Option<(std::sync::Arc<djcraft_engine::Track>, f64)>; DECKS],
    loop_in: [Option<f64>; DECKS],
    loop_notice: [String; DECKS],
}

impl Shell {
    fn has_stem_waveforms(&self) -> bool {
        (0..self.visible_decks()).any(|deck| {
            self.stem_waveforms[deck]
                && self
                    .engine
                    .as_ref()
                    .and_then(|engine| engine.track(deck))
                    .is_some_and(|track| track.stem_levels(0).is_some())
        })
    }

    fn visible_decks(&self) -> usize {
        if self.settings.four_decks { DECKS } else { 2 }
    }

    /// Four decks follow the longest-running eligible transport; idle decks use pairing.
    fn sync_reference(&self, deck: usize) -> Option<usize> {
        select_sync_reference(deck, self.visible_decks(), &self.grids, &self.sync_playback)
    }

    fn set_four_decks(&mut self, enabled: bool, cx: &mut Context<Self>) {
        // Never hide a running deck, including an effects tail.
        if !enabled
            && self.engine.as_mut().is_some_and(|engine| {
                engine.state().decks[2..]
                    .iter()
                    .any(|d| d.playing || d.effect_tail)
                    || engine.state().monitor.cue_mask() & 0b1100 != 0
            })
        {
            return;
        }
        self.end_cue_preview(cx);
        self.end_wheel_scrub();
        self.release_pads(cx);
        self.end_waveform_drag(cx);
        self.pitch_drag = None;
        self.mixer_drag = None;
        self.effect_drag = None;
        self.settings.four_decks = enabled;
        if !enabled && self.active_deck >= 2 {
            self.active_deck %= 2;
        }
        if let Err(error) = self.settings.save(&crate::storage::machine_dir()) {
            eprintln!("Could not save deck layout: {error}");
        }
        cx.set_global(self.settings.clone());
        cx.notify();
    }

    /// Opens the audio engine and loads `tracks` onto the decks in order.
    pub fn new(
        tracks: Vec<String>,
        mut settings: Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if tracks.len() > 2 {
            settings.four_decks = true;
        }
        let (engine, engine_error) = match crate::settings::open_audio(&settings) {
            Ok(mut engine) => {
                for deck in 0..DECKS {
                    let _ = engine.apply(
                        Role::Host,
                        Action::Deck {
                            deck: deck as u8,
                            action: DeckAction::SetKeyLock { on: true },
                        },
                    );
                }
                engine.set_cache_dir(crate::settings::waveform_dir());
                let _ = engine.apply(
                    Role::Host,
                    Action::Mixer {
                        action: MixerAction::SetMonitor {
                            monitor: djcraft_protocol::MonitorState {
                                cue_decks: Some(0),
                                ..Default::default()
                            },
                        },
                    },
                );
                engine.set_sampler_dir(data_dir().join("Sampler"));
                (Some(engine), None)
            }
            Err(error) => (None, Some(error.to_string().into())),
        };
        let scan = settings.scan_on_startup;
        let library = cx.new(|cx| LibraryView::new(false, scan, window, cx));
        library.update(cx, |view, cx| {
            view.set_provider(settings.info_provider, cx);
            view.set_watching(settings.watch_folders);
        });
        cx.subscribe(&library, |this, _, event: &LibraryEvent, cx| match event {
            LibraryEvent::NewSet => {
                this.play_history = Default::default();
                this.browser.update(cx, |view, cx| view.refresh(cx));
                cx.notify();
            }
            LibraryEvent::Queue(tracks) => this.enqueue_automix(tracks.clone(), cx),
            LibraryEvent::Load { path, deck, .. } => {
                this.load_from_library(path, *deck, cx);
                this.refresh_grids(cx);
                this.show(Tab::Live, cx);
            }
            LibraryEvent::Selected { .. } => {}
            // The browser under the decks reads the same database, and a
            // track on a deck may just have been analysed.
            LibraryEvent::Changed => {
                this.browser.update(cx, |view, cx| view.refresh(cx));
                this.refresh_grids(cx);
                cx.notify();
            }
            LibraryEvent::Columns(hidden) => this.set_hidden_columns(hidden.clone(), cx),
        })
        .detach();
        let browser = cx.new(|cx| LibraryView::new(true, false, window, cx));
        cx.subscribe(&browser, |this, _, event: &LibraryEvent, cx| match event {
            LibraryEvent::NewSet => {
                this.play_history = Default::default();
                cx.notify();
            }
            LibraryEvent::Queue(tracks) => this.enqueue_automix(tracks.clone(), cx),
            LibraryEvent::Load { path, deck, .. } => {
                this.load_from_library(path, *deck, cx);
                this.refresh_grids(cx);
                cx.notify();
            }
            LibraryEvent::Columns(hidden) => this.set_hidden_columns(hidden.clone(), cx),
            LibraryEvent::Changed => {
                this.library
                    .update(cx, |view, cx| view.refresh_and_analyse(cx));
                this.refresh_grids(cx);
                cx.notify();
            }
            LibraryEvent::Selected { .. } => {}
        })
        .detach();
        for view in [&library, &browser] {
            view.update(cx, |view, cx| {
                view.set_hidden_columns(&settings.hidden_columns, cx)
            });
        }
        let mut shell = Self {
            stems_starting: true,
            automix: Default::default(),
            saved_queue: Vec::new(),
            automix_notice: String::new(),
            // Tracks named on the command line go to the decks, so show them.
            tab: match settings.start_tab {
                _ if !tracks.is_empty() => Tab::Live,
                StartTab::Library => Tab::Library,
                StartTab::Live => Tab::Live,
                StartTab::Radio => Tab::Radio,
            },
            library,
            browser,
            settings,
            settings_view: None,
            focus_settings: false,
            engine,
            controllers: match crate::controllers::Controllers::load() {
                Ok(controllers) => controllers,
                Err(error) => {
                    eprintln!("Controller setup: {error}");
                    None
                }
            },
            engine_error,
            deck_focus: cx.focus_handle(),
            active_deck: 0,
            monitor: Default::default(),
            focus_loaded_deck: false,
            cue_preview: None,
            cue_anchor: std::array::from_fn(|_| None),
            pitch_drag: None,
            pitch_areas: std::array::from_fn(|_| Rc::default()),
            deck_paths: vec![None; DECKS],
            grids: vec![None; DECKS],
            deck_keys: vec![None; DECKS],
            deck_modes: vec![None; DECKS],
            deck_metadata: vec![None; DECKS],
            deck_artwork: vec![None; DECKS],
            artwork_paths: vec![None; DECKS],
            overview_areas: (0..DECKS).map(|_| Rc::default()).collect(),
            waveform_areas: (0..DECKS).map(|_| Rc::default()).collect(),
            drag: None,
            display_positions: [DisplayPosition::default(); DECKS],
            last_frame: Instant::now(),
            play_history: Default::default(),
            sync_playback: Default::default(),
            overview_cache: std::array::from_fn(|_| Rc::default()),
            pending_seeks: [None; DECKS],
            mixer_values: mixer_controls::initial_values(),
            mixer_areas: (0..mixer_controls::CONTROL_COUNT)
                .map(|_| Rc::default())
                .collect(),
            mixer_drag: None,
            mixer_peaks: [0.; DECKS + 2],
            mixer_curve: CrossfaderCurve::default(),
            crossfader_routing: djcraft_protocol::CrossfaderRouting::default(),
            eq_mode: EqMode::Classic,
            stem_waveforms: [false; DECKS],
            deck_eq_modes: [EqMode::Classic; DECKS],
            stem_tracks: std::array::from_fn(|_| None),
            stem_busy: [false; DECKS],
            stem_progress: std::array::from_fn(|_| {
                std::sync::Arc::new(std::sync::atomic::AtomicU8::new(254))
            }),
            stem_status: std::array::from_fn(|_| String::new()),
            wheel_scrub: None,
            tempo_returns: [None; DECKS],
            last_loaded: None,
            pending_load: None,
            pads: std::array::from_fn(|_| pad_controls::Pads::default()),
            pad_cues: crate::pad_store::load(&data_dir()),
            cue_appearances: crate::pad_store::load_appearances(&data_dir()),
            pad_tracks: std::array::from_fn(|_| None),
            deck_panel: [2; DECKS],
            deck_tools_open: false,
            deck_tools_deck: 0,
            deck_tools_from: 0.,
            deck_tools_changed: Instant::now(),
            effect_view: [0; DECKS],
            effect_selected: [0; DECKS],
            effect_details: [None; DECKS],
            effect_drag: None,
            loop_size: [2; DECKS],
            loop_options: [loop_controls::LoopOptions::default(); DECKS],
            loop_view: [0; DECKS],
            loop_last: [None; DECKS],
            loudness_tracks: std::array::from_fn(|_| None),
            preparations: crate::preparation::load(&data_dir()),
            preparation_pending: Default::default(),
            preparation_selected: [None; DECKS],
            preparation_drag: None,
            preparation_notice: String::new(),
            saved_loops: crate::loop_store::load(&data_dir()),
            loop_recording: std::array::from_fn(|_| None),
            loop_in: [None; DECKS],
            loop_notice: std::array::from_fn(|_| String::new()),
        };
        let prefer_gpu = shell.settings.stem_gpu;
        let model = shell.settings.stem_model.name();
        let warmup = cx
            .background_executor()
            .spawn(async move { crate::stems::warm_up(prefer_gpu, model) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = warmup.await {
                eprintln!("Stem startup: {error}");
            }
            let _ = this.update(cx, |this, cx| {
                this.stems_starting = false;
                cx.notify();
            });
        })
        .detach();
        for (deck, path) in tracks.into_iter().take(DECKS).enumerate() {
            shell.send(deck, DeckAction::Load { path });
        }
        if shell.controllers.is_some() {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(5))
                        .await;
                    if this
                        .update(cx, |this, cx| this.poll_controllers(cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        // Queue progression must not depend on animation frames or a visible window.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        let was_enabled = this.automix.enabled;
                        let mut visuals_moving = false;
                        if let Some(engine) = &mut this.engine {
                            let state = engine.state();
                            visuals_moving = state.decks.iter().any(|deck| {
                                deck.playing
                                    || deck.loading
                                    || deck.effect_tail
                                    || deck.track.as_ref().is_some_and(|track| !track.fully_loaded)
                            });
                            this.automix = state.automix;
                        }
                        this.ensure_loudness(cx);
                        this.persist_automix();
                        // A command's immediate repaint can precede audio-thread
                        // acknowledgement. Bootstrap animation from authoritative
                        // state even when that first frame still saw a stopped deck.
                        if (visuals_moving && this.last_frame.elapsed().as_millis() >= 50)
                            || was_enabled
                            || this.automix.enabled
                        {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        if let Some(engine) = &mut shell.engine {
            for (path, preparation) in &shell.preparations {
                let Some(path) = crate::track_identity::resolve(path) else {
                    continue;
                };
                let _ = engine.apply(
                    Role::Host,
                    Action::Mixer {
                        action: MixerAction::SetTrackPreparation {
                            path: path.clone(),
                            preparation: preparation.clone(),
                        },
                    },
                );
            }
        }
        shell.restore_automix(cx);
        shell.refresh_grids(cx);
        shell
    }

    /// Looks up the beat grid of each deck's track in the library, and tells
    /// the browser which track to suggest what comes next for: the one
    /// loaded last.
    fn refresh_grids(&mut self, cx: &mut Context<Self>) {
        let library = self.library.read(cx);
        self.deck_metadata = self
            .deck_paths
            .iter()
            .map(|p| p.as_deref().and_then(|p| library.deck_metadata(p)))
            .collect();
        self.deck_keys = self
            .deck_paths
            .iter()
            .map(|path| path.as_deref().and_then(|path| library.key_at(path)))
            .collect();
        self.deck_modes = self
            .deck_paths
            .iter()
            .map(|path| path.as_deref().and_then(|path| library.modal_key_at(path)))
            .collect();
        self.grids = self
            .deck_paths
            .iter()
            .map(|path| path.as_deref().and_then(|path| library.beat_grid_at(path)))
            .collect();
        let reference = self
            .last_loaded
            .and_then(|deck| self.deck_paths.get(deck)?.as_deref())
            .and_then(|path| library.track_at(path));
        self.browser
            .update(cx, |view, cx| view.set_reference(reference, cx));
        self.refresh_deck_artwork(cx);
        for deck in 0..DECKS {
            self.send(
                deck,
                DeckAction::SetBeatGrid {
                    grid: self.grids[deck].map(|g| djcraft_protocol::BeatGrid {
                        bpm: g.bpm,
                        first_beat: g.first_beat_seconds,
                        downbeat: g.downbeat,
                        confidence: g.confidence,
                    }),
                },
            );
            self.send(
                deck,
                DeckAction::SetEffectTempo {
                    bpm: self.grids[deck].map(|g| g.bpm),
                },
            );
        }
    }

    /// Hides the named columns in every track list, and remembers that.
    fn set_hidden_columns(&mut self, hidden: Vec<String>, cx: &mut Context<Self>) {
        self.settings.hidden_columns = hidden;
        cx.set_global(self.settings.clone());
        for view in [&self.library, &self.browser] {
            view.update(cx, |view, cx| {
                view.set_hidden_columns(&self.settings.hidden_columns, cx)
            });
        }
        if let Err(error) = self.settings.save(&crate::storage::machine_dir()) {
            eprintln!("could not save settings: {error}");
        }
    }

    pub fn show(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        // The two library views share a database; whichever is coming into
        // view may be behind the other.
        match tab {
            Tab::Library => self.library.update(cx, |view, cx| view.refresh(cx)),
            Tab::Live => self.browser.update(cx, |view, cx| view.refresh(cx)),
            Tab::Radio => {}
        }
        if tab != Tab::Library {
            self.library.update(cx, |view, cx| view.pause_players(cx));
        }
        cx.notify();
    }

    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_view.is_some() {
            return;
        }
        let audio: SharedString = match (&self.engine, &self.engine_error) {
            (Some(engine), _) => engine.output_description().to_string().into(),
            (None, Some(error)) => error.clone(),
            (None, None) => "Not available".into(),
        };
        let settings = self.settings.clone();
        let view = cx.new(|cx| SettingsView::new(settings, audio, cx));
        cx.subscribe(&view, |this, _, event: &SettingsEvent, cx| {
            match event {
                SettingsEvent::Changed(settings) => {
                    this.settings = settings.clone();
                    this.settings.apply_theme(cx);
                    cx.set_global(settings.clone());
                    this.library.update(cx, |view, cx| {
                        view.set_provider(settings.info_provider, cx);
                        view.apply_settings(settings, cx);
                    });
                    // How tempos are shown changes every view.
                    cx.refresh_windows();
                    if let Err(error) = this.settings.save(&crate::storage::machine_dir()) {
                        eprintln!("could not save settings: {error}");
                    }
                }
                SettingsEvent::Closed => this.settings_view = None,
            }
            cx.notify();
        })
        .detach();
        self.settings_view = Some(view);
        self.focus_settings = true;
        cx.notify();
    }

    /// Loads a library track. With no deck named, it goes to the first one
    /// that is not playing; if every deck is playing, nothing is replaced.
    fn load_from_library(&mut self, path: &Path, deck: Option<usize>, cx: &mut Context<Self>) {
        let visible = self.visible_decks();
        let Some(engine) = &mut self.engine else {
            return;
        };
        let free = || {
            engine
                .state()
                .decks
                .iter()
                .take(visible)
                .position(|deck| !deck.playing)
        };
        let Some(deck) = deck.or_else(free) else {
            return;
        };
        let path = path.to_string_lossy().into_owned();
        self.request_load(deck, path, cx);
    }

    fn load_external_track(&mut self, deck: usize, files: &ExternalPaths, cx: &mut Context<Self>) {
        // A deck holds one track: use the first supported file in a multi-file drop.
        let path = files.paths().iter().find(|path| {
            path.is_file()
                && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    [
                        "mp3", "flac", "wav", "aif", "aiff", "m4a", "aac", "ogg", "opus",
                    ]
                    .contains(&e.to_ascii_lowercase().as_str())
                })
        });
        if let Some(path) = path {
            self.request_load(deck, path.to_string_lossy().into_owned(), cx);
        }
    }

    fn request_load(&mut self, deck: usize, path: String, cx: &mut Context<Self>) {
        if deck >= DECKS {
            return;
        }
        if deck >= 2 && !self.settings.four_decks {
            self.set_four_decks(true, cx);
        }
        if self
            .engine
            .as_mut()
            .is_some_and(|engine| engine.deck_has_open_output(deck))
        {
            self.pending_load = Some((deck, path));
            self.show(Tab::Live, cx);
        } else {
            self.pending_load = None;
            self.send(deck, DeckAction::Load { path });
            self.refresh_grids(cx);
        }
        cx.notify();
    }

    fn end_wheel_scrub(&mut self) {
        if let Some((deck, position, resume, _)) = self.wheel_scrub.take() {
            self.send(deck, DeckAction::Scratch { speed: None });
            self.send(deck, DeckAction::Seek { seconds: position });
            if resume {
                self.send(deck, DeckAction::Play);
            }
        }
    }

    fn wheel_waveform(
        &mut self,
        deck: usize,
        event: &ScrollWheelEvent,
        overview: bool,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if self.drag.is_some() {
            return;
        }
        if self
            .wheel_scrub
            .is_some_and(|(active, _, _, _)| active != deck || overview)
        {
            self.end_wheel_scrub();
        }
        let Some(state) = self
            .engine
            .as_mut()
            .and_then(|engine| engine.state().decks.get(deck).cloned())
        else {
            return;
        };
        let Some(track) = state.track else {
            return;
        };
        if !overview && state.playing {
            self.send(
                deck,
                DeckAction::Nudge {
                    seconds: (crate::wheel::steps(event) as f64 * 0.005).clamp(-0.5, 0.5),
                },
            );
            cx.notify();
            return;
        }
        let current = self
            .wheel_scrub
            .map(|(_, position, _, _)| position)
            .or_else(|| self.pending_seeks[deck].map(|(position, _)| position))
            .unwrap_or(state.position_seconds);
        let seconds =
            (current + crate::wheel::steps(event) as f64 * if overview { 1. } else { 0.1 }).clamp(
                if overview {
                    0.
                } else {
                    self.grids[deck].map_or(0., |g| -16. * g.beat_seconds())
                },
                track.loaded_seconds.min(track.duration_seconds).max(0.),
            );
        if overview {
            self.send(deck, DeckAction::Seek { seconds });
        } else {
            let resume = self
                .wheel_scrub
                .map_or(state.playing, |(_, _, resume, _)| resume);
            if self.wheel_scrub.is_none() {
                self.send(deck, DeckAction::Pause);
            }
            self.wheel_scrub = Some((deck, seconds, resume, Instant::now()));
            self.send(deck, DeckAction::ScratchTo { seconds });
        }
        cx.notify();
    }

    fn sync_deck(&mut self, deck: usize, cx: &mut Context<Self>) {
        self.end_wheel_scrub();
        let Some(other) = self.sync_reference(deck) else {
            return;
        };
        let Some(engine) = &mut self.engine else {
            return;
        };
        let state = engine.state();
        if let (Some(grid), Some(reference)) = (self.grids[deck], self.grids[other]) {
            let local = &state.decks[deck];
            let master = &state.decks[other];
            let ratio = reference.bpm * master.tempo / grid.bpm;
            if !(MIN_TEMPO..=MAX_TEMPO).contains(&ratio) {
                return;
            }
            let offset_seconds = if local.playing && master.playing {
                nearest_beat_offset(
                    local.position_seconds,
                    grid,
                    master.position_seconds,
                    reference,
                )
            } else {
                0.
            };
            self.send(
                deck,
                DeckAction::Sync {
                    ratio,
                    offset_seconds,
                },
            );
            cx.notify();
        }
    }

    fn wheel_tempo(&mut self, deck: usize, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if let Some(state) = self
            .engine
            .as_mut()
            .and_then(|engine| engine.state().decks.get(deck).cloned())
        {
            let ratio = (state.tempo + crate::wheel::steps(event) as f64 * 0.001).clamp(
                1. - state.tempo_range.extent(),
                1. + state.tempo_range.extent(),
            );
            self.send(deck, DeckAction::SetTempo { ratio });
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn begin_waveform_drag(&mut self, deck: usize, x: f32, overview: bool, cx: &mut Context<Self>) {
        self.end_wheel_scrub();
        let Some(state) = self
            .engine
            .as_mut()
            .and_then(|engine| engine.state().decks.get(deck).cloned())
        else {
            return;
        };
        let Some(track) = state.track else {
            return;
        };
        let area = if overview {
            self.overview_areas[deck].get()
        } else {
            self.waveform_areas[deck].get()
        };
        let width = area.size.width.as_f32();
        if width <= 0. {
            return;
        }
        self.end_waveform_drag(cx);
        let drag = WaveformDrag {
            deck,
            origin_x: if overview { area.origin.x.as_f32() } else { x },
            origin_seconds: if overview {
                0.
            } else {
                self.display_positions[deck]
                    .0
                    .unwrap_or(state.position_seconds)
            },
            seconds_per_pixel: if overview {
                track.duration_seconds / width as f64
            } else {
                -VISIBLE_SECONDS * state.tempo / width as f64
            },
            minimum: if overview {
                0.
            } else {
                self.grids[deck].map_or(0., |g| -16. * g.beat_seconds())
            },
            duration: track.loaded_seconds.min(track.duration_seconds).max(0.),
            position: state.position_seconds,
            resume: state.playing,
            vinyl: !overview,
        };
        self.send(deck, DeckAction::Pause);
        self.drag = Some(drag);
        self.move_waveform_drag(x, cx);
    }

    fn move_waveform_drag(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(mut drag) = self.drag else {
            return;
        };
        drag.position = drag.at(x);
        self.drag = Some(drag);
        self.send(
            drag.deck,
            if drag.vinyl {
                DeckAction::ScratchTo {
                    seconds: drag.position,
                }
            } else {
                DeckAction::Seek {
                    seconds: drag.position,
                }
            },
        );
        cx.notify();
    }

    fn end_waveform_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag.take() {
            if drag.vinyl {
                self.send(drag.deck, DeckAction::Scratch { speed: None });
            }
            self.send(
                drag.deck,
                DeckAction::Seek {
                    seconds: drag.position,
                },
            );
            if drag.resume {
                self.send(drag.deck, DeckAction::Play);
            }
            cx.notify();
        }
    }

    /// Sends a deck action to the engine as the session host.
    fn send(&mut self, deck: usize, action: DeckAction) {
        let cue_anchor = self.transport_anchor_for(deck, &action);
        if matches!(action, DeckAction::Load { .. } | DeckAction::Pause) {
            self.sync_playback[deck] = SyncPlayback::default();
        }
        if matches!(
            action,
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
                | DeckAction::JogScratch { .. }
        ) && let Some(controllers) = &mut self.controllers
        {
            for release in controllers.cancel_deck(deck as u8) {
                if let Some(engine) = &mut self.engine {
                    let _ = engine.apply(Role::Host, release);
                }
            }
        }
        if matches!(action, DeckAction::Load { .. }) {
            self.release_pad(deck);
            self.loop_in[deck] = None;
            self.loop_last[deck] = None;
            self.loop_recording[deck] = None;
            self.loop_notice[deck].clear();
        }
        if matches!(
            action,
            DeckAction::SetTempo { .. } | DeckAction::Sync { .. } | DeckAction::Load { .. }
        ) {
            self.tempo_returns[deck] = None;
        }
        if matches!(action, DeckAction::Load { .. })
            && self
                .wheel_scrub
                .is_some_and(|(active, _, _, _)| active == deck)
        {
            self.end_wheel_scrub();
        }
        if let Some(display) = self.display_positions.get_mut(deck) {
            match &action {
                DeckAction::Seek { seconds } | DeckAction::CuePreview { seconds, .. } => {
                    display.0 = Some(*seconds);
                    self.pending_seeks[deck] = Some((*seconds, Instant::now()));
                }
                DeckAction::Load { .. } => {
                    display.0 = None;
                    self.pending_seeks[deck] = None;
                }
                _ => {}
            }
        }
        let Some(engine) = &mut self.engine else {
            return;
        };
        let action = Action::Deck {
            deck: deck as u8,
            action,
        };
        if let Action::Deck {
            action: DeckAction::Load { path },
            ..
        } = &action
            && let Some(slot) = self.deck_paths.get_mut(deck)
        {
            *slot = Some(PathBuf::from(path));
            self.last_loaded = Some(deck);
        }
        let loading = matches!(
            &action,
            Action::Deck {
                action: DeckAction::Load { .. },
                ..
            }
        );
        match engine.apply(Role::Host, action) {
            Ok(()) if loading => {
                self.cue_anchor[deck] = cue_anchor;
                self.active_deck = deck;
                self.focus_loaded_deck = true;
            }
            Err(reason) => eprintln!("action rejected: {reason:?}"),
            Ok(()) => self.cue_anchor[deck] = cue_anchor,
        }
    }

    fn poll_controllers(&mut self, cx: &mut Context<Self>) {
        let Some(mut controllers) = self.controllers.take() else {
            return;
        };
        let batches = controllers.drain();
        controllers.set_tempo_bpms(
            &self
                .grids
                .iter()
                .map(|grid| grid.map(|g| g.bpm))
                .collect::<Vec<_>>(),
        );
        for action in controllers.take_releases() {
            self.controller_action(action, cx);
        }
        for command in controllers.take_commands() {
            self.controller_command(command, cx);
        }
        for (port, events) in batches {
            let mut jog_batch = crate::controllers::JogBatch::default();
            for event in events {
                let Some(engine) = &mut self.engine else {
                    break;
                };
                let state = engine.state();
                for action in controllers.process(&port, event, &state) {
                    for action in jog_batch.push(action) {
                        self.controller_action(action, cx);
                    }
                }
                let commands = controllers.take_commands();
                if !commands.is_empty() {
                    for action in jog_batch.flush() {
                        self.controller_action(action, cx);
                    }
                }
                for command in commands {
                    self.controller_command(command, cx);
                }
            }
            for action in jog_batch.flush() {
                self.controller_action(action, cx);
            }
        }
        if let Some(engine) = &mut self.engine {
            let state = engine.state();
            let view = if self.tab == Tab::Library {
                &self.library
            } else {
                &self.browser
            };
            controllers.set_browser_folders(view.read(cx).controller_folders());
            for action in controllers.tick(&state, &self.loop_in) {
                self.controller_action(action, cx);
            }
        }
        self.controllers = Some(controllers);
    }

    fn controller_action(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Deck { deck, action } if usize::from(deck) < self.deck_paths.len() => {
                if deck >= 2 && !self.settings.four_decks {
                    self.set_four_decks(true, cx);
                }
                match action {
                    DeckAction::Load { path } => self.request_load(usize::from(deck), path, cx),
                    DeckAction::SetHotCues { cues } => {
                        self.controller_store_cues(usize::from(deck), cues)
                    }
                    action => self.send(usize::from(deck), action),
                }
            }
            action => {
                if let Some(engine) = &mut self.engine {
                    if let Err(reason) = engine.apply(Role::Host, action) {
                        eprintln!("Controller action rejected: {reason:?}");
                    }
                }
            }
        }
        cx.notify();
    }

    fn controller_command(
        &mut self,
        command: djcraft_controllers::HostCommand,
        cx: &mut Context<Self>,
    ) {
        use djcraft_controllers::HostCommand;
        let deck = usize::from(command.deck());
        if deck >= self.deck_paths.len() {
            return;
        }
        if deck >= 2 && !self.settings.four_decks {
            self.set_four_decks(true, cx);
        }
        match command {
            HostCommand::Browse { .. }
            | HostCommand::BrowseBack
            | HostCommand::LoadSelected { .. } => {
                if self.tab != Tab::Library && self.tab != Tab::Live {
                    self.show(Tab::Live, cx);
                }
                let view = if self.tab == Tab::Library {
                    &self.library
                } else {
                    &self.browser
                };
                view.update(cx, |view, cx| match command {
                    HostCommand::Browse { steps } => view.controller_browse(steps, cx),
                    HostCommand::BrowseBack => view.controller_back(cx),
                    HostCommand::LoadSelected { deck } => {
                        view.controller_load(usize::from(deck), cx)
                    }
                    _ => unreachable!(),
                });
            }
            HostCommand::AutoLoop { beats, .. } => {
                self.loop_size[deck] = match beats {
                    1 => 0,
                    2 => 1,
                    4 => 2,
                    16 => 4,
                    _ => return,
                };
                let same_length = self.grids[deck].is_some_and(|grid| {
                    self.loop_state(deck)
                        .and_then(|s| s.loop_region)
                        .is_some_and(|[a, b]| {
                            ((b - a) / grid.beat_seconds() - f64::from(beats)).abs() < 0.01
                        })
                });
                self.auto_loop(deck, same_length, cx);
            }
            HostCommand::LoopPad { pad, alternate, .. } => {
                self.controller_loop_pad(deck, pad, alternate, cx);
            }
            HostCommand::SamplePad { slot, stop, .. } => {
                self.controller_sample_pad(deck, slot, stop);
            }
            HostCommand::Sync { .. } => self.sync_deck(deck, cx),
            HostCommand::Cue { pressed: true, .. } => self.begin_cue_preview(deck, cx),
            HostCommand::Cue { pressed: false, .. } => {
                if self
                    .cue_preview
                    .as_ref()
                    .is_some_and(|(active, _, _)| *active == deck)
                {
                    self.end_cue_preview(cx);
                }
            }
            HostCommand::ToggleHeadphoneCue { .. } => self.toggle_headphone_cue(deck),
            HostCommand::SelectDeck { .. } => {
                self.active_deck = deck;
            }
            HostCommand::Stutter { .. } => {
                if let Some(state) = self.engine.as_mut().map(|e| e.state().decks[deck].clone())
                    && state.track.is_some()
                {
                    let anchor = self.cue_anchor[deck]
                        .as_ref()
                        .filter(|(_, path)| *path == state.path)
                        .map_or(0., |(time, _)| *time);
                    self.send(deck, DeckAction::Seek { seconds: anchor });
                    self.send(deck, DeckAction::Play);
                    self.cue_anchor[deck] = Some((anchor, state.path));
                }
            }
        }
        cx.notify();
    }

    fn toggle_headphone_cue(&mut self, deck: usize) {
        if let Some(engine) = &mut self.engine {
            let mut monitor = engine.state().monitor;
            monitor.cue_decks = Some(monitor.cue_mask() ^ (1 << deck));
            let _ = engine.apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::SetMonitor { monitor },
                },
            );
        }
    }

    fn tool(&self, id: impl Into<ElementId>, icon: IconName, tooltip: &'static str) -> Button {
        Button::new(id)
            .outline()
            .small()
            .rounded(ButtonRounded::Size(px(999.)))
            .icon(icon)
            .tooltip(tooltip)
    }

    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new().h(TOP_BAR_HEIGHT).child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_between()
                .px_3()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        // Toolbar controls must take priority over the native drag hitbox.
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .child("faderCraft"),
                        )
                        .child(self.tab_button(
                            Tab::Live,
                            "performance",
                            IconName::Disc3,
                            "Perform",
                            cx,
                        ))
                        .child(self.tab_button(
                            Tab::Library,
                            "library",
                            IconName::Library,
                            "Library",
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .when(self.tab == Tab::Live, |this| {
                            this.child(
                                self.tool(
                                    "stems",
                                    IconName::AudioLines,
                                    "Prepare stems for loaded decks",
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.prepare_stems(cx))),
                            )
                            .child(
                                Button::new("rgb-waveforms")
                                    .small()
                                    .outline()
                                    .label("RGB")
                                    .selected(self.settings.rgb_waveforms)
                                    .tooltip("RGB waveforms: red bass, green mids, blue highs. Toggle off for deck colours.")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.settings.rgb_waveforms = !this.settings.rgb_waveforms;
                                        if let Err(error) = this.settings.save(&crate::storage::machine_dir()) {
                                            eprintln!("Could not save waveform preference: {error}");
                                        }
                                        cx.set_global(this.settings.clone());
                                        cx.notify();
                                    })),
                            )
                            .child(self.eq_mode_menu(cx))
                            .when(self.has_stem_waveforms(), |bar| {
                                bar.child(
                                    div().flex().gap_2().text_size(px(10.)).children(
                                        crate::waveform::STEM_LABELS
                                            .into_iter()
                                            .zip(crate::waveform::STEM_COLOURS)
                                            .map(|(label, colour)| {
                                                div().text_color(rgb(colour)).child(label)
                                            }),
                                    ),
                                )
                            })
                            .children([false, true].into_iter().map(|four| {
                                Button::new(if four { "four-decks" } else { "two-decks" })
                                    .small()
                                    .outline()
                                    .label(if four { "4 Decks" } else { "2 Decks" })
                                    .selected(self.settings.four_decks == four)
                                    .tooltip(if four {
                                        "Show decks A, B, C and D"
                                    } else {
                                        "Show A/B. Stop and uncue C/D before hiding them."
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_four_decks(four, cx)
                                    }))
                            }))
                        })
                        .child(
                            // Keep Windows title-bar dragging from swallowing Settings clicks.
                            div()
                                .occlude()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(
                                    self.tool("settings-button", IconName::Settings, "Settings")
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.open_settings(cx)),
                                        ),
                                ),
                        ),
                ),
        )
    }

    fn tab_button(
        &self,
        tab: Tab,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Button::new(id)
            .outline()
            .rounded(ButtonRounded::Size(px(999.)))
            .icon(icon)
            .label(label)
            .selected(self.tab == tab)
            .on_click(cx.listener(move |this, _, _, cx| this.show(tab, cx)))
    }

    fn panel(&self, label: impl Into<SharedString>, cx: &Context<Self>) -> Div {
        div()
            .flex()
            .flex_1()
            .p_4()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .text_color(cx.theme().muted_foreground)
            .child(label.into())
    }

    fn deck(&self, index: usize, state: &DeckState, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme();
        let (title, time): (SharedString, String) = match (&state.track, &state.error) {
            _ if state.loading => ("Loading…".into(), String::new()),
            (_, Some(error)) => (error.clone().into(), String::new()),
            (Some(track), _) => (
                track.title.clone().into(),
                format!(
                    "{} / {}",
                    clock(state.position_seconds / state.tempo),
                    clock(track.duration_seconds / state.tempo)
                ),
            ),
            (None, None) => ("No track loaded".into(), String::new()),
        };
        let track = self.engine.as_ref().and_then(|engine| engine.track(index));
        let grid = self.grids.get(index).copied().flatten();
        let area = self.overview_areas[index].clone();
        let duration = state
            .track
            .as_ref()
            .map_or(1., |track| track.duration_seconds.max(0.001));
        let playing = state.playing;

        let deck_accent = deck_colour(index);
        let deck_selected = self.active_deck == index;
        let headphone_cued = self.monitor.cue_enabled(index);
        let load_button = Button::new(("load", index))
            .tooltip("Choose an audio file for this deck.")
            .outline()
            .small()
            .icon(IconName::FolderOpen)
            .when(self.settings.four_decks, |button| button.w(px(28.)))
            .on_click(cx.listener(move |_, _, _, cx| {
                let paths = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Load".into()),
                });
                cx.spawn(async move |this, cx| {
                    let Ok(Ok(Some(paths))) = paths.await else {
                        return;
                    };
                    let Some(path) = paths.first() else {
                        return;
                    };
                    let path = path.to_string_lossy().into_owned();
                    this.update(cx, |this, cx| {
                        this.request_load(index, path, cx);
                        this.refresh_grids(cx);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }));
        let play_button = Button::new(("play", index))
            .tooltip("Play or pause this deck.")
            .primary()
            .small()
            .when(self.settings.four_decks, |button| button.w(px(28.)))
            .icon(if playing {
                IconName::Pause
            } else {
                IconName::Play
            })
            .disabled(state.track.is_none())
            .on_click(cx.listener(move |this, _, _, cx| {
                let action = if playing {
                    DeckAction::Pause
                } else {
                    DeckAction::Play
                };
                this.send(index, action);
                cx.notify();
            }));
        let sync_button = Button::new(("sync", index))
            .outline()
            .small()
            .label("Sync")
            .when(self.settings.four_decks, |button| button.w_full())
            .tooltip(if self.settings.four_decks {
                "Match the longest-playing other deck with a beat grid; align the nearest beat when both play. If none is playing, prefer the paired deck."
            } else {
                "Match the other deck's tempo; align the nearest beat when both play"
            })
            .disabled(
                state.track.is_none()
                    || self.grids[index].is_none()
                    || self.sync_reference(index).is_none(),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.sync_deck(index, cx)));
        let cue_button = div()
            .id(("cue-hold", index))
            .when(self.settings.four_decks, |group| group.w_full())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.begin_cue_preview(index, cx);
                    window.focus(&this.deck_focus, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                Button::new(("cue", index))
                    .outline()
                    .small()
                    .label("Cue")
                    .when(self.settings.four_decks, |button| button.w_full())
                    .tooltip("Hold to preview; release to stop and return to the starting point.")
                    .disabled(state.track.is_none()),
            );
        let stems_button = self
            .prepare_stems_button(index, cx)
            .small()
            .when(self.settings.four_decks, |button| button.w_full());
        let controls = if self.settings.four_decks {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .w(px(64.))
                .flex_shrink_0()
                .child(cue_button)
                .child(sync_button)
                .child(stems_button)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(play_button)
                        .child(load_button),
                )
        } else {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(cue_button)
                .child(play_button)
                .child(sync_button)
                .child(load_button)
                .child(stems_button)
        }
        .into_any_element();
        let display_controls = Some(controls);
        let deck_select = Button::new(("select-deck", index))
            .outline()
            .w(px(40.))
            .h(px(28.))
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .label(deck_label(index))
            .selected(deck_selected)
            .border_color(deck_accent)
            .bg(if deck_selected {
                Hsla::from(deck_accent)
            } else {
                Hsla::from(deck_accent).opacity(0.12)
            })
            .text_color(if deck_selected {
                rgb(0x070b12)
            } else {
                deck_accent
            })
            .tooltip("Activate this deck for keyboard controls. Does not change headphone cue.")
            .on_click(cx.listener(move |this, _, window, cx| {
                this.active_deck = index;
                window.focus(&this.deck_focus, cx);
                cx.notify();
            }));

        let identity = div().flex().items_center().gap_1().flex_shrink_0().when(self.settings.four_decks, |group| group.flex_col())
                    .child(deck_select)
                    .child(Button::new(("headphone-cue", index)).outline().small()
                        .icon(IconName::Headphones)
                        .selected(headphone_cued)
                        .when(headphone_cued, |button| button.bg(Hsla::from(deck_accent)).text_color(rgb(0x070b12)))
                        .tooltip("Toggle this deck in headphones. Independent of deck activation; multiple decks can be cued.")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_headphone_cue(index);
                            cx.notify();
                        }))).into_any_element();
        let (display_identity, footer_identity) = if self.settings.four_decks {
            (Some(identity), None)
        } else {
            (None, Some(identity))
        };

        div()
            .id(("deck-drop", index))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.deck_focus, cx);
                }),
            )
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(rgb(0x183849)))
            .on_drop(cx.listener(move |this, files: &ExternalPaths, _, cx| {
                this.load_external_track(index, files, cx);
                cx.stop_propagation();
            }))
            .drag_over::<DraggedTrack>(|style, _, _, _| style.bg(rgb(0x183849)))
            .on_drop(cx.listener(move |this, dragged: &DraggedTrack, _, cx| {
                this.request_load(index, dragged.path.to_string_lossy().into_owned(), cx);
                cx.stop_propagation();
            }))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap_1()
            .p_2()
            .min_h_0()
            .when_some(
                self.pending_load
                    .as_ref()
                    .filter(|(deck, _)| *deck == index),
                |this, (_, path)| {
                    let name = Path::new(path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    this.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .rounded_md()
                            .bg(rgb(0x523a15))
                            .child(format!(
                                "Deck {} is playing through the master. Replace it with {name}?",
                                deck_label(index)
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new(("cancel-load", index))
                                            .small()
                                            .outline()
                                            .label("Cancel")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.pending_load = None;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new(("confirm-load", index))
                                            .small()
                                            .primary()
                                            .label("Replace track")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if this
                                                    .pending_load
                                                    .as_ref()
                                                    .is_some_and(|(deck, _)| *deck == index)
                                                {
                                                    let (deck, path) =
                                                        this.pending_load.take().unwrap();
                                                    this.send(deck, DeckAction::Load { path });
                                                    this.refresh_grids(cx);
                                                    cx.notify();
                                                }
                                            })),
                                    ),
                            ),
                    )
                },
            )
            .border_1()
            .border_color(theme.border)
            .when(deck_selected, |d| {
                d.border_2().border_color(deck_accent).p(px(7.))
            })
            .rounded_lg()
            .child(if self.settings.four_decks {
                div()
                    .flex()
                    .items_center()
                    .when(index % 2 == 1, |d| d.flex_row_reverse())
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .text_color(deck_colour(index))
                            .font_weight(FontWeight::BOLD)
                            .child(deck_label(index)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(index % 2 == 1, |d| d.text_right())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .when(state.track.is_some(), |this| {
                        this.child(
                            div()
                                .id(("bpm-wheel", index))
                                .on_scroll_wheel(cx.listener(
                                    move |this, event: &ScrollWheelEvent, _, cx| {
                                        this.wheel_tempo(index, event, cx)
                                    },
                                ))
                                .text_sm()
                                .flex_shrink_0()
                                .child(
                                    grid.map(|grid| format!("{:.2} BPM", grid.bpm * state.tempo))
                                        .unwrap_or_else(|| "-- BPM".into()),
                                ),
                        )
                    })
                    .child(div().text_sm().flex_shrink_0().child(time))
            } else {
                self.compact_deck_header(index, state, title.clone(), cx)
            })
            .child(
                div()
                    .flex()
                    .when(index % 2 == 1, |row| row.flex_row_reverse())
                    .items_center()
                    .h(px(if self.settings.four_decks {
                        28.
                    } else {
                        OVERVIEW_HEIGHT
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .id(("overview", index))
                            .on_scroll_wheel(cx.listener(
                                move |this, event: &ScrollWheelEvent, _, cx| {
                                    this.wheel_waveform(index, event, true, cx)
                                },
                            ))
                            .relative()
                            .h(px(if self.settings.four_decks {
                                28.
                            } else {
                                OVERVIEW_HEIGHT
                            }))
                            .flex_shrink_0()
                            .bg(theme.secondary.opacity(0.5))
                            .overflow_hidden()
                            .when_some(track, |this, track| {
                                this.child(overview(
                                    track.clone(),
                                    state.position_seconds / duration,
                                    self.overview_cache[index].clone(),
                                    area.clone(),
                                    theme.foreground,
                                    state.hot_cues,
                                    state.cue_appearance.clone(),
                                    index,
                                    self.settings.rgb_waveforms,
                                ))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    this.begin_waveform_drag(
                                        index,
                                        event.position.x.as_f32(),
                                        true,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .when(!self.settings.four_decks, |row| {
                        row.child(self.deck_header_times(index, state, cx))
                    }),
            )
            .child(self.deck_display(index, state, display_controls, display_identity, cx))
            .child(
                div()
                    .flex()
                    .when(index % 2 == 1, |d| d.flex_row_reverse())
                    .flex_wrap()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .child(
                        div().flex().items_center().gap_1().children(
                            ["Loops", "Effects", "Pads", "Prepare"]
                                .into_iter()
                                .enumerate()
                                .map(|(panel, label)| {
                                    Button::new(("deck-panel", index * 4 + panel))
                                        .tooltip("Toggle tools over the other deck on this side.")
                                        .xsmall()
                                        .outline()
                                        .label(label)
                                        .selected(
                                            self.deck_panel[index] == panel
                                                && (!self.settings.four_decks
                                                    || (self.deck_tools_open
                                                        && self.deck_tools_deck == index)),
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.release_pads(cx);
                                            if this.settings.four_decks {
                                                let same_deck = this.deck_tools_deck == index;
                                                let close = this.deck_tools_open
                                                    && same_deck
                                                    && this.deck_panel[index] == panel;
                                                this.deck_tools_from = if same_deck {
                                                    this.deck_tools_progress()
                                                } else {
                                                    0.
                                                };
                                                this.deck_tools_changed = Instant::now();
                                                this.deck_tools_deck = index;
                                                this.deck_tools_open = !close;
                                                this.active_deck = index;
                                            }
                                            this.deck_panel[index] = panel;
                                            cx.notify();
                                        }))
                                })
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .child(div().flex_1())
                    .child(div().w(px(10.)).flex_shrink_0())
                    .children(footer_identity),
            )
            .when(!self.settings.four_decks, |deck| {
                deck.child(
                    div()
                        .id(("deck-tools-scroll", index))
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(self.deck_tools(index, state, cx)),
                )
            })
    }

    fn deck_tools_progress(&self) -> f32 {
        let t = (self.deck_tools_changed.elapsed().as_secs_f32() / 0.22).min(1.);
        let eased = t * t * (3. - 2. * t);
        self.deck_tools_from
            + ((if self.deck_tools_open { 1. } else { 0. }) - self.deck_tools_from) * eased
    }

    fn deck_column(&self, side: usize, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        let progress = self.deck_tools_progress();
        let owner = self.deck_tools_deck;
        let overlay = self.settings.four_decks && owner % 2 == side && progress > 0.;
        let mut column = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap_2();
        for index in [side, side + 2]
            .into_iter()
            .filter(|&d| d < self.visible_decks())
        {
            if self.settings.four_decks {
                // Keep identical slots while the owner is rendered in the overlay.
                // A flexible empty placeholder redistributes the other deck's space.
                let slot = div().h(px(236.)).flex_shrink_0().flex().flex_col();
                column = column.child(if overlay && index == owner {
                    slot
                } else {
                    slot.children(decks.get(index).map(|state| self.deck(index, state, cx)))
                });
            } else if let Some(state) = decks.get(index) {
                column = column.child(self.deck(index, state, cx));
            }
        }
        if overlay && let Some(state) = decks.get(owner) {
            column = column.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(if owner >= 2 {
                        244. * (1. - progress)
                    } else {
                        0.
                    }))
                    .h(px(236. + 244. * progress))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded_lg()
                    .bg(cx.theme().background)
                    .occlude()
                    .child(
                        div()
                            .h(px(236.))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .child(self.deck(owner, state, cx)),
                    )
                    .child(
                        div()
                            .id(("overlay-deck-tools", owner))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_2()
                            .border_1()
                            .border_color(Hsla::from(deck_colour(owner)).opacity(0.35))
                            .child(self.deck_tools(owner, state, cx)),
                    ),
            );
        }
        column
    }

    fn deck_tools(&self, index: usize, state: &DeckState, cx: &mut Context<Self>) -> AnyElement {
        match self.deck_panel[index] {
            0 => self.loop_panel(index, state, cx).into_any_element(),
            1 => self.effect_panel(index, state, cx),
            2 => self.pad_panel(index, state, false, cx).into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .gap_2()
                .when(state.track.is_none(), |d| {
                    d.child("Load a track to prepare phrases, cues and loudness.")
                })
                .child(self.preparation_panel(index, state, cx))
                .child(self.suggestions_panel(index, state, cx))
                .into_any_element(),
        }
    }

    fn content(&self, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        let body = div().flex().flex_col().flex_1().min_h_0().gap_2().p_2();
        match self.tab {
            Tab::Library => body.child(self.library.clone()),
            Tab::Live => body.child(
                div()
                    .id("performance-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .h_full()
                            .min_h(px(780.))
                            .gap_2()
                            .child(self.rhythm_panel(decks, cx))
                            .when_some(self.engine_error.clone(), |this, error| {
                                this.child(div().text_color(cx.theme().danger).child(error))
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .h(px(
                                        if !self.settings.four_decks
                                            && self.eq_mode == EqMode::Stems
                                        {
                                            510.
                                        } else {
                                            480.
                                        },
                                    ))
                                    .gap_2()
                                    .child(self.deck_column(0, decks, cx))
                                    .child(self.mixer_panel(cx))
                                    .child(self.deck_column(1, decks, cx)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .px_2()
                                            .py_1()
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("MUSIC LIBRARY"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("Drag a track onto a deck"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_1()
                                            .min_h_0()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(self.browser.clone()),
                                            )
                                            .child(self.automix_panel(cx))
                                            .when(
                                                self.effect_details.iter().any(Option::is_some),
                                                |row| {
                                                    row.child(
                                                        div()
                                                            .id("fx-detail-dock")
                                                            .w(px(320.))
                                                            .flex_shrink_0()
                                                            .flex()
                                                            .flex_col()
                                                            .gap_2()
                                                            .overflow_y_scroll()
                                                            .children(
                                                                decks
                                                                    .iter()
                                                                    .enumerate()
                                                                    .filter_map(|(deck, state)| {
                                                                        self.effect_detail_panel(
                                                                            deck, state, cx,
                                                                        )
                                                                    }),
                                                            ),
                                                    )
                                                },
                                            ),
                                    ),
                            ),
                    ),
            ),
            Tab::Radio => body.child(self.panel("Radio: live mixes from other DJs", cx)),
        }
    }
}

fn nearest_beat_offset(
    position: f64,
    grid: TempoResult,
    reference_position: f64,
    reference: TempoResult,
) -> f64 {
    let phase = ((position - grid.first_beat_seconds) / grid.beat_seconds()).rem_euclid(1.);
    let reference_phase = ((reference_position - reference.first_beat_seconds)
        / reference.beat_seconds())
    .rem_euclid(1.);
    ((reference_phase - phase + 0.5).rem_euclid(1.) - 0.5) * grid.beat_seconds()
}

fn returning_tempo(start: f64, elapsed: f64) -> f64 {
    let t = (elapsed / 2.).clamp(0., 1.);
    let eased = t * t * (3. - 2. * t);
    start + (1. - start) * eased
}

fn clock(seconds: f64) -> String {
    let sign = if seconds < 0. { "−" } else { "" };
    let seconds = seconds.abs() as u64;
    format!("{sign}{}:{:02}", seconds / 60, seconds % 60)
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.stems_starting {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_4()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .child(div().text_3xl().child("faderCraft"))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading stem model and warming up audio processing…"),
                )
                .into_any_element();
        }
        for deck in 0..DECKS {
            if let Some((start, began)) = self.tempo_returns[deck] {
                let elapsed = began.elapsed().as_secs_f64();
                let ratio = returning_tempo(start, elapsed);
                if let Some(engine) = &mut self.engine {
                    // Bypass send's manual-adjustment cancellation for this ramp.
                    if engine
                        .apply(
                            Role::Host,
                            Action::Deck {
                                deck: deck as u8,
                                action: DeckAction::SetTempo { ratio },
                            },
                        )
                        .is_ok()
                        && elapsed >= 2.
                    {
                        self.tempo_returns[deck] = None;
                    }
                } else {
                    self.tempo_returns[deck] = None;
                }
            }
        }
        if self
            .wheel_scrub
            .is_some_and(|(_, _, _, last)| last.elapsed().as_millis() >= 150)
        {
            self.end_wheel_scrub();
        }
        self.refresh_pad_tracks();
        let mut auto_paths_changed = false;
        let mut decks = match &mut self.engine {
            Some(engine) => {
                let state = engine.state();
                self.monitor = state.monitor;
                self.automix = state.automix.clone();
                if self.automix.enabled && self.automix.smart {
                    self.eq_mode = EqMode::Classic;
                }
                for (deck, channel) in state.decks.iter().enumerate() {
                    if let Some(path) = channel.path.as_ref().map(PathBuf::from) {
                        if self.deck_paths[deck].as_ref() != Some(&path) {
                            self.deck_paths[deck] = Some(path);
                            self.pad_tracks[deck] = None;
                            auto_paths_changed = true;
                        }
                    }
                }
                self.mixer_values[10] = state.master_gain.sqrt();
                self.mixer_values[11] = state.crossfader;
                self.mixer_values[20] = state.sampler_gain.sqrt();
                self.mixer_values[21] = state.monitor.gain.sqrt();
                self.mixer_values[22] = state.monitor.mix;
                self.mixer_curve = state.crossfader_curve;
                self.crossfader_routing = state.crossfader_routing;
                self.mixer_peaks[DECKS..DECKS + 2].copy_from_slice(&state.master_stereo_peak);
                for (i, deck) in state.decks.iter().enumerate() {
                    self.deck_eq_modes[i] = deck.eq_mode;
                    self.mixer_values[mixer_controls::channel_index(i)] =
                        (deck.trim_db + 12.) / 24.;
                    for band in 0..3 {
                        let gain = deck.eq[band];
                        self.mixer_values[mixer_controls::channel_index(i) + 3 - band] =
                            if gain <= 1. {
                                gain / 2.
                            } else {
                                0.5 + gain.log10() * 20. / 12.
                            };
                    }
                    self.mixer_values[mixer_controls::channel_index(i) + 4] = deck.volume.sqrt();
                    self.mixer_values[41 + i] = deck.effects[0].parameter;
                    self.mixer_peaks[i] = deck.peak;
                    self.mixer_values
                        [mixer_controls::stem_index(i)..mixer_controls::stem_index(i) + 4]
                        .copy_from_slice(&mixer_controls::stem_controls_from_gains(
                            deck.eq_mode,
                            deck.stem_gains,
                        ));
                }
                state.decks
            }
            None => vec![DeckState::default(); DECKS],
        };
        if auto_paths_changed {
            self.refresh_grids(cx);
        }
        for (deck, state) in decks.iter().enumerate() {
            self.ensure_preparation(deck, state, cx);
        }
        self.persist_automix();
        self.check_stem_cache(cx);
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        for (index, deck) in decks.iter().enumerate() {
            let transport_playing = deck.playing
                && self
                    .cue_preview
                    .as_ref()
                    .is_none_or(|(preview, _, _)| *preview != index);
            self.sync_playback[index].update(deck.path.as_deref(), transport_playing, now);
            let audible = deck.playing
                && self
                    .cue_preview
                    .as_ref()
                    .is_none_or(|(preview, _, _)| *preview != index)
                && self.drag.is_none_or(|drag| drag.deck != index)
                && self
                    .wheel_scrub
                    .is_none_or(|(scrub, _, _, _)| scrub != index)
                && deck.peak > 0.00001
                && deck.volume > 0.
                && self.mixer_values[10] > 0.
                && djcraft_engine::routed_crossfader_gain(
                    index,
                    self.crossfader_routing,
                    djcraft_engine::crossfader_gains(self.mixer_values[11], self.mixer_curve),
                ) > 1e-6;
            if self.play_history[index].update(deck.path.as_deref(), audible, elapsed)
                && let (Some(path), Some(track)) = (&deck.path, &deck.track)
            {
                let saved = self.library.update(cx, |view, cx| {
                    view.record_play(Path::new(path), &track.title, cx)
                });
                if saved {
                    self.play_history[index].recorded = true;
                    self.browser.update(cx, |view, cx| view.refresh(cx));
                }
            }
        }

        for (index, deck) in decks.iter_mut().enumerate() {
            // Audio consumes seek commands on its next callback. Keep the
            // requested position visible until acknowledged, including paused seeks.
            if let Some((target, sent)) = self.pending_seeks[index] {
                if (deck.position_seconds - target).abs() < 0.03
                    || now.duration_since(sent).as_secs_f64() > 0.25
                {
                    self.pending_seeks[index] = None;
                } else {
                    deck.position_seconds = target;
                }
            }
            if deck.loop_region.is_some()
                && self.display_positions[index]
                    .0
                    .is_some_and(|old| old > deck.position_seconds + 0.02)
            {
                self.display_positions[index] = DisplayPosition::default();
            }
            deck.position_seconds = if let Some(drag) = self.drag.filter(|drag| drag.deck == index)
            {
                drag.position
            } else {
                self.display_positions[index].update(
                    deck.position_seconds,
                    deck.playing,
                    deck.tempo,
                    elapsed,
                )
            };
            if let Some(track) = &deck.track {
                deck.position_seconds = deck.position_seconds.min(track.duration_seconds);
            }
        }
        // Positions and finished loads come from the engine, not from UI
        // events, so keep drawing while anything is moving.
        let moving = |deck: &DeckState| {
            deck.playing
                || deck.effect_tail
                || deck.loading
                || deck.track.as_ref().is_some_and(|t| !t.fully_loaded)
        };
        if self.tempo_returns.iter().any(Option::is_some)
            || (self.settings.four_decks && self.deck_tools_changed.elapsed().as_secs_f32() < 0.22)
            || self.wheel_scrub.is_some()
            || decks.iter().any(moving)
            || self.pending_seeks.iter().any(Option::is_some)
            || self.mixer_peaks.iter().any(|peak| *peak > 0.001)
        {
            window.request_animation_frame();
        }
        if std::mem::take(&mut self.focus_loaded_deck) && self.settings_view.is_none() {
            window.focus(&self.deck_focus, cx);
        }
        if std::mem::take(&mut self.focus_settings)
            && let Some(view) = &self.settings_view
        {
            let handle = view.read(cx).focus_handle().clone();
            window.focus(&handle, cx);
        }
        div()
            .track_focus(&self.deck_focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.deck_focus.is_focused(window)
                    && event.keystroke.key == "space"
                    && !event.is_held
                    && this.settings_view.is_none()
                {
                    this.toggle_active_deck(cx);
                    cx.stop_propagation();
                }
            }))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    this.move_preparation_drag(event.position.x.as_f32(), cx);
                    this.move_waveform_drag(event.position.x.as_f32(), cx);
                    this.move_pitch(event.position.y.as_f32(), cx);
                    this.move_mixer_control(event.position, cx);
                    this.move_effect_control(event.position.y.as_f32(), cx);
                } else if event.pressed_button.is_none() {
                    this.end_cue_preview(cx);
                    this.pitch_drag = None;
                    this.release_pads(cx);
                    this.end_preparation_drag(cx);
                    this.end_waveform_drag(cx);
                    this.mixer_drag = None;
                    this.effect_drag = None;
                }
            }))
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| this.release_pads(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| this.release_pads(cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.end_cue_preview(cx);
                    this.pitch_drag = None;
                    this.release_pads(cx);
                    this.end_preparation_drag(cx);
                    this.end_waveform_drag(cx);
                    this.mixer_drag = None;
                    this.effect_drag = None;
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.end_cue_preview(cx);
                    this.pitch_drag = None;
                    this.release_pads(cx);
                    this.end_preparation_drag(cx);
                    this.end_waveform_drag(cx);
                    this.mixer_drag = None;
                    this.effect_drag = None;
                }),
            )
            .child(self.top_bar(cx))
            .child(self.content(&decks, cx))
            .when_some(self.settings_view.clone(), |this, view| {
                this.child(deferred(view))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn four_deck_sync_uses_longest_continuous_playback() {
        use super::{SyncPlayback, select_sync_reference};
        let grid = djcraft_library::TempoResult {
            bpm: 120.,
            first_beat_seconds: 0.,
            confidence: 1.,
            downbeat: 0,
        };
        let mut grids = [Some(grid); super::DECKS];
        let mut playback: [SyncPlayback; super::DECKS] = Default::default();
        let now = std::time::Instant::now();
        let later = now + std::time::Duration::from_secs(10);
        playback[2].update(Some("c"), true, now);
        playback[1].update(Some("b"), true, later);
        // C wins over paired B, even when B's playhead is further into its track.
        assert_eq!(select_sync_reference(0, 4, &grids, &playback), Some(2));
        assert_eq!(select_sync_reference(2, 4, &grids, &playback), Some(1));
        assert_eq!(select_sync_reference(0, 2, &grids, &playback), Some(1));
        playback[2].update(Some("c"), true, later);
        assert_eq!(playback[2].started, Some(now));
        playback[2].update(Some("c"), false, later);
        assert_eq!(select_sync_reference(0, 4, &grids, &playback), Some(1));
        playback[2].update(Some("c"), true, later + std::time::Duration::from_secs(1));
        assert_eq!(select_sync_reference(0, 4, &grids, &playback), Some(1));
        grids[1] = None;
        assert_eq!(select_sync_reference(0, 4, &grids, &playback), Some(2));
        playback[2].update(Some("replacement"), false, later);
        assert_eq!(playback[2].started, None);
        assert_eq!(select_sync_reference(0, 4, &grids, &playback), Some(2));
        grids.fill(None);
        assert_eq!(select_sync_reference(0, 4, &grids, &playback), None);
    }

    use super::{DisplayPosition, HistoryTracker, WaveformDrag, nearest_beat_offset};
    use djcraft_library::TempoResult;
    #[test]
    fn tempo_return_eases_to_unity_without_overshoot() {
        for start in [0.5, 0.995, 1.005, 2.] {
            assert_eq!(super::returning_tempo(start, 0.), start);
            assert_eq!(super::returning_tempo(start, 2.), 1.);
            assert_eq!(super::returning_tempo(start, 3.), 1.);
            let mut previous = start;
            for step in 1..=200 {
                let value = super::returning_tempo(start, step as f64 / 100.);
                assert!((value - 1.).abs() <= (previous - 1.).abs());
                previous = value;
            }
        }
    }

    #[test]
    fn sync_chooses_nearest_beat_across_phase_wrap() {
        let grid = TempoResult {
            bpm: 120.,
            first_beat_seconds: 0.,
            downbeat: 0,
            confidence: 1.,
        };
        assert!((nearest_beat_offset(0.49, grid, 0.01, grid) - 0.02).abs() < 1e-9);
        assert!((nearest_beat_offset(0.01, grid, 0.49, grid) + 0.02).abs() < 1e-9);
        let reference = TempoResult {
            bpm: 100.,
            first_beat_seconds: 0.1,
            ..grid
        };
        assert!(nearest_beat_offset(0.25, grid, 0.4, reference).abs() < 1e-9);
    }

    #[test]
    fn display_clock_smooths_buffer_steps_without_drifting() {
        let mut clock = DisplayPosition::default();
        let dt = 1. / 120.;
        let mut last = clock.update(0., true, 1., dt);
        let mut squared_error = 0.;
        let mut raw_error = 0.;
        let mut last_raw = 0.;
        for frame in 1..1200 {
            let time = frame as f64 * dt;
            let raw = (time / 0.04).floor() * 0.04;
            let displayed = clock.update(raw, true, 1., dt);
            assert!((displayed - raw).abs() <= 0.051);
            assert!(displayed >= last, "visual clock ran backwards");
            squared_error += (displayed - last - dt).powi(2);
            raw_error += (raw - last_raw - dt).powi(2);
            last = displayed;
            last_raw = raw;
        }
        assert!(squared_error < raw_error * 0.1);
        assert_eq!(clock.update(25., true, 1., dt), 25.);
        assert_eq!(clock.update(12., false, 1., dt), 12.);
        assert_eq!(clock.update(30., true, 1., 0.5), 30.);
    }

    #[test]
    fn waveform_drag_directions_and_track_limits() {
        let mut drag = WaveformDrag {
            minimum: 0.,
            deck: 0,
            origin_x: 400.,
            origin_seconds: 20.,
            seconds_per_pixel: -8. / 800.,
            duration: 100.,
            position: 20.,
            resume: false,
            vinyl: true,
        };
        assert_eq!(drag.at(500.), 19.);
        assert_eq!(drag.at(300.), 21.);
        assert_eq!(drag.at(5000.), 0.);
        assert_eq!(drag.at(-10000.), 100.);
        drag.origin_x = 100.;
        drag.origin_seconds = 0.;
        drag.seconds_per_pixel = 100. / 800.;
        assert_eq!(drag.at(500.), 50.);
        assert_eq!(drag.at(0.), 0.);
        assert_eq!(drag.at(1000.), 100.);
    }
    #[test]
    fn history_counts_listening_not_previews_and_resets_for_track_changes() {
        let mut history = HistoryTracker::default();
        assert!(!history.update(Some("a"), true, 0.1));
        for _ in 0..100 {
            assert!(!history.update(Some("a"), false, 0.25));
        }
        for _ in 0..39 {
            assert!(!history.update(Some("a"), true, 0.25));
        }
        assert!(history.update(Some("a"), true, 0.25));
        history.recorded = true;
        assert!(!history.update(Some("a"), true, 0.25));
        assert!(!history.update(Some("b"), true, 0.25));
        assert_eq!(history.seconds, 0.);
        assert!(!history.update(Some("b"), true, 120.));
        assert_eq!(history.seconds, 0.25);
        assert!(!history.update(None, true, 0.25));
        assert_eq!(history.seconds, 0.);
    }
}
