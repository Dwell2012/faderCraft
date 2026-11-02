use super::mixer_controls::smooth_knob_svg;
use super::*;
use gpui_kit::component::Icon;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;

fn vertical_tool_icon(label: &str) -> Icon {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 18 68\"><text x=\"0\" y=\"0\" transform=\"translate(9 34) rotate(-90)\" text-anchor=\"middle\" dominant-baseline=\"middle\" fill=\"currentColor\" font-family=\"Arial, sans-serif\" font-size=\"9\" font-weight=\"400\">{label}</text></svg>"
    );
    Icon::default().data(svg.as_bytes()).w(px(18.)).h(px(68.))
}

fn small_solid_cog() -> Icon {
    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path fill=\"#778087\" d=\"M19.14 12.94c.04-.3.06-.61.06-.94s-.02-.64-.07-.94l2.03-1.58a.5.5 0 0 0 .12-.64l-1.92-3.32a.5.5 0 0 0-.61-.22l-2.39.96a7.1 7.1 0 0 0-1.63-.95l-.36-2.54a.5.5 0 0 0-.5-.42h-3.84a.5.5 0 0 0-.49.42l-.36 2.54c-.59.24-1.13.56-1.63.95l-2.39-.96a.5.5 0 0 0-.61.22L2.64 8.84a.5.5 0 0 0 .12.64l2.03 1.58a6.6 6.6 0 0 0 0 1.88l-2.03 1.58a.5.5 0 0 0-.12.64l1.92 3.32c.13.22.39.31.61.22l2.39-.96c.5.39 1.04.71 1.63.95l.36 2.54c.04.24.25.42.49.42h3.84c.25 0 .46-.18.5-.42l.36-2.54c.59-.24 1.13-.56 1.63-.95l2.39.96a.5.5 0 0 0 .61-.22l1.92-3.32a.5.5 0 0 0-.12-.64l-2.03-1.58ZM12 15.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7Z\"/></svg>";
    Icon::default().data(SVG.as_bytes()).w(px(11.)).h(px(11.))
}

pub(super) fn current_key_label(
    key: Option<djcraft_library::Camelot>,
    state: &DeckState,
) -> String {
    let Some(key) = key else {
        return format!("Key unknown · Shift {:+.0} st", state.key_shift);
    };
    let offset = state.key_shift as f64
        + if state.key_lock {
            0.
        } else {
            12. * state.tempo.log2()
        };
    let nearest = offset.round() as i32;
    let shifted = key.shifted(nearest);
    let cents = ((offset - nearest as f64) * 100.).round() as i32;
    let detune = if cents == 0 {
        String::new()
    } else {
        format!(" {cents:+}¢")
    };
    format!(
        "Key {} · {shifted}{detune} · Shift {:+.0} st",
        shifted.name(),
        state.key_shift
    )
}

impl Shell {
    pub(super) fn key_hover_text(&self, deck: usize, state: &DeckState) -> String {
        let base = current_key_label(self.deck_keys[deck], state);
        let Some(mode) = self.deck_modes[deck].as_ref() else {
            return format!("{base}\nMode unresolved");
        };
        let offset = state.key_shift as f64
            + if state.key_lock {
                0.
            } else {
                12. * state.tempo.log2()
            };
        let nearest = offset.round() as i32;
        let cents = ((offset - nearest as f64) * 100.).round() as i32;
        let detune = if cents == 0 {
            String::new()
        } else {
            format!(" {cents:+}¢")
        };
        format!("{base}\nEstimated mode: {}{detune}", mode.label(nearest),)
    }
    pub(super) fn transport_anchor_for(
        &mut self,
        deck: usize,
        action: &DeckAction,
    ) -> Option<(f64, Option<String>)> {
        if !matches!(
            action,
            DeckAction::Play
                | DeckAction::Load { .. }
                | DeckAction::Unload
                | DeckAction::Seek { .. }
                | DeckAction::TriggerHotCue { .. }
                | DeckAction::StartPadLoop { .. }
        ) {
            return self.cue_anchor[deck].clone();
        }
        let state = self.engine.as_mut()?.state().decks[deck].clone();
        playback_anchor(
            &state,
            action,
            self.pending_seeks[deck]
                .filter(|(_, at)| at.elapsed().as_millis() < 200)
                .map(|(s, _)| s),
            self.cue_anchor[deck].clone(),
        )
    }
    /// A fixed-height rhythm panel gives two decks twice the waveform height of four decks.
    pub(super) fn rhythm_panel(&self, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h(px(100.))
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .overflow_hidden()
            .children(
                decks
                    .iter()
                    .take(self.visible_decks())
                    .enumerate()
                    .map(|(index, state)| {
                        let track = self.engine.as_ref().and_then(|e| e.track(index));
                        let grid = self.grids[index];
                        let colour = deck_colour(index);
                        let stems_ready = track.is_some_and(|track| track.stem_levels(0).is_some());
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .w(px(94.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(colour)
                                    .font_weight(FontWeight::BOLD)
                                    .gap_1()
                                    .child(deck_label(index))
                                    .child(Button::new(("waveform-mode", index)).xsmall().outline()
                                        .label(if self.stem_waveforms[index] && stems_ready { "Stems" } else { "Wave" })
                                        .selected(self.stem_waveforms[index] && stems_ready)
                                        .disabled(!stems_ready)
                                        .tooltip(if stems_ready {
                                            "Switch this deck between low/mid/high waveform and vocal/instrument/bass/drum overlays."
                                        } else { "Prepare stems to enable this deck's stem waveform view." })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.stem_waveforms[index] = !this.stem_waveforms[index];
                                            cx.notify();
                                        }))),
                            )
                            .child(
                                div()
                                    .id(("rhythm-deck", index))
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .drag_over::<ExternalPaths>(|style, _, _, _| {
                                        style.bg(rgb(0x183849))
                                    })
                                    .on_drop(cx.listener(
                                        move |this, files: &ExternalPaths, _, cx| {
                                            this.load_external_track(index, files, cx);
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .drag_over::<DraggedTrack>(|style, _, _, _| {
                                        style.bg(rgb(0x183849))
                                    })
                                    .on_drop(cx.listener(
                                        move |this, dragged: &DraggedTrack, _, cx| {
                                            this.request_load(
                                                index,
                                                dragged.path.to_string_lossy().into_owned(),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .when_some(track, |this, track| {
                                        this.child(waveform(
                                            track.clone(),
                                            state.position_seconds,
                                            state.tempo,
                                            grid,
                                            self.waveform_areas[index].clone(),
                                            cx.theme().foreground,
                                            state.loop_region,
                                            state.roll_position_seconds,
                                            colour.into(),
                                            state.hot_cues,
                                            state.cue_appearance.clone(),
                                            index,
                                            self.stem_waveforms[index],
                                            self.settings.rgb_waveforms,
                                            if state.eq_mode == EqMode::Classic { state.eq } else { [1.; 3] },
                                            state.stem_gains,
                                            self.overview_cache[index].clone(),
                                        ))
                                    })
                                    .when(track.is_none(), |this| {
                                        this.child(
                                            div()
                                                .flex()
                                                .h_full()
                                                .items_center()
                                                .justify_center()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Drop a track to load"),
                                        )
                                    })
                                    .on_scroll_wheel(cx.listener(
                                        move |this, event: &ScrollWheelEvent, _, cx| {
                                            this.wheel_waveform(index, event, false, cx)
                                        },
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                            this.begin_waveform_drag(
                                                index,
                                                event.position.x.as_f32(),
                                                false,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                    })
                    .collect::<Vec<_>>(),
            )
    }

    fn tool_side_tab(&self, deck: usize, kind: usize, cx: &mut Context<Self>) -> AnyElement {
        let label = if kind == 1 {
            self.pads[deck].page.label().to_uppercase()
        } else {
            ["EFFECTS", "", "LOOPS"][kind].to_owned()
        };
        let button = Button::new(("deck-tool-side-tab", deck * 3 + kind))
            .ghost()
            .rounded(ButtonRounded::None)
            .w_full()
            .h_full()
            .min_w_0()
            .p_0()
            .tooltip(format!("Choose {label} view"))
            .child(vertical_tool_icon(&label));
        let weak = cx.weak_entity();
        match kind {
            0 => {
                let selected = self.effect_view[deck];
                button
                    .dropdown_menu(move |mut menu, _, _| {
                        for (view, name) in effect_controls::VIEWS.into_iter().enumerate() {
                            let weak = weak.clone();
                            menu = menu.item(
                                PopupMenuItem::new(name).checked(view == selected).on_click(
                                    move |_, _, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            this.effect_view[deck] = view;
                                            cx.notify();
                                        });
                                    },
                                ),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
            1 => {
                let selected = self.pads[deck].page;
                let favorites = self.pads[deck].favorites;
                button
                    .dropdown_menu(move |mut menu, _, _| {
                        for page in pad_controls::Page::ALL {
                            let weak = weak.clone();
                            menu = menu.item(
                                PopupMenuItem::new(page.label())
                                    .checked(page == selected)
                                    .on_click(move |_, _, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            this.release_pad(deck);
                                            this.pads[deck].page = page;
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        let weak = weak.clone();
                        menu = menu.item(
                            PopupMenuItem::new("Favourites")
                                .checked(favorites)
                                .on_click(move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.release_pad(deck);
                                        this.pads[deck].favorites = !favorites;
                                        cx.notify();
                                    });
                                }),
                        );
                        menu
                    })
                    .into_any_element()
            }
            _ => {
                let selected = self.loop_view[deck];
                button
                    .dropdown_menu(move |mut menu, _, _| {
                        for (view, name) in loop_controls::VIEWS.into_iter().enumerate() {
                            let weak = weak.clone();
                            menu = menu.item(
                                PopupMenuItem::new(name).checked(view == selected).on_click(
                                    move |_, _, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            if view == 1 {
                                                if let Some(previous) = this.saved_loops_deck {
                                                    this.loop_view[previous] = 0;
                                                }
                                                this.saved_loops_deck = Some(deck);
                                                this.library_side_tab = 1;
                                            } else if this.saved_loops_deck == Some(deck) {
                                                this.saved_loops_deck = None;
                                                this.library_side_tab = 0;
                                            }
                                            this.loop_view[deck] = view;
                                            cx.notify();
                                        });
                                    },
                                ),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
        }
    }

    fn tool_side_rail(
        &self,
        deck: usize,
        kind: usize,
        state: &DeckState,
        active: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let weak = cx.weak_entity();
        let cog = Button::new(("deck-tool-options", deck * 3 + kind))
            .ghost()
            .rounded(ButtonRounded::None)
            .w_full()
            .h(px(18.))
            .min_w_0()
            .p_0()
            .tooltip(match kind {
                0 => "Effect timing",
                1 => "Pad page and layout options",
                _ => "Loop options",
            })
            .child(small_solid_cog())
            .dropdown_caret(false);
        let cog = match kind {
            0 => {
                let beats = state.effect_beats;
                cog.dropdown_menu(move |mut menu, _, _| {
                    for (value, label) in djcraft_protocol::EFFECT_BEATS
                        .into_iter()
                        .zip(["1/8", "1/4", "1/2", "1", "2", "4", "8"])
                    {
                        let weak = weak.clone();
                        menu = menu.item(
                            PopupMenuItem::new(format!("FX beats: {label}"))
                                .checked((beats - value).abs() < 0.001)
                                .on_click(move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.send(
                                            deck,
                                            DeckAction::SetEffectBeats { beats: value },
                                        );
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                })
                .into_any_element()
            }
            1 => {
                let page = self.pads[deck].page;
                let locked = self.pads[deck].locked;
                let quantize = self.pads[deck].quantize;
                let quantize_window = self.pads[deck].quantize_window;
                let stutter = self.pads[deck].stutter;
                let repeat = self.pads[deck].sample_repeat;
                let split = self.pads[deck].split;
                cog.dropdown_menu(move |menu, window, cx| {
                    let page_weak = weak.clone();
                    let menu = menu.submenu(
                        if split.is_some() {
                            format!("Upper: {} Options", page.label())
                        } else {
                            format!("{} Options", page.label())
                        },
                        window,
                        cx,
                        move |menu, window, cx| {
                            Self::pad_page_options_menu(
                                menu,
                                window,
                                cx,
                                page_weak.clone(),
                                deck,
                                page,
                                locked,
                                quantize,
                                quantize_window,
                                stutter,
                                repeat,
                            )
                        },
                    );
                    let menu = if let Some(lower) = split {
                        let lower_weak = weak.clone();
                        menu.submenu(
                            format!("Lower: {} Options", lower.label()),
                            window,
                            cx,
                            move |menu, window, cx| {
                                Self::pad_page_options_menu(
                                    menu,
                                    window,
                                    cx,
                                    lower_weak.clone(),
                                    deck,
                                    lower,
                                    locked,
                                    quantize,
                                    quantize_window,
                                    stutter,
                                    repeat,
                                )
                            },
                        )
                    } else {
                        menu
                    };
                    menu.submenu("Pad Options", window, cx, {
                        let weak = weak.clone();
                        move |mut menu, _, _| {
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
                            for page in pad_controls::Page::ALL {
                                let weak = weak.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(format!("Split lower: {}", page.label()))
                                        .checked(split == Some(page))
                                        .on_click(move |_, _, cx| {
                                            let _ = weak.update(cx, |this, cx| {
                                                this.release_pad(deck);
                                                this.pads[deck].split = Some(page);
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                            menu
                        }
                    })
                })
                .into_any_element()
            }
            _ => {
                let options = self.loop_options[deck];
                let roll = state.loop_roll;
                cog.dropdown_menu(move |mut menu, _, _| {
                    for (index, label, checked) in [
                        (0, "Smart Loop", options.smart),
                        (1, "Loop Back", options.backward),
                        (2, "Quantize · 1 beat", options.quantize),
                        (3, "Loop Roll", roll),
                    ] {
                        let weak = weak.clone();
                        menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                            move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    match index {
                                        0 => this.loop_options[deck].smart = !checked,
                                        1 => this.loop_options[deck].backward = !checked,
                                        2 => this.loop_options[deck].quantize = !checked,
                                        _ => this.send(
                                            deck,
                                            DeckAction::SetLoopRoll { enabled: !checked },
                                        ),
                                    };
                                    cx.notify();
                                });
                            },
                        ));
                    }
                    menu
                })
                .into_any_element()
            }
        };
        let label = if kind == 1 {
            self.pads[deck].page.label().to_uppercase()
        } else {
            ["EFFECTS", "", "LOOPS"][kind].to_owned()
        };
        let tab = if active {
            self.tool_side_tab(deck, kind, cx)
        } else {
            Button::new(("four-deck-tool-tab", deck * 3 + kind))
                .ghost()
                .rounded(ButtonRounded::None)
                .w_full()
                .h_full()
                .min_w_0()
                .p_0()
                .tooltip(format!("Show {label}"))
                .child(vertical_tool_icon(&label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.release_pads(cx);
                    this.deck_panel[deck] = [1, 2, 0][kind];
                    this.active_deck = deck;
                    cx.notify();
                }))
                .into_any_element()
        };
        div()
            .w(px(18.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_1()
            .border_color(cx.theme().border)
            .child(cog)
            .child(div().flex_1().min_h_0().flex().items_center().child(tab))
    }

    pub(super) fn deck_display(
        &self,
        index: usize,
        state: &DeckState,
        controls: Option<AnyElement>,
        identity: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Div {
        let colour = deck_colour(index);
        let position = state.position_seconds;
        let loaded = state.track.is_some();
        let muted = cx.theme().muted_foreground;
        let platter = div()
            .id(("platter", index))
            .relative()
            .w(px(if self.settings.four_decks { 96. } else { 185. }))
            .h(px(if self.settings.four_decks { 96. } else { 185. }))
            .flex_shrink_0()
            .rounded_full()
            .border_3()
            .border_color(colour)
            .bg(cx.theme().secondary.opacity(0.3))
            .child(
                div()
                    .absolute()
                    .inset(px(if self.settings.four_decks { 4. } else { 6. }))
                    .rounded_full()
                    .border_2()
                    .border_color(Hsla::from(colour).opacity(0.35)),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let centre =
                            bounds.origin + point(bounds.size.width / 2., bounds.size.height / 2.);
                        let radius = bounds.size.width.as_f32() / 2. - 7.;
                        if loaded {
                            let angle = (position * 0.5555556 * std::f64::consts::TAU) as f32;
                            let mut pointer = PathBuilder::stroke(px(2.5));
                            let at = |distance: f32| {
                                centre
                                    + point(px(angle.sin() * distance), px(-angle.cos() * distance))
                            };
                            pointer.move_to(at(radius * 0.45));
                            pointer.line_to(at(radius - 3.));
                            if let Ok(path) = pointer.build() {
                                window.paint_path(path, colour);
                            }
                        }
                    },
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(muted)
                            .child(if state.playing {
                                "PLAYING"
                            } else if loaded {
                                "READY"
                            } else {
                                "EMPTY"
                            }),
                    ),
            )
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                this.wheel_waveform(index, event, false, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.begin_waveform_drag(index, event.position.x.as_f32(), false, cx);
                    cx.stop_propagation();
                }),
            );

        if self.settings.four_decks {
            let panel = self.deck_panel[index];
            let tools = match panel {
                0 => {
                    if self.loop_view[index] == 2 {
                        self.compact_direct_loop_panel(index, state, cx)
                            .into_any_element()
                    } else {
                        self.compact_loop_panel(index, state, cx).into_any_element()
                    }
                }
                1 => self.effect_panel(index, state, cx),
                _ => self.pad_panel(index, state, false, cx).into_any_element(),
            };
            let mut tools = Some(tools);
            let mut sections: Vec<AnyElement> = Vec::with_capacity(4);
            for selected_panel in [1, 2, 0] {
                let kind = [2, 0, 1][selected_panel];
                sections.push(
                    self.tool_side_rail(index, kind, state, panel == selected_panel, cx)
                        .into_any_element(),
                );
                if panel == selected_panel {
                    sections.push(
                        div()
                            .id(("four-deck-tool-content", index))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .bg(cx.theme().secondary.opacity(0.5))
                            .when(index % 2 == 0, |cell| cell.pl(px(4.)))
                            .when(index % 2 == 1, |cell| cell.pr(px(4.)))
                            .child(tools.take().unwrap())
                            .into_any_element(),
                    );
                }
            }
            return div()
                .flex()
                .h(px(96.))
                .flex_shrink_0()
                .items_center()
                .gap(px(4.))
                .when(index % 2 == 1, |row| row.flex_row_reverse())
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .h(px(96.))
                        .gap(px(4.))
                        .when(index % 2 == 1, |row| row.flex_row_reverse())
                        .children(sections),
                )
                .child(platter)
                .children(controls)
                .children(identity)
                .child(self.pitch_control(index, state, cx));
        }

        div()
            .flex()
            .flex_col()
            .gap_2()
            .h(px(193.))
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .gap(px(8.))
                    .when(index % 2 == 1, |row| row.flex_row_reverse())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_h_0()
                                    .gap(px(4.))
                                    .when(index % 2 == 1, |row| row.flex_row_reverse())
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .h_full()
                                            .flex()
                                            .gap(px(4.))
                                            .bg(cx.theme().secondary.opacity(0.5))
                                            .when(index % 2 == 1, |row| row.flex_row_reverse())
                                            .child(self.tool_side_rail(index, 0, state, true, cx))
                                            .child(
                                                div()
                                                    .id(("deck-effect-cell", index))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .h_full()
                                                    .overflow_y_scroll()
                                                    .bg(cx.theme().secondary.opacity(0.5))
                                                    .child(self.effect_panel(index, state, cx)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(166.))
                                            .flex_shrink_0()
                                            .min_w_0()
                                            .h_full()
                                            .bg(cx.theme().secondary.opacity(0.5)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_h_0()
                                    .gap(px(4.))
                                    .when(index % 2 == 1, |row| row.flex_row_reverse())
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .h_full()
                                            .flex()
                                            .bg(cx.theme().secondary.opacity(0.5))
                                            .when(index % 2 == 1, |row| row.flex_row_reverse())
                                            .child(self.tool_side_rail(index, 1, state, true, cx))
                                            .child(
                                                div()
                                                    .id(("deck-pad-cell", index))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .h_full()
                                                    .overflow_y_scroll()
                                                    .when(index % 2 == 0, |cell| cell.pl(px(4.)))
                                                    .when(index % 2 == 1, |cell| cell.pr(px(4.)))
                                                    .child(self.pad_panel(index, state, false, cx)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(166.))
                                            .flex_shrink_0()
                                            .min_w_0()
                                            .h_full()
                                            .flex()
                                            .bg(cx.theme().secondary.opacity(0.5))
                                            .when(index % 2 == 1, |row| row.flex_row_reverse())
                                            .child(self.tool_side_rail(index, 2, state, true, cx))
                                            .child(
                                                div()
                                                    .id(("deck-loop-cell", index))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .h_full()
                                                    .overflow_y_scroll()
                                                    .when(index % 2 == 0, |cell| cell.pl(px(4.)))
                                                    .when(index % 2 == 1, |cell| cell.pr(px(4.)))
                                                    .child(if self.loop_view[index] == 2 {
                                                        self.compact_direct_loop_panel(
                                                            index, state, cx,
                                                        )
                                                        .into_any_element()
                                                    } else {
                                                        self.compact_loop_panel(index, state, cx)
                                                            .into_any_element()
                                                    }),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .when(index % 2 == 0, |area| area.items_end())
                            .when(index % 2 == 1, |area| area.items_start())
                            .child(platter),
                    )
                    .child(
                        div()
                            .w(px(132.))
                            .h_full()
                            .flex_shrink_0()
                            .flex()
                            .gap(px(4.))
                            .when(index % 2 == 1, |row| row.flex_row_reverse())
                            .children(controls)
                            .child(
                                div()
                                    .w(px(66.))
                                    .h_full()
                                    .flex_shrink_0()
                                    .child(self.pitch_control(index, state, cx)),
                            ),
                    ),
            )
    }
}

impl Shell {
    pub(super) fn begin_cue_preview(&mut self, deck: usize, cx: &mut Context<Self>) {
        self.end_cue_preview(cx);
        let Some(state) = self.engine.as_mut().map(|e| e.state().decks[deck].clone()) else {
            return;
        };
        if state.track.is_none() {
            return;
        }
        let seconds = self.cue_anchor[deck]
            .as_ref()
            .filter(|(_, path)| *path == state.path)
            .map_or_else(
                || {
                    if state.playing {
                        return 0.;
                    }
                    self.pending_seeks[deck]
                        .map_or(state.position_seconds, |(position, _)| position)
                },
                |(position, _)| *position,
            );
        self.cue_anchor[deck] = Some((seconds, state.path.clone()));
        if !state.playing {
            self.cue_preview = Some((deck, seconds, state.path));
        }
        self.send(
            deck,
            DeckAction::CuePreview {
                seconds,
                play: !state.playing,
            },
        );
        cx.notify();
    }
    pub(super) fn end_cue_preview(&mut self, cx: &mut Context<Self>) {
        if let Some((deck, seconds, path)) = self.cue_preview.take() {
            let same = self
                .engine
                .as_mut()
                .is_some_and(|e| e.state().decks[deck].path == path);
            if same {
                self.send(
                    deck,
                    DeckAction::CuePreview {
                        seconds,
                        play: false,
                    },
                );
            }
            cx.notify();
        }
    }
    pub(super) fn toggle_active_deck(&mut self, cx: &mut Context<Self>) {
        if self.cue_preview.is_some() {
            return;
        }
        let Some(state) = self.engine.as_mut().map(|engine| engine.state()) else {
            return;
        };
        if state
            .decks
            .get(self.active_deck)
            .is_none_or(|deck| deck.track.is_none())
        {
            return;
        }
        use djcraft_protocol::script::{ControlSource, InvocationContext, Program};
        let program = Program::compile("do_action(\"active\", \"toggle_play\")")
            .expect("built-in Space binding must compile");
        let context = InvocationContext {
            source: ControlSource::Skin,
            state: Some(&state),
            active_deck: Some(self.active_deck as u8),
            visible_decks: if self.settings.four_decks { 4 } else { 2 },
        };
        if let Ok(actions) = program.run_in(context, 0.0) {
            for dispatch in actions {
                if let Action::Deck { deck, action } = dispatch.action {
                    self.send(usize::from(deck), action);
                }
            }
            cx.notify();
        }
    }
    pub(super) fn move_pitch(&mut self, y: f32, cx: &mut Context<Self>) {
        let Some(deck) = self.pitch_drag else {
            return;
        };
        if self.settings.four_decks
            && let Some((start_y, start_tempo)) = self.pitch_knob_start
        {
            let range = self
                .engine
                .as_mut()
                .map(|e| e.state().decks[deck].tempo_range)
                .unwrap_or_default();
            let ratio = (start_tempo + (start_y - y) as f64 / 140. * range.extent() * 2.)
                .clamp(1. - range.extent(), 1. + range.extent());
            self.send(deck, DeckAction::SetTempo { ratio });
            cx.notify();
            return;
        }
        let bounds = self.pitch_areas[deck].get();
        let fraction = ((y - bounds.origin.y.as_f32() - 5.)
            / (bounds.size.height.as_f32() - 10.).max(1.))
        .clamp(0., 1.);
        let range = self
            .engine
            .as_mut()
            .map(|e| e.state().decks[deck].tempo_range)
            .unwrap_or_default();
        self.send(
            deck,
            DeckAction::SetTempo {
                ratio: pitch_ratio(fraction as f64, range),
            },
        );
        cx.notify();
    }
    fn four_deck_pitch_control(
        &self,
        deck: usize,
        state: &DeckState,
        cx: &mut Context<Self>,
    ) -> Div {
        let range = state.tempo_range;
        let tempo = state.tempo;
        let colour = deck_colour(deck);
        let value = (((tempo - 1.) / range.extent()) * 0.5 + 0.5).clamp(0., 1.) as f32;
        let base = smooth_knob_svg(value, 0.5, false);
        let highlight = smooth_knob_svg(value, 0.5, true);
        let adjustment = (tempo - 1.) * 100.;
        let tempo_tooltip = if state.key_lock {
            "Key Lock on: changing tempo preserves the musical key. Drag vertically or scroll to adjust."
        } else {
            "Key Lock off: changing tempo also changes pitch and musical key. Drag vertically or scroll to adjust."
        };
        div()
            .flex()
            .flex_col()
            .w(px(62.))
            .h(px(96.))
            .flex_shrink_0()
            .gap(px(4.))
            .child(
                Button::new(("pitch-range", deck))
                    .outline()
                    .xsmall()
                    .rounded(ButtonRounded::None)
                    .w_full()
                    .h(px(29.))
                    .p_0()
                    .bg(cx.theme().secondary.opacity(0.5))
                    .label(format!("±{}%", range.percent()))
                    .tooltip("Cycle tempo range: Low 8%, Medium 16%, High 32%, Wide 48%.")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send(
                            deck,
                            DeckAction::SetTempoRange {
                                range: range.next(),
                            },
                        );
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id(("pitch-knob", deck))
                    .flex()
                    .w_full()
                    .h(px(30.))
                    .items_center()
                    .justify_center()
                    .bg(cx.theme().secondary.opacity(0.5))
                    .tooltip(move |window, cx| Tooltip::new(tempo_tooltip).build(window, cx))
                    .child(
                        div()
                            .relative()
                            .w(px(29.))
                            .h(px(29.))
                            .child(
                                Icon::default()
                                    .data(base.as_bytes())
                                    .text_color(rgb(0x727a83))
                                    .size_full(),
                            )
                            .child(
                                div().absolute().top_0().left_0().size_full().child(
                                    Icon::default()
                                        .data(highlight.as_bytes())
                                        .text_color(colour)
                                        .size_full(),
                                ),
                            ),
                    )
                    .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                        this.wheel_tempo(deck, event, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.pitch_drag = Some(deck);
                            this.pitch_knob_start = Some((event.position.y.as_f32(), tempo));
                            cx.stop_propagation();
                        }),
                    ),
            )
            .child(
                Button::new(("tempo-reset", deck))
                    .outline()
                    .xsmall()
                    .rounded(ButtonRounded::None)
                    .w_full()
                    .h(px(29.))
                    .p_0()
                    .bg(cx.theme().secondary.opacity(0.5))
                    .label(if adjustment.abs() < 0.05 {
                        "0.0%".to_owned()
                    } else {
                        format!("{adjustment:+.1}%")
                    })
                    .tooltip("Gradually return tempo to zero adjustment.")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tempo_returns[deck] = Some((tempo, Instant::now()));
                        cx.notify();
                    })),
            )
    }

    fn pitch_control(&self, deck: usize, state: &DeckState, cx: &mut Context<Self>) -> Div {
        if self.settings.four_decks {
            return self.four_deck_pitch_control(deck, state, cx);
        }
        let area = self.pitch_areas[deck].clone();
        let four_decks = self.settings.four_decks;
        let colour = deck_colour(deck);
        let tempo = state.tempo;
        let range = state.tempo_range;
        div()
            .flex()
            .flex_col()
            .items_center()
            .w(px(if self.settings.four_decks { 80. } else { 66. }))
            .h_full()
            .gap_1()
            .child(
                Button::new(("pitch-range", deck))
                    .outline()
                    .xsmall()
                    .when(!self.settings.four_decks, |button| {
                        button
                            .rounded(ButtonRounded::None)
                            .w_full()
                            .h(px(29.))
                            .p_0()
                            .bg(cx.theme().secondary.opacity(0.5))
                    })
                    .label(format!("±{}%", range.percent()))
                    .tooltip("Cycle tempo range: Low 8%, Medium 16%, High 32%, Wide 48%.")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send(
                            deck,
                            DeckAction::SetTempoRange {
                                range: range.next(),
                            },
                        );
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id(("pitch-fader", deck))
                    .w(px(if self.settings.four_decks { 42. } else { 66. }))
                    .flex_1()
                    .when(!self.settings.four_decks, |fader| {
                        fader.bg(cx.theme().secondary.opacity(0.5))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.pitch_drag = Some(deck);
                            this.move_pitch(event.position.y.as_f32(), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                        this.wheel_tempo(deck, event, cx)
                    }))
                    .child(
                        canvas(
                            move |bounds, _, _| area.set(bounds),
                            move |bounds, _, window, _| {
                                let x = bounds.origin.x + bounds.size.width / 2.;
                                let h = bounds.size.height.as_f32() - 10.;
                                let y = bounds.origin.y + px(5.);
                                window.paint_quad(fill(
                                    Bounds::new(point(x - px(1.), y), size(px(2.), px(h))),
                                    rgb(0x626b76),
                                ));
                                for i in 0..=10 {
                                    let major = i == 0 || i == 5 || i == 10;
                                    let tick_w = if !four_decks && major { 22. } else { 16. };
                                    let tick_h = if !four_decks && major { 2. } else { 1. };
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(x - px(tick_w / 2.), y + px(h * i as f32 / 10.)),
                                            size(px(tick_w), px(tick_h)),
                                        ),
                                        rgb(0x626b76),
                                    ));
                                }
                                let f = ((tempo - 1.) / range.extent() * 0.5 + 0.5).clamp(0., 1.);
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(x - px(13.), y + px(h * f as f32 - 3.)),
                                        size(px(26.), px(6.)),
                                    ),
                                    colour,
                                ));
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(
                Button::new(("tempo-reset", deck))
                    .outline()
                    .xsmall()
                    .when(!self.settings.four_decks, |button| {
                        button
                            .rounded(ButtonRounded::None)
                            .w_full()
                            .h(px(29.))
                            .p_0()
                            .bg(cx.theme().secondary.opacity(0.5))
                    })
                    .label(format!("{:+.1}%", (tempo - 1.) * 100.))
                    .tooltip("Gradually return tempo to zero adjustment.")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tempo_returns[deck] = Some((tempo, Instant::now()));
                        cx.notify();
                    })),
            )
    }
}
fn playback_anchor(
    state: &DeckState,
    action: &DeckAction,
    pending: Option<f64>,
    old: Option<(f64, Option<String>)>,
) -> Option<(f64, Option<String>)> {
    if matches!(action, DeckAction::Load { .. } | DeckAction::Unload) {
        return None;
    }
    if state.track.is_none() {
        return old;
    }
    match action {
        DeckAction::Play if !state.playing => Some((
            pending.unwrap_or(state.position_seconds),
            state.path.clone(),
        )),
        DeckAction::Seek { .. } if !state.playing => None,
        DeckAction::TriggerHotCue {
            slot, play: true, ..
        } if !state.playing => state
            .hot_cues
            .get(usize::from(*slot))?
            .map(|cue| (cue.seconds, state.path.clone()))
            .or(old),
        DeckAction::StartPadLoop { start, .. } if !state.playing => {
            Some((*start, state.path.clone()))
        }
        _ => old,
    }
}

fn pitch_ratio(fraction: f64, range: djcraft_protocol::TempoFaderRange) -> f64 {
    1. + (fraction.clamp(0., 1.) * 2. - 1.) * range.extent()
}

#[cfg(test)]
mod pitch_tests {
    #[test]
    fn transport_cue_keeps_the_play_start_through_controls_and_seeks_while_playing() {
        use super::*;
        let mut state = DeckState {
            path: Some("track".into()),
            position_seconds: 12.,
            track: Some(djcraft_protocol::TrackInfo {
                title: "track".into(),
                duration_seconds: 100.,
                loaded_seconds: 100.,
                fully_loaded: true,
            }),
            ..Default::default()
        };
        let anchor = playback_anchor(&state, &DeckAction::Play, Some(10.), None);
        assert_eq!(anchor, Some((10., state.path.clone())));
        state.playing = true;
        state.position_seconds = 45.;
        for action in [
            DeckAction::SetTempo { ratio: 1.1 },
            DeckAction::Pause,
            DeckAction::Seek { seconds: 70. },
            DeckAction::Play,
        ] {
            assert_eq!(
                playback_anchor(&state, &action, None, anchor.clone()),
                anchor
            );
        }
        state.playing = false;
        state.position_seconds = 22.;
        assert_eq!(
            playback_anchor(
                &state,
                &DeckAction::Seek { seconds: 30. },
                None,
                anchor.clone()
            ),
            None
        );
        assert_eq!(
            playback_anchor(&state, &DeckAction::Play, None, anchor.clone()),
            Some((22., state.path.clone()))
        );
        assert_eq!(
            playback_anchor(
                &state,
                &DeckAction::Load { path: "new".into() },
                None,
                anchor
            ),
            None
        );
    }
    #[test]
    fn current_key_follows_shift_and_unlocked_tempo() {
        use djcraft_library::Camelot;
        use djcraft_protocol::DeckState;
        let original = Camelot::parse("Am");
        let mut state = DeckState {
            tempo: 1.,
            key_lock: true,
            key_shift: 2.,
            ..Default::default()
        };
        assert_eq!(
            super::current_key_label(original, &state),
            "Key B minor · 10A · Shift +2 st"
        );
        state.key_shift = 0.;
        assert!(super::current_key_label(original, &state).starts_with("Key A minor · 8A"));
        state.key_lock = false;
        state.tempo = 2f64.powf(1.25 / 12.);
        assert!(super::current_key_label(original, &state).starts_with("Key B♭ minor · 3A +25¢"));
        assert_eq!(
            super::current_key_label(None, &state),
            "Key unknown · Shift +0 st"
        );
    }
    #[test]
    fn pitch_fader_has_unity_at_centre_and_preserves_full_tempo_range() {
        use djcraft_protocol::TempoFaderRange::*;
        for range in [Low, Medium, High, Wide] {
            assert_eq!(super::pitch_ratio(0., range), 1. - range.extent());
            assert_eq!(super::pitch_ratio(0.5, range), 1.);
            assert_eq!(super::pitch_ratio(1., range), 1. + range.extent());
            assert!(super::pitch_ratio(0.49, range) < 1.);
            assert!(super::pitch_ratio(0.51, range) > 1.);
        }
    }
}
