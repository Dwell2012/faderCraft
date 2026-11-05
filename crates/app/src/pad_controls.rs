use super::*;
use djcraft_protocol::{EffectKind, EffectState, HOT_CUES, HotCue};
use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::{
    Icon, WindowExt,
    input::{Input, InputState},
};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Page {
    HotCues,
    Loop,
    Roll,
    Slicer,
    BeatJump,
    CueLoop,
    SavedLoops,
    ManualLoop,
    Stems,
    Fx,
    Sampler,
    AutoScratch,
    KeyShift,
    StemFx,
}
impl Page {
    pub(super) const ALL: [Self; 14] = [
        Self::HotCues,
        Self::Loop,
        Self::Roll,
        Self::Slicer,
        Self::BeatJump,
        Self::CueLoop,
        Self::SavedLoops,
        Self::ManualLoop,
        Self::Stems,
        Self::StemFx,
        Self::Fx,
        Self::Sampler,
        Self::AutoScratch,
        Self::KeyShift,
    ];
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::HotCues => "Hot Cues",
            Self::Loop => "Loop",
            Self::Roll => "Loop Roll",
            Self::Slicer => "Slicer",
            Self::BeatJump => "Beat Jump",
            Self::CueLoop => "CueLoop",
            Self::SavedLoops => "Saved Loops",
            Self::ManualLoop => "Manual Loop",
            Self::Stems => "Stems",
            Self::StemFx => "Stem FX",
            Self::Fx => "Pad FX",
            Self::Sampler => "Sampler",
            Self::AutoScratch => "Scratch",
            Self::KeyShift => "Key Shift",
        }
    }
}
struct Held {
    track: Arc<Track>,
    began: Instant,
    page: Page,
    index: usize,
    restore: Restore,
}
enum Restore {
    SlipCue,
    CueLoop {
        hold: bool,
    },
    Loop {
        region: Option<[f64; 2]>,
        roll: bool,
    },
    Cue {
        seconds: f64,
    },
    Fx {
        slot: usize,
        effect: EffectState,
    },
}
pub(super) struct Pads {
    stem_fx_target: usize,
    sample_edit: u8,
    sample_hold: Option<usize>,
    sample_level: f32,
    pub(super) sample_repeat: bool,
    scratch_over: bool,
    pub(super) page: Page,
    pub(super) split: Option<Page>,
    pub(super) favorites: bool,
    bank: [i32; 14],
    size: [i32; 14],
    step: [i32; 14],
    delete_saved: bool,
    pub(super) quantize: bool,
    pub(super) quantize_window: f64,
    pub(super) locked: bool,
    pub(super) stutter: bool,
    loop_mode: usize,
    held: Option<Held>,
    slicer_anchor: Option<f64>,
    notice: String,
}
impl Default for Pads {
    fn default() -> Self {
        let mut bank = [0; 14];
        bank[Page::BeatJump as usize] = 2;
        Self {
            stem_fx_target: 0,
            sample_edit: 0,
            sample_hold: None,
            sample_level: 0.7,
            sample_repeat: false,
            scratch_over: false,
            page: Page::HotCues,
            split: None,
            favorites: false,
            bank,
            size: [0; 14],
            step: [0; 14],
            delete_saved: false,
            quantize: true,
            quantize_window: 0.05,
            locked: false,
            stutter: false,
            loop_mode: 0,
            held: None,
            slicer_anchor: None,
            notice: String::new(),
        }
    }
}
fn pad_beats(page: Page, bank: i32, index: usize, _alternate: bool) -> f64 {
    if page == Page::BeatJump {
        return 2f64.powi((bank + (index / 2) as i32).clamp(-5, 7))
            * if index % 2 == 0 { -1. } else { 1. };
    }
    2f64.powi((bank + index as i32 - 2).clamp(-5, 7))
}

fn cue_loop_length(
    grid: Option<TempoResult>,
    region: Option<[f64; 2]>,
    loop_size: i32,
) -> Option<f64> {
    grid.map(|grid| region.map_or(grid.beat_seconds() * 2f64.powi(loop_size), |[a, b]| b - a))
}

// Absolute offsets from the unshifted track: centre pad restores its key.
fn key_shift_pad(mode: i32, index: usize) -> f32 {
    let degree = [-3i32, -2, -1, 0, 1, 2, 3, 7][index % 8];
    match mode {
        1 => (degree * 2).clamp(-12, 12) as f32,
        2 | 3 => {
            let scale = if mode == 2 {
                [0, 2, 4, 5, 7, 9, 11]
            } else {
                [0, 2, 3, 5, 7, 8, 10]
            };
            (degree.div_euclid(7) * 12 + scale[degree.rem_euclid(7) as usize]) as f32
        }
        _ => degree as f32,
    }
}

fn key_shift_step(mode: i32, current: f32, direction: i32) -> f32 {
    if direction == 0 {
        return current;
    }
    let scale = if mode == 2 {
        [0, 2, 4, 5, 7, 9, 11]
    } else {
        [0, 2, 3, 5, 7, 8, 10]
    };
    let mut next = current.round() as i32;
    loop {
        next += direction.signum();
        if !(-12..=12).contains(&next) {
            return current;
        }
        if mode == 0
            || (mode == 1 && next % 2 == 0)
            || (mode >= 2 && scale.contains(&next.rem_euclid(12)))
        {
            return next as f32;
        }
    }
}
fn snapped(seconds: f64, grid: Option<TempoResult>, enabled: bool) -> f64 {
    if enabled {
        if let Some(grid) = grid {
            return (grid.first_beat_seconds
                + ((seconds - grid.first_beat_seconds) / grid.beat_seconds()).round()
                    * grid.beat_seconds())
            .max(0.);
        }
    }
    seconds
}

fn hot_cue_position(
    seconds: f64,
    grid: Option<TempoResult>,
    quantize: bool,
    quantize_window: f64,
) -> f64 {
    let Some(grid) = grid.filter(|_| quantize) else {
        return seconds;
    };
    let target = snapped(seconds, Some(grid), true);
    let beat = grid.beat_seconds();
    // Include the boundary despite floating-point rounding of grid positions.
    if (target - seconds).abs() <= beat * quantize_window + beat * 1e-10 {
        target
    } else {
        seconds
    }
}

fn sample_capture_region(
    position: f64,
    selected_loop: Option<[f64; 2]>,
    grid: Option<TempoResult>,
    beats: f64,
    loaded_seconds: f64,
) -> Option<[f64; 2]> {
    selected_loop.or_else(|| {
        grid.map(|g| {
            let start = position.clamp(0., loaded_seconds);
            [
                start,
                (start + beats * g.beat_seconds()).min(loaded_seconds),
            ]
        })
    })
}
const PAD_FX: [EffectKind; 16] = [
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
impl Shell {
    pub(super) fn follow_controller_pad_mode(
        &mut self,
        deck: usize,
        mode: djcraft_controllers::PadMode,
        cx: &mut Context<Self>,
    ) {
        let page = match mode {
            djcraft_controllers::PadMode::HotCues => Page::HotCues,
            djcraft_controllers::PadMode::Loop => Page::ManualLoop,
            djcraft_controllers::PadMode::Sampler => Page::Sampler,
            djcraft_controllers::PadMode::Stems => Page::Stems,
            djcraft_controllers::PadMode::BeatJump => Page::BeatJump,
        };
        if self.pads[deck].page != page || self.pads[deck].split.is_some() {
            self.release_pad(deck);
            self.pads[deck].page = page;
            self.pads[deck].split = None;
            self.pads[deck].notice.clear();
            cx.notify();
        }
    }
    fn pad_state(&mut self, deck: usize) -> Option<DeckState> {
        self.engine.as_mut().map(|e| e.state().decks[deck].clone())
    }
    fn pad_send(&mut self, deck: usize, action: DeckAction) -> bool {
        use djcraft_protocol::script::{ButtonAction, ControlSource, Program};
        let action = match action {
            DeckAction::TriggerHotCue {
                slot,
                play: true,
                loop_length: None,
            } => {
                let Some(state) = self.engine.as_mut().map(|engine| engine.state()) else {
                    return false;
                };
                match Program::do_action(deck, ButtonAction::HotCue(slot)).and_then(|program| {
                    program.run_with_state(ControlSource::Skin, 0., Some(&state))
                }) {
                    Ok(dispatch) => dispatch.action,
                    Err(_) => return false,
                }
            }
            DeckAction::SlipReturn => {
                let Ok(program) = Program::do_action(deck, ButtonAction::ReleaseHotCue) else {
                    return false;
                };
                let Ok(dispatch) = program.run(ControlSource::Skin, 0.) else {
                    return false;
                };
                dispatch.action
            }
            action => Action::Deck {
                deck: deck as u8,
                action,
            },
        };
        let Action::Deck { action, .. } = action else {
            unreachable!()
        };
        let cue_anchor = self.transport_anchor_for(deck, &action);
        let metadata = matches!(
            action,
            DeckAction::SetCueAppearance { .. } | DeckAction::SetHotCues { .. }
        );
        let result = self.engine.as_mut().map(|engine| {
            engine.apply(
                Role::Host,
                Action::Deck {
                    deck: deck as u8,
                    action,
                },
            )
        });
        let okay = matches!(result, Some(Ok(())));
        if !okay {
            self.pads[deck].notice = "Action unavailable: check loaded audio and beat grid.".into();
        }
        if !metadata {
            self.display_positions[deck] = DisplayPosition::default();
        }
        if okay {
            self.cue_anchor[deck] = cue_anchor;
        }
        okay
    }
    pub(super) fn refresh_pad_tracks(&mut self) {
        if let Some(engine) = self.engine.as_mut() {
            engine.state();
        }
        for deck in 0..DECKS {
            let track = self.engine.as_ref().and_then(|e| e.track(deck)).cloned();
            let changed = match (&track, &self.pad_tracks[deck]) {
                (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                (None, None) => false,
                _ => true,
            };
            if !changed {
                continue;
            }
            self.pads[deck].held = None;
            self.pads[deck].slicer_anchor = None;
            self.pads[deck].notice.clear();
            self.pad_tracks[deck] = track;
            let cues = self.deck_paths[deck]
                .as_ref()
                .and_then(|path| self.pad_cues.get(&crate::track_identity::key(path)))
                .copied()
                .unwrap_or([None; HOT_CUES]);
            self.pad_send(deck, DeckAction::SetHotCues { cues });
            let styles = self.deck_paths[deck]
                .as_ref()
                .and_then(|p| self.cue_appearances.get(&crate::track_identity::key(p)))
                .cloned()
                .unwrap_or_default();
            for (slot, appearance) in styles.into_iter().enumerate() {
                self.pad_send(
                    deck,
                    DeckAction::SetCueAppearance {
                        slot: slot as u8,
                        appearance,
                    },
                );
            }
        }
    }
    pub(super) fn release_pad(&mut self, deck: usize) {
        if let Some(slot) = self.pads[deck].sample_hold.take() {
            self.pad_send(
                deck,
                DeckAction::Sample {
                    slot: slot as u8,
                    play: false,
                    restart: false,
                    repeat: false,
                    level: self.pads[deck].sample_level,
                },
            );
        }
        let Some(held) = self.pads[deck].held.take() else {
            return;
        };
        if !self
            .engine
            .as_ref()
            .and_then(|e| e.track(deck))
            .is_some_and(|track| Arc::ptr_eq(track, &held.track))
        {
            return;
        }
        match held.restore {
            Restore::SlipCue => {
                self.pad_send(deck, DeckAction::SlipReturn);
            }
            Restore::CueLoop { hold } => {
                if hold || held.began.elapsed().as_millis() < 250 {
                    self.pad_send(deck, DeckAction::ClearLoop);
                }
            }
            Restore::Loop { region, roll } => {
                self.pad_send(deck, DeckAction::ClearLoop);
                self.pad_send(deck, DeckAction::SetLoopRoll { enabled: roll });
                if let Some([start, end]) = region {
                    self.pad_send(deck, DeckAction::SetLoop { start, end });
                }
            }
            Restore::Cue { seconds } => {
                self.pad_send(deck, DeckAction::Pause);
                self.pad_send(deck, DeckAction::Seek { seconds });
            }
            Restore::Fx { slot, effect } => {
                self.pad_send(
                    deck,
                    DeckAction::SetEffect {
                        slot: slot as u8,
                        effect,
                    },
                );
            }
        }
    }
    pub(super) fn release_pads(&mut self, cx: &mut Context<Self>) {
        self.release_side_sample(cx);
        if self
            .pads
            .iter()
            .any(|p| p.held.is_some() || p.sample_hold.is_some())
        {
            for deck in 0..DECKS {
                self.release_pad(deck);
            }
            cx.notify();
        }
    }
    fn hold_pad(&mut self, deck: usize, page: Page, index: usize, restore: Restore) {
        if let Some(track) = self.engine.as_ref().and_then(|e| e.track(deck)).cloned() {
            self.pads[deck].held = Some(Held {
                track,
                began: Instant::now(),
                page,
                index,
                restore,
            });
        }
    }
    pub(super) fn accept_suggested_cues(
        &mut self,
        deck: usize,
        suggestions: &[crate::preparation::SuggestedCue],
        cx: &mut Context<Self>,
    ) {
        if self.pads[deck].locked {
            self.pads[deck].notice = "Unlock pads before accepting cue suggestions.".into();
            cx.notify();
            return;
        }
        let Some(state) = self.pad_state(deck).filter(|s| !s.loading) else {
            return;
        };
        let cues = crate::preparation::fill_cues(state.hot_cues, suggestions);
        self.save_pad_cues(deck, cues);
        for (slot, cue) in cues.iter().enumerate() {
            if state.hot_cues[slot].is_none() {
                if let Some(suggestion) = cue.and_then(|c| {
                    suggestions
                        .iter()
                        .find(|s| (s.seconds - c.seconds).abs() < 0.001)
                }) {
                    self.save_cue_appearance(
                        deck,
                        slot,
                        djcraft_protocol::CueAppearance {
                            label: suggestion.label.into(),
                            colour: None,
                        },
                    );
                }
            }
        }
        cx.notify();
    }
    fn save_pad_cues(&mut self, deck: usize, cues: [Option<HotCue>; HOT_CUES]) {
        if !self.pad_send(deck, DeckAction::SetHotCues { cues }) {
            return;
        }
        for (slot, cue) in cues.iter().enumerate() {
            if cue.is_none() {
                self.save_cue_appearance(deck, slot, Default::default());
            }
        }
        if let Some(path) = &self.deck_paths[deck] {
            self.pad_cues.insert(crate::track_identity::key(path), cues);
            if let Err(error) = crate::pad_store::save(&data_dir(), &self.pad_cues) {
                self.pads[deck].notice = format!("Could not save cues: {error}");
            }
        }
    }

    pub(super) fn controller_store_cues(
        &mut self,
        deck: usize,
        mut cues: [Option<HotCue>; HOT_CUES],
    ) {
        if self.pads[deck].locked {
            self.pads[deck].notice = "Unlock cue editing in Options first.".into();
            return;
        }
        if let Some(state) = self.pad_state(deck) {
            for (slot, cue) in cues.iter_mut().enumerate() {
                if state.hot_cues[slot].is_none()
                    && let Some(cue) = cue
                {
                    // Match the UI's near-beat snap, but retain saved loop bounds.
                    if cue.loop_end.is_none() {
                        let snapped = hot_cue_position(
                            cue.seconds,
                            self.grids[deck],
                            self.pads[deck].quantize && !state.quantize_disabled,
                            self.pads[deck].quantize_window,
                        );
                        if state
                            .track
                            .as_ref()
                            .is_some_and(|t| snapped < t.loaded_seconds)
                        {
                            cue.seconds = snapped;
                        }
                    }
                }
            }
        }
        self.save_pad_cues(deck, cues);
    }
    fn save_cue_appearance(
        &mut self,
        deck: usize,
        slot: usize,
        appearance: djcraft_protocol::CueAppearance,
    ) {
        if !self.pad_send(
            deck,
            DeckAction::SetCueAppearance {
                slot: slot as u8,
                appearance: appearance.clone(),
            },
        ) {
            return;
        }
        if let Some(path) = &self.deck_paths[deck] {
            self.cue_appearances
                .entry(crate::track_identity::key(path))
                .or_default()[slot] = appearance;
            if let Err(e) = crate::pad_store::save_appearances(&data_dir(), &self.cue_appearances) {
                self.pads[deck].notice = format!("Could not save cue name/colour: {e}");
            }
        }
    }
    fn edit_cue(&mut self, deck: usize, slot: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.pads[deck].locked {
            self.pads[deck].notice = "Unlock cue editing in Options first.".into();
            cx.notify();
            return;
        }
        let Some(state) = self.pad_state(deck) else {
            return;
        };
        if state.hot_cues[slot].is_none() {
            self.pads[deck].notice = "Set this cue before naming it.".into();
            cx.notify();
            return;
        }
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Cue name (up to 48 characters)"));
        input.update(cx, |i, cx| {
            i.set_value(state.cue_appearance[slot].label.clone(), window, cx);
            i.focus(window, cx);
        });
        let colour = Rc::new(Cell::new(state.cue_appearance[slot].colour));
        let original_path = state.path;
        let original_cue = state.hot_cues[slot];
        let view = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let save_input = input.clone();
            let colour = colour.clone();
            let save_colour = colour.clone();
            let view = view.clone();
            let original_path = original_path.clone();
            let save = Rc::new(move |cx: &mut App| {
                let label: String = save_input
                    .read(cx)
                    .value()
                    .trim()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(48)
                    .collect();
                let _ = view.update(cx, |this, cx| {
                    if !this.pads[deck].locked
                        && this.pad_state(deck).is_some_and(|s| {
                            s.path == original_path && s.hot_cues[slot] == original_cue
                        })
                    {
                        this.save_cue_appearance(
                            deck,
                            slot,
                            djcraft_protocol::CueAppearance {
                                label,
                                colour: save_colour.get(),
                            },
                        );
                    }
                    cx.notify();
                });
            });
            let confirm_save = save.clone();
            dialog
                .title(format!("Hot cue {}", slot + 1))
                .w(px(420.))
                .on_ok(move |_, _, cx| {
                    confirm_save(cx);
                    true
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(Input::new(&input))
                        .child(
                            Button::new("cue-colour")
                                .outline()
                                .label("Colour")
                                .dropdown_caret(true)
                                .dropdown_menu(move |mut menu, _, _| {
                                    let reset = colour.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new("Default for this slot")
                                            .checked(colour.get().is_none())
                                            .on_click(move |_, _, _| reset.set(None)),
                                    );
                                    for i in 0..16 {
                                        let c = crate::pad_store::cue_colour(
                                            deck,
                                            i,
                                            &Default::default(),
                                        );
                                        let selected = colour.clone();
                                        menu = menu.item(
                                            PopupMenuItem::element(move |_, _| {
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .w(px(16.))
                                                            .h(px(16.))
                                                            .rounded_sm()
                                                            .bg(rgb(c)),
                                                    )
                                                    .child(format!("Colour {}", i + 1))
                                            })
                                            .checked(colour.get() == Some(c))
                                            .on_click(move |_, _, _| selected.set(Some(c))),
                                        );
                                    }
                                    menu
                                }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .child("Right-click to edit · Shift-click a Hot Cue pad to delete"),
                        ),
                )
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("cue-edit-cancel")
                                .outline()
                                .label("Cancel")
                                .on_click(|_, w, cx| w.close_dialog(cx)),
                        )
                        .child(
                            Button::new("cue-edit-save")
                                .primary()
                                .label("Save")
                                .on_click(move |_, w, cx| {
                                    save(cx);
                                    w.close_dialog(cx);
                                }),
                        ),
                )
        });
    }
    pub(super) fn controller_sample_pad(&mut self, deck: usize, slot: u8, stop: bool) {
        if slot < 16 {
            self.pad_send(
                deck,
                DeckAction::Sample {
                    slot,
                    play: !stop,
                    restart: !stop,
                    repeat: !stop && self.pads[deck].sample_repeat,
                    level: self.pads[deck].sample_level,
                },
            );
        }
    }

    fn press_pad(
        &mut self,
        deck: usize,
        page: Page,
        index: usize,
        alternate: bool,
        cx: &mut Context<Self>,
    ) {
        self.release_pad(deck);
        self.pads[deck].notice.clear();
        let Some(state) = self.pad_state(deck) else {
            return;
        };
        if page == Page::Sampler && index < 16 {
            let edit = std::mem::take(&mut self.pads[deck].sample_edit);
            if edit > 0 && self.pads[deck].locked {
                self.pads[deck].notice = "Unlock sample editing in Options first.".into();
                cx.notify();
                return;
            }
            if edit == 1 || (!alternate && state.samples[index] == 0 && state.track.is_none()) {
                self.choose_sample(deck, index, cx);
                return;
            }
            if edit == 2 {
                self.pad_send(deck, DeckAction::DefaultSample { slot: index as u8 });
                cx.notify();
                return;
            }
            if alternate {
                if state.samples[index] == 2 {
                    self.pad_send(
                        deck,
                        DeckAction::Sample {
                            slot: index as u8,
                            play: false,
                            restart: false,
                            repeat: false,
                            level: self.pads[deck].sample_level,
                        },
                    );
                } else if !self.pads[deck].locked {
                    self.pad_send(deck, DeckAction::ClearSample { slot: index as u8 });
                }
            } else if state.samples[index] == 0 || edit == 3 {
                if !self.pads[deck].locked {
                    let beats = 4. * 2f64.powi(self.pads[deck].size[page as usize]);
                    let region = sample_capture_region(
                        state.position_seconds,
                        state.loop_region,
                        self.grids[deck],
                        beats,
                        state.track.as_ref().map_or(0., |t| t.loaded_seconds),
                    );
                    if let Some([start, end]) = region {
                        if self.pad_send(
                            deck,
                            DeckAction::CaptureSample {
                                slot: index as u8,
                                start,
                                end,
                            },
                        ) {
                            self.pads[deck].notice = format!(
                                "Captured slot {} · saving to bank · master output",
                                index + 1
                            );
                        }
                    } else {
                        self.pads[deck].notice =
                            "Set a loop or analyse tempo to capture a sample.".into();
                    }
                }
            } else {
                let mode = self.pads[deck].bank[page as usize];
                self.pad_send(
                    deck,
                    DeckAction::Sample {
                        slot: index as u8,
                        play: mode != 0
                            || !self.pads[deck].sample_repeat
                            || state.samples[index] != 2,
                        restart: mode == 2
                            || !self.pads[deck].sample_repeat
                            || state.samples[index] != 2,
                        repeat: self.pads[deck].sample_repeat,
                        level: self.pads[deck].sample_level,
                    },
                );
                if mode == 1 {
                    self.pads[deck].sample_hold = Some(index);
                }
            }
            cx.notify();
            return;
        }
        let Some(track) = &state.track else {
            return;
        };
        if index >= HOT_CUES {
            return;
        }
        let grid = self.grids[deck];
        let step = grid.map(|g| g.beat_seconds());
        let quantize = self.pads[deck].quantize && !state.quantize_disabled;
        let start = snapped(state.position_seconds, grid, quantize).min(track.loaded_seconds);
        let loop_hold = self.pads[deck].loop_mode;
        match page {
            Page::Sampler => {}
            Page::KeyShift => {
                let semitones = if alternate {
                    0.
                } else {
                    key_shift_pad(self.pads[deck].bank[page as usize], index)
                };
                self.pad_send(deck, DeckAction::SetKeyShift { semitones });
            }
            Page::AutoScratch => {
                if index < 8 {
                    self.pad_send(
                        deck,
                        DeckAction::AutoScratch {
                            pattern: if alternate { None } else { Some(index as u8) },
                            beat_seconds: step.unwrap_or(0.5),
                            first_beat: grid.map_or(0., |g| g.first_beat_seconds),
                            quantize,
                            over: self.pads[deck].scratch_over,
                        },
                    );
                }
            }
            Page::HotCues | Page::CueLoop => {
                let mut cues = state.hot_cues;
                if alternate && page == Page::HotCues {
                    if !self.pads[deck].locked {
                        cues[index] = None;
                        self.save_pad_cues(deck, cues);
                    }
                } else {
                    let existed = cues[index].is_some();
                    if !existed {
                        if self.pads[deck].locked {
                            return;
                        }
                        let start = if page == Page::HotCues {
                            hot_cue_position(
                                state.position_seconds,
                                grid,
                                quantize,
                                self.pads[deck].quantize_window,
                            )
                            .min(track.loaded_seconds)
                        } else {
                            start
                        };
                        cues[index] = Some(HotCue {
                            seconds: state.loop_region.map_or(start, |r| r[0]),
                            loop_end: state.loop_region.map(|r| r[1]),
                        });
                        self.save_pad_cues(deck, cues);
                    }
                    if existed || page == Page::CueLoop {
                        let loop_length = if page == Page::CueLoop {
                            cue_loop_length(grid, state.loop_region, self.loop_size[deck])
                        } else {
                            None
                        };
                        if page == Page::CueLoop && loop_length.is_none() {
                            self.pads[deck].notice = "CueLoop needs an analysed beat grid.".into();
                        } else if self.pad_send(
                            deck,
                            DeckAction::TriggerHotCue {
                                slot: index as u8,
                                play: state.playing || existed,
                                loop_length,
                            },
                        ) {
                            let cue = cues[index].unwrap();
                            if page == Page::CueLoop {
                                self.loop_in[deck] = Some(cue.seconds);
                                if !alternate || self.pads[deck].stutter {
                                    self.hold_pad(
                                        deck,
                                        page,
                                        index,
                                        Restore::CueLoop {
                                            hold: self.pads[deck].stutter,
                                        },
                                    );
                                }
                            } else if state.slip_enabled && existed {
                                self.hold_pad(deck, page, index, Restore::SlipCue);
                            } else if self.pads[deck].stutter {
                                self.hold_pad(
                                    deck,
                                    page,
                                    index,
                                    Restore::Cue {
                                        seconds: cue.seconds,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            Page::Loop | Page::Roll | Page::Slicer => {
                let Some(beat) = step else {
                    self.pads[deck].notice = "This page needs an analysed beat grid.".into();
                    cx.notify();
                    return;
                };
                let (a, b) = if page == Page::Slicer {
                    let slice_step = beat * 2f64.powi(self.pads[deck].step[page as usize]);
                    let origin = grid.unwrap().first_beat_seconds;
                    let block = origin
                        + ((state
                            .roll_position_seconds
                            .unwrap_or(state.position_seconds)
                            - origin)
                            / (slice_step * 8.))
                            .floor()
                            * slice_step
                            * 8.;
                    if alternate {
                        self.pads[deck].slicer_anchor = Some(block.max(0.));
                    }
                    let a = self.pads[deck].slicer_anchor.unwrap_or(block).max(0.)
                        + index as f64 * slice_step;
                    (a, a + beat * 2f64.powi(self.pads[deck].size[page as usize]))
                } else {
                    let length =
                        pad_beats(page, self.pads[deck].bank[page as usize], index, alternate)
                            * beat;
                    (start, start + length)
                };
                let roll = page != Page::Loop || loop_hold == 2;
                let held = page != Page::Loop || loop_hold != 0;
                if page == Page::Loop
                    && !held
                    && state
                        .loop_region
                        .is_some_and(|r| ((r[1] - r[0]) - (b - a)).abs() < 0.001)
                {
                    self.pad_send(deck, DeckAction::ClearLoop);
                } else if self.pad_send(
                    deck,
                    DeckAction::StartPadLoop {
                        start: a,
                        end: b,
                        roll,
                    },
                ) {
                    self.loop_last[deck] = Some([a, b]);
                    if held {
                        self.hold_pad(
                            deck,
                            page,
                            index,
                            Restore::Loop {
                                region: state.loop_region,
                                roll: state.loop_roll,
                            },
                        );
                    }
                }
            }
            Page::BeatJump => {
                if let Some(beat) = step {
                    self.pad_send(
                        deck,
                        DeckAction::Jump {
                            seconds: beat
                                * pad_beats(
                                    page,
                                    self.pads[deck].bank[page as usize],
                                    index,
                                    alternate,
                                ),
                        },
                    );
                }
            }
            Page::SavedLoops => {
                let path = self.deck_paths[deck]
                    .as_ref()
                    .map(crate::track_identity::key);
                if let Some(path) = path {
                    let existing = self.saved_loops.get(&path).and_then(|slots| slots[index]);
                    if self.pads[deck].delete_saved {
                        self.pads[deck].delete_saved = false;
                        if !self.pads[deck].locked {
                            self.saved_loops.entry(path).or_insert([None; 16])[index] = None;
                            if let Err(e) = crate::loop_store::save(&data_dir(), &self.saved_loops)
                            {
                                self.pads[deck].notice = e;
                            }
                        }
                        cx.notify();
                        return;
                    }
                    if let Some([a, b]) = existing {
                        self.pad_send(
                            deck,
                            if alternate {
                                DeckAction::StartPadLoop {
                                    start: a,
                                    end: b,
                                    roll: false,
                                }
                            } else {
                                DeckAction::ArmLoop { start: a, end: b }
                            },
                        );
                    } else if !self.pads[deck].locked {
                        let region = state.loop_region.or_else(|| {
                            step.map(|beat| {
                                [
                                    start,
                                    start + beat * 2f64.powi(self.pads[deck].size[page as usize]),
                                ]
                            })
                        });
                        if let Some([a, b]) = region {
                            if self.pad_send(
                                deck,
                                DeckAction::StartPadLoop {
                                    start: a,
                                    end: b,
                                    roll: false,
                                },
                            ) {
                                self.saved_loops.entry(path).or_insert([None; 16])[index] =
                                    Some([a, b]);
                                if let Err(e) =
                                    crate::loop_store::save(&data_dir(), &self.saved_loops)
                                {
                                    self.pads[deck].notice = e;
                                }
                            }
                        }
                    }
                }
            }
            Page::ManualLoop => match index {
                0 => {
                    self.loop_in[deck] = Some(start);
                    self.pad_send(deck, DeckAction::ClearLoop);
                }
                1 => {
                    if let Some(a) = self.loop_in[deck] {
                        self.apply_loop(deck, a, start, cx);
                    }
                }
                2 => {
                    self.pad_send(deck, DeckAction::ClearLoop);
                }
                3 => {
                    if let Some([a, b]) = self.loop_last[deck] {
                        self.apply_loop(deck, a, b, cx);
                    }
                }
                4 | 5 => self.resize_loop(deck, if index == 4 { -1 } else { 1 }, cx),
                6 | 7 => {
                    if let (Some([a, b]), Some(beat)) = (state.loop_region, step) {
                        let offset = beat * if index == 6 { -1. } else { 1. };
                        self.apply_loop(deck, a + offset, b + offset, cx);
                    }
                }
                _ => {}
            },
            Page::Stems => {
                if !state.stems_ready {
                    self.pads[deck].notice = "Prepare this track's stems first.".into();
                } else {
                    let mut gains = state.stem_gains;
                    match index {
                        0..=3 => {
                            if alternate {
                                gains = [0.; 4];
                                gains[index] = 1.;
                            } else {
                                gains[index] = if gains[index] > 0. { 0. } else { 1. };
                            }
                        }
                        4 => gains = [1., 0., 0., 0.],
                        5 => gains = [0., 1., 1., 1.],
                        6 => gains = [1.; 4],
                        7 => gains = [0.; 4],
                        _ => {}
                    }
                    self.pad_send(deck, DeckAction::SetStemGains { gains });
                }
            }
            Page::StemFx => {
                if index < 4 {
                    self.pads[deck].stem_fx_target = index;
                } else if !state.stems_ready {
                    self.pads[deck].notice = "Prepare this track's stems first.".into();
                } else {
                    let stem = self.pads[deck].stem_fx_target;
                    let kind = PAD_FX[self.pads[deck].bank[page as usize].rem_euclid(4) as usize
                        * 4
                        + index
                        - 4];
                    let old = state.stem_effects[stem];
                    let effect = if old.kind == kind {
                        EffectState {
                            enabled: !old.enabled,
                            ..old
                        }
                    } else {
                        EffectState {
                            kind,
                            enabled: true,
                            amount: old.amount,
                            sync: grid.is_some(),
                            ..EffectState::default()
                        }
                    };
                    self.pad_send(
                        deck,
                        DeckAction::SetStemEffect {
                            stem: stem as u8,
                            effect,
                        },
                    );
                    self.pads[deck].notice.clear();
                }
            }
            Page::Fx => {
                if let Some(slot) = state.effects.iter().position(|fx| !fx.enabled) {
                    let old = state.effects[slot];
                    let effect = EffectState {
                        kind: PAD_FX[index],
                        enabled: true,
                        sync: grid.is_some(),
                        beats: 0.5,
                        amount: if matches!(
                            PAD_FX[index],
                            EffectKind::Brake | EffectKind::BeatRepeat
                        ) {
                            1.
                        } else {
                            0.5
                        },
                        ..EffectState::default()
                    };
                    if self.pad_send(
                        deck,
                        DeckAction::SetEffect {
                            slot: slot as u8,
                            effect,
                        },
                    ) {
                        self.hold_pad(deck, page, index, Restore::Fx { slot, effect: old });
                    }
                } else {
                    self.pads[deck].notice =
                        "All six FX slots are active; free one to use Pad FX.".into();
                }
            }
        }
        cx.notify();
    }
    fn import_sample(&mut self, deck: usize, slot: usize, path: String, cx: &mut Context<Self>) {
        if self.pads[deck].locked {
            self.pads[deck].notice = "Unlock sample editing in Options first.".into();
        } else {
            self.release_pad(deck);
            if self.pad_send(
                deck,
                DeckAction::LoadSample {
                    slot: slot as u8,
                    path,
                },
            ) {
                self.pads[deck].notice = format!("Loading slot {}…", slot + 1);
            }
        }
        cx.notify();
    }
    fn choose_sample(&mut self, deck: usize, slot: usize, cx: &mut Context<Self>) {
        if self.pads[deck].locked {
            self.pads[deck].notice = "Unlock sample editing in Options first.".into();
            cx.notify();
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Load sampler audio".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            if let Some(path) = paths.first() {
                let path = path.to_string_lossy().into_owned();
                let _ = this.update(cx, |this, cx| this.import_sample(deck, slot, path, cx));
            }
        })
        .detach();
    }
    fn sampler_volume(&mut self, deck: usize, change: f32) {
        self.pads[deck].sample_level = (self.pads[deck].sample_level + change).clamp(0., 1.);
        let level = self.pads[deck].sample_level;
        if let Some(state) = self.pad_state(deck) {
            for (slot, value) in state.samples.into_iter().enumerate() {
                if value == 2 {
                    self.pad_send(
                        deck,
                        DeckAction::Sample {
                            slot: slot as u8,
                            play: true,
                            restart: false,
                            repeat: self.pads[deck].sample_repeat,
                            level,
                        },
                    );
                }
            }
        }
        self.pads[deck].notice = format!("Sampler {:.0}% · master output", level * 100.);
    }
    fn pad_parameter(
        &mut self,
        deck: usize,
        page: Page,
        parameter: usize,
        direction: i32,
        cx: &mut Context<Self>,
    ) {
        match page {
            Page::KeyShift => {
                if parameter == 0 {
                    self.pads[deck].bank[page as usize] =
                        (self.pads[deck].bank[page as usize] + direction).rem_euclid(4);
                } else {
                    if let Some(state) = self.pad_state(deck) {
                        let semitones = key_shift_step(
                            self.pads[deck].bank[page as usize],
                            state.key_shift,
                            direction,
                        );
                        self.pad_send(deck, DeckAction::SetKeyShift { semitones });
                    }
                }
            }
            Page::Sampler => {
                if parameter == 0 {
                    self.pads[deck].bank[page as usize] =
                        (self.pads[deck].bank[page as usize] + direction).rem_euclid(3);
                } else {
                    self.pads[deck].size[page as usize] =
                        (self.pads[deck].size[page as usize] + direction).clamp(0, 3);
                }
            }
            Page::AutoScratch => {
                if parameter == 0 {
                    self.pads[deck].scratch_over = !self.pads[deck].scratch_over;
                } else {
                    self.pads[deck].quantize = !self.pads[deck].quantize;
                }
            }
            Page::HotCues if parameter == 0 => {
                if let Some(state) = self.pad_state(deck) {
                    let mut cues: Vec<_> = state
                        .hot_cues
                        .iter()
                        .enumerate()
                        .filter_map(|(i, c)| c.map(|c| (i, c.seconds)))
                        .collect();
                    cues.sort_by(|a, b| a.1.total_cmp(&b.1));
                    let next = if direction > 0 {
                        cues.iter()
                            .find(|(_, s)| *s > state.position_seconds + 0.02)
                            .or(cues.first())
                    } else {
                        cues.iter()
                            .rev()
                            .find(|(_, s)| *s < state.position_seconds - 0.02)
                            .or(cues.last())
                    };
                    if let Some((i, _)) = next {
                        self.pad_send(
                            deck,
                            DeckAction::TriggerHotCue {
                                slot: *i as u8,
                                play: state.playing,
                                loop_length: None,
                            },
                        );
                    }
                }
            }
            Page::HotCues => {
                if let Some(g) = self.grids[deck] {
                    self.pad_send(
                        deck,
                        DeckAction::Jump {
                            seconds: g.beat_seconds() * direction as f64,
                        },
                    );
                }
            }
            Page::ManualLoop if parameter == 0 => self.resize_loop(deck, direction, cx),
            Page::SavedLoops => {
                if parameter == 0 {
                    self.pads[deck].size[page as usize] =
                        (self.pads[deck].size[page as usize] + direction).clamp(-5, 7);
                    self.resize_loop(deck, direction, cx);
                } else if let (Some(state), Some(grid)) = (self.pad_state(deck), self.grids[deck]) {
                    if let Some([a, b]) = state.loop_region {
                        let shift = grid.beat_seconds() * direction as f64;
                        self.apply_loop(deck, a + shift, b + shift, cx);
                    }
                }
            }
            Page::Loop if parameter == 1 => {
                self.pads[deck].loop_mode =
                    (self.pads[deck].loop_mode as i32 + direction).rem_euclid(3) as usize
            }
            Page::Slicer if parameter == 1 => {
                self.pads[deck].step[page as usize] =
                    (self.pads[deck].step[page as usize] + direction).clamp(-2, 3)
            }
            Page::CueLoop => {
                if parameter == 1 {
                    self.pads[deck].stutter = !self.pads[deck].stutter;
                }
            }
            Page::StemFx => {
                if parameter == 0 {
                    self.pads[deck].bank[page as usize] =
                        (self.pads[deck].bank[page as usize] + direction).rem_euclid(4);
                } else if let Some(engine) = &mut self.engine {
                    let stem = self.pads[deck].stem_fx_target;
                    let mut effect = engine.state().decks[deck].stem_effects[stem];
                    effect.amount = (effect.amount + direction as f32 * 0.05).clamp(0., 1.);
                    self.pad_send(
                        deck,
                        DeckAction::SetStemEffect {
                            stem: stem as u8,
                            effect,
                        },
                    );
                }
            }
            Page::Stems | Page::Fx => {}
            Page::Loop | Page::Roll | Page::BeatJump => {
                self.pads[deck].bank[page as usize] =
                    (self.pads[deck].bank[page as usize] + direction).clamp(-3, 4)
            }
            _ => {
                self.pads[deck].size[page as usize] =
                    (self.pads[deck].size[page as usize] + direction).clamp(-5, 7)
            }
        }
        cx.notify();
    }
    fn pad_text(
        &self,
        deck: usize,
        page: Page,
        index: usize,
        state: &DeckState,
    ) -> (String, bool, bool) {
        let loaded = state.track.is_some();
        let grid = self.grids[deck].is_some();
        let held = self.pads[deck]
            .held
            .as_ref()
            .is_some_and(|h| h.page == page && h.index == index);
        match page {
            Page::KeyShift => {
                let shift = key_shift_pad(self.pads[deck].bank[page as usize], index);
                (
                    if shift == 0. {
                        "Reset".into()
                    } else {
                        format!("{shift:+.0} st")
                    },
                    (state.key_shift - shift).abs() < 0.01,
                    loaded,
                )
            }
            Page::Sampler => (
                if state.sample_loading[index] {
                    "Loading…".to_owned()
                } else if state.sample_errors[index].is_some() {
                    "Load failed".to_owned()
                } else if !state.sample_names[index].is_empty() {
                    state.sample_names[index].clone()
                } else {
                    "Empty".to_owned()
                },
                state.samples[index] == 2,
                !state.sample_loading[index],
            ),
            Page::AutoScratch => (
                [
                    "Baby",
                    "Double",
                    "Forward",
                    "Transformer",
                    "Crab",
                    "Chirp",
                    "Fast baby",
                    "Slow drag",
                ]
                .get(index)
                .unwrap_or(&"—")
                .to_string(),
                state.auto_scratch,
                index < 8 && loaded,
            ),
            Page::HotCues | Page::CueLoop => {
                let cue = state.hot_cues[index];
                (
                    cue.map_or("SET".to_owned(), |c| {
                        format!(
                            "{}{}",
                            if c.loop_end.is_some() { "↻ " } else { "" },
                            if state.cue_appearance[index].label.is_empty() {
                                clock(c.seconds)
                            } else {
                                state.cue_appearance[index].label.clone()
                            }
                        )
                    }),
                    cue.is_some() || held,
                    loaded,
                )
            }
            Page::Loop | Page::Roll => {
                let beats = pad_beats(page, self.pads[deck].bank[page as usize], index, false);
                (
                    format!("{beats} beats"),
                    held || state.loop_region.is_some_and(|r| {
                        self.grids[deck].is_some_and(|g| {
                            ((r[1] - r[0]) / g.beat_seconds() - beats).abs() < 0.001
                        })
                    }),
                    loaded && grid,
                )
            }
            Page::BeatJump => (
                format!(
                    "{:+} beats",
                    pad_beats(page, self.pads[deck].bank[page as usize], index, false)
                ),
                false,
                loaded && grid,
            ),
            Page::Slicer => (
                format!("Slice {}", index + 1),
                held,
                loaded && grid && index < 8,
            ),
            Page::SavedLoops => {
                let saved = self.deck_paths[deck]
                    .as_ref()
                    .and_then(|p| self.saved_loops.get(&crate::track_identity::key(p)))
                    .and_then(|s| s[index]);
                (
                    saved.map_or("SAVE".to_owned(), |r| clock(r[0])),
                    saved.is_some(),
                    loaded && (saved.is_some() || grid || state.loop_region.is_some()),
                )
            }
            Page::ManualLoop => (
                ["In", "Out", "Exit", "Reloop", "½", "×2", "← Beat", "Beat →"]
                    .get(index)
                    .unwrap_or(&"—")
                    .to_string(),
                false,
                loaded && index < 8,
            ),
            Page::Stems => (
                [
                    "Vocals",
                    "Melody",
                    "Bass",
                    "Drums",
                    "Acapella",
                    "Instrumental",
                    "Reset",
                    "Mute all",
                ]
                .get(index)
                .unwrap_or(&"—")
                .to_string(),
                index < 4 && state.stem_gains[index] > 0.,
                loaded && state.stems_ready && index < 8,
            ),
            Page::StemFx => {
                let stem = self.pads[deck].stem_fx_target;
                if index < 4 {
                    (
                        ["Vocals", "Melody", "Bass", "Drums"][index].into(),
                        index == stem,
                        loaded,
                    )
                } else {
                    let kind = PAD_FX[self.pads[deck].bank[page as usize].rem_euclid(4) as usize
                        * 4
                        + index
                        - 4];
                    let effect = state.stem_effects[stem];
                    (
                        kind.label().into(),
                        effect.enabled && effect.kind == kind,
                        loaded && state.stems_ready,
                    )
                }
            }
            Page::Fx => (PAD_FX[index].label().into(), held, loaded),
        }
    }
    pub(super) fn pad_page_options_menu(
        mut menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
        weak: WeakEntity<Self>,
        deck: usize,
        page: Page,
        locked: bool,
        quantize: bool,
        quantize_window: f64,
        stutter: bool,
        repeat: bool,
    ) -> PopupMenu {
        for (i, label, checked) in [
            (0, "Lock cue/loop editing", locked),
            (1, "Quantize on set", quantize),
            (2, "Cue stutter / hold", stutter),
            (3, "Delete saved loop: next pad", false),
            (4, "Release held slicer bank", false),
            (5, "Loop samples", repeat),
            (6, "Sampler volume +10%", false),
            (7, "Sampler volume -10%", false),
            (8, "Load from disk: next pad", false),
            (9, "Restore default: next pad", false),
            (10, "Capture deck: next pad", false),
        ] {
            let relevant = match i {
                0 => matches!(
                    page,
                    Page::HotCues | Page::CueLoop | Page::SavedLoops | Page::Sampler
                ),
                1 => matches!(
                    page,
                    Page::HotCues | Page::CueLoop | Page::Loop | Page::Roll | Page::ManualLoop
                ),
                2 => matches!(page, Page::HotCues | Page::CueLoop),
                3 => page == Page::SavedLoops,
                4 => page == Page::Slicer,
                _ => page == Page::Sampler,
            };
            if !relevant {
                continue;
            }
            if i == 1 && page == Page::HotCues {
                let weak = weak.clone();
                menu = menu.submenu_with_icon(
                    quantize.then(|| Icon::new(IconName::Check).xsmall()),
                    "Quantize on set",
                    window,
                    cx,
                    move |mut submenu, _, _| {
                        for (label, window) in [
                            ("No Quantize", None),
                            ("5% of beat", Some(0.05)),
                            ("10% of beat", Some(0.10)),
                            ("20% of beat", Some(0.20)),
                            ("30% of beat", Some(0.30)),
                        ] {
                            let weak = weak.clone();
                            submenu = submenu.item(
                                PopupMenuItem::new(label)
                                    .checked(match window {
                                        None => !quantize,
                                        Some(window) => quantize && quantize_window == window,
                                    })
                                    .on_click(move |_, _, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            this.pads[deck].quantize = window.is_some();
                                            if let Some(window) = window {
                                                this.pads[deck].quantize_window = window;
                                            }
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        submenu
                    },
                );
                continue;
            }
            let weak = weak.clone();
            menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                match i {
                                    0 => this.pads[deck].locked = !locked,
                                    1 => this.pads[deck].quantize = !quantize,
                                    2 => this.pads[deck].stutter = !stutter,
                                    3 => {
                                        this.pads[deck].delete_saved = true;
                                        this.pads[deck].notice =
                                            "Click a saved-loop pad to delete it.".into();
                                    }
                                    4 => this.pads[deck].slicer_anchor = None,
                                    5 => this.pads[deck].sample_repeat = !repeat,
                                    6 | 7 => this.sampler_volume(deck, if i == 6 { 0.1 } else { -0.1 }),
                                    _ => {
                                        this.pads[deck].sample_edit=(i-7) as u8;
                                        this.pads[deck].notice=match i {8=>"Click a pad to load an audio file into it.",9=>"Click a pad to restore its factory sound (slots 1–12).",_=>"Click a pad to capture the deck's loop or selected beats."}.into();
                                    },
                                };
                                cx.notify();
                            });
                        },
            ));
        }
        let controls: &[(usize, &str)] = match page {
            Page::Loop => &[(0, "Loop bank"), (1, "Loop mode")],
            Page::Roll | Page::BeatJump => &[(0, "Beat bank")],
            Page::Slicer => &[(0, "Slice length"), (1, "Slice step")],
            Page::CueLoop => &[],
            Page::SavedLoops => &[(0, "Saved loop length"), (1, "Move loop")],
            Page::ManualLoop => &[(0, "Loop length")],
            Page::Sampler => &[(0, "Trigger mode"), (1, "Capture length")],
            Page::AutoScratch => &[(0, "Cut / over"), (1, "Quantize / immediate")],
            Page::KeyShift => &[(0, "Key layout"), (1, "Key shift")],
            Page::StemFx => &[(0, "Effect bank"), (1, "Wet amount")],
            Page::HotCues | Page::Stems | Page::Fx => &[],
        };
        if !controls.is_empty() {
            menu = menu.separator();
        }
        for &(parameter, label) in controls {
            for (direction, direction_label) in [(-1, "Previous"), (1, "Next")] {
                if page == Page::AutoScratch && direction < 0 {
                    continue;
                }
                let weak = weak.clone();
                menu = menu.item(
                    PopupMenuItem::new(if page == Page::AutoScratch {
                        format!("Toggle {label}")
                    } else {
                        format!("{label}: {direction_label}")
                    })
                    .on_click(move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.pad_parameter(deck, page, parameter, direction, cx)
                        });
                    }),
                );
            }
        }
        menu
    }
    fn pad_menu(&self, deck: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let weak = cx.entity().downgrade();
        let page = self.pads[deck].page;
        let locked = self.pads[deck].locked;
        let quantize = self.pads[deck].quantize;
        let quantize_window = self.pads[deck].quantize_window;
        let stutter = self.pads[deck].stutter;
        let repeat = self.pads[deck].sample_repeat;
        Button::new(("pad-options", deck))
            .tooltip("Configure cue locks, quantization and pad behaviour.")
            .xsmall()
            .outline()
            .label("Options ▾")
            .dropdown_menu(move |menu, window, cx| {
                Self::pad_page_options_menu(
                    menu,
                    window,
                    cx,
                    weak.clone(),
                    deck,
                    page,
                    locked,
                    quantize,
                    quantize_window,
                    stutter,
                    repeat,
                )
            })
    }
    pub(super) fn pad_panel(
        &self,
        deck: usize,
        state: &DeckState,
        upper: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let page = self.pads[deck].page;
        let sixteen = self.effect_view[deck] == 4;
        let split = if sixteen { None } else { self.pads[deck].split };
        let compact = !upper;
        let accent = deck_colour(deck);
        let mut panel = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w_full()
            .gap(px(4.))
            .when(compact, |panel| panel.h_full())
            .when(!compact, |panel| {
                panel
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
            });
        let weak = cx.entity().downgrade();
        let mut header = div()
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                let direction = crate::wheel::steps(event).signum() as i32;
                if direction != 0 {
                    this.release_pad(deck);
                    this.pads[deck].page = Page::ALL
                        [(page as i32 + direction).rem_euclid(Page::ALL.len() as i32) as usize];
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .flex()
            .items_center()
            .gap_1()
            .flex_wrap()
            .when(self.settings.four_decks || upper, |header| {
                header.child(
                    Button::new(("pad-view", deck * 2 + usize::from(upper)))
                        .tooltip(
                            "Toggle favourite pad pages; in the upper bank, return to effects.",
                        )
                        .xsmall()
                        .outline()
                        .label(if upper {
                            "Pads 16 · FX"
                        } else if self.pads[deck].favorites {
                            "Favourites"
                        } else {
                            "PADS"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.release_pad(deck);
                            if upper {
                                this.effect_view[deck] = 0;
                            } else {
                                this.pads[deck].favorites = !this.pads[deck].favorites;
                            }
                            cx.notify();
                        })),
                )
            })
            .when(self.settings.four_decks || upper, |header| {
                header.child(
                    Button::new(("pad-page", deck * 2 + usize::from(upper)))
                        .tooltip("Choose the pad mode.")
                        .xsmall()
                        .outline()
                        .label(page.label())
                        .dropdown_caret(true)
                        .dropdown_menu(move |mut menu, _, _| {
                            for selected in Page::ALL {
                                let weak = weak.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(selected.label())
                                        .checked(selected == page)
                                        .on_click(move |_, _, cx| {
                                            let _ = weak.update(cx, |this, cx| {
                                                this.release_pad(deck);
                                                this.pads[deck].page = selected;
                                                this.pads[deck].notice.clear();
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                            if !sixteen {
                                let weak_off = weak.clone();
                                menu = menu.item(
                                    PopupMenuItem::new("Single page")
                                        .checked(split.is_none())
                                        .on_click(move |_, _, cx| {
                                            let _ = weak_off.update(cx, |this, cx| {
                                                this.release_pad(deck);
                                                this.pads[deck].split = None;
                                                cx.notify();
                                            });
                                        }),
                                );
                                for selected in Page::ALL {
                                    let weak = weak.clone();
                                    menu =
                                        menu.item(
                                            PopupMenuItem::new(format!(
                                                "Split lower: {}",
                                                selected.label()
                                            ))
                                            .checked(split == Some(selected))
                                            .on_click(move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.release_pad(deck);
                                                    this.pads[deck].split = Some(selected);
                                                    cx.notify();
                                                });
                                            }),
                                        );
                                }
                            }
                            menu
                        }),
                )
            });
        if !upper {
            let mut parameters = div().flex().gap_2().flex_wrap();
            for param in 0..2 {
                let param_page = if param == 1 {
                    split.unwrap_or(page)
                } else {
                    page
                };
                if param_page == Page::HotCues {
                    continue;
                }
                let parameter = if split.is_some() { 0 } else { param };
                let label = match param_page {
                    Page::KeyShift => {
                        if parameter == 0 {
                            [
                                "Semitones",
                                "Whole tones",
                                "Major scale",
                                "Natural minor scale",
                            ][self.pads[deck].bank[param_page as usize] as usize]
                                .into()
                        } else {
                            format!("Shift {:+.0} st", state.key_shift)
                        }
                    }
                    Page::Sampler => {
                        if parameter == 0 {
                            ["Trigger", "Hold", "Retrigger"]
                                [self.pads[deck].bank[param_page as usize] as usize]
                                .into()
                        } else {
                            format!(
                                "Capture {} beats",
                                4 * 2i32.pow(self.pads[deck].size[param_page as usize] as u32)
                            )
                        }
                    }
                    Page::AutoScratch => {
                        if parameter == 0 {
                            if self.pads[deck].scratch_over {
                                "Over"
                            } else {
                                "Cut"
                            }
                            .into()
                        } else {
                            if self.pads[deck].quantize {
                                "Quantized"
                            } else {
                                "Immediate"
                            }
                            .into()
                        }
                    }
                    Page::HotCues => {
                        if parameter == 0 {
                            "Cue".into()
                        } else {
                            if self.pads[deck].quantize {
                                "±5% beat"
                            } else {
                                "Free"
                            }
                            .into()
                        }
                    }
                    Page::Loop if parameter == 1 => {
                        ["Toggle", "Hold", "Roll"][self.pads[deck].loop_mode].into()
                    }
                    Page::Slicer if parameter == 1 => format!(
                        "Step {}",
                        2f64.powi(self.pads[deck].step[param_page as usize])
                    ),
                    Page::CueLoop if parameter == 1 => {
                        if self.pads[deck].stutter {
                            "Hold".into()
                        } else {
                            "Latch".into()
                        }
                    }
                    Page::CueLoop => {
                        format!("Loop {} beats", 2f64.powi(self.loop_size[deck]))
                    }
                    Page::ManualLoop => format!("Size {} beats", 2f64.powi(self.loop_size[deck])),
                    Page::SavedLoops if parameter == 1 => "Move 1 beat".into(),
                    Page::Loop | Page::Roll | Page::BeatJump => {
                        format!("Bank {:+}", self.pads[deck].bank[param_page as usize])
                    }
                    Page::StemFx => {
                        if parameter == 0 {
                            format!(
                                "FX bank {}",
                                self.pads[deck].bank[param_page as usize].rem_euclid(4) + 1
                            )
                        } else {
                            format!(
                                "Wet {:.0}%",
                                state.stem_effects[self.pads[deck].stem_fx_target].amount * 100.
                            )
                        }
                    }
                    Page::Stems => {
                        if state.stems_ready {
                            "Stems ready".into()
                        } else {
                            "Prepare stems".into()
                        }
                    }
                    Page::Fx => "Hold to apply".into(),
                    _ => format!(
                        "Size {} beats",
                        2f64.powi(self.pads[deck].size[param_page as usize])
                    ),
                };
                let assigned = !matches!(param_page, Page::Stems | Page::Fx)
                    && !(param_page == Page::CueLoop && parameter == 0)
                    && !(parameter == 1
                        && matches!(param_page, Page::Roll | Page::BeatJump | Page::ManualLoop));
                if !assigned {
                    if parameter == 0 {
                        parameters = parameters.child(div().text_size(px(10.)).child(label));
                    }
                    continue;
                }
                let mut control = div()
                    .id(("pad-param", deck * 2 + param))
                    .flex()
                    .items_center()
                    .gap_1()
                    .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                        let d = crate::wheel::steps(event).signum() as i32;
                        if d != 0 {
                            this.pad_parameter(deck, param_page, parameter, d, cx);
                        }
                        cx.stop_propagation();
                    }));
                for direction in [-1, 1] {
                    if direction == 1 {
                        control = control.child(div().text_size(px(10.)).child(label.clone()));
                    }
                    control = control.child(
                        Button::new((
                            "pad-param-arrow",
                            deck * 4 + param * 2 + usize::from(direction > 0),
                        ))
                        .xsmall()
                        .outline()
                        .tooltip(if direction < 0 {
                            "Decrease this pad parameter."
                        } else {
                            "Increase this pad parameter."
                        })
                        .label(if direction < 0 { "‹" } else { "›" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pad_parameter(deck, param_page, parameter, direction, cx)
                        })),
                    );
                }
                parameters = parameters.child(control);
            }
            header = header.child(parameters);
            if (self.settings.four_decks || upper)
                && !matches!(
                    page,
                    Page::BeatJump | Page::Stems | Page::Fx | Page::KeyShift
                )
            {
                header = header.child(self.pad_menu(deck, cx));
            }
        }
        if !compact {
            panel = panel.child(header);
        }
        if !upper {
            if self.pads[deck].favorites {
                let mut favorites = div().flex().gap_1().flex_wrap();
                for selected in [
                    Page::HotCues,
                    Page::Loop,
                    Page::Roll,
                    Page::Stems,
                    Page::BeatJump,
                    Page::Fx,
                ] {
                    favorites = favorites.child(
                        Button::new(("pad-favourite", deck * 10 + selected as usize))
                            .tooltip("Switch to this favourite pad page.")
                            .xsmall()
                            .outline()
                            .label(selected.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.release_pad(deck);
                                this.pads[deck].page = selected;
                                cx.notify();
                            })),
                    );
                }
                panel = panel.child(favorites);
            }
        }
        for row in 0..2 {
            let rowpage = if row == 1 {
                split.unwrap_or(page)
            } else {
                page
            };
            let mut pads = div()
                .flex()
                .w_full()
                .gap(px(4.))
                .when(compact, |pads| pads.flex_1().min_h_0());
            for col in 0..4 {
                let index = if split.is_some() {
                    col
                } else {
                    row * 4 + col + if sixteen && !upper { 8 } else { 0 }
                };
                let (label, active, enabled) = self.pad_text(deck, rowpage, index, state);
                let is_cue = matches!(rowpage, Page::HotCues | Page::CueLoop);
                let accent = if is_cue {
                    rgb(crate::pad_store::cue_colour(
                        deck,
                        index,
                        &state.cue_appearance[index],
                    ))
                } else {
                    accent
                };
                pads = pads.child(
                    div()
                        .id((
                            "performance-pad",
                            deck * 32 + usize::from(upper) * 16 + row * 4 + col,
                        ))
                        .flex_1()
                        .min_w_0()
                        .when(compact, |pad| pad.h_full())
                        .when(!compact, |pad| pad.h(px(43.)))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .items_center()
                        .when(!compact, |pad| pad.rounded_md())
                        .border_1()
                        .border_color(if active || is_cue {
                            accent.into()
                        } else {
                            cx.theme().border
                        })
                        .bg(if active {
                            Hsla::from(accent).opacity(0.16)
                        } else {
                            cx.theme().secondary.opacity(0.4)
                        })
                        .text_color(if active || is_cue {
                            accent.into()
                        } else {
                            cx.theme().foreground
                        })
                        .text_size(px(10.))
                        .cursor_pointer()
                        .when(!enabled, |pad| pad.opacity(0.35))
                        .child(div().w_full().px_1().text_center().truncate().child(label))
                        .drag_over::<DraggedTrack>(|style, _, _, _| style.bg(rgb(0x183849)))
                        .on_drop(cx.listener(move |this, dragged: &DraggedTrack, _, cx| {
                            if rowpage == Page::Sampler {
                                this.import_sample(
                                    deck,
                                    index,
                                    dragged.path.to_string_lossy().into_owned(),
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }))
                        .on_scroll_wheel(cx.listener(
                            move |this, event: &ScrollWheelEvent, _, cx| {
                                if rowpage == Page::Sampler {
                                    let delta = event.delta.pixel_delta(px(20.)).y;
                                    this.sampler_volume(
                                        deck,
                                        if delta > px(0.) { 0.02 } else { -0.02 },
                                    );
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            },
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                if enabled {
                                    if rowpage == Page::HotCues && event.modifiers.control {
                                        this.release_pad(deck);
                                        if this
                                            .pad_state(deck)
                                            .is_some_and(|s| s.hot_cues[index].is_some())
                                        {
                                            this.pad_send(
                                                deck,
                                                DeckAction::TriggerHotCue {
                                                    slot: index as u8,
                                                    play: false,
                                                    loop_length: None,
                                                },
                                            );
                                        }
                                        cx.notify();
                                    } else {
                                        this.press_pad(
                                            deck,
                                            rowpage,
                                            index,
                                            event.modifiers.shift,
                                            cx,
                                        );
                                    }
                                }
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, _, window, cx| {
                                if enabled {
                                    if is_cue {
                                        this.edit_cue(deck, index, window, cx);
                                    } else {
                                        this.press_pad(deck, rowpage, index, true, cx);
                                    }
                                }
                                cx.stop_propagation();
                            }),
                        ),
                );
            }
            if compact && split.is_some() {
                panel = panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .gap(px(4.))
                        .child(
                            div()
                                .h(px(12.))
                                .flex_shrink_0()
                                .text_size(px(10.))
                                .text_color(cx.theme().muted_foreground)
                                .child(rowpage.label()),
                        )
                        .child(pads),
                );
            } else {
                if row == 1 {
                    if let Some(split) = split {
                        panel = panel.child(
                            div()
                                .text_size(px(10.))
                                .text_color(cx.theme().muted_foreground)
                                .child(split.label()),
                        );
                    }
                }
                panel = panel.child(pads);
            }
        }
        if !upper {
            if !compact && (page == Page::KeyShift || split == Some(Page::KeyShift)) {
                panel = panel.child(div().text_size(px(10.)).child(
                    "Latched offsets from the original key · Reset restores 0 · Scale pads follow major or natural minor intervals · Key Lock keeps tempo changes from altering pitch"
                ));
            }
            if !compact && (page == Page::Sampler || split == Some(Page::Sampler)) {
                let message=state.sample_errors.iter().enumerate().find_map(|(i,e)|e.as_ref().map(|e|format!("Slot {}: {e}",i+1))).unwrap_or_else(||"Drop a library track onto a pad · Options → Load from disk · Bank saved automatically".into());
                panel = panel.child(div().text_size(px(10.)).child(message));
            }
            if !self.pads[deck].notice.is_empty() {
                panel = panel.child(
                    div()
                        .text_size(px(10.))
                        .child(self.pads[deck].notice.clone()),
                );
            }
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Page, cue_loop_length, hot_cue_position, pad_beats, sample_capture_region, snapped,
    };
    use djcraft_library::TempoResult;

    #[test]
    fn key_shift_pads_use_signed_scale_intervals() {
        use super::key_shift_pad;
        assert_eq!(key_shift_pad(0, 0), -3.);
        assert_eq!(key_shift_pad(1, 0), -6.);
        assert_eq!(key_shift_pad(2, 0), -5.);
        assert_eq!(key_shift_pad(3, 0), -5.);
        assert_eq!(key_shift_pad(2, 5), 4.);
        assert_eq!(key_shift_pad(3, 5), 3.);
        for mode in 0..4 {
            assert_eq!(key_shift_pad(mode, 3), 0.);
            for pad in 0..16 {
                assert!((-12.0..=12.0).contains(&key_shift_pad(mode, pad)));
            }
        }
        use super::key_shift_step;
        assert_eq!(key_shift_step(0, 0., -1), -1.);
        assert_eq!(key_shift_step(1, 0., 1), 2.);
        assert_eq!(key_shift_step(2, 2., 1), 4.);
        assert_eq!(key_shift_step(3, 2., 1), 3.);
        assert_eq!(key_shift_step(3, 0., -1), -2.);
        assert_eq!(key_shift_step(2, -12., -1), -12.);
        assert_eq!(key_shift_step(0, 12., 1), 12.);
        assert_eq!(key_shift_step(2, 3., 0), 3.);
    }
    #[core::prelude::v1::test]
    fn pad_banks_jump_pairs_and_quantization() {
        for bank in -3..=4 {
            for index in 0..8 {
                let loop_beats = pad_beats(Page::Loop, bank, index, false);
                assert_eq!(pad_beats(Page::Roll, bank, index, false), loop_beats);
                assert_eq!(pad_beats(Page::Roll, bank, index, true), loop_beats);
            }
        }
        assert_eq!(super::Pads::default().bank[Page::BeatJump as usize], 2);
        for (pair, beats) in [4., 8., 16., 32.].into_iter().enumerate() {
            assert_eq!(pad_beats(Page::BeatJump, 2, pair * 2, false), -beats);
            assert_eq!(pad_beats(Page::BeatJump, 2, pair * 2 + 1, false), beats);
        }
        assert_eq!(pad_beats(Page::BeatJump, 1, 0, false), -2.);
        let grid = TempoResult {
            bpm: 120.,
            first_beat_seconds: 0.1,
            downbeat: 0,
            confidence: 1.,
        };
        assert!((snapped(1.42, Some(grid), true) - 1.6).abs() < 1e-8);
        assert_eq!(snapped(1.42, Some(grid), false), 1.42);
    }

    #[test]
    fn cue_loop_uses_the_current_loop_length() {
        let grid = TempoResult {
            bpm: 120.,
            first_beat_seconds: 0.,
            downbeat: 0,
            confidence: 1.,
        };
        assert_eq!(cue_loop_length(Some(grid), None, 2), Some(2.));
        assert_eq!(cue_loop_length(Some(grid), None, 3), Some(4.));
        assert_eq!(cue_loop_length(Some(grid), Some([1., 2.5]), 2), Some(1.5));
        assert_eq!(cue_loop_length(None, None, 2), None);
    }

    #[core::prelude::v1::test]
    fn sample_capture_uses_exact_playhead_or_selected_loop() {
        let grid = TempoResult {
            bpm: 120.,
            first_beat_seconds: 0.1,
            downbeat: 0,
            confidence: 1.,
        };
        assert_eq!(
            sample_capture_region(1.42, None, Some(grid), 4., 10.),
            Some([1.42, 3.42])
        );
        assert_eq!(
            sample_capture_region(1.42, Some([0.6, 2.6]), Some(grid), 4., 10.),
            Some([0.6, 2.6])
        );
        assert_eq!(
            sample_capture_region(9.8, None, Some(grid), 4., 10.),
            Some([9.8, 10.])
        );
        assert_eq!(
            sample_capture_region(1.42, Some([0.6, 2.6]), None, 4., 10.),
            Some([0.6, 2.6])
        );
        assert_eq!(sample_capture_region(1.42, None, None, 4., 10.), None);
    }

    #[core::prelude::v1::test]
    fn hot_cue_snap_preserves_offbeat_placements() {
        for bpm in [90., 120., 150.] {
            let grid = TempoResult {
                bpm,
                first_beat_seconds: 0.1,
                downbeat: 0,
                confidence: 1.,
            };
            let beat = grid.beat_seconds();
            let target = grid.first_beat_seconds + 3. * beat;
            for offset in [-0.05, -0.02, 0., 0.02, 0.05] {
                assert!(
                    (hot_cue_position(target + offset * beat, Some(grid), true, 0.05) - target)
                        .abs()
                        < 1e-9
                );
            }
            for offset in [-0.5, -0.051, 0.051, 0.5] {
                let position = target + offset * beat;
                assert_eq!(hot_cue_position(position, Some(grid), true, 0.05), position);
            }
            let close = target + 0.01 * beat;
            assert_eq!(hot_cue_position(close, Some(grid), false, 0.05), close);
            assert_eq!(hot_cue_position(close, None, true, 0.05), close);
            assert_eq!(
                hot_cue_position(target + 0.15 * beat, Some(grid), true, 0.10),
                target + 0.15 * beat
            );
            assert_eq!(
                hot_cue_position(target + 0.15 * beat, Some(grid), true, 0.20),
                target
            );
        }
    }
}
