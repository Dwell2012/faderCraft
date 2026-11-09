use super::*;
use djcraft_protocol::{EFFECT_SLOTS, EffectKind as Kind, EffectState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
pub(super) const VIEWS: [&str; 5] = ["FX x3", "FX x1", "FX x3 Full", "FX x6", "Pads 16"];
const BEATS: [f32; 10] = djcraft_protocol::EFFECT_BEATS;
const BEAT_LABELS: [&str; 10] = ["1/8", "1/4", "1/2", "3/4", "1", "2", "4", "8", "16", "32"];
fn value(fx: EffectState, parameter: u8) -> f32 {
    let parameter = if fx.kind == Kind::Filter && parameter < 2 {
        1 - parameter
    } else {
        parameter
    };
    match parameter {
        0 => fx.amount,
        1 => fx.parameter,
        _ if fx.kind == Kind::Filter => fx.resonance,
        _ => fx.feedback / 0.9,
    }
}
fn set_value(fx: &mut EffectState, parameter: u8, value: f32) {
    let value = value.clamp(0., 1.);
    let parameter = if fx.kind == Kind::Filter && parameter < 2 {
        1 - parameter
    } else {
        parameter
    };
    match parameter {
        0 => fx.amount = value,
        1 => {
            fx.parameter = value;
            if fx.kind == Kind::Filter {
                fx.amount = 1.;
            }
        }
        _ if fx.kind == Kind::Filter => fx.resonance = value,
        _ => fx.feedback = value * 0.9,
    }
}
fn parameter_label(fx: EffectState, parameter: u8) -> String {
    let parameter = if fx.kind == Kind::Filter && parameter < 2 {
        1 - parameter
    } else {
        parameter
    };
    if parameter == 0 {
        return format!("Mix {:.0}%", fx.amount * 100.);
    }
    if parameter == 2 {
        if fx.kind == Kind::Filter {
            return format!("Resonance {:.0}%", fx.resonance * 100.);
        }
        return format!("Feedback {:.0}%", fx.feedback * 100.);
    }
    if fx.sync && fx.kind.timed() {
        return format!("{:.4} beats", fx.beats).replace(".0000", "");
    }
    let p = fx.parameter;
    match fx.kind {
        Kind::Filter => {
            if (p - 0.5).abs() < 0.0001 {
                "Filter 50% · Neutral".into()
            } else {
                let hz = EffectState::filter_cutoff(p);
                format!("{} {:.0} Hz", if p < 0.5 { "LP" } else { "HP" }, hz)
            }
        }
        Kind::Echo
        | Kind::Delay
        | Kind::PingPong
        | Kind::LowCutEcho
        | Kind::EchoOut
        | Kind::Spiral => format!("Time {:.0} ms", 50. + p * 950.),
        Kind::Flanger | Kind::Chorus | Kind::Phaser => format!("Rate {:.2} Hz", 0.1 + p * 1.9),
        Kind::Cut | Kind::Tremolo | Kind::AutoPan => format!("Rate {:.1} Hz", 0.1 + p * 15.9),
        Kind::Reverb => format!("Size {:.0}%", p * 100.),
        Kind::Distortion => format!("Drive {:.1}x", 1. + p * 29.),
        Kind::Bitcrusher => format!("Crush {:.0}%", p * 100.),
        Kind::Noise => format!("Colour {:.0}%", p * 100.),
        Kind::PitchShift => format!("Pitch {:+.1} st", (p - 0.5) * 24.),
        Kind::BeatRepeat => format!("Slice {:.0} ms", 30. + p * 1970.),
        Kind::Brake => format!("Stop {:.2} s", 0.2 + p * 3.8),
        Kind::ShepardToneSaw | Kind::ShepardToneTri => format!("Cycle {:.2} s", 0.05 + p * 0.95),
    }
}
fn detail_help(kind: Kind) -> &'static str {
    match kind {
        Kind::EchoOut => "Dry audio fades into repeating echoes. Use 100% Mix for a full exit.",
        Kind::Spiral => "Each feedback repeat rises by seven semitones.",
        Kind::PitchShift => "Dual-grain pitch shifter: -12 to +12 semitones. Centre is neutral.",
        Kind::BeatRepeat => {
            "Repeats the preceding slice (or fills one first). Timing changes resample the slice; toggle to recapture."
        }
        Kind::Brake => {
            "Slows the sound to a stop while the deck timeline continues. Toggle to retrigger; 100% Mix gives a complete stop."
        }
        Kind::ShepardToneSaw | Kind::ShepardToneTri => {
            "Generated Shepard tone. Beat sets the octave glide cycle; Mix sets tone level."
        }
        Kind::Noise => {
            "Filtered noise follows the incoming audio level; silent input stays silent."
        }
        Kind::Delay => "A single delayed copy. Echo adds repeating feedback.",
        Kind::Reverb => {
            "Stereo feedback-network reverb. Size changes the space; feedback controls decay."
        }
        _ => {
            "Drag dials vertically or scroll. Shift-wheel is fine; double-click resets. Slots run in numbered order."
        }
    }
}
impl Shell {
    fn effect_slot_disabled(&self, deck: usize, slot: usize) -> bool {
        slot == 0
            && self
                .controllers
                .as_ref()
                .is_some_and(|controller| controller.fx1_routes_colour(deck))
    }
    fn effect_timing_options(&self, deck: usize, beats: f32, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        Button::new(("fx-timing-options", deck))
            .xsmall()
            .ghost()
            .icon(IconName::Settings)
            .dropdown_caret(false)
            .tooltip("Effect timing options: shared beats for this deck.")
            .dropdown_menu(move |mut menu, _, _| {
                for (value, label) in BEATS.into_iter().zip(BEAT_LABELS) {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked((beats - value).abs() < 0.001)
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.send(deck, DeckAction::SetEffectBeats { beats: value });
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
    fn update_effect(
        &mut self,
        deck: usize,
        slot: usize,
        change: impl FnOnce(&mut EffectState),
        cx: &mut Context<Self>,
    ) {
        if self.effect_slot_disabled(deck, slot) {
            return;
        }
        if let Some(engine) = self.engine.as_mut() {
            let mut effect = engine.state().decks[deck].effects[slot];
            let previous_beats = effect.beats;
            change(&mut effect);
            if effect.beats != previous_beats {
                self.send(
                    deck,
                    DeckAction::SetEffectBeats {
                        beats: effect.beats,
                    },
                );
            }
            self.send(
                deck,
                DeckAction::SetEffect {
                    slot: slot as u8,
                    effect,
                },
            );
        }
        cx.notify();
    }
    pub(super) fn move_effect_control(&mut self, y: f32, cx: &mut Context<Self>) {
        if let Some((deck, slot, parameter, start, value)) = self.effect_drag {
            let has_grid = self.grids[deck].is_some();
            self.update_effect(
                deck,
                slot,
                |fx| {
                    let v = (value + (start - y) / 160.).clamp(0., 1.);
                    if parameter == 1 && fx.sync && fx.kind.timed() && has_grid {
                        fx.beats = BEATS[(v * (BEATS.len() - 1) as f32).round() as usize];
                    } else {
                        set_value(fx, parameter, v);
                    }
                },
                cx,
            );
        }
    }
    fn effect_dial(
        &self,
        deck: usize,
        slot: usize,
        parameter: u8,
        fx: EffectState,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = self.effect_slot_disabled(deck, slot);
        let accent = if disabled {
            rgb(0x727a83)
        } else {
            deck_colour(deck)
        };
        let synced = parameter == 1 && fx.sync && fx.kind.timed() && self.grids[deck].is_some();
        let v = if synced {
            BEATS
                .iter()
                .position(|b| (*b - fx.beats).abs() < 0.001)
                .unwrap_or(2) as f32
                / (BEATS.len() - 1) as f32
        } else {
            value(fx, parameter)
        };
        let knob_base = mixer_controls::smooth_knob_svg(v, 0.5, false);
        let knob_highlight = mixer_controls::smooth_knob_svg(v, 0.5, true);
        div()
            .id(("fx-dial", deck * 100 + slot * 10 + parameter as usize))
            .flex()
            .flex_col()
            .items_center()
            .w(px(if compact { 28. } else { 91. }))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(parameter_label(fx, parameter))
                    .build(window, cx)
            })
            .when(!disabled, |dial| dial.cursor_pointer())
            .child(
                div()
                    .relative()
                    .w(px(30.))
                    .h(px(30.))
                    .when(compact, |dial| dial.w(px(24.)).h(px(24.)))
                    .child(
                        gpui_kit::component::Icon::default()
                            .data(knob_base.as_bytes())
                            .text_color(rgb(0x727a83))
                            .w_full()
                            .h_full(),
                    )
                    .child(
                        div().absolute().top_0().left_0().size_full().child(
                            gpui_kit::component::Icon::default()
                                .data(knob_highlight.as_bytes())
                                .text_color(accent)
                                .w_full()
                                .h_full(),
                        ),
                    ),
            )
            .when(!compact, |dial| {
                dial.child(div().text_size(px(10.)).child(parameter_label(
                    EffectState {
                        sync: fx.sync && self.grids[deck].is_some(),
                        ..fx
                    },
                    parameter,
                )))
            })
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                if this.effect_slot_disabled(deck, slot) {
                    return;
                }
                let step = crate::wheel::steps(event);
                this.update_effect(
                    deck,
                    slot,
                    |fx| {
                        if synced {
                            let i = BEATS
                                .iter()
                                .position(|b| (*b - fx.beats).abs() < 0.001)
                                .unwrap_or(2) as i32;
                            fx.beats = BEATS[(i + if step > 0. {
                                1
                            } else if step < 0. {
                                -1
                            } else {
                                0
                            })
                            .clamp(0, (BEATS.len() - 1) as i32)
                                as usize];
                        } else {
                            set_value(fx, parameter, value(*fx, parameter) + step * 0.02);
                        }
                    },
                    cx,
                );
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.effect_slot_disabled(deck, slot) {
                        return;
                    }
                    if event.click_count == 2 {
                        this.effect_drag = None;
                        this.update_effect(
                            deck,
                            slot,
                            |fx| {
                                if synced {
                                    fx.beats = 0.5;
                                } else {
                                    let reset = 0.5;
                                    set_value(fx, parameter, reset);
                                }
                            },
                            cx,
                        );
                    } else {
                        this.effect_drag =
                            Some((deck, slot, parameter, event.position.y.as_f32(), v));
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
    fn effect_row(
        &self,
        deck: usize,
        slot: usize,
        fx: EffectState,
        dials: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.entity().downgrade();
        let accent = deck_colour(deck);
        let compact = dials == 1;
        let disabled = self.effect_slot_disabled(deck, slot);
        let mut row = div()
            .opacity(if disabled { 0.5 } else { 1. })
            .flex()
            .items_center()
            .w_full()
            .gap_1()
            .when(!compact, |row| {
                row.p_1()
                    .rounded_md()
                    .border_1()
                    .border_color(if fx.active_in_slot(slot) {
                        accent.into()
                    } else {
                        cx.theme().border
                    })
            })
            .when(compact, |row| row.h(px(30.)).gap(px(2.)));
        let selector = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(2.))
            .child(
                div().flex_1().min_w_0().child(
                    Button::new(("fx-toggle", deck * 6 + slot))
                        .disabled(disabled)
                        .small()
                        .outline()
                        .flex_1()
                        .min_w_0()
                        .when(compact, |button| {
                            button
                                .w_full()
                                .h(px(30.))
                                .p_0()
                                .rounded(ButtonRounded::None)
                                .bg(cx.theme().secondary.opacity(0.4))
                        })
                        .selected(fx.active_in_slot(slot))
                        .when(compact && fx.active_in_slot(slot), |button| {
                            button
                                .bg(Hsla::from(accent))
                                .border_color(accent)
                                .text_color(rgb(0x090b0e))
                        })
                        .tooltip("Enable or bypass this effect.")
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .justify_center()
                                .gap(px(3.))
                                .min_w_0()
                                .child(if compact {
                                    fx.kind.label().to_owned()
                                } else {
                                    format!("{} · {}", slot + 1, fx.kind.label())
                                })
                                .when(
                                    fx.kind.timed() && fx.sync && self.grids[deck].is_some(),
                                    |label| {
                                        let beats = BEATS
                                            .iter()
                                            .position(|b| (*b - fx.beats).abs() < 0.001)
                                            .map(|i| BEAT_LABELS[i].to_owned())
                                            .unwrap_or_else(|| format!("{}", fx.beats));
                                        label.child(
                                            div()
                                                .text_size(px(9.))
                                                .text_color(if compact && fx.active_in_slot(slot) {
                                                    rgb(0x343a40)
                                                } else {
                                                    rgb(0x777e88)
                                                })
                                                .child(format!(
                                                    "({beats} {})",
                                                    if fx.beats <= 1. { "beat" } else { "beats" }
                                                )),
                                        )
                                    },
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.update_effect(deck, slot, |fx| fx.enabled = !fx.enabled, cx);
                        })),
                ),
            )
            .child(
                div().min_w_0().flex_shrink_0().child(
                    Button::new(("fx-select", deck * 6 + slot))
                        .disabled(disabled)
                        .tooltip("Choose the effect for this slot.")
                        .xsmall()
                        .outline()
                        .label("▾")
                        .when(compact, |button| {
                            button
                                .w(px(28.))
                                .min_w_0()
                                .h(px(30.))
                                .px_1()
                                .rounded(ButtonRounded::None)
                                .bg(cx.theme().secondary.opacity(0.4))
                        })
                        .dropdown_menu(move |mut menu, _, _| {
                            for kind in Kind::ALL {
                                let weak = weak.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(kind.label())
                                        .checked(kind == fx.kind)
                                        .on_click(move |_, _, cx| {
                                            let _ = weak.update(cx, |this, cx| {
                                                this.update_effect(
                                                    deck,
                                                    slot,
                                                    |fx| {
                                                        *fx = EffectState {
                                                            kind,
                                                            enabled: fx.enabled,
                                                            beats: fx.beats,
                                                            ..EffectState::default()
                                                        };
                                                    },
                                                    cx,
                                                )
                                            });
                                        }),
                                );
                            }
                            menu
                        }),
                ),
            )
            .when(dials == 0 || compact, |selector| {
                selector.child(
                    Button::new(("fx-details", deck * 6 + slot))
                        .disabled(disabled)
                        .tooltip("Show or hide this effect’s detailed controls.")
                        .xsmall()
                        .outline()
                        .when(compact, |button| {
                            button
                                .rounded(ButtonRounded::None)
                                .w(px(28.))
                                .h(px(30.))
                                .p_0()
                                .bg(cx.theme().secondary.opacity(0.4))
                        })
                        .label("…")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.effect_details[deck] = if this.effect_details[deck] == Some(slot) {
                                None
                            } else {
                                Some(slot)
                            };
                            cx.notify();
                        })),
                )
            });
        row = row.child(selector);
        if compact {
            return row
                .child(
                    div()
                        .w(px(36.))
                        .h(px(30.))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().secondary.opacity(0.4))
                        .child(self.effect_dial(deck, slot, 0, fx, true, cx)),
                )
                .into_any_element();
        }
        let dials = if dials == 0 {
            0
        } else if fx.kind == Kind::Filter || (fx.kind.feedback_control() && fx.kind != Kind::Delay)
        {
            3
        } else {
            2
        };
        let mut parameters = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .justify_around()
            .gap_2()
            .flex_wrap();
        for parameter in 0..dials {
            if parameter == 1 && fx.kind.timed() && fx.sync {
                continue;
            }
            parameters =
                parameters.child(self.effect_dial(deck, slot, parameter as u8, fx, false, cx));
        }
        if dials > 0 {
            let mut options = div().flex().flex_col().gap_1();
            if fx.kind.timed() {
                options = options.child(Button::new(("fx-row-sync", deck * 6 + slot))
                    .xsmall().outline().label("Beat sync").selected(fx.sync)
                    .tooltip("Sync effect timing to the track tempo using this deck's shared FX beats.")
                    .disabled(self.grids[deck].is_none() && !fx.sync)
                    .on_click(cx.listener(move |this, _, _, cx| this.update_effect(deck, slot, |fx| fx.sync = !fx.sync, cx))));
            }
            if fx.kind.has_tail() {
                options = options.child(
                    Button::new(("fx-row-tails", deck * 6 + slot))
                        .xsmall()
                        .outline()
                        .label("Tails")
                        .selected(fx.tails)
                        .tooltip("Let the effect tail ring out after bypassing it.")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.update_effect(deck, slot, |fx| fx.tails = !fx.tails, cx)
                        })),
                );
            }
            parameters = parameters.child(options);
            row = row.child(parameters);
        }
        row.into_any_element()
    }
    pub(super) fn effect_panel(
        &self,
        deck: usize,
        state: &DeckState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = self.effect_view[deck];
        if view == 4 {
            return self.pad_panel(deck, state, true, cx).into_any_element();
        }
        let selected = self.effect_selected[deck];
        let weak = cx.entity().downgrade();
        let mut panel = div()
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .when(view == 0, |panel| {
                panel.h_full().justify_between().gap(px(4.))
            })
            .when(view != 0, |panel| {
                panel
                    .p_2()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded_md()
            });
        let mut header = div()
            .flex()
            .items_center()
            .gap_1()
            .flex_wrap()
            .when(self.settings.four_decks, |header| {
                header.child(
                    Button::new(("fx-cycle", deck))
                        .tooltip("Cycle through effect layouts.")
                        .xsmall()
                        .outline()
                        .label(VIEWS[view])
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.effect_view[deck] = (this.effect_view[deck] + 1) % VIEWS.len();
                            cx.notify();
                        })),
                )
            })
            .when(self.settings.four_decks, |header| {
                header.child(
                    Button::new(("fx-view", deck))
                        .tooltip("Choose the effect layout.")
                        .xsmall()
                        .outline()
                        .label("▾")
                        .dropdown_menu(move |mut menu, _, _| {
                            for (i, name) in VIEWS.into_iter().enumerate() {
                                let weak = weak.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(name).checked(i == view).on_click(
                                        move |_, _, cx| {
                                            let _ = weak.update(cx, |this, cx| {
                                                this.effect_view[deck] = i;
                                                cx.notify();
                                            });
                                        },
                                    ),
                                );
                            }
                            menu
                        }),
                )
            })
            .child(div().text_size(px(10.)).child(format!(
                    "PRE-FADER · {} active",
                    state
                        .effects
                        .iter()
                        .enumerate()
                        .filter(|(slot, fx)| fx.active_in_slot(*slot))
                        .count()
                )));
        if view == 1 {
            for slot in 0..EFFECT_SLOTS {
                header = header.child(
                    Button::new(("fx-focus", deck * 6 + slot))
                        .disabled(self.effect_slot_disabled(deck, slot))
                        .tooltip("Select this effect slot.")
                        .xsmall()
                        .outline()
                        .label(format!(
                            "{}{}",
                            slot + 1,
                            if slot == selected { "•" } else { "" }
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.effect_selected[deck] = slot;
                            cx.notify();
                        })),
                );
            }
        }
        if self.settings.four_decks {
            header = header.child(self.effect_timing_options(deck, state.effect_beats, cx));
        }
        if view != 0 && !self.settings.four_decks {
            panel = panel.child(header);
        }
        match view {
            1 => {
                panel =
                    panel.child(self.effect_row(deck, selected, state.effects[selected], 2, cx));
            }
            3 => {
                for pair in 0..3 {
                    let mut row = div().flex().gap_1();
                    for slot in pair * 2..pair * 2 + 2 {
                        row = row.child(div().flex_1().min_w_0().child(self.effect_row(
                            deck,
                            slot,
                            state.effects[slot],
                            0,
                            cx,
                        )));
                    }
                    panel = panel.child(row);
                }
            }
            _ => {
                for slot in 0..3 {
                    panel = panel.child(self.effect_row(
                        deck,
                        slot,
                        state.effects[slot],
                        if view == 2 { 2 } else { 1 },
                        cx,
                    ));
                }
            }
        }
        panel.into_any_element()
    }
    pub(super) fn effect_detail_panel(
        &self,
        deck: usize,
        state: &DeckState,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let slot = self.effect_details[deck]?;
        if self.effect_slot_disabled(deck, slot) {
            return None;
        }
        let fx = state.effects[slot];
        let mut details = div()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(format!(
                        "Deck {} · {} · {}",
                        deck_label(deck),
                        slot + 1,
                        fx.kind.label()
                    ))
                    .child(
                        Button::new(("fx-close", deck))
                            .tooltip("Close the effect detail panel.")
                            .xsmall()
                            .outline()
                            .label("×")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.effect_details[deck] = None;
                                cx.notify();
                            })),
                    ),
            );
        let mut controls = div().flex().items_center().gap_1().flex_wrap();
        // Separate IDs for the expanded instance of a dial.
        controls = controls.child(
            div()
                .id(("fx-expanded", deck))
                .flex()
                .flex_wrap()
                .child(self.effect_dial(deck, slot, 0, fx, false, cx))
                .when(!(fx.kind.timed() && fx.sync), |row| {
                    row.child(self.effect_dial(deck, slot, 1, fx, false, cx))
                })
                .when(
                    fx.kind == Kind::Filter
                        || (fx.kind.feedback_control() && fx.kind != Kind::Delay),
                    |row| row.child(self.effect_dial(deck, slot, 2, fx, false, cx)),
                ),
        );
        details = details.child(controls);
        let mut options = div().flex().gap_1().items_center().flex_wrap();
        if fx.kind.timed() {
            let bpm = self.grids[deck].map(|g| g.bpm * state.tempo);
            options = options.child(
                Button::new(("fx-sync", deck))
                    .tooltip("Toggle effect timing synced to the track tempo.")
                    .xsmall()
                    .outline()
                    .label(if fx.sync {
                        "Beat sync ON"
                    } else {
                        "Beat sync OFF"
                    })
                    .disabled(bpm.is_none() && !fx.sync)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update_effect(deck, slot, |fx| fx.sync = !fx.sync, cx)
                    })),
            );
            if fx.sync {
                options = options.child(
                    div()
                        .text_size(px(10.))
                        .child(format!("Shared FX beats: {}", state.effect_beats)),
                );
            }
            options = options.child(div().text_size(px(10.)).child(
                bpm.map_or("No beat grid · manual timing".into(), |bpm| {
                    format!("{bpm:.1} BPM")
                }),
            ));
        }
        if fx.kind.has_tail() {
            options = options.child(
                Button::new(("fx-tails", deck))
                    .tooltip("Allow the effect tail to ring out after switching it off.")
                    .xsmall()
                    .outline()
                    .label(if fx.tails { "Tails ON" } else { "Tails OFF" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update_effect(deck, slot, |fx| fx.tails = !fx.tails, cx)
                    })),
            );
        }
        details = details
            .child(options)
            .child(div().text_size(px(10.)).child(detail_help(fx.kind)));
        Some(details.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{EffectState, parameter_label, set_value, value};
    #[test]
    fn resonance_dial_changes_only_resonance_and_clamps() {
        let mut fx = EffectState::default();
        let before = fx;
        set_value(&mut fx, 2, 0.75);
        assert_eq!(value(fx, 2), 0.75);
        assert_eq!(parameter_label(fx, 2), "Resonance 75%");
        assert_eq!(
            fx,
            EffectState {
                resonance: 0.75,
                ..before
            }
        );
        set_value(&mut fx, 2, 2.);
        assert_eq!(fx.resonance, 1.);
        set_value(&mut fx, 2, -1.);
        assert_eq!(
            fx,
            EffectState {
                resonance: 0.,
                ..before
            }
        );
    }
}
