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
#[path = "rhythm_shift_controls.rs"]
mod rhythm_shift_controls;
#[path = "sampler_controls.rs"]
mod sampler_controls;
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

use crate::library_view::{DeckMetadata, DraggedTrack, LibraryEvent, LibraryView};
use crate::settings::{Settings, StartTab, data_dir};
use crate::settings_view::{SettingsEvent, SettingsView};
use crate::waveform::{VISIBLE_SECONDS, overview, waveform};

const TOP_BAR_HEIGHT: Pixels = px(52.);
const OVERVIEW_HEIGHT: f32 = 44.;
const FOUR_DECK_ORDER: [usize; DECKS] = [2, 0, 1, 3];

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

/// A nominal 33⅓ rpm record: one full turn advances 1.8 source seconds.
const PLATTER_SECONDS_PER_REV: f64 = 1.8;

#[derive(Clone, Copy)]
struct PlatterDrag {
    deck: usize,
    last_angle: Option<f64>,
    position: f64,
    limit: f64,
    vinyl: bool,
    slip: bool,
}

impl PlatterDrag {
    fn angle(point: Point<Pixels>, centre: Point<Pixels>) -> Option<f64> {
        let x = (point.x - centre.x).as_f32() as f64;
        let y = (point.y - centre.y).as_f32() as f64;
        (x * x + y * y >= 64.).then(|| y.atan2(x))
    }

    fn turn(&mut self, point: Point<Pixels>, centre: Point<Pixels>) -> f64 {
        let angle = Self::angle(point, centre);
        let delta = match (self.last_angle, angle) {
            (Some(previous), Some(current)) => {
                (current - previous + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI
            }
            _ => 0.,
        };
        self.last_angle = angle;
        let seconds = delta * PLATTER_SECONDS_PER_REV / std::f64::consts::TAU;
        if self.vinyl {
            self.position = (self.position + seconds).clamp(0., self.limit);
        }
        seconds
    }
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

fn select_sync_master(
    preferred: Option<usize>,
    visible: usize,
    grids: &[Option<TempoResult>],
    playback: &[SyncPlayback; DECKS],
) -> Option<usize> {
    let eligible = |deck: usize| deck < visible && grids.get(deck).is_some_and(Option::is_some);
    preferred.filter(|&deck| eligible(deck)).or_else(|| {
        (0..visible)
            .filter(|&deck| eligible(deck))
            // A playing deck wins over an idle one; break ties by deck number.
            .min_by_key(|&deck| {
                (
                    playback[deck].started.is_none(),
                    playback[deck].started,
                    deck,
                )
            })
    })
}

fn camelot_tonic(key: djcraft_library::Camelot) -> i32 {
    let relative = (i32::from(key.number) - 8).rem_euclid(12) * 7 % 12;
    (relative - if key.major { 0 } else { 3 }).rem_euclid(12)
}

fn shortest_key_shift(from: djcraft_library::Camelot, to: djcraft_library::Camelot) -> f32 {
    let delta = (camelot_tonic(to) - camelot_tonic(from)).rem_euclid(12);
    (if delta > 6 { delta - 12 } else { delta }) as f32
}

fn mirrors_dual_deck(action: &DeckAction) -> bool {
    matches!(
        action,
        DeckAction::Play
            | DeckAction::Pause
            | DeckAction::SetTempo { .. }
            | DeckAction::Seek { .. }
            | DeckAction::SetKeyLock { .. }
            | DeckAction::SetQuantize { .. }
            | DeckAction::SetSlip { .. }
            | DeckAction::SetKeyShift { .. }
            | DeckAction::SetTempoRange { .. }
            | DeckAction::ScratchTo { .. }
            | DeckAction::JogScratch { .. }
            | DeckAction::HoldJog
            | DeckAction::Scratch { .. }
            | DeckAction::PitchBend { .. }
            | DeckAction::EndNudge
    )
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
    sync_enabled: [bool; DECKS],
    key_sync_enabled: [bool; DECKS],
    sync_master: Option<usize>,
    sync_refresh: Instant,
    tap_history: [Vec<Instant>; DECKS],
    tap_bpm: [Option<f64>; DECKS],
    dual_deck: [bool; 2],
    deck_select_held: [bool; DECKS],
    keyboard_shift: bool,
    waveform_visible_seconds: f64,
    preview_path: Option<std::path::PathBuf>,
    preview_revision: u64,
    deck_focus: FocusHandle,
    active_deck: usize,
    monitor: djcraft_protocol::MonitorState,
    focus_loaded_deck: bool,
    cue_preview: [Option<(f64, Option<String>)>; DECKS],
    cue_anchor: [Option<(f64, Option<String>)>; DECKS],
    pitch_drag: Option<usize>,
    pitch_knob_start: Option<(f32, f64)>,
    pitch_areas: [Rc<Cell<Bounds<Pixels>>>; DECKS],
    automix: djcraft_protocol::AutomixState,
    saved_queue: Vec<djcraft_protocol::QueueTrack>,
    automix_notice: String,
    tab: Tab,
    library: Entity<LibraryView>,
    analysis_progress: Arc<crate::analysis::Progress>,
    /// The track browser under the decks.
    browser: Entity<LibraryView>,
    settings: Settings,
    performance_settings: crate::performance_settings::PerformanceSettings,
    performance_dirty: Option<std::time::Instant>,
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
    deck_metadata: Vec<Option<DeckMetadata>>,
    deck_file_bpm: [Option<f64>; DECKS],
    deck_artwork: Vec<Option<Arc<Image>>>,
    artwork_paths: Vec<Option<PathBuf>>,
    /// Where each deck's whole-track waveform is on screen, for seeking.
    overview_areas: Vec<Rc<Cell<Bounds<Pixels>>>>,
    waveform_areas: Vec<Rc<Cell<Bounds<Pixels>>>>,
    drag: Option<WaveformDrag>,
    platter_areas: Vec<Rc<Cell<Bounds<Pixels>>>>,
    platter_drag: Option<PlatterDrag>,
    display_positions: [DisplayPosition; DECKS],
    last_frame: Instant,
    overview_cache: [Rc<std::cell::RefCell<crate::waveform::OverviewCache>>; DECKS],
    pending_seeks: [Option<(f64, Instant)>; DECKS],
    jump_refresh_until: [Option<Instant>; DECKS],
    startup_seek: Option<f64>,
    mixer_values: [f32; mixer_controls::CONTROL_COUNT],
    mixer_last_change: [Option<Instant>; mixer_controls::CONTROL_COUNT],
    mixer_areas: Vec<Rc<Cell<Bounds<Pixels>>>>,
    mixer_drag: Option<(usize, f32, f32)>,
    mixer_peaks: [f32; DECKS + 2],
    mixer_clip_until: [Option<Instant>; DECKS + 2],
    mixer_curve: CrossfaderCurve,
    mixer_tab: usize,
    mic_enabled: bool,
    crossfader_routing: djcraft_protocol::CrossfaderRouting,
    eq_mode: EqMode,
    stem_waveforms: [bool; DECKS],
    deck_eq_modes: [EqMode; DECKS],
    stem_tracks: [Option<std::sync::Arc<djcraft_engine::Track>>; DECKS],
    stem_busy: [bool; DECKS],
    library_stem_jobs:
        std::collections::HashMap<PathBuf, std::sync::Arc<std::sync::atomic::AtomicU8>>,
    ready_stem_paths: std::collections::HashSet<PathBuf>,
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
    effect_view: [usize; DECKS],
    effect_selected: [usize; DECKS],
    effect_details: [Option<usize>; DECKS],
    effect_drag: Option<(usize, usize, u8, f32, f32)>,
    loop_size: [i32; DECKS],
    loop_options: [loop_controls::LoopOptions; DECKS],
    loop_view: [usize; DECKS],
    saved_loops_deck: Option<usize>,
    library_side_tab: usize,
    library_side_width: f32,
    library_side_drag: Option<(f32, f32)>,
    rhythm_pack: Option<String>,
    rhythm_notice: String,
    sampler_state: djcraft_protocol::SamplerState,
    sampler_ui: sampler_controls::SamplerUi,
    sampler_list: bool,
    sampler_slot: usize,
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
    controller_loop_press: [Option<f64>; DECKS],
    loop_notice: [String; DECKS],
}

impl Shell {
    fn capture_performance_settings(&mut self) -> bool {
        let state = self.engine.as_mut().map(|engine| engine.state());
        let modes = self
            .controllers
            .as_ref()
            .map(|controllers| controllers.scratch_modes());
        let mut changed = self.performance_settings.capture(state.as_ref(), modes);
        for (deck, pads) in self.pads.iter().enumerate() {
            changed |= self.performance_settings.cue_quantize[deck] != pads.quantize
                || self.performance_settings.cue_quantize_window[deck] != pads.quantize_window;
            self.performance_settings.cue_quantize[deck] = pads.quantize;
            self.performance_settings.cue_quantize_window[deck] = pads.quantize_window;
        }
        changed
    }

    fn persist_performance_settings(&mut self) {
        if self.capture_performance_settings() {
            self.performance_dirty = Some(std::time::Instant::now());
        }
        if self
            .performance_dirty
            .is_some_and(|changed| changed.elapsed().as_millis() >= 250)
        {
            match self
                .performance_settings
                .save(&crate::storage::machine_dir())
            {
                Ok(()) => self.performance_dirty = None,
                Err(error) => {
                    eprintln!("Could not save deck setup: {error}");
                    self.performance_dirty = Some(std::time::Instant::now());
                }
            }
        }
    }
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

    fn active_sync_master(&self) -> Option<usize> {
        select_sync_master(
            self.sync_master,
            self.visible_decks(),
            &self.grids,
            &self.sync_playback,
        )
    }

    fn sync_reference(&self, deck: usize) -> Option<usize> {
        self.active_sync_master().filter(|&master| master != deck)
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
        self.end_platter_drag(cx);
        self.pitch_drag = None;
        self.pitch_knob_start = None;
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
        quick: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if tracks.len() > 2 {
            settings.four_decks = true;
        }
        let performance_settings =
            crate::performance_settings::PerformanceSettings::load(&crate::storage::machine_dir());
        let (engine, engine_error) = match crate::settings::open_audio(&settings) {
            Ok(mut engine) => {
                performance_settings.apply_effects(&mut engine);
                for action in [
                    MixerAction::SetCrossfaderCurve {
                        curve: settings.crossfader_curve,
                    },
                    MixerAction::SetCrossfaderRouting {
                        routing: settings.crossfader_routing,
                    },
                ] {
                    let _ = engine.apply(Role::Host, Action::Mixer { action });
                }
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
            LibraryEvent::SeparateStems { path } => this.prepare_library_stems(path.clone(), cx),
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
            LibraryEvent::SeparateStems { path } => this.prepare_library_stems(path.clone(), cx),
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
        let pads = std::array::from_fn(|deck| {
            let mut pads = pad_controls::Pads::default();
            pads.quantize = performance_settings.cue_quantize[deck];
            pads.quantize_window = performance_settings.cue_quantize_window[deck];
            pads
        });
        let analysis_progress = library.read(cx).analysis_progress();
        let mut shell = Self {
            stems_starting: !quick,
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
            analysis_progress,
            browser,
            settings: settings.clone(),
            settings_view: None,
            performance_dirty: None,
            focus_settings: false,
            engine,
            controllers: match crate::controllers::Controllers::load(
                &settings,
                &performance_settings.scratch_modes,
            ) {
                Ok(controllers) => controllers,
                Err(error) => {
                    eprintln!("Controller setup: {error}");
                    None
                }
            },
            engine_error,
            performance_settings,
            deck_focus: cx.focus_handle(),
            active_deck: 0,
            monitor: Default::default(),
            focus_loaded_deck: false,
            cue_preview: std::array::from_fn(|_| None),
            cue_anchor: std::array::from_fn(|_| None),
            pitch_drag: None,
            pitch_knob_start: None,
            pitch_areas: std::array::from_fn(|_| Rc::default()),
            deck_paths: vec![None; DECKS],
            grids: vec![None; DECKS],
            deck_keys: vec![None; DECKS],
            deck_modes: vec![None; DECKS],
            deck_metadata: (0..DECKS).map(|_| None).collect(),
            deck_file_bpm: [None; DECKS],
            deck_artwork: vec![None; DECKS],
            artwork_paths: vec![None; DECKS],
            overview_areas: (0..DECKS).map(|_| Rc::default()).collect(),
            waveform_areas: (0..DECKS).map(|_| Rc::default()).collect(),
            drag: None,
            platter_areas: (0..DECKS).map(|_| Rc::default()).collect(),
            platter_drag: None,
            display_positions: [DisplayPosition::default(); DECKS],
            last_frame: Instant::now(),
            play_history: Default::default(),
            sync_playback: Default::default(),
            sync_enabled: [false; DECKS],
            key_sync_enabled: [false; DECKS],
            sync_master: None,
            sync_refresh: Instant::now(),
            tap_history: std::array::from_fn(|_| Vec::new()),
            tap_bpm: [None; DECKS],
            dual_deck: [false; 2],
            deck_select_held: [false; DECKS],
            keyboard_shift: false,
            waveform_visible_seconds: VISIBLE_SECONDS,
            preview_path: None,
            preview_revision: 0,
            overview_cache: std::array::from_fn(|_| Rc::default()),
            pending_seeks: [None; DECKS],
            jump_refresh_until: [None; DECKS],
            startup_seek: std::env::var("DJCRAFT_START_SEEK_SECONDS")
                .ok()
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.),
            mixer_values: mixer_controls::initial_values(),
            mixer_last_change: [None; mixer_controls::CONTROL_COUNT],
            mixer_areas: (0..mixer_controls::CONTROL_COUNT)
                .map(|_| Rc::default())
                .collect(),
            mixer_drag: None,
            mixer_peaks: [0.; DECKS + 2],
            mixer_clip_until: [None; DECKS + 2],
            mixer_curve: CrossfaderCurve::default(),
            mixer_tab: 0,
            mic_enabled: false,
            crossfader_routing: djcraft_protocol::CrossfaderRouting::default(),
            eq_mode: EqMode::Classic,
            stem_waveforms: [false; DECKS],
            deck_eq_modes: [EqMode::Classic; DECKS],
            stem_tracks: std::array::from_fn(|_| None),
            stem_busy: [false; DECKS],
            library_stem_jobs: Default::default(),
            ready_stem_paths: crate::stems::ready_paths(settings.stem_model.name()),
            stem_progress: std::array::from_fn(|_| {
                std::sync::Arc::new(std::sync::atomic::AtomicU8::new(254))
            }),
            stem_status: std::array::from_fn(|_| String::new()),
            wheel_scrub: None,
            tempo_returns: [None; DECKS],
            last_loaded: None,
            pending_load: None,
            pads,
            pad_cues: crate::pad_store::load(&data_dir()),
            cue_appearances: crate::pad_store::load_appearances(&data_dir()),
            pad_tracks: std::array::from_fn(|_| None),
            deck_panel: [1; DECKS],
            effect_view: [0; DECKS],
            effect_selected: [0; DECKS],
            effect_details: [None; DECKS],
            effect_drag: None,
            loop_size: [2; DECKS],
            loop_options: [loop_controls::LoopOptions::default(); DECKS],
            loop_view: [0; DECKS],
            saved_loops_deck: None,
            library_side_tab: 0,
            library_side_width: 400.,
            library_side_drag: None,
            rhythm_pack: None,
            rhythm_notice: String::new(),
            sampler_state: Default::default(),
            sampler_ui: Default::default(),
            sampler_list: false,
            sampler_slot: 0,
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
            controller_loop_press: [None; DECKS],
            loop_notice: std::array::from_fn(|_| String::new()),
        };
        if !quick {
            let model = shell.settings.stem_model.name();
            let warmup = cx
                .background_executor()
                .spawn(async move { crate::stems::warm_up(model) });
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
        }
        for (deck, path) in tracks.into_iter().take(DECKS).enumerate() {
            shell.send(deck, DeckAction::Load { path });
        }
        {
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
                        let mut sampler_changed = false;
                        if let Some(engine) = &mut this.engine {
                            let state = engine.state();
                            visuals_moving = state.decks.iter().any(|deck| {
                                deck.playing
                                    || deck.loading
                                    || deck.effect_tail
                                    || deck.track.as_ref().is_some_and(|track| !track.fully_loaded)
                            });
                            sampler_changed = this.sampler_state != state.sampler;
                            this.sampler_state = state.sampler;
                            this.automix = state.automix;
                        }
                        this.ensure_loudness(cx);
                        this.persist_performance_settings();
                        this.persist_automix();
                        // A command's immediate repaint can precede audio-thread
                        // acknowledgement. Bootstrap animation from authoritative
                        // state even when that first frame still saw a stopped deck.
                        if (visuals_moving && this.last_frame.elapsed().as_millis() >= 50)
                            || sampler_changed
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
        shell.sync_library_stem_states(cx);
        shell
    }

    /// Looks up the beat grid of each deck's track in the library, and tells
    /// the browser which track to suggest what comes next for: the one
    /// loaded last.
    fn refresh_grids(&mut self, cx: &mut Context<Self>) {
        for path in self.deck_paths.iter().flatten() {
            self.analysis_progress.prioritize(path);
        }
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
        self.refresh_deck_file_details(cx);
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

    fn nudge_deck_grid(&mut self, deck: usize, beats: i32, cx: &mut Context<Self>) {
        let Some(path) = self.deck_paths[deck].clone() else {
            return;
        };
        self.library.update(cx, |view, cx| {
            view.nudge_beat_grid_at(std::path::Path::new(&path), beats, cx);
        });
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
        let controller_present = self
            .controllers
            .as_ref()
            .is_some_and(|c| c.has_matching_device());
        let view = cx.new(|cx| SettingsView::new(settings, audio, controller_present, cx));
        cx.subscribe(&view, |this, view, event: &SettingsEvent, cx| {
            match event {
                SettingsEvent::Changed(settings) | SettingsEvent::MappingSelected(settings) => {
                    if matches!(event, SettingsEvent::MappingSelected(_)) || this.settings.controller_mapping != settings.controller_mapping || this.settings.controller_options != settings.controller_options {
                        this.persist_performance_settings();
                        match crate::controllers::Controllers::load(settings, &this.performance_settings.scratch_modes) {
                            Ok(mut next) => {
                                if let Some(mut old) = this.controllers.take() {
                                    for action in old.release_all() { this.controller_action(action, cx); }
                                }
                                if let Some(controller) = &mut next { controller.use_connected(); }
                                this.controllers = next;
                                view.update(cx, |view, cx| view.controller_notice(if settings.controller_mapping == "@disabled" { "Controller input disabled." } else { "Mapping applied. Matching connected devices are enabled; new connections will ask first." }.into(), cx));
                            }
                            Err(error) => {
                                view.update(cx, |view, cx| view.controller_notice(error, cx));
                                return;
                            }
                        }
                    }
                    if this.engine.is_some() {
                        if this.settings.crossfader_curve != settings.crossfader_curve {
                            let dispatch = djcraft_protocol::script::Program::set_curve(settings.crossfader_curve)
                                .run(djcraft_protocol::script::ControlSource::Skin, 0.).unwrap();
                            this.apply_control_dispatch(dispatch, cx);
                        }
                        if this.settings.crossfader_routing != settings.crossfader_routing {
                            let dispatch = djcraft_protocol::script::Program::set_routing(settings.crossfader_routing)
                                .run(djcraft_protocol::script::ControlSource::Skin, 0.).unwrap();
                            this.apply_control_dispatch(dispatch, cx);
                        }
                    }
                    if this.settings.stem_model != settings.stem_model {
                        this.ready_stem_paths = crate::stems::ready_paths(settings.stem_model.name());
                    }
                    this.settings = settings.clone();
                    this.sync_library_stem_states(cx);
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
        if self.drag.is_some() || self.platter_drag.is_some() {
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

    fn toggle_sync(&mut self, deck: usize, cx: &mut Context<Self>) {
        if self.sync_enabled[deck] {
            self.sync_enabled[deck] = false;
            self.send(deck, DeckAction::EndNudge);
        } else if self.sync_reference(deck).is_some() && self.grids[deck].is_some() {
            self.sync_enabled[deck] = true;
            self.sync_deck(deck, cx);
        }
        cx.notify();
    }

    fn set_sync_master(&mut self, deck: usize, cx: &mut Context<Self>) {
        if deck >= self.visible_decks() || self.grids[deck].is_none() {
            return;
        }
        self.sync_master = Some(deck);
        if self.sync_enabled[deck] {
            self.send(deck, DeckAction::EndNudge);
        }
        self.sync_enabled[deck] = false;
        self.key_sync_enabled[deck] = false;
        for other in 0..DECKS {
            if other != deck && self.sync_enabled[other] {
                self.sync_deck(other, cx);
            }
        }
        cx.notify();
    }

    fn track_start(&mut self, deck: usize, cx: &mut Context<Self>) {
        self.end_cue_preview(cx);
        self.send(deck, DeckAction::Seek { seconds: 0. });
        cx.notify();
    }

    fn key_sync(&mut self, deck: usize, cx: &mut Context<Self>) {
        if self.key_sync_enabled[deck] {
            self.key_sync_enabled[deck] = false;
            self.send(deck, DeckAction::SetKeyShift { semitones: 0. });
            return;
        }
        let Some(reference) = self.sync_reference(deck) else {
            return;
        };
        let (Some(from), Some(to)) = (self.deck_keys[deck], self.deck_keys[reference]) else {
            return;
        };
        let semitones = shortest_key_shift(from, to);
        self.send(deck, DeckAction::SetKeyShift { semitones });
        self.key_sync_enabled[deck] = true;
        cx.notify();
    }

    fn tap_bpm(&mut self, deck: usize, cx: &mut Context<Self>) {
        let now = Instant::now();
        let taps = &mut self.tap_history[deck];
        if taps
            .last()
            .is_some_and(|last| now.duration_since(*last).as_secs_f64() > 2.)
        {
            taps.clear();
        }
        taps.push(now);
        if taps.len() > 6 {
            taps.remove(0);
        }
        let intervals: Vec<f64> = taps
            .windows(2)
            .map(|pair| pair[1].duration_since(pair[0]).as_secs_f64())
            .collect();
        if intervals.len() < 2
            || intervals
                .iter()
                .any(|interval| !(0.25..=2.).contains(interval))
        {
            return;
        }
        let average = intervals.iter().sum::<f64>() / intervals.len() as f64;
        self.tap_bpm[deck] = Some(60. / average);
        cx.notify();
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
            self.set_skin_tempo(deck, ((ratio - 1.) / state.tempo_range.extent()) as f32, cx);
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn begin_waveform_drag(&mut self, deck: usize, x: f32, overview: bool, cx: &mut Context<Self>) {
        self.end_wheel_scrub();
        self.end_platter_drag(cx);
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
                -self.waveform_visible_seconds * state.tempo / width as f64
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

    fn begin_platter_drag(&mut self, deck: usize, cursor: Point<Pixels>, cx: &mut Context<Self>) {
        self.end_waveform_drag(cx);
        self.end_platter_drag(cx);
        self.end_wheel_scrub();
        let Some(state) = self
            .engine
            .as_mut()
            .and_then(|engine| engine.state().decks.get(deck).cloned())
        else {
            return;
        };
        let Some(track) = &state.track else { return };
        let bounds = self.platter_areas[deck].get();
        let centre = bounds.origin + point(bounds.size.width / 2., bounds.size.height / 2.);
        let vinyl = self.performance_settings.scratch_modes[deck].unwrap_or(true);
        self.platter_drag = Some(PlatterDrag {
            deck,
            last_angle: PlatterDrag::angle(cursor, centre),
            position: state.position_seconds,
            limit: track.loaded_seconds.min(track.duration_seconds).max(0.),
            vinyl,
            slip: state.slip_enabled,
        });
        if vinyl {
            self.send(deck, DeckAction::HoldJog);
        }
        cx.notify();
    }

    fn move_platter_drag(&mut self, cursor: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(mut drag) = self.platter_drag else {
            return;
        };
        let bounds = self.platter_areas[drag.deck].get();
        let centre = bounds.origin + point(bounds.size.width / 2., bounds.size.height / 2.);
        let seconds = drag.turn(cursor, centre);
        self.platter_drag = Some(drag);
        if seconds.abs() < 1e-7 {
            return;
        }
        if drag.vinyl {
            self.send(
                drag.deck,
                DeckAction::ScratchTo {
                    seconds: drag.position,
                },
            );
        } else {
            self.send(
                drag.deck,
                DeckAction::PitchBend {
                    seconds: seconds.clamp(-0.5, 0.5),
                },
            );
        }
        cx.notify();
    }

    fn end_platter_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.platter_drag.take() else {
            return;
        };
        if drag.vinyl {
            self.send(drag.deck, DeckAction::Scratch { speed: None });
            if !drag.slip {
                self.send(
                    drag.deck,
                    DeckAction::Seek {
                        seconds: drag.position,
                    },
                );
            }
        } else {
            self.send(drag.deck, DeckAction::EndNudge);
        }
        cx.notify();
    }

    fn refresh_after_jump(&mut self, deck: usize) {
        self.pending_seeks[deck] = None;
        self.display_positions[deck] = DisplayPosition::default();
        // The audio callback resolves relative jumps against its actual playhead.
        // Keep a paused waveform redrawing long enough to receive that position.
        self.jump_refresh_until[deck] =
            Some(Instant::now() + std::time::Duration::from_millis(250));
    }

    /// Sends a deck action to the engine as the session host.
    fn send(&mut self, deck: usize, action: DeckAction) {
        let cue_anchor = self.transport_anchor_for(deck, &action);
        if matches!(
            action,
            DeckAction::Load { .. } | DeckAction::Unload | DeckAction::Pause
        ) {
            self.sync_playback[deck] = SyncPlayback::default();
        }
        if matches!(
            action,
            DeckAction::Load { .. }
                | DeckAction::Unload
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
        if matches!(action, DeckAction::Load { .. } | DeckAction::Unload) {
            self.release_pad(deck);
            if self.sync_master == Some(deck) {
                self.sync_master = None;
            }
            self.loop_in[deck] = None;
            self.controller_loop_press[deck] = None;
            self.loop_last[deck] = None;
            self.loop_recording[deck] = None;
            self.loop_notice[deck].clear();
        }
        if matches!(
            action,
            DeckAction::SetTempo { .. }
                | DeckAction::Sync { .. }
                | DeckAction::Load { .. }
                | DeckAction::Unload
        ) {
            self.tempo_returns[deck] = None;
        }
        if matches!(action, DeckAction::Load { .. } | DeckAction::Unload)
            && self
                .wheel_scrub
                .is_some_and(|(active, _, _, _)| active == deck)
        {
            self.end_wheel_scrub();
        }
        if let Some(display) = self.display_positions.get_mut(deck) {
            match &action {
                DeckAction::Seek { seconds } | DeckAction::CuePreview { seconds, .. } => {
                    self.jump_refresh_until[deck] = None;
                    display.0 = Some(*seconds);
                    self.pending_seeks[deck] = Some((*seconds, Instant::now()));
                }
                DeckAction::Load { .. } | DeckAction::Unload => {
                    self.jump_refresh_until[deck] = None;
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
        let unloading = matches!(
            &action,
            Action::Deck {
                action: DeckAction::Unload,
                ..
            }
        );
        let loading = matches!(
            &action,
            Action::Deck {
                action: DeckAction::Load { .. },
                ..
            }
        );
        let jumping = matches!(
            &action,
            Action::Deck {
                action: DeckAction::Jump { .. },
                ..
            }
        );
        match engine.apply(Role::Host, action) {
            Ok(()) if loading => {
                if let Some(path) = self.deck_paths.get(deck).and_then(|p| p.as_deref()) {
                    self.analysis_progress.prioritize(path);
                }
                self.cue_anchor[deck] = cue_anchor;
                self.active_deck = deck;
                self.focus_loaded_deck = true;
                self.stem_busy[deck] = false;
                self.stem_status[deck].clear();
            }
            Err(reason) => eprintln!("action rejected: {reason:?}"),
            Ok(()) => {
                self.cue_anchor[deck] = cue_anchor;
                if jumping {
                    self.refresh_after_jump(deck);
                }
                if unloading {
                    self.deck_paths[deck] = None;
                }
            }
        }
    }

    fn poll_controllers(&mut self, cx: &mut Context<Self>) {
        let Some(mut controllers) = self.controllers.take() else {
            return;
        };
        let previous_prompt = controllers.pending_connection();
        let previous_status = self.stems_starting.then(|| controllers.connection_status());
        let batches = controllers.drain();
        if previous_prompt != controllers.pending_connection()
            || previous_status.is_some_and(|previous| previous != controllers.connection_status())
        {
            cx.notify();
        }
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
            self.controller_command(&mut controllers, command, cx);
        }
        for (port, events) in batches {
            let mut jog_batch = crate::controllers::JogBatch::default();
            for event in events {
                let Some(engine) = &mut self.engine else {
                    break;
                };
                let state = engine.state();
                let actions = controllers.process(&port, event, &state);
                for (deck, mode) in controllers.take_pad_modes().into_iter().enumerate() {
                    if let Some(mode) = mode {
                        self.follow_controller_pad_mode(deck, mode, cx);
                        if self.dual_deck[deck % 2] {
                            self.follow_controller_pad_mode(deck ^ 2, mode, cx);
                        }
                    }
                }
                for action in actions {
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
                    self.controller_command(&mut controllers, command, cx);
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
            let master = self.active_sync_master();
            let following = std::array::from_fn::<_, DECKS, _>(|deck| {
                master.is_some_and(|master| master != deck) && self.sync_enabled[deck]
            });
            for action in controllers.tick(&state, &self.loop_in, &following) {
                self.controller_action(action, cx);
            }
        }
        if let Some(view) = &self.settings_view {
            view.update(cx, |view, cx| {
                view.set_controller_present(controllers.has_matching_device(), cx)
            });
        }
        self.controllers = Some(controllers);
    }

    fn controller_action(&mut self, action: Action, cx: &mut Context<Self>) {
        if let Action::Deck {
            deck,
            action: deck_action,
        } = &action
        {
            let index = usize::from(*deck);
            if index < DECKS && self.dual_deck[index % 2] && mirrors_dual_deck(deck_action) {
                let mirrored_action = match deck_action {
                    DeckAction::ScratchTo { seconds } => {
                        if let Some(state) = self.engine.as_mut().map(|engine| engine.state()) {
                            DeckAction::ScratchTo {
                                seconds: (state.decks[index ^ 2].position_seconds + seconds
                                    - state.decks[index].position_seconds)
                                    .max(0.),
                            }
                        } else {
                            deck_action.clone()
                        }
                    }
                    DeckAction::JogScratch {
                        seconds,
                        speed,
                        interval_seconds,
                    } => {
                        if let Some(state) = self.engine.as_mut().map(|engine| engine.state()) {
                            DeckAction::JogScratch {
                                seconds: (state.decks[index ^ 2].position_seconds + seconds
                                    - state.decks[index].position_seconds)
                                    .max(0.),
                                speed: *speed,
                                interval_seconds: *interval_seconds,
                            }
                        } else {
                            deck_action.clone()
                        }
                    }
                    _ => deck_action.clone(),
                };
                self.apply_control_dispatch(
                    djcraft_protocol::script::Dispatch {
                        source: djcraft_protocol::script::ControlSource::Controller,
                        action: Action::Deck {
                            deck: *deck ^ 2,
                            action: mirrored_action,
                        },
                    },
                    cx,
                );
            }
        }
        self.apply_control_dispatch(
            djcraft_protocol::script::Dispatch {
                source: djcraft_protocol::script::ControlSource::Controller,
                action,
            },
            cx,
        );
    }

    fn run_skin_button(
        &mut self,
        deck: usize,
        action: djcraft_protocol::script::ButtonAction,
        cx: &mut Context<Self>,
    ) -> bool {
        self.run_control_button(
            deck,
            action,
            djcraft_protocol::script::ControlSource::Skin,
            cx,
        )
    }

    fn run_control_button(
        &mut self,
        deck: usize,
        action: djcraft_protocol::script::ButtonAction,
        source: djcraft_protocol::script::ControlSource,
        cx: &mut Context<Self>,
    ) -> bool {
        use djcraft_protocol::script::Program;
        let Some(state) = self.engine.as_mut().map(|engine| engine.state()) else {
            return false;
        };
        match Program::do_action(deck, action)
            .and_then(|program| program.run_with_state(source, 0.0, Some(&state)))
        {
            Ok(dispatch) => self.apply_control_dispatch(dispatch, cx),
            Err(reason) => {
                eprintln!("{source:?} button rejected: {reason}");
                false
            }
        }
    }

    /// Common destination for skin scripts and controller actions. Deck actions
    /// retain the host's load, cue and transport bookkeeping before engine apply.
    fn apply_control_dispatch(
        &mut self,
        dispatch: djcraft_protocol::script::Dispatch,
        cx: &mut Context<Self>,
    ) -> bool {
        let source = dispatch.source;
        let action = dispatch.action;
        let curve = match &action {
            Action::Mixer {
                action: MixerAction::SetCrossfaderCurve { curve },
            } => Some(*curve),
            _ => None,
        };
        let routing = match &action {
            Action::Mixer {
                action: MixerAction::SetCrossfaderRouting { routing },
            } => Some(*routing),
            _ => None,
        };
        let applied = match action {
            Action::Host { deck, action } if usize::from(deck) < self.deck_paths.len() => {
                let deck = usize::from(deck);
                match action {
                    djcraft_protocol::HostAction::PadMode { mode } => {
                        self.follow_controller_pad_mode(deck, mode, cx)
                    }
                    djcraft_protocol::HostAction::PerformancePad {
                        mode,
                        pad,
                        alternate,
                    } => {
                        self.follow_controller_pad_mode(deck, mode, cx);
                        self.controller_performance_pad(deck, mode, pad, alternate, cx);
                    }
                    djcraft_protocol::HostAction::ReleasePerformancePad => {
                        self.release_pad(deck);
                    }
                    _ => return false,
                }
                true
            }
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
                true
            }
            action => {
                if let Some(engine) = &mut self.engine {
                    match engine.apply(Role::Host, action) {
                        Ok(()) => true,
                        Err(reason) => {
                            eprintln!("{source:?} action rejected: {reason:?}");
                            false
                        }
                    }
                } else {
                    false
                }
            }
        };
        if applied
            && source != djcraft_protocol::script::ControlSource::Skin
            && (curve.is_some() || routing.is_some())
        {
            if let Some(curve) = curve {
                self.settings.crossfader_curve = curve;
            }
            if let Some(routing) = routing {
                self.settings.crossfader_routing = routing;
                self.crossfader_routing = routing;
            }
            if let Some(view) = &self.settings_view {
                view.update(cx, |view, cx| {
                    view.sync_crossfader(
                        self.settings.crossfader_curve,
                        self.settings.crossfader_routing,
                        cx,
                    )
                });
            }
            if let Err(error) = self.settings.save(&crate::storage::machine_dir()) {
                eprintln!("Could not save controller crossfader settings: {error}");
            }
            cx.set_global(self.settings.clone());
        }
        cx.notify();
        applied
    }

    fn controller_command(
        &mut self,
        controllers: &mut crate::controllers::Controllers,
        command: djcraft_controllers::HostCommand,
        cx: &mut Context<Self>,
    ) {
        use djcraft_controllers::HostCommand;
        let deck = usize::from(command.deck());
        if deck < DECKS && self.dual_deck[deck % 2] {
            let partner = (deck ^ 2) as u8;
            let mirrored = match command {
                HostCommand::Sync { .. } => Some(HostCommand::Sync { deck: partner }),
                HostCommand::Cue { pressed, .. } => Some(HostCommand::Cue {
                    deck: partner,
                    pressed,
                }),
                HostCommand::LoopButton { button, .. } => Some(HostCommand::LoopButton {
                    deck: partner,
                    button,
                }),
                HostCommand::LoopPad { pad, alternate, .. } => Some(HostCommand::LoopPad {
                    deck: partner,
                    pad,
                    alternate,
                }),
                HostCommand::PerformancePad {
                    mode,
                    pad,
                    alternate,
                    ..
                } => Some(HostCommand::PerformancePad {
                    deck: partner,
                    mode,
                    pad,
                    alternate,
                }),
                HostCommand::ReleasePerformancePad { .. } => {
                    Some(HostCommand::ReleasePerformancePad { deck: partner })
                }
                HostCommand::TrackStart { .. } => Some(HostCommand::TrackStart { deck: partner }),
                HostCommand::ToggleVinyl { .. } => Some(HostCommand::ToggleVinyl { deck: partner }),
                HostCommand::ToggleSlip { .. } => Some(HostCommand::ToggleSlip { deck: partner }),
                HostCommand::KeySync { .. } => Some(HostCommand::KeySync { deck: partner }),
                HostCommand::KeyReset { .. } => Some(HostCommand::KeyReset { deck: partner }),
                HostCommand::AutoLoop { beats, .. } => Some(HostCommand::AutoLoop {
                    deck: partner,
                    beats,
                }),
                _ => None,
            };
            if let Some(mirrored) = mirrored {
                self.controller_command_single(controllers, mirrored, cx);
            }
        }
        self.controller_command_single(controllers, command, cx);
    }

    fn controller_command_single(
        &mut self,
        controllers: &mut crate::controllers::Controllers,
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
            HostCommand::BrowserZoom { steps } => {
                self.waveform_visible_seconds = (self.waveform_visible_seconds
                    * 1.1f64.powi(-steps.clamp(-30, 30)))
                .clamp(2., 32.);
                cx.notify();
            }
            HostCommand::Browser { action } => {
                if action == djcraft_protocol::BrowserAction::PreviewToggle {
                    let view = if self.tab == Tab::Library {
                        &self.library
                    } else {
                        &self.browser
                    };
                    let path = view.read(cx).controller_selected_path(cx);
                    if let Some(path) = path {
                        if self.preview_path.as_ref() == Some(&path) {
                            self.preview_revision = self.preview_revision.wrapping_add(1);
                            if let Some(engine) = &mut self.engine {
                                let _ = engine.preview(None);
                            }
                            self.preview_path = None;
                        } else if let Some(engine) = &self.engine {
                            self.preview_revision = self.preview_revision.wrapping_add(1);
                            let revision = self.preview_revision;
                            let sample_rate = engine.sample_rate();
                            self.preview_path = Some(path.clone());
                            let decoded = cx
                                .background_executor()
                                .spawn(async move { Track::decode(&path, sample_rate) });
                            cx.spawn(async move |this, cx| {
                                let result = decoded.await;
                                let _ = this.update(cx, |this, _| {
                                    if this.preview_revision == revision {
                                        match result {
                                            Ok(track) => {
                                                if let Some(engine) = &mut this.engine {
                                                    let _ = engine.preview(Some(track));
                                                }
                                            }
                                            Err(error) => {
                                                eprintln!("Preview decode: {error}");
                                                this.preview_path = None;
                                            }
                                        }
                                    }
                                });
                            })
                            .detach();
                        }
                    }
                } else if action == djcraft_protocol::BrowserAction::PreviewSkip {
                    if let Some(engine) = &mut self.engine {
                        let _ = engine.preview_skip(30.);
                    }
                } else if action == djcraft_protocol::BrowserAction::FullView {
                    self.show(
                        if self.tab == Tab::Library {
                            Tab::Live
                        } else {
                            Tab::Library
                        },
                        cx,
                    );
                } else {
                    if matches!(
                        action,
                        djcraft_protocol::BrowserAction::Suggestions
                            | djcraft_protocol::BrowserAction::Related
                    ) {
                        self.show(Tab::Live, cx);
                    } else if self.tab != Tab::Library && self.tab != Tab::Live {
                        self.show(Tab::Live, cx);
                    }
                    let view = if self.tab == Tab::Library {
                        &self.library
                    } else {
                        &self.browser
                    };
                    view.update(cx, |view, cx| view.controller_action(action, cx));
                }
            }
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
            HostCommand::LoopButton { button, .. } => {
                self.controller_loop_button(deck, button, cx);
            }
            HostCommand::LoopPad { pad, alternate, .. } => {
                self.controller_loop_pad(deck, pad, alternate, cx);
            }
            HostCommand::SamplePad { slot, stop, .. } => {
                self.controller_sample_pad(deck, slot, stop);
            }
            HostCommand::PerformancePad {
                mode,
                pad,
                alternate,
                ..
            } => {
                self.controller_performance_pad(deck, mode, pad, alternate, cx);
            }
            HostCommand::ReleasePerformancePad { .. } => self.release_pad(deck),
            HostCommand::Sync { .. } => self.toggle_sync(deck, cx),
            HostCommand::Cue { pressed: true, .. } => self.begin_cue_preview(deck, cx),
            HostCommand::Cue { pressed: false, .. } => {
                self.end_deck_cue_preview(deck, cx);
            }
            HostCommand::ToggleHeadphoneCue { .. } => self.toggle_headphone_cue(
                deck,
                djcraft_protocol::script::ControlSource::Controller,
                cx,
            ),
            HostCommand::ToggleSlip { .. } => self.toggle_slip(
                deck,
                djcraft_protocol::script::ControlSource::Controller,
                cx,
            ),
            HostCommand::ToggleVinyl { .. } => {
                let enabled = self.flip_vinyl_setting(deck);
                for action in controllers.set_vinyl_mode(deck as u8, enabled) {
                    self.controller_action(action, cx);
                }
                cx.notify();
            }
            HostCommand::SelectDeck { .. } => {
                let partner = deck ^ 2;
                if self.deck_select_held[partner] {
                    self.dual_deck[deck % 2] = true;
                } else if self.dual_deck[deck % 2] {
                    self.dual_deck[deck % 2] = false;
                    self.active_deck = deck;
                } else {
                    self.active_deck = deck;
                }
                self.deck_select_held[deck] = true;
            }
            HostCommand::ReleaseDeckSelect { .. } => self.deck_select_held[deck] = false,
            HostCommand::DualDeckToggle { .. } => {
                self.dual_deck[deck % 2] = !self.dual_deck[deck % 2]
            }
            HostCommand::SetSyncMaster { .. } => self.set_sync_master(deck, cx),
            HostCommand::KeySync { .. } => self.key_sync(deck, cx),
            HostCommand::KeyReset { .. } => {
                self.key_sync_enabled[deck] = false;
                self.send(deck, DeckAction::SetKeyShift { semitones: 0. });
            }
            HostCommand::TapBpm { .. } => self.tap_bpm(deck, cx),
            HostCommand::TrackStart { .. } => self.track_start(deck, cx),
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

    fn toggle_slip(
        &mut self,
        deck: usize,
        source: djcraft_protocol::script::ControlSource,
        cx: &mut Context<Self>,
    ) {
        self.run_control_button(
            deck,
            djcraft_protocol::script::ButtonAction::ToggleSlip,
            source,
            cx,
        );
    }

    fn flip_vinyl_setting(&mut self, deck: usize) -> bool {
        let enabled = !self.performance_settings.scratch_modes[deck].unwrap_or(true);
        self.performance_settings.scratch_modes[deck] = Some(enabled);
        self.performance_dirty = Some(Instant::now());
        enabled
    }

    fn toggle_vinyl(&mut self, deck: usize, cx: &mut Context<Self>) {
        let enabled = self.flip_vinyl_setting(deck);
        if let Some(controllers) = &mut self.controllers {
            let actions = controllers.set_vinyl_mode(deck as u8, enabled);
            for action in actions {
                self.controller_action(action, cx);
            }
        }
        cx.notify();
    }

    fn toggle_headphone_cue(
        &mut self,
        deck: usize,
        source: djcraft_protocol::script::ControlSource,
        cx: &mut Context<Self>,
    ) {
        self.run_control_button(
            deck,
            djcraft_protocol::script::ButtonAction::ToggleHeadphoneCue,
            source,
            cx,
        );
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
                .h_full()
                .relative()
                .top(px(3.))
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
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(self.tab == Tab::Live, |header| {
                            header.child(self.master_header_meters())
                        }),
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
        let mode_buttons =
            (!self.settings.four_decks).then(|| self.deck_mode_buttons(index, state, false, cx));
        let theme = cx.theme();
        let title: SharedString = match (&state.track, &state.error) {
            _ if state.loading => "Loading…".into(),
            (_, Some(error)) => error.clone().into(),
            (Some(track), _) => track.title.clone().into(),
            (None, None) => "No track loaded".into(),
        };
        let track = self.engine.as_ref().and_then(|engine| engine.track(index));
        let area = self.overview_areas[index].clone();
        let duration = state
            .track
            .as_ref()
            .map_or(1., |track| track.duration_seconds.max(0.001));
        let overview_height = if self.settings.four_decks {
            30.
        } else {
            OVERVIEW_HEIGHT
        };
        let playing = state.playing;

        let deck_accent = deck_colour(index);
        let deck_selected = self.active_deck == index;
        let deck_button_selected = if self.keyboard_shift {
            self.dual_deck[index % 2]
        } else {
            deck_selected
        };
        let headphone_cued = self.monitor.cue_enabled(index);
        let load_button = Button::new(("load", index))
            .tooltip("Choose an audio file for this deck.")
            .outline()
            .small()
            .icon(IconName::FolderOpen)
            .when(self.settings.four_decks, |button| {
                button.rounded(ButtonRounded::None).w(px(30.)).h(px(30.))
            })
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
        let eject_button = Button::new(("eject", index))
            .outline()
            .small()
            .rounded(ButtonRounded::None)
            .w(px(30.))
            .h(px(30.))
            .icon(IconName::Eject)
            .tooltip("Eject the loaded track from this deck.")
            .disabled(state.track.is_none())
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.run_skin_button(index, djcraft_protocol::script::ButtonAction::Unload, cx)
                {
                    return;
                }
                this.deck_paths[index] = None;
                this.pad_tracks[index] = None;
                if this.last_loaded == Some(index) {
                    this.last_loaded = None;
                }
                this.refresh_grids(cx);
                cx.notify();
            }));
        let play_button = Button::new(("play", index))
            .tooltip("Play or pause this deck.")
            .primary()
            .small()
            .when(self.settings.four_decks, |button| {
                button
                    .rounded(ButtonRounded::None)
                    .w_full()
                    .flex_1()
                    .border_1()
                    .bg(rgb(if playing { 0x30343a } else { 0x174d2e }))
                    .text_color(rgb(0xffffff))
            })
            .when(!self.settings.four_decks, |button| {
                button
                    .rounded(ButtonRounded::None)
                    .w_full()
                    .flex_1()
                    .border_1()
                    .bg(rgb(if playing { 0x30343a } else { 0x174d2e }))
                    .text_color(rgb(0xffffff))
            })
            .icon(if playing {
                IconName::Pause
            } else {
                IconName::Play
            })
            .disabled(state.track.is_none())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.run_skin_button(
                    index,
                    djcraft_protocol::script::ButtonAction::TogglePlay,
                    cx,
                );
            }));
        let sync_master = self.active_sync_master();
        let sync_button = Button::new(("sync", index))
            .outline()
            .small()
            .label(if self.keyboard_shift {
                "MASTER"
            } else {
                "SYNC"
            })
            .selected(if self.keyboard_shift {
                sync_master == Some(index)
            } else {
                self.sync_enabled[index] && self.sync_reference(index).is_some()
            })
            .when(self.settings.four_decks, |button| {
                button.rounded(ButtonRounded::None).w_full().h(px(30.))
            })
            .when(!self.settings.four_decks, |button| {
                button.rounded(ButtonRounded::None).w_full().h(px(30.))
            })
            .tooltip(if self.keyboard_shift {
                "Choose this deck as the sync master"
            } else if sync_master == Some(index) {
                "This deck is the sync master"
            } else {
                "Follow the sync master's tempo and align beats when both decks play"
            })
            .disabled(
                state.track.is_none()
                    || self.grids[index].is_none()
                    || (!self.keyboard_shift && self.sync_reference(index).is_none()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.keyboard_shift {
                    this.set_sync_master(index, cx);
                } else {
                    this.toggle_sync(index, cx);
                }
            }));
        let cue_button = div()
            .id(("cue-hold", index))
            .w_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if this.keyboard_shift {
                        this.track_start(index, cx);
                    } else {
                        this.begin_cue_preview(index, cx);
                    }
                    window.focus(&this.deck_focus, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                Button::new(("cue", index))
                    .outline()
                    .small()
                    .label(if self.keyboard_shift { "START" } else { "CUE" })
                    .w_full()
                    .when(self.settings.four_decks, |button| {
                        button
                            .rounded(ButtonRounded::None)
                            .h(px(30.))
                            .text_color(rgb(0xd94b4b))
                    })
                    .when(!self.settings.four_decks, |button| {
                        button
                            .rounded(ButtonRounded::None)
                            .h(px(30.))
                            .text_color(rgb(0xd94b4b))
                    })
                    .tooltip("Hold to preview; release to stop and return to the starting point.")
                    .disabled(state.track.is_none()),
            );
        let deck_select = Button::new(("select-deck", index))
            .outline()
            .w(px(30.))
            .h(px(30.))
            .when(self.settings.four_decks, |button| {
                button.rounded(ButtonRounded::None).min_w_0().p_0()
            })
            .when(!self.settings.four_decks, |button| {
                button.rounded(ButtonRounded::None).min_w_0().px_0()
            })
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .label(if self.keyboard_shift { "D" } else { deck_label(index) })
            .selected(deck_button_selected)
            .border_color(deck_accent)
            .bg(if deck_button_selected {
                Hsla::from(deck_accent)
            } else {
                Hsla::from(deck_accent).opacity(0.12)
            })
            .text_color(if deck_button_selected {
                rgb(0x070b12)
            } else {
                deck_accent
            })
            .tooltip(if self.keyboard_shift {
                "Toggle Dual Deck transport for this deck and its paired deck. Mixer controls remain separate."
            } else {
                "Activate this deck for keyboard controls. Does not change headphone cue."
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.keyboard_shift {
                    this.dual_deck[index % 2] = !this.dual_deck[index % 2];
                } else {
                    this.active_deck = index;
                }
                window.focus(&this.deck_focus, cx);
                cx.notify();
            }));
        let headphone_button = Button::new(("headphone-cue", index))
            .outline()
            .small()
            .rounded(ButtonRounded::None).w(px(30.)).h(px(30.))
            .icon(IconName::Headphones)
            .selected(headphone_cued)
            .when(headphone_cued, |button| button.bg(Hsla::from(deck_accent)).text_color(rgb(0x070b12)))
            .tooltip("Toggle this deck in headphones. Independent of deck activation; multiple decks can be cued.")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_headphone_cue(index, djcraft_protocol::script::ControlSource::Skin, cx);
            }));
        let quantize_button = Button::new(("platter-quantize", index))
            .outline()
            .rounded(ButtonRounded::None)
            .w(px(60.))
            .h(px(13.))
            .min_w_0()
            .p_0()
            .child(div().text_size(px(9.)).line_height(px(9.)).font_weight(FontWeight::NORMAL).child(if self.keyboard_shift { "TAP BPM" } else { "QUANTIZE" }))
            .selected(!state.quantize_disabled)
            .border_color(if state.quantize_disabled { cx.theme().border } else { Hsla::from(deck_accent) })
            .bg(if state.quantize_disabled {
                cx.theme().background
            } else {
                Hsla::from(deck_accent)
            })
            .text_color(if state.quantize_disabled { cx.theme().muted_foreground } else { Hsla::from(rgb(0x070b12)) })
            .tooltip("Deck quantize: bypass or restore Hot Cue, loop and scratch quantize without changing their settings.")
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.keyboard_shift {
                    this.tap_bpm(index, cx);
                } else {
                    this.run_skin_button(
                        index,
                        djcraft_protocol::script::ButtonAction::ToggleQuantize,
                        cx,
                    );
                }
            }));
        let (controls, display_identity, display_sync) = if self.settings.four_decks {
            let controls = div()
                .flex()
                .flex_col()
                .w(px(64.))
                .h(px(98.))
                .gap(px(4.))
                .flex_shrink_0()
                .child(
                    div()
                        .flex()
                        .w_full()
                        .gap(px(4.))
                        .child(load_button)
                        .child(eject_button),
                )
                .child(cue_button)
                .child(play_button);
            let identity = div()
                .flex()
                .w(px(64.))
                .h(px(30.))
                .gap(px(4.))
                .when(index % 2 == 1, |row| row.flex_row_reverse())
                .child(headphone_button)
                .child(deck_select)
                .into_any_element();
            (
                controls.into_any_element(),
                Some(identity),
                Some(sync_button.xsmall().p_0().w(px(30.)).into_any_element()),
            )
        } else {
            let identity = div()
                .flex()
                .w_full()
                .h(px(30.))
                .items_center()
                .justify_center()
                .gap(px(4.))
                .when(index == 1, |row| row.flex_row_reverse())
                .child(headphone_button)
                .child(deck_select)
                .into_any_element();
            let controls = div()
                .flex()
                .flex_col()
                .w(px(64.))
                .h_full()
                .flex_shrink_0()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .w_full()
                        .h(px(30.))
                        .gap(px(4.))
                        .child(quantize_button)
                        .children(mode_buttons),
                )
                .child(
                    div()
                        .flex()
                        .w_full()
                        .gap(px(4.))
                        .when(index == 1, |row| row.flex_row_reverse())
                        .child(
                            load_button
                                .rounded(ButtonRounded::None)
                                .w(px(30.))
                                .h(px(30.)),
                        )
                        .child(eject_button),
                )
                .child(sync_button)
                .child(cue_button)
                .child(play_button);
            (controls.into_any_element(), Some(identity), None)
        };
        let display_controls = Some(controls);

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
            .p(px(4.))
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
            .bg(rgb(0x000000))
            .border_1()
            .border_color(theme.border)
            .when(!deck_selected, |d| d.p(px(4.)))
            .when(deck_selected, |d| {
                d.border_color(deck_accent)
                    .shadow(vec![BoxShadow {
                        color: Hsla::from(deck_accent).opacity(0.75),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(3.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .p(px(4.))
            })
            .rounded_lg()
            .child(self.compact_deck_header(index, state, title.clone(), cx))
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .when(index % 2 == 1, |row| row.flex_row_reverse())
                    .items_center()
                    .h(px(overview_height))
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
                            .h(px(overview_height))
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
                                    true,
                                ))
                            })
                            .when(self.stem_busy[index], |overview| {
                                let progress = self.stem_progress[index]
                                    .load(std::sync::atomic::Ordering::Relaxed);
                                // Queue/cache-loading sentinels are not percentages.
                                // Keep a short activity mark until separation reports progress.
                                let fraction = if progress <= 100 {
                                    progress as f32 / 100.
                                } else {
                                    0.
                                };
                                overview.child(
                                    div().absolute().top_0().left_0().w_full().h(px(3.)).child(
                                        canvas(
                                            |_, _, _| (),
                                            move |bounds, _, window, _| {
                                                window.paint_quad(fill(
                                                    Bounds::new(
                                                        bounds.origin,
                                                        size(
                                                            px((bounds.size.width.as_f32()
                                                                * fraction)
                                                                .max(3.)),
                                                            px(3.),
                                                        ),
                                                    ),
                                                    Hsla::from(deck_colour(index)).opacity(0.5),
                                                ));
                                            },
                                        )
                                        .size_full(),
                                    ),
                                )
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
                    .when(self.settings.four_decks, |row| {
                        row.child(self.deck_header_key_controls(index, state, cx))
                    })
                    .child(self.deck_header_times(index, state, cx)),
            )
            .child(self.deck_display(
                index,
                state,
                display_controls,
                display_identity,
                display_sync,
                cx,
            ))
    }

    fn deck_column(&self, side: usize, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        let mut column = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(4.));
        for index in [side, side + 2]
            .into_iter()
            .filter(|&d| d < self.visible_decks())
        {
            if self.settings.four_decks {
                let slot = div().h(px(186.)).flex_shrink_0().flex().flex_col();
                column = column.child(
                    slot.children(decks.get(index).map(|state| self.deck(index, state, cx))),
                );
            } else if let Some(state) = decks.get(index) {
                column = column.child(self.deck(index, state, cx));
            }
        }
        column
    }

    fn content(&self, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        let body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(4.))
            .p(px(4.));
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
                            .gap(px(4.))
                            .child(self.rhythm_panel(decks, cx))
                            .when_some(self.engine_error.clone(), |this, error| {
                                this.child(div().text_color(cx.theme().danger).child(error))
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .h(px(if self.settings.four_decks { 376. } else { 328. }))
                                    .gap(px(4.))
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
                                            .gap(px(4.))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(self.browser.clone()),
                                            )
                                            .child(self.library_side_panel(decks, cx))
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
                                                            .gap(px(4.))
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

impl Drop for Shell {
    fn drop(&mut self) {
        let changed = self.capture_performance_settings();
        if changed || self.performance_dirty.is_some() {
            if let Err(error) = self
                .performance_settings
                .save(&crate::storage::machine_dir())
            {
                eprintln!("Could not save deck setup on close: {error}");
            }
        }
    }
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
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            self.controllers
                                .as_ref()
                                .map(|controller| controller.connection_status())
                                .unwrap_or_else(|| "Controller input disabled".into()),
                        ),
                )
                .into_any_element();
        }
        if let Some(seconds) = self.startup_seek
            && self.engine.as_mut().is_some_and(|engine| {
                let state = engine.state();
                state.decks[0].track.is_some() && !state.decks[0].loading
            })
        {
            self.startup_seek = None;
            self.send(0, DeckAction::Seek { seconds });
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
                self.sampler_state = state.sampler.clone();
                self.monitor = state.monitor;
                self.automix = state.automix.clone();
                if self.automix.enabled && self.automix.smart {
                    self.eq_mode = EqMode::Classic;
                }
                for (deck, channel) in state.decks.iter().enumerate() {
                    if let Some(path) = channel.path.as_ref().map(PathBuf::from) {
                        if self.deck_paths[deck].as_ref() != Some(&path) {
                            self.deck_paths[deck] = Some(path);
                            self.controller_loop_press[deck] = None;
                            self.pad_tracks[deck] = None;
                            auto_paths_changed = true;
                        }
                    }
                }
                self.mixer_values[10] = state.master_gain.sqrt();
                self.mixer_values[46] = state.booth_gain.sqrt();
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
                    self.mixer_values[41 + i] = deck.colour_fx.position;
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
        for (peak, clip_until) in self.mixer_peaks.iter().zip(&mut self.mixer_clip_until) {
            if *peak >= 1. {
                *clip_until = Some(now + std::time::Duration::from_secs(1));
            }
        }
        let elapsed = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        for (index, deck) in decks.iter().enumerate() {
            let transport_playing = deck.playing && self.cue_preview[index].is_none();
            self.sync_playback[index].update(deck.path.as_deref(), transport_playing, now);
            let audible = deck.playing
                && self.cue_preview[index].is_none()
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

        if now.duration_since(self.sync_refresh).as_millis() >= 250 {
            self.sync_refresh = now;
            for deck in 0..DECKS {
                if self.key_sync_enabled[deck] {
                    if let Some(reference) = self.sync_reference(deck)
                        && let (Some(from), Some(to)) =
                            (self.deck_keys[deck], self.deck_keys[reference])
                    {
                        let desired = shortest_key_shift(from, to);
                        if (decks[deck].key_shift - desired).abs() > 0.01 {
                            self.send(deck, DeckAction::SetKeyShift { semitones: desired });
                        }
                    }
                }
                if !self.sync_enabled[deck] {
                    continue;
                }
                let Some(reference) = self.sync_reference(deck) else {
                    continue;
                };
                let (Some(local_grid), Some(master_grid)) =
                    (self.grids[deck], self.grids[reference])
                else {
                    continue;
                };
                let (local, master) = (&decks[deck], &decks[reference]);
                if local.track.is_none() || master.track.is_none() {
                    self.sync_enabled[deck] = false;
                    continue;
                }
                if !local.playing || !master.playing {
                    continue;
                }
                let desired = master_grid.bpm * master.tempo / local_grid.bpm;
                let drift = nearest_beat_offset(
                    local.position_seconds,
                    local_grid,
                    master.position_seconds,
                    master_grid,
                )
                .abs();
                if (local.tempo - desired).abs() > 0.0002 || drift > 0.025 {
                    self.sync_deck(deck, cx);
                }
            }
        }

        for (index, deck) in decks.iter_mut().enumerate() {
            if let Some(until) = self.jump_refresh_until[index] {
                self.display_positions[index] = DisplayPosition::default();
                if now >= until {
                    self.jump_refresh_until[index] = None;
                }
            }
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
            deck.position_seconds = if let Some(drag) = self
                .platter_drag
                .filter(|drag| drag.deck == index && drag.vinyl)
            {
                drag.position
            } else if let Some(drag) = self.drag.filter(|drag| drag.deck == index) {
                drag.position
            } else {
                self.display_positions[index].update(
                    deck.position_seconds,
                    deck.playing && !deck.scratching,
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
            || self.wheel_scrub.is_some()
            || decks.iter().any(moving)
            || self.pending_seeks.iter().any(Option::is_some)
            || self.jump_refresh_until.iter().any(Option::is_some)
            || self.mixer_peaks.iter().any(|peak| *peak > 0.001)
            || self
                .mixer_clip_until
                .iter()
                .flatten()
                .any(|until| *until > now)
            || self
                .mixer_last_change
                .iter()
                .flatten()
                .any(|at| at.elapsed().as_secs_f32() < 2.)
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
            .when_some(
                self.controllers
                    .as_ref()
                    .and_then(|c| c.pending_connection()),
                |view, (name, mapping)| {
                    let accept = name.clone();
                    view.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .p_3()
                            .child(
                                div()
                                    .flex_1()
                                    .child(format!("{name} detected — use {mapping}?")),
                            )
                            .child(
                                Button::new("controller-accept")
                                    .label("Use controller")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(controllers) = &mut this.controllers {
                                            controllers.answer_connection(accept.clone(), true);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("controller-dismiss")
                                    .outline()
                                    .label("Not now")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(controllers) = &mut this.controllers {
                                            controllers.answer_connection(name.clone(), false);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                },
            )
            .track_focus(&self.deck_focus)
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, cx| {
                if this.keyboard_shift != event.shift {
                    this.keyboard_shift = event.shift;
                    cx.notify();
                }
            }))
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
                    this.move_platter_drag(event.position, cx);
                    this.move_pitch(event.position.y.as_f32(), cx);
                    this.move_mixer_control(event.position, cx);
                    this.move_effect_control(event.position.y.as_f32(), cx);
                    if let Some((start_x, start_width)) = this.library_side_drag {
                        this.library_side_width =
                            (start_width + start_x - event.position.x.as_f32()).clamp(220., 640.);
                        cx.notify();
                    }
                } else if event.pressed_button.is_none() {
                    this.end_cue_preview(cx);
                    this.pitch_drag = None;
                    this.pitch_knob_start = None;
                    this.release_pads(cx);
                    this.end_preparation_drag(cx);
                    this.end_waveform_drag(cx);
                    this.end_platter_drag(cx);
                    this.mixer_drag = None;
                    this.effect_drag = None;
                    this.library_side_drag = None;
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
                    this.pitch_knob_start = None;
                    this.release_pads(cx);
                    this.end_preparation_drag(cx);
                    this.end_waveform_drag(cx);
                    this.end_platter_drag(cx);
                    this.mixer_drag = None;
                    this.effect_drag = None;
                    this.library_side_drag = None;
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.end_cue_preview(cx);
                    this.pitch_drag = None;
                    this.pitch_knob_start = None;
                    this.release_pads(cx);
                    this.end_preparation_drag(cx);
                    this.end_waveform_drag(cx);
                    this.end_platter_drag(cx);
                    this.mixer_drag = None;
                    this.effect_drag = None;
                    this.library_side_drag = None;
                }),
            )
            .child(self.top_bar(cx))
            .child(
                div()
                    .h(px(22.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.library.read(cx).analysis_status()),
                    )
                    .child(
                        Button::new("resume-library-analysis")
                            .small()
                            .ghost()
                            .label("Check pending")
                            .tooltip("Check for unanalysed tracks and resume background analysis")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.library
                                    .update(cx, |view, cx| view.refresh_and_analyse(cx));
                            })),
                    ),
            )
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
    fn sync_elects_one_master_when_deck_two_joins_deck_three() {
        use super::{SyncPlayback, select_sync_master};
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
        assert_eq!(select_sync_master(None, 4, &grids, &playback), Some(2));
        assert_eq!(select_sync_master(Some(1), 4, &grids, &playback), Some(1));
        assert_eq!(select_sync_master(Some(2), 2, &grids, &playback), Some(1));
        playback[2].update(Some("c"), true, later);
        assert_eq!(playback[2].started, Some(now));
        playback[2].update(Some("c"), false, later);
        assert_eq!(select_sync_master(None, 4, &grids, &playback), Some(1));
        playback[2].update(Some("c"), true, later + std::time::Duration::from_secs(1));
        assert_eq!(select_sync_master(None, 4, &grids, &playback), Some(1));
        grids[1] = None;
        assert_eq!(select_sync_master(Some(1), 4, &grids, &playback), Some(2));
        playback[2].update(Some("replacement"), false, later);
        assert_eq!(playback[2].started, None);
        assert_eq!(select_sync_master(None, 4, &grids, &playback), Some(0));
        grids.fill(None);
        assert_eq!(select_sync_master(Some(2), 4, &grids, &playback), None);
    }

    use super::{
        DisplayPosition, HistoryTracker, WaveformDrag, mirrors_dual_deck, nearest_beat_offset,
    };
    use djcraft_protocol::DeckAction;

    #[test]
    fn dual_deck_mirrors_tempo_but_keeps_mixer_levels_independent() {
        assert!(mirrors_dual_deck(&DeckAction::SetTempo { ratio: 1.08 }));
        assert!(!mirrors_dual_deck(&DeckAction::SetVolume { level: 0.5 }));
    }
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
    fn platter_drag_follows_circular_record_motion_at_33_and_a_third_rpm() {
        use super::PlatterDrag;
        use gpui_kit::{point, px};
        let centre = point(px(100.), px(100.));
        let top = point(px(100.), px(60.));
        let right = point(px(140.), px(100.));
        let bottom = point(px(100.), px(140.));
        let left = point(px(60.), px(100.));
        let mut drag = PlatterDrag {
            deck: 0,
            last_angle: PlatterDrag::angle(top, centre),
            position: 10.,
            limit: 60.,
            vinyl: true,
            slip: false,
        };
        for cursor in [right, bottom, left, top] {
            assert!((drag.turn(cursor, centre) - 0.45).abs() < 1e-6);
        }
        assert!((drag.position - 11.8).abs() < 1e-6);
        for cursor in [left, bottom, right, top] {
            assert!((drag.turn(cursor, centre) + 0.45).abs() < 1e-6);
        }
        assert!((drag.position - 10.).abs() < 1e-6);
        drag.position = 0.01;
        drag.turn(left, centre);
        assert_eq!(drag.position, 0.);
        assert_eq!(drag.turn(centre, centre), 0.);
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
