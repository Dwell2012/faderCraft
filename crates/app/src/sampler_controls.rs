use super::*;
use djcraft_protocol::SamplerAction;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{
    WindowExt,
    input::{Input, InputState},
};

#[derive(Default)]
pub(super) struct SamplerUi {
    mode: usize,
    capture_deck: usize,
    capture_beats: Option<u8>,
    repeat: bool,
    locked: bool,
    hold: Option<usize>,
    notice: String,
}

impl Shell {
    /// Independent named sampler banks on their own master-routed bus.
    pub(super) fn sampler_side_panel(&self, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        let state = &self.sampler_state;
        let accent = rgb(0x48d1dd);
        let mode = self.sampler_ui.mode;
        let repeat = self.sampler_ui.repeat;
        let locked = self.sampler_ui.locked;
        let slot = self.sampler_slot;
        let list = self.sampler_list;
        let mut samples = div()
            .id("sampler-side-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(4.));
        for row in 0..if list { 16 } else { 4 } {
            let mut pads = div().flex().gap(px(4.)).flex_shrink_0();
            for col in 0..if list { 1 } else { 4 } {
                let index = if list { row } else { row * 4 + col };
                let playing = state.slots[index] == 2;
                let loading = state.loading[index];
                let empty = state.slots[index] == 0;
                let error = state.errors[index].as_ref();
                let name = if loading {
                    "Loading…".into()
                } else if error.is_some() {
                    "Load failed".into()
                } else if empty {
                    "Load sample".into()
                } else if state.names[index].is_empty() {
                    format!("Sample {}", index + 1)
                } else {
                    state.names[index].clone()
                };
                pads = pads.child(
                    div()
                        .id(("side-sample", index))
                        .flex_1()
                        .min_w_0()
                        .h(px(if list { 40. } else { 58. }))
                        .flex()
                        .flex_col()
                        .justify_between()
                        .p(px(4.))
                        .border_1()
                        .border_color(if playing || index == slot {
                            accent.into()
                        } else {
                            cx.theme().border
                        })
                        .bg(if playing {
                            Hsla::from(accent).opacity(0.24)
                        } else {
                            cx.theme().secondary.opacity(0.5)
                        })
                        .text_size(px(10.))
                        .cursor_pointer()
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .child(format!("{:02}", index + 1))
                                .child(if playing {
                                    "▶"
                                } else if empty {
                                    "+"
                                } else {
                                    "●"
                                }),
                        )
                        .child(div().w_full().truncate().child(name))
                        .child(div().h(px(2.)).w_full().bg(if playing {
                            Hsla::from(accent)
                        } else {
                            cx.theme().border
                        }))
                        .drag_over::<DraggedTrack>(|style, _, _, _| style.bg(rgb(0x183849)))
                        .on_drop(cx.listener(move |this, dragged: &DraggedTrack, _, cx| {
                            this.sampler_slot = index;
                            this.import_side_sample(
                                index,
                                dragged.path.to_string_lossy().into_owned(),
                                cx,
                            );
                            cx.stop_propagation();
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.sampler_slot = index;
                                if !loading {
                                    if empty {
                                        this.choose_side_sample(index, cx);
                                    } else {
                                        this.trigger_side_sample(index, false, cx);
                                    }
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, _, _, cx| {
                                this.sampler_slot = index;
                                this.trigger_side_sample(index, true, cx);
                                cx.stop_propagation();
                            }),
                        ),
                );
            }
            samples = samples.child(pads);
        }
        let error = state.error.clone().or_else(|| state.errors[slot].clone());
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(4.))
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .items_center()
                    .child(self.sampler_bank_menu(cx))
                    .child(
                        Button::new("sampler-new-bank")
                            .xsmall()
                            .outline()
                            .label("New")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.edit_sample_bank(false, window, cx)
                            })),
                    )
                    .child(
                        Button::new("sampler-rename-bank")
                            .xsmall()
                            .outline()
                            .label("Rename")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.edit_sample_bank(true, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(
                        Button::new("sampler-side-mode")
                            .xsmall()
                            .outline()
                            .label(["On/Off", "Hold", "Stutter"][mode])
                            .tooltip("Cycle trigger mode; right-click any pad to stop")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.release_side_sample(cx);
                                this.sampler_ui.mode = (mode + 1) % 3;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("sampler-side-repeat")
                            .xsmall()
                            .outline()
                            .label("Loop")
                            .selected(repeat)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.sampler_ui.repeat = !repeat;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("sampler-side-view")
                            .xsmall()
                            .outline()
                            .label(if list { "Pads" } else { "List" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.sampler_list = !list;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("sampler-side-stop")
                            .xsmall()
                            .outline()
                            .label("Stop all")
                            .tooltip("Stop every playing sample in the sampler")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sampler_ui.hold = None;
                                this.sampler_send(SamplerAction::StopAll, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(samples)
            .child(self.sampler_capture_controls(decks, cx))
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(
                        Button::new("sampler-side-load")
                            .xsmall()
                            .outline()
                            .label(format!("Load {:02}", slot + 1))
                            .disabled(locked)
                            .on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.choose_side_sample(slot, cx)
                                }),
                            ),
                    )
                    .child(
                        Button::new("sampler-side-clear")
                            .xsmall()
                            .outline()
                            .label("Clear")
                            .disabled(locked || state.slots[slot] == 0)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.release_side_sample(cx);
                                this.sampler_send(SamplerAction::Clear { slot: slot as u8 }, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("sampler-side-lock")
                            .xsmall()
                            .outline()
                            .label(if locked { "Locked" } else { "Unlocked" })
                            .selected(locked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.sampler_ui.locked = !locked;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .items_center()
                    .child(div().flex_1().text_size(px(10.)).child(format!(
                        "MASTER · {:.0}%",
                        self.mixer_values[20].powi(2) * 100.
                    )))
                    .children([-0.05f32, 0.05].into_iter().enumerate().map(|(i, delta)| {
                        Button::new(("sampler-side-volume", i))
                            .xsmall()
                            .outline()
                            .label(if delta < 0. { "−" } else { "+" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let value = (this.mixer_values[20] + delta).clamp(0., 1.);
                                this.set_mixer_control(20, value, cx);
                            }))
                    })),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(cx.theme().muted_foreground)
                    .child(error.unwrap_or_else(|| {
                        if self.sampler_ui.notice.is_empty() {
                            "Drop tracks onto pads · right-click stops · banks save automatically"
                                .into()
                        } else {
                            self.sampler_ui.notice.clone()
                        }
                    })),
            )
    }

    fn sampler_capture_controls(&self, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        let source = self.sampler_ui.capture_deck;
        let deck = &decks[source];
        let beats = self.sampler_ui.capture_beats;
        let slot = self.sampler_slot;
        let ready = deck.stems_ready && deck.stem_gains.iter().any(|g| *g > 0.);
        let range = capture_region(
            deck.position_seconds,
            deck.loop_region,
            self.grids[source].map(|g| g.beat_seconds()),
            beats,
            deck.track.as_ref().map_or(0., |t| t.loaded_seconds),
        );
        let stems = crate::waveform::STEM_LABELS
            .into_iter()
            .zip(deck.stem_gains)
            .filter(|(_, gain)| *gain > 0.)
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
            .join(" + ");
        let weak = cx.weak_entity();
        let bank_weak = weak.clone();
        let source_menu = Button::new("sampler-capture-source")
            .xsmall()
            .outline()
            .label(format!("Deck {}", deck_label(source)))
            .dropdown_caret(true)
            .tooltip("Capture from this deck only, using its currently enabled stems")
            .dropdown_menu(move |mut menu, _, _| {
                for deck in 0..DECKS {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!("Deck {}", deck_label(deck)))
                            .checked(deck == source)
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.sampler_ui.capture_deck = deck;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        let length = Button::new("sampler-capture-length").xsmall().outline()
            .label(beats.map_or_else(|| "Loop / 4b".to_string(), |b| format!("{b} beats")))
            .dropdown_caret(true)
            .tooltip("Use the active deck loop (otherwise four beats), or capture a beat length from the playhead")
            .dropdown_menu(move |mut menu, _, _| {
                for value in [None, Some(1), Some(2), Some(4), Some(8), Some(16), Some(32)] {
                    let weak = bank_weak.clone();
                    let label = value.map_or_else(|| "Deck loop / 4 beats".to_string(), |b| format!("{b} beats from playhead"));
                    menu = menu.item(PopupMenuItem::new(label).checked(value == beats).on_click(move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| { this.sampler_ui.capture_beats = value; cx.notify(); });
                    }));
                }
                menu
            });
        div().flex().flex_col().gap(px(4.)).flex_shrink_0()
            .child(div().flex().gap(px(4.)).child(source_menu).child(length)
                .child(Button::new("sampler-capture").xsmall().outline().label(format!("Capture {:02}", slot + 1))
                    .disabled(self.sampler_ui.locked || !ready || range.is_none() || self.sampler_state.loading[slot])
                    .tooltip("Save only the selected deck's enabled stems into this sample slot; independent of deck faders and other decks")
                    .on_click(cx.listener(move |this, _, _, cx| this.capture_side_sample(cx)))))
            .child(div().text_size(px(10.)).text_color(cx.theme().muted_foreground).truncate()
                .child(if !deck.stems_ready { format!("Deck {}: prepare stems to capture", deck_label(source)) }
                    else if stems.is_empty() { "Enable at least one stem on the source deck".into() }
                    else if range.is_none() { "Set a deck loop or analyse tempo to capture".into() }
                    else { format!("Capture: {stems}") }))
    }

    fn capture_side_sample(&mut self, cx: &mut Context<Self>) {
        if self.sampler_ui.locked {
            return;
        }
        let source = self.sampler_ui.capture_deck;
        let Some(deck) = self
            .engine
            .as_mut()
            .map(|engine| engine.state().decks[source].clone())
        else {
            return;
        };
        let range = capture_region(
            deck.position_seconds,
            deck.loop_region,
            self.grids[source].map(|g| g.beat_seconds()),
            self.sampler_ui.capture_beats,
            deck.track.as_ref().map_or(0., |t| t.loaded_seconds),
        );
        let Some([start, end]) = range else {
            self.sampler_ui.notice =
                "Set a deck loop or analyse tempo before capture (maximum 120 seconds).".into();
            cx.notify();
            return;
        };
        self.release_side_sample(cx);
        let slot = self.sampler_slot;
        if self.sampler_send(
            SamplerAction::Capture {
                deck: source as u8,
                slot: slot as u8,
                start,
                end,
            },
            cx,
        ) {
            self.sampler_ui.notice = format!(
                "Captured deck {} selected stems into slot {:02}",
                deck_label(source),
                slot + 1
            );
        }
    }

    fn sampler_bank_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let banks = self.sampler_state.banks.clone();
        let selected = self.sampler_state.selected_bank.clone();
        let name = banks
            .iter()
            .find(|b| b.id == selected)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "My Samples".into());
        let weak = cx.weak_entity();
        Button::new("sampler-bank-menu")
            .xsmall()
            .outline()
            .flex_1()
            .min_w_0()
            .label(name)
            .dropdown_caret(true)
            .tooltip("Choose a named sample bank. Switching stops sampler playback only.")
            .dropdown_menu(move |mut menu, _, _| {
                for bank in banks.clone() {
                    let weak = weak.clone();
                    let checked = bank.id == selected;
                    menu = menu.item(PopupMenuItem::new(bank.name).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.release_pads(cx);
                                this.sampler_send(
                                    SamplerAction::SelectBank {
                                        id: bank.id.clone(),
                                    },
                                    cx,
                                );
                            });
                        },
                    ));
                }
                menu
            })
    }

    fn edit_sample_bank(&mut self, rename: bool, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.sampler_state.selected_bank.clone();
        let value = if rename {
            self.sampler_state
                .banks
                .iter()
                .find(|b| b.id == id)
                .map(|b| b.name.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Bank name (up to 48 characters)"));
        input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
            input.focus(window, cx);
        });
        let weak = cx.weak_entity();
        let error = Rc::new(std::cell::RefCell::new(String::new()));
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let save_input = input.clone();
            let weak = weak.clone();
            let id = id.clone();
            let errors = error.clone();
            let save = Rc::new(move |cx: &mut App| {
                let name = save_input.read(cx).value().trim().to_string();
                weak.update(cx, |this, cx| {
                    if !rename {
                        this.release_pads(cx);
                    }
                    let ok = this.sampler_send(
                        if rename {
                            SamplerAction::RenameBank {
                                id: id.clone(),
                                name,
                            }
                        } else {
                            SamplerAction::CreateBank { name }
                        },
                        cx,
                    );
                    *errors.borrow_mut() = if ok {
                        String::new()
                    } else {
                        this.sampler_ui.notice.clone()
                    };
                    ok
                })
                .unwrap_or(false)
            });
            let confirm = save.clone();
            dialog
                .title(if rename {
                    "Rename sample bank"
                } else {
                    "New sample bank"
                })
                .w(px(380.))
                .on_ok(move |_, window, cx| {
                    let ok = confirm(cx);
                    if !ok {
                        window.refresh();
                    }
                    ok
                })
                .child(Input::new(&input))
                .child(
                    div()
                        .text_size(px(11.))
                        .child(if error.borrow().is_empty() {
                            "Choose a unique name, up to 48 characters.".to_string()
                        } else {
                            error.borrow().clone()
                        }),
                )
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("sample-bank-cancel")
                                .outline()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("sample-bank-save")
                                .primary()
                                .label(if rename { "Save" } else { "Create" })
                                .on_click(move |_, window, cx| {
                                    if save(cx) {
                                        window.close_dialog(cx);
                                    } else {
                                        window.refresh();
                                    }
                                }),
                        ),
                )
        });
    }

    fn sampler_send(&mut self, action: SamplerAction, cx: &mut Context<Self>) -> bool {
        let Some(engine) = &mut self.engine else {
            self.sampler_ui.notice = "Audio engine unavailable".into();
            cx.notify();
            return false;
        };
        let result = engine.apply(
            Role::Host,
            Action::Mixer {
                action: MixerAction::Sampler { action },
            },
        );
        self.sampler_state = engine.state().sampler;
        self.sampler_ui.notice = match &result {
            Ok(()) => String::new(),
            Err(error) => self
                .sampler_state
                .error
                .clone()
                .unwrap_or_else(|| format!("Sampler: {error:?}")),
        };
        cx.notify();
        result.is_ok()
    }

    fn import_side_sample(&mut self, slot: usize, path: String, cx: &mut Context<Self>) {
        if self.sampler_ui.locked {
            self.sampler_ui.notice = "Unlock sample editing first".into();
            cx.notify();
            return;
        }
        self.release_side_sample(cx);
        self.sampler_send(
            SamplerAction::Load {
                slot: slot as u8,
                path,
            },
            cx,
        );
    }

    fn choose_side_sample(&mut self, slot: usize, cx: &mut Context<Self>) {
        if self.sampler_ui.locked {
            self.sampler_ui.notice = "Unlock sample editing first".into();
            cx.notify();
            return;
        }
        let bank = self.sampler_state.selected_bank.clone();
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
                let _ = this.update(cx, |this, cx| {
                    if this.sampler_state.selected_bank == bank {
                        this.import_side_sample(slot, path, cx);
                    } else {
                        this.sampler_ui.notice =
                            "Bank changed; choose the destination again".into();
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn trigger_side_sample(&mut self, slot: usize, stop: bool, cx: &mut Context<Self>) {
        self.release_side_sample(cx);
        if self.sampler_state.slots[slot] == 0 || self.sampler_state.loading[slot] {
            return;
        }
        let play = !stop && (self.sampler_ui.mode != 0 || self.sampler_state.slots[slot] != 2);
        if self.sampler_send(
            SamplerAction::Play {
                slot: slot as u8,
                play,
                restart: play,
                repeat: self.sampler_ui.repeat,
                level: 1.,
            },
            cx,
        ) && play
            && self.sampler_ui.mode == 1
        {
            self.sampler_ui.hold = Some(slot);
        }
    }
    pub(super) fn release_side_sample(&mut self, cx: &mut Context<Self>) {
        if let Some(slot) = self.sampler_ui.hold.take() {
            self.sampler_send(
                SamplerAction::Play {
                    slot: slot as u8,
                    play: false,
                    restart: false,
                    repeat: false,
                    level: 1.,
                },
                cx,
            );
        }
    }
}

/// Source-time capture, independent of faders, other decks, and sampler playback.
fn capture_region(
    position: f64,
    region: Option<[f64; 2]>,
    beat_seconds: Option<f64>,
    beats: Option<u8>,
    loaded: f64,
) -> Option<[f64; 2]> {
    let [start, end] = if beats.is_none() && region.is_some() {
        region?
    } else {
        let beat = beat_seconds.filter(|beat| beat.is_finite() && *beat > 0.)?;
        let start = position.max(0.);
        [
            start,
            (start + beat * f64::from(beats.unwrap_or(4))).min(loaded),
        ]
    };
    (start.is_finite()
        && end.is_finite()
        && start >= 0.
        && end <= loaded
        && end - start >= 0.01
        && end - start <= 120.)
        .then_some([start, end])
}

#[cfg(test)]
mod capture_tests {
    #[test]
    fn capture_uses_loop_or_exact_playhead_and_limits_source_range() {
        use super::capture_region;
        assert_eq!(
            capture_region(8., Some([2., 4.]), None, None, 10.),
            Some([2., 4.])
        );
        assert_eq!(
            capture_region(1.25, None, Some(0.5), None, 10.),
            Some([1.25, 3.25])
        );
        assert_eq!(
            capture_region(1.25, Some([2., 4.]), Some(0.5), Some(8), 10.),
            Some([1.25, 5.25])
        );
        assert_eq!(
            capture_region(9., None, Some(0.5), Some(4), 10.),
            Some([9., 10.])
        );
        assert_eq!(capture_region(10., None, Some(0.5), Some(4), 10.), None);
        assert_eq!(capture_region(0., None, None, None, 10.), None);
        assert_eq!(capture_region(0., Some([0., 121.]), None, None, 200.), None);
    }
}
