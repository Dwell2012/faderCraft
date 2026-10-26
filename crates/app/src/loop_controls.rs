use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

#[derive(Clone, Copy)]
pub(super) struct LoopOptions {
    smart: bool,
    quantize: bool,
    backward: bool,
}
impl Default for LoopOptions {
    fn default() -> Self {
        Self {
            smart: true,
            quantize: true,
            backward: false,
        }
    }
}
const VIEWS: [&str; 5] = ["Main", "Adjust", "Saved", "Record", "Direct"];
const MIN_LOOP_EXPONENT: i32 = -4;
fn snap(time: f64, grid: TempoResult) -> f64 {
    (grid.first_beat_seconds
        + ((time - grid.first_beat_seconds) / grid.beat_seconds()).round() * grid.beat_seconds())
    .max(0.)
}
fn auto_region(position: f64, grid: TempoResult, beats: f64, options: LoopOptions) -> [f64; 2] {
    let anchor = if options.quantize {
        snap(position, grid)
    } else {
        position
    };
    let length = beats * grid.beat_seconds();
    if options.backward {
        [anchor - length, anchor]
    } else {
        [anchor, anchor + length]
    }
}
fn manual_region(
    start: f64,
    end: f64,
    grid: Option<TempoResult>,
    options: LoopOptions,
) -> [f64; 2] {
    if end <= start {
        return [start, end];
    }
    let Some(grid) = grid else {
        return [start, end];
    };
    if options.smart {
        let start = snap(start, grid);
        [
            start,
            start + ((end - start) / grid.beat_seconds()).round().max(1.) * grid.beat_seconds(),
        ]
    } else if options.quantize {
        [snap(start, grid), snap(end, grid)]
    } else {
        [start, end]
    }
}

impl Shell {
    pub(super) fn controller_loop_pad(
        &mut self,
        deck: usize,
        pad: u8,
        alternate: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.loop_state(deck).filter(|s| s.track.is_some()) else {
            return;
        };
        match pad {
            0 => {
                self.loop_in[deck] = Some(state.position_seconds);
                self.send(deck, DeckAction::ClearLoop);
            }
            1 => {
                if let Some(start) = self.loop_in[deck] {
                    let [a, b] = manual_region(
                        start,
                        state.position_seconds,
                        self.grids[deck],
                        self.loop_options[deck],
                    );
                    self.apply_loop(deck, a, b, cx);
                }
            }
            2 => {
                if state.loop_region.is_some() {
                    self.send(deck, DeckAction::ClearLoop);
                } else if let Some([a, b]) = self.loop_last[deck] {
                    self.apply_loop(deck, a, b, cx);
                }
            }
            3 => self.resize_loop(deck, if alternate { 1 } else { -1 }, cx),
            _ => {}
        }
        cx.notify();
    }
    pub(super) fn loop_state(&mut self, deck: usize) -> Option<DeckState> {
        self.engine.as_mut().map(|e| e.state().decks[deck].clone())
    }
    pub(super) fn apply_loop(
        &mut self,
        deck: usize,
        start: f64,
        end: f64,
        cx: &mut Context<Self>,
    ) -> bool {
        let success = self.engine.as_mut().is_some_and(|engine| {
            engine
                .apply(
                    Role::Host,
                    Action::Deck {
                        deck: deck as u8,
                        action: DeckAction::SetLoop { start, end },
                    },
                )
                .is_ok()
        });
        if success {
            self.loop_in[deck] = Some(start);
            self.loop_last[deck] = Some([start, end]);
            self.loop_notice[deck].clear();
        } else {
            self.loop_notice[deck] = "Loop must fit within loaded audio (minimum 10 ms).".into();
        }
        self.display_positions[deck] = DisplayPosition::default();
        cx.notify();
        success
    }
    pub(super) fn auto_loop(&mut self, deck: usize, toggle: bool, cx: &mut Context<Self>) {
        let Some(state) = self.loop_state(deck) else {
            return;
        };
        if toggle && state.loop_region.is_some() {
            self.send(deck, DeckAction::ClearLoop);
            self.loop_notice[deck].clear();
            cx.notify();
            return;
        }
        if let Some(grid) = self.grids[deck] {
            let [start, end] = auto_region(
                state.position_seconds,
                grid,
                2f64.powi(self.loop_size[deck]),
                self.loop_options[deck],
            );
            self.apply_loop(deck, start, end, cx);
        }
    }
    pub(super) fn resize_loop(&mut self, deck: usize, step: i32, cx: &mut Context<Self>) {
        let previous = self.loop_size[deck];
        self.loop_size[deck] = (previous + step).clamp(MIN_LOOP_EXPONENT, 6);
        if let Some([start, end]) = self.loop_state(deck).and_then(|s| s.loop_region) {
            let length = (end - start) * 2f64.powi(self.loop_size[deck] - previous);
            let region = if self.loop_options[deck].backward {
                [end - length, end]
            } else {
                [start, start + length]
            };
            if !self.apply_loop(deck, region[0], region[1], cx) {
                self.loop_size[deck] = previous;
            }
        }
        cx.notify();
    }
    pub(super) fn adjust_loop(
        &mut self,
        deck: usize,
        target: usize,
        direction: f64,
        cx: &mut Context<Self>,
    ) {
        if let Some([mut start, mut end]) = self.loop_state(deck).and_then(|s| s.loop_region) {
            let step = self.grids[deck].map_or(0.01, |g| g.beat_seconds() / 16.) * direction;
            if target != 2 {
                start += step;
            }
            if target != 0 {
                end += step;
            }
            self.apply_loop(deck, start, end, cx);
        }
    }
    pub(super) fn loop_saved_slot(
        &mut self,
        deck: usize,
        slot: usize,
        delete: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.deck_paths[deck]
            .as_ref()
            .map(crate::track_identity::key)
        else {
            return;
        };
        let existing = self.saved_loops.get(&path).and_then(|slots| slots[slot]);
        if delete {
            self.saved_loops.entry(path).or_insert([None; 16])[slot] = None;
        } else if let Some([start, end]) = existing {
            self.apply_loop(deck, start, end, cx);
            return;
        } else {
            if self.loop_state(deck).and_then(|s| s.loop_region).is_none() {
                self.loop_size[deck] = 2;
                self.auto_loop(deck, false, cx);
            }
            let Some(region) = self.loop_state(deck).and_then(|s| s.loop_region) else {
                return;
            };
            self.saved_loops.entry(path).or_insert([None; 16])[slot] = Some(region);
        }
        if let Err(error) = crate::loop_store::save(&data_dir(), &self.saved_loops) {
            self.loop_notice[deck] = format!("Could not save loops: {error}");
        }
        cx.notify();
    }
    fn record_loop(&mut self, deck: usize, cx: &mut Context<Self>) {
        let Some(state) = self.loop_state(deck) else {
            return;
        };
        let Some(track) = self.engine.as_ref().and_then(|e| e.track(deck)).cloned() else {
            return;
        };
        let region = if let Some((original, start)) = self.loop_recording[deck].take() {
            if !std::sync::Arc::ptr_eq(&original, &track) {
                return;
            }
            [start, state.position_seconds]
        } else if let Some(region) = state.loop_region {
            region
        } else if state.playing {
            self.loop_recording[deck] = Some((track, state.position_seconds));
            cx.notify();
            return;
        } else {
            return;
        };
        let root = data_dir().join("Sampler Recordings");
        self.loop_notice[deck] = "Saving source audio…".into();
        let identity = track.clone();
        let task = cx
            .background_executor()
            .spawn(async move { crate::loop_store::export(&track, region, &root) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .engine
                    .as_ref()
                    .and_then(|e| e.track(deck))
                    .is_some_and(|t| std::sync::Arc::ptr_eq(t, &identity))
                {
                    this.loop_notice[deck] = match result {
                        Ok(path) => format!("Saved {}", path.display()),
                        Err(e) => format!("Recording failed: {e}"),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn loop_menu(
        &self,
        deck: usize,
        roll: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let view = cx.weak_entity();
        let options = self.loop_options[deck];
        Button::new(("loop-options", deck))
            .tooltip("Configure smart loops, quantization and loop behaviour.")
            .xsmall()
            .outline()
            .label("Options")
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for (index, label, checked) in [
                    (0, "Smart Loop", options.smart),
                    (1, "Loop Back", options.backward),
                    (2, "Quantize · 1 beat", options.quantize),
                    (3, "Loop Roll", roll),
                ] {
                    let view = view.clone();
                    menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| {
                                match index {
                                    0 => this.loop_options[deck].smart = !checked,
                                    1 => this.loop_options[deck].backward = !checked,
                                    2 => this.loop_options[deck].quantize = !checked,
                                    _ => this
                                        .send(deck, DeckAction::SetLoopRoll { enabled: !checked }),
                                };
                                cx.notify();
                            });
                        },
                    ));
                }
                menu
            })
    }
    pub(super) fn loop_panel(&self, deck: usize, state: &DeckState, cx: &mut Context<Self>) -> Div {
        let active = state.loop_region.is_some();
        let loaded = state.track.is_some();
        let beats = state
            .loop_region
            .and_then(|[a, b]| self.grids[deck].map(|g| (b - a) / g.beat_seconds()))
            .unwrap_or(2f64.powi(self.loop_size[deck]));
        let beat_label = format!("{beats:.4}");
        let label = format!(
            "{} beats",
            beat_label.trim_end_matches('0').trim_end_matches('.')
        );
        let colour = if deck == 0 {
            rgb(0xc8ff38)
        } else {
            rgb(0x3864e8)
        };
        let progress = state.loop_region.map_or(0., |[a, b]| {
            ((state.position_seconds - a) / (b - a)).clamp(0., 1.) as f32
        });
        let view = self.loop_view[deck];
        let weak = cx.weak_entity();
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_1()
            .flex_shrink_0()
            .p_2()
            .border_1()
            .rounded_md()
            .border_color(if active {
                colour.into()
            } else {
                cx.theme().border
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new(("loop-view", deck))
                            .tooltip("Choose main, adjustment, saved or direct loop controls.")
                            .xsmall()
                            .outline()
                            .label(format!("LOOP · {}", VIEWS[view]))
                            .dropdown_caret(true)
                            .dropdown_menu(move |mut menu, _, _| {
                                for (i, label) in VIEWS.into_iter().enumerate() {
                                    let weak = weak.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(label).checked(view == i).on_click(
                                            move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.loop_view[deck] = i;
                                                    cx.notify();
                                                });
                                            },
                                        ),
                                    );
                                }
                                menu
                            }),
                    )
                    .child(self.loop_menu(deck, state.loop_roll, cx)),
            )
            .child(
                div()
                    .id(("loop-size-wheel", deck))
                    .flex()
                    .items_center()
                    .gap_2()
                    .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                        let step = crate::wheel::steps(event);
                        if step != 0. {
                            this.resize_loop(deck, if step > 0. { 1 } else { -1 }, cx);
                        }
                        cx.stop_propagation();
                    }))
                    .child(
                        Button::new(("loop-half", deck))
                            .xsmall()
                            .outline()
                            .label("◀")
                            .tooltip("Half loop length")
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.resize_loop(deck, -1, cx)),
                            ),
                    )
                    .child(
                        Button::new(("loop-toggle", deck))
                            .tooltip("Activate or exit the loop. Wheel to change its length.")
                            .small()
                            .outline()
                            .label(label)
                            .selected(active)
                            .disabled(!loaded || (!active && self.grids[deck].is_none()))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.auto_loop(deck, true, cx)),
                            ),
                    )
                    .child(
                        Button::new(("loop-double", deck))
                            .xsmall()
                            .outline()
                            .label("▶")
                            .tooltip("Double loop length")
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.resize_loop(deck, 1, cx)),
                            ),
                    )
                    .child(
                        Button::new(("reloop", deck))
                            .tooltip("Return to the most recent loop.")
                            .xsmall()
                            .outline()
                            .label("Reloop")
                            .disabled(self.loop_last[deck].is_none())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some([a, b]) = this.loop_last[deck] {
                                    this.apply_loop(deck, a, b, cx);
                                }
                            })),
                    ),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        window.paint_quad(fill(
                            Bounds::new(
                                bounds.origin,
                                size(bounds.size.width * progress, bounds.size.height),
                            ),
                            colour,
                        ));
                    },
                )
                .h(px(3.))
                .w_full(),
            );
        match view {
            0 => {
                panel = panel.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new(("loop-in", deck))
                                .tooltip("Set the loop start at the playhead.")
                                .small()
                                .outline()
                                .label("In")
                                .disabled(!loaded)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(state) = this.loop_state(deck) {
                                        this.loop_in[deck] = Some(state.position_seconds);
                                    }
                                    this.send(deck, DeckAction::ClearLoop);
                                    this.loop_notice[deck].clear();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(("loop-out", deck))
                                .tooltip("Set the loop end and activate the loop.")
                                .small()
                                .outline()
                                .label("Out")
                                .disabled(!loaded || self.loop_in[deck].is_none())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let (Some(start), Some(state)) =
                                        (this.loop_in[deck], this.loop_state(deck))
                                    {
                                        let [a, b] = manual_region(
                                            start,
                                            state.position_seconds,
                                            this.grids[deck],
                                            this.loop_options[deck],
                                        );
                                        this.apply_loop(deck, a, b, cx);
                                    }
                                })),
                        ),
                );
            }
            1 => {
                panel = panel.child(
                    div().flex().flex_wrap().gap_2().children(
                        ["In", "Move", "Out"]
                            .into_iter()
                            .enumerate()
                            .map(|(target, label)| {
                                div()
                                    .id(("adjust-wheel", deck * 3 + target))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .on_scroll_wheel(cx.listener(
                                        move |this, event: &ScrollWheelEvent, _, cx| {
                                            this.adjust_loop(
                                                deck,
                                                target,
                                                crate::wheel::steps(event) as f64,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .child(
                                        Button::new(("adjust-left", deck * 3 + target))
                                            .tooltip("Move the selected loop boundary earlier.")
                                            .xsmall()
                                            .outline()
                                            .label("−")
                                            .disabled(!active)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.adjust_loop(deck, target, -1., cx)
                                            })),
                                    )
                                    .child(div().text_xs().child(label))
                                    .child(
                                        Button::new(("adjust-right", deck * 3 + target))
                                            .tooltip("Move the selected loop boundary later.")
                                            .xsmall()
                                            .outline()
                                            .label("+")
                                            .disabled(!active)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.adjust_loop(deck, target, 1., cx)
                                            })),
                                    )
                            })
                            .collect::<Vec<_>>(),
                    ),
                );
            }
            2 => {
                let slots = self.deck_paths[deck]
                    .as_ref()
                    .and_then(|p| self.saved_loops.get(&crate::track_identity::key(p)))
                    .copied()
                    .unwrap_or([None; 16]);
                panel = panel.child(
                    div().flex().flex_wrap().gap_2().children(
                        (0..4)
                            .map(|slot| {
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(
                                        Button::new(("saved-loop", deck * 4 + slot))
.tooltip("Recall this saved loop, or save the current loop in an empty slot.")
                                            .xsmall()
                                            .outline()
                                            .label(format!(
                                                "{} {}",
                                                slot + 1,
                                                if slots[slot].is_some() { "▶" } else { "Save" }
                                            ))
                                            .disabled(!loaded)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.loop_saved_slot(deck, slot, false, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new(("delete-loop", deck * 4 + slot))
.tooltip("Remove this saved loop.")
                                            .xsmall()
                                            .outline()
                                            .label("×")
                                            .disabled(slots[slot].is_none())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.loop_saved_slot(deck, slot, true, cx)
                                            })),
                                    )
                            })
                            .collect::<Vec<_>>(),
                    ),
                );
            }
            3 => {
                panel = panel.child(
                    Button::new(("loop-record", deck))
                        .small()
                        .outline()
                        .label(if self.loop_recording[deck].is_some() {
                            "Stop REC"
                        } else {
                            "REC"
                        })
                        .selected(self.loop_recording[deck].is_some())
                        .disabled(
                            !loaded
                                || (!active
                                    && !state.playing
                                    && self.loop_recording[deck].is_none()),
                        )
                        .tooltip("Save original source audio as WAV in Sampler Recordings")
                        .on_click(cx.listener(move |this, _, _, cx| this.record_loop(deck, cx))),
                );
            }
            _ => {
                panel = panel.child(
                    div().flex().flex_wrap().gap_1().children(
                        (MIN_LOOP_EXPONENT..=5)
                            .map(|exponent| {
                                let selected =
                                    active && (beats - 2f64.powi(exponent)).abs() < 0.001;
                                Button::new((
                                    "direct-loop",
                                    deck * 10 + (exponent - MIN_LOOP_EXPONENT) as usize,
                                ))
                                .tooltip("Activate a loop of this length.")
                                .xsmall()
                                .outline()
                                .label(format!("{}", 2f64.powi(exponent)))
                                .selected(selected)
                                .disabled(!loaded || self.grids[deck].is_none())
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.loop_size[deck] = exponent;
                                        this.auto_loop(deck, selected, cx);
                                    },
                                ))
                            })
                            .collect::<Vec<_>>(),
                    ),
                );
            }
        }
        panel.child(
            div()
                .text_size(px(10.))
                .text_color(cx.theme().muted_foreground)
                .child(if !self.loop_notice[deck].is_empty() {
                    self.loop_notice[deck].clone()
                } else if let Some([a, b]) = state.loop_region {
                    format!(
                        "{}  {a:.2}s – {b:.2}s",
                        if state.loop_roll { "ROLL" } else { "ACTIVE" }
                    )
                } else if let Some(start) = self.loop_in[deck] {
                    format!("IN {start:.2}s · Out closes loop · click size for auto")
                } else if self.grids[deck].is_none() {
                    "Manual In/Out available · analyse for beat loops".into()
                } else {
                    "Click size to loop · wheel to resize".into()
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{LoopOptions, auto_region, manual_region};
    use djcraft_library::TempoResult;
    #[test]
    fn directions_quantize_and_smart_manual() {
        let grid = TempoResult {
            bpm: 120.,
            first_beat_seconds: 0.,
            confidence: 1.,
            downbeat: 0,
        };
        let mut options = LoopOptions::default();
        assert_eq!(auto_region(2.3, grid, 4., options), [2.5, 4.5]);
        options.backward = true;
        assert_eq!(auto_region(2.3, grid, 4., options), [0.5, 2.5]);
        options.quantize = false;
        assert_eq!(
            auto_region(2.3, grid, 4., options),
            [0.2999999999999998, 2.3]
        );
        assert_eq!(manual_region(2.06, 4.02, Some(grid), options), [2., 4.]);
        options.smart = false;
        assert_eq!(manual_region(2.06, 4.02, Some(grid), options), [2.06, 4.02]);
    }
}
