use super::*;
use djcraft_protocol::script::{ControlSource, EqBand, Param, Program};
use gpui_kit::component::tooltip::Tooltip;

pub(super) fn smooth_knob_svg(value: f32, neutral: f32, highlight: bool) -> String {
    let value = (value.clamp(0., 1.) * 1024.).round() / 1024.;
    let angle = (value - 0.5) * std::f32::consts::PI * 1.5;
    let at = |radius: f32| (23. + angle.sin() * radius, 23. - angle.cos() * radius);
    let (x1, y1) = at(3.);
    let (x2, y2) = at(22.);
    let (ax, ay) = at(20.);
    let neutral_angle = (neutral - 0.5) * std::f32::consts::PI * 1.5;
    let nx = 23. + neutral_angle.sin() * 20.;
    let ny = 23. - neutral_angle.cos() * 20.;
    let colour = "#ffffff";
    let progress = if (value - neutral).abs() > 0.001 {
        format!(
            r##"<circle cx="23" cy="23" r="14.3" fill="{colour}" fill-opacity="0.2"/><path d="M{nx:.3} {ny:.3} A20 20 0 {} {} {ax:.3} {ay:.3}" fill="none" stroke="{colour}" stroke-width="2.2" stroke-linecap="round"/>"##,
            if (angle - neutral_angle).abs() > std::f32::consts::PI {
                1
            } else {
                0
            },
            if value > neutral { 1 } else { 0 }
        )
    } else {
        String::new()
    };
    let body = if highlight {
        format!(
            r##"{progress}<path d="M{x1:.3} {y1:.3} L{x2:.3} {y2:.3}" stroke="#ffffff" stroke-width="2" stroke-linecap="round"/>"##
        )
    } else {
        r##"<path d="M8.858 37.142 A20 20 0 1 1 37.142 37.142" fill="none" stroke="#ffffff" stroke-opacity="0.6" stroke-width="1.6" stroke-linecap="round"/><circle cx="23" cy="23" r="15" fill="none" stroke="#ffffff" stroke-width="1.6"/>"##.to_owned()
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="46" height="46" viewBox="0 0 46 46" shape-rendering="geometricPrecision">{body}</svg>"##
    )
}

// Preserve A/B and bus control IDs; C/D append their own independent controls.
pub(super) const CONTROL_COUNT: usize = 46;
pub(super) fn channel_index(deck: usize) -> usize {
    if deck < 2 {
        deck * 5
    } else {
        23 + (deck - 2) * 5
    }
}
pub(super) fn stem_index(deck: usize) -> usize {
    if deck < 2 {
        12 + deck * 4
    } else {
        33 + (deck - 2) * 4
    }
}
fn control_deck(index: usize) -> Option<usize> {
    match index {
        0..=9 => Some(index / 5),
        12..=19 => Some((index - 12) / 4),
        23..=32 => Some(2 + (index - 23) / 5),
        33..=40 => Some(2 + (index - 33) / 4),
        41..=44 => Some(index - 41),
        _ => None,
    }
}
fn is_stem(index: usize) -> bool {
    (12..20).contains(&index) || (33..41).contains(&index)
}
fn is_knob(index: usize) -> bool {
    if let Some(deck) = control_deck(index) {
        index >= 41 || is_stem(index) || index - channel_index(deck) < 4
    } else {
        index >= 20
    }
}
fn opposite_deck_control(index: usize) -> Option<usize> {
    if index >= 41 {
        return None;
    }
    let deck = control_deck(index)?;
    if is_stem(index) {
        Some(stem_index(deck ^ 1) + index - stem_index(deck))
    } else {
        let offset = index - channel_index(deck);
        (offset != 0).then(|| channel_index(deck ^ 1) + offset)
    }
}
pub(super) fn initial_values() -> [f32; CONTROL_COUNT] {
    let mut values = [0.5; CONTROL_COUNT];
    for deck in 0..DECKS {
        values[channel_index(deck) + 4] = 1.;
    }
    values[10] = 1.;
    values[20] = 1.;
    values[21] = std::f32::consts::FRAC_1_SQRT_2;
    values[45] = std::f32::consts::FRAC_1_SQRT_2;
    values[22] = 0.;
    values
}
fn control_action(index: usize, value: f32) -> Action {
    if index == 10 {
        return Program::set_mixer_param(Param::MasterGain)
            .unwrap()
            .run(ControlSource::Skin, value)
            .unwrap()
            .action;
    }
    if index == 11 {
        return Program::set_mixer_param(Param::Crossfader)
            .unwrap()
            .run(ControlSource::Skin, value)
            .unwrap()
            .action;
    }
    let deck = control_deck(index).expect("deck control");
    let offset = index - channel_index(deck);
    let param = match offset {
        0 => Param::Trim,
        1 => Param::Eq(EqBand::High),
        2 => Param::Eq(EqBand::Mid),
        3 => Param::Eq(EqBand::Low),
        4 => Param::Volume,
        _ => unreachable!("non-channel control"),
    };
    let normalized = if param.domain() == djcraft_protocol::script::ParamDomain::Bipolar {
        value * 2.0 - 1.0
    } else {
        value
    };
    Program::set_param(deck, param)
        .unwrap()
        .run(ControlSource::Skin, normalized)
        .unwrap()
        .action
}
fn value_label(index: usize, value: f32) -> String {
    if index == 45 {
        return if value <= 0.001 {
            "−∞".into()
        } else {
            let db = 20. * (value * value).log10();
            if db.abs() < 0.05 {
                "0".into()
            } else {
                format!("{db:+.1}")
            }
        };
    }
    if index >= 41 {
        return if (value - 0.5).abs() < 0.0001 {
            "Off".into()
        } else {
            format!(
                "{} {:.0}%",
                if value < 0.5 { "LP" } else { "HP" },
                (value - 0.5).abs() * 200.
            )
        };
    }
    if index == 22 {
        let [cue, master] = djcraft_engine::cue_mix_gains(value);
        return format!("{:.0}/{:.0}", cue * 100., master * 100.);
    }
    if index == 20 || index == 21 {
        return if value <= 0.001 {
            "−∞".into()
        } else {
            let db = 20. * (value * value).log10();
            if db.abs() < 0.05 {
                "0".into()
            } else {
                format!("{db:+.1}")
            }
        };
    }
    if index == 11 {
        return format!("L  {:3.0}%  R", value * 100.);
    }
    if control_deck(index).is_some_and(|deck| index == channel_index(deck)) {
        let db = value * 24. - 12.;
        return if db.abs() < 0.05 {
            "0".into()
        } else {
            format!("{db:+.1}")
        };
    }
    if value <= 0.001 {
        return if is_knob(index) { "KILL" } else { "−∞" }.into();
    }
    let gain = if is_knob(index) {
        if value <= 0.5 {
            value * 2.
        } else {
            10f32.powf((value - 0.5) * 12. / 20.)
        }
    } else {
        value * value
    };
    let db = 20. * gain.log10();
    if db.abs() < 0.05 {
        "0".into()
    } else {
        format!("{db:+.1}")
    }
}

impl Shell {
    fn colour_fx_control(&self, deck: usize, cx: &mut Context<Self>) -> Div {
        let weak = cx.weak_entity();
        let selector = gpui_kit::component::popover::Popover::new(("colour-fx-select", deck))
            .overlay_closable(true)
            .trigger(
                Button::new(("colour-fx-arrow", deck))
                    .ghost()
                    .xsmall()
                    .w(px(14.))
                    .h(px(14.))
                    .p_0()
                    .label("▾")
                    .tooltip("Choose one or more Colour FX"),
            )
            .content(move |_, _, cx| {
                let selected = weak
                    .update(cx, |shell, _| {
                        shell
                            .engine
                            .as_mut()
                            .map(|engine| engine.state().decks[deck].colour_fx.selected)
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);
                div().flex().flex_col().gap_2().w(px(190.)).p_2().children(
                    djcraft_protocol::COLOUR_FX_LABELS
                        .into_iter()
                        .enumerate()
                        .map(|(index, label)| {
                            let weak = weak.clone();
                            gpui_kit::component::checkbox::Checkbox::new((
                                "colour-fx-choice",
                                deck * 7 + index,
                            ))
                            .label(label)
                            .checked(selected & (1 << index) != 0)
                            .on_click(cx.listener(
                                move |_, checked: &bool, _, cx| {
                                    let checked = *checked;
                                    let _ = weak.update(cx, |shell, cx| {
                                        if let Some(engine) = &mut shell.engine {
                                            let mut state = engine.state().decks[deck].colour_fx;
                                            if checked {
                                                state.selected |= 1 << index;
                                            } else {
                                                state.selected &= !(1 << index);
                                            }
                                            shell.send(deck, DeckAction::SetColourFx { state });
                                        }
                                        cx.notify();
                                    });
                                    cx.notify();
                                },
                            ))
                        }),
                )
            });
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(14.))
                    .child(
                        div()
                            .w_full()
                            .h(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(10.))
                            .child("CFX"),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right(px(-4.))
                            .w(px(14.))
                            .h(px(14.))
                            .child(selector),
                    ),
            )
            .child(self.mixer_control(41 + deck, "", cx))
    }
    pub(super) fn master_header_meters(&self) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().text_size(px(9.)).child("MASTER"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children((0..2).map(|channel| {
                        let index = DECKS + channel;
                        let peak = self.mixer_peaks[index];
                        let level = ((20. * peak.max(0.00001).log10() + 48.) / 48.).clamp(0., 1.);
                        let clip_lit = self.mixer_clip_until[index]
                            .is_some_and(|until| until > Instant::now());
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .child(div().text_size(px(8.)).child(if channel == 0 {
                                "L"
                            } else {
                                "R"
                            }))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |bounds, _, window, _| {
                                        let step = (bounds.size.width.as_f32() - 12.) / 24.;
                                        for segment in 0..24 {
                                            let color = if segment as f32 / 24. >= level {
                                                rgb(0x252b33)
                                            } else if segment >= 21 {
                                                rgb(0xf15b58)
                                            } else if segment >= 17 {
                                                rgb(0xe8c65c)
                                            } else {
                                                rgb(0xc8ff38)
                                            };
                                            window.paint_quad(fill(
                                                Bounds::new(
                                                    bounds.origin
                                                        + point(px(segment as f32 * step), px(0.)),
                                                    size(px((step - 1.).max(1.)), px(5.)),
                                                ),
                                                color,
                                            ));
                                        }
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                bounds.origin
                                                    + point(bounds.size.width - px(6.), px(0.)),
                                                size(px(6.), px(5.)),
                                            ),
                                            if clip_lit {
                                                rgb(0xff5555)
                                            } else {
                                                rgb(0x3d2527)
                                            },
                                        ));
                                    },
                                )
                                .w(px(110.))
                                .h(px(5.)),
                            )
                    })),
            )
    }

    fn mixer_tabs(&self, cx: &mut Context<Self>) -> Div {
        div().flex().w_full().h(px(29.)).flex_shrink_0().children(
            ["MIXER", "OUTPUT"]
                .into_iter()
                .enumerate()
                .map(|(tab, label)| {
                    div()
                        .id(("mixer-tab", tab))
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .h(px(29.))
                        .items_center()
                        .justify_center()
                        .text_size(px(11.))
                        .when(tab == 0, |tab| tab.border_r_1())
                        .border_color(cx.theme().border)
                        .when(self.mixer_tab == tab, |tab| tab.bg(cx.theme().background))
                        .when(self.mixer_tab != tab, |tab| {
                            tab.bg(cx.theme().secondary.opacity(0.5)).border_b_1()
                        })
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.mixer_tab = tab;
                            cx.notify();
                        }))
                }),
        )
    }

    fn mixer_output_panel(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .w_full()
                    .h(px(184.))
                    .flex_shrink_0()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .items_center()
                                    .gap(px(4.))
                                    .p(px(4.))
                                    .bg(cx.theme().secondary.opacity(0.5))
                                    .child(div().text_size(px(10.)).child("MASTER"))
                                    .child(self.mixer_control(10, "VOL", cx)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .items_center()
                                    .gap(px(4.))
                                    .p(px(4.))
                                    .bg(cx.theme().secondary.opacity(0.5))
                                    .child(div().text_size(px(10.)).child("SAMPLER"))
                                    .child(self.mixer_control(20, "VOL", cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .items_center()
                            .gap(px(4.))
                            .p(px(4.))
                            .bg(cx.theme().secondary.opacity(0.5))
                            .child(div().text_size(px(10.)).child("HEADPHONES"))
                            .child(self.mixer_control(21, "VOL", cx))
                            .child(div().flex_1())
                            .child(self.mixer_control(22, "MIX", cx))
                            .child(
                                div()
                                    .text_size(px(8.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child("CUE / MASTER"),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .justify_center()
                    .gap(px(4.))
                    .p(px(4.))
                    .bg(cx.theme().secondary.opacity(0.5))
                    .child(div().w_full().text_center().text_size(px(10.)).child("MIC"))
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_center()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .id("mic-toggle")
                                    .flex()
                                    .h(px(29.))
                                    .items_center()
                                    .justify_center()
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .when(self.mic_enabled, |button| button.bg(rgb(0x1a4b34)))
                                    .when(!self.mic_enabled, |button| {
                                        button.bg(cx.theme().background)
                                    })
                                    .text_size(px(10.))
                                    .child(if self.mic_enabled { "ON" } else { "OFF" })
                                    .tooltip(|window, cx| {
                                        Tooltip::new("Microphone input is not yet routed to the audio engine.").build(window, cx)
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mic_enabled = !this.mic_enabled;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .justify_center()
                                    .child(self.mixer_control(45, "VOL", cx)),
                            ),
                    ),
            )
    }

    fn crossfader_selectors(&self, right: bool, cx: &mut Context<Self>) -> Div {
        let mask = if right {
            self.crossfader_routing.right
        } else {
            self.crossfader_routing.left
        };
        div().flex().flex_col().gap(px(4.)).children(
            (0..2)
                .map(|row| {
                    div().flex().gap(px(4.)).children(
                        (0..2)
                            .map(|col| {
                                let deck = row * 2 + col;
                                Button::new((
                                    if right { "cross-right" } else { "cross-left" },
                                    deck,
                                ))
                                .xsmall()
                                .outline()
                                .rounded(ButtonRounded::None)
                                .w(px(29.))
                                .h(px(29.))
                                .p_0()
                                .label(deck_label(deck))
                                .selected(mask & (1 << deck) != 0)
                                .text_color(cx.theme().muted_foreground)
                                .when(mask & (1 << deck) != 0, |button| {
                                    button
                                        .bg(Hsla::from(deck_colour(deck)))
                                        .border_color(deck_colour(deck))
                                        .text_color(rgb(0x090b0e))
                                })
                                .tooltip(if right {
                                    "Toggle this deck on the right crossfader side."
                                } else {
                                    "Toggle this deck on the left crossfader side."
                                })
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let mut routing = this.crossfader_routing;
                                        if right {
                                            routing.right ^= 1 << deck;
                                        } else {
                                            routing.left ^= 1 << deck;
                                        }
                                        if let Some(engine) = &mut this.engine
                                            && engine
                                                .apply(
                                                    Role::Host,
                                                    Action::Mixer {
                                                        action: MixerAction::SetCrossfaderRouting {
                                                            routing,
                                                        },
                                                    },
                                                )
                                                .is_ok()
                                        {
                                            this.crossfader_routing = routing;
                                            this.settings.crossfader_routing = routing;
                                            if let Err(error) =
                                                this.settings.save(&crate::storage::machine_dir())
                                            {
                                                eprintln!(
                                                    "Could not save crossfader routing: {error}"
                                                );
                                            }
                                            cx.set_global(this.settings.clone());
                                        }
                                        cx.notify();
                                    },
                                ))
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    fn set_mixer_control(&mut self, index: usize, value: f32, cx: &mut Context<Self>) {
        let value = value.clamp(0., 1.);
        if index == 45 {
            if (self.mixer_values[index] - value).abs() > 0.0001 {
                self.mixer_last_change[index] = Some(Instant::now());
            }
            self.mixer_values[index] = value;
            cx.notify();
            return;
        }
        let action = if (20..=22).contains(&index) {
            let param = match index {
                20 => Param::SamplerGain,
                21 => Param::HeadphoneGain,
                22 => Param::HeadphoneMix,
                _ => unreachable!(),
            };
            let Some(state) = self.engine.as_mut().map(|engine| engine.state()) else {
                return;
            };
            Program::set_mixer_param(param)
                .unwrap()
                .run_with_state(ControlSource::Skin, value, Some(&state))
                .unwrap()
                .action
        } else if index >= 41 {
            let deck = index - 41;
            let Some(state) = self.engine.as_mut().map(|e| e.state()) else {
                return;
            };
            Program::set_param(deck, Param::ColourFx)
                .unwrap()
                .run_with_state(ControlSource::Skin, value * 2.0 - 1.0, Some(&state))
                .unwrap()
                .action
        } else if is_stem(index) {
            let deck = control_deck(index).expect("stem control");
            let current = self
                .engine
                .as_mut()
                .map(|engine| engine.state().decks[deck].stem_gains)
                .unwrap_or([1.; 4]);
            Action::Deck {
                deck: deck as u8,
                action: DeckAction::SetStemGains {
                    gains: changed_stem_group(
                        self.deck_eq_modes[deck],
                        index - stem_index(deck),
                        value,
                        current,
                    ),
                },
            }
        } else {
            control_action(index, value)
        };
        if self.apply_control_dispatch(
            djcraft_protocol::script::Dispatch {
                source: ControlSource::Skin,
                action,
            },
            cx,
        ) {
            if (self.mixer_values[index] - value).abs() > 0.0001 {
                self.mixer_last_change[index] = Some(Instant::now());
            }
            self.mixer_values[index] = value;
        }
        cx.notify();
    }
    pub(super) fn move_mixer_control(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((index, start_y, start_value)) = self.mixer_drag else {
            return;
        };
        let area = self.mixer_areas[index].get();
        let four_deck_volume = self.settings.four_decks
            && control_deck(index).is_some_and(|deck| index == channel_index(deck) + 4);
        let value = if is_knob(index) || index == 10 || four_deck_volume {
            start_value + (start_y - position.y.as_f32()) / 140.
        } else if index == 11 {
            (position.x - area.origin.x - px(8.)).as_f32()
                / (area.size.width.as_f32() - 16.).max(1.)
        } else {
            1. - (position.y - area.origin.y - px(8.)).as_f32()
                / (area.size.height.as_f32() - 16.).max(1.)
        };
        self.set_mixer_control(index, value, cx);
    }
    fn mixer_control(
        &self,
        index: usize,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = self.mixer_values[index];
        let area = self.mixer_areas[index].clone();
        let four_deck_volume = self.settings.four_decks
            && control_deck(index).is_some_and(|deck| index == channel_index(deck) + 4);
        let knob = is_knob(index) || index == 10 || four_deck_volume;
        let enabled = !is_stem(index)
            || self
                .engine
                .as_ref()
                .and_then(|e| e.track(control_deck(index).unwrap()))
                .is_some_and(|t| t.stems_ready());
        let horizontal = index == 11;
        let four_decks = self.settings.four_decks;
        let accent = control_deck(index)
            .map(deck_colour)
            .unwrap_or_else(|| rgb(0x79d9aa));
        let drawing = canvas(
            move |bounds, _, _| area.set(bounds),
            move |bounds, _, window, _| {
                let w = bounds.size.width.as_f32();
                let h = bounds.size.height.as_f32();
                let mut rect = |x: f32, y: f32, width: f32, height: f32, color: Rgba| {
                    window.paint_quad(fill(
                        Bounds::new(
                            bounds.origin + point(px(x), px(y)),
                            size(px(width), px(height)),
                        ),
                        color,
                    ));
                };
                if knob {
                    // SVG layers render knobs; this canvas retains interaction bounds.
                } else if horizontal {
                    rect(8., h / 2. - 2., w - 16., 4., rgb(0x303640));
                    if four_decks {
                        rect(w / 2. - 1., 3., 2., h - 6., rgb(0x747b85));
                    } else {
                        for step in 0..=10 {
                            let major = step == 0 || step == 5 || step == 10;
                            let tick_h = if major { 16. } else { 10. };
                            let tick_w = if major { 2. } else { 1. };
                            rect(
                                8. + step as f32 * (w - 16.) / 10. - tick_w / 2.,
                                h / 2. - tick_h / 2.,
                                tick_w,
                                tick_h,
                                rgb(0x747b85),
                            );
                        }
                    }
                    rect(8. + value * (w - 16.) - 5., 2., 10., h - 4., accent);
                } else {
                    rect(w / 2. - 2., 8., 4., h - 16., rgb(0x303640));
                    let tick_count = if four_decks { 4 } else { 10 };
                    for step in 0..=tick_count {
                        let major = !four_decks && (step == 0 || step == 5 || step == 10);
                        let tick_w = if major { 28. } else { 20. };
                        let tick_h = if major { 2. } else { 1. };
                        rect(
                            w / 2. - tick_w / 2.,
                            8. + step as f32 * (h - 16.) / tick_count as f32,
                            tick_w,
                            tick_h,
                            rgb(0x4c535d),
                        );
                    }
                    rect(
                        w / 2. - 12.,
                        8. + (1. - value) * (h - 16.) - 4.,
                        24.,
                        8.,
                        accent,
                    );
                }
            },
        )
        .w_full()
        .when(knob, |this| this.h(px(if four_decks { 22. } else { 46. })))
        .when(horizontal, |this| this.h(px(22.)))
        .when(!knob && !horizontal, |this| {
            this.flex_1()
                .min_h(px(if self.settings.four_decks { 48. } else { 80. }))
        });
        let drawing = if knob {
            let neutral = match index {
                10 | 20 => 1.,
                21 | 45 => std::f32::consts::FRAC_1_SQRT_2,
                22 => 0.,
                _ => 0.5,
            };
            let base = smooth_knob_svg(value, neutral, false);
            let highlight = smooth_knob_svg(value, neutral, true);
            let diameter = if four_decks { 22. } else { 46. };
            div()
                .relative()
                .w_full()
                .h(px(diameter))
                .child(drawing)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .relative()
                                .w(px(diameter))
                                .h(px(diameter))
                                .child(
                                    gpui_kit::component::Icon::default()
                                        .data(base.as_bytes())
                                        .text_color(rgb(0x727a83))
                                        .size_full(),
                                )
                                .child(
                                    div().absolute().top_0().left_0().size_full().child(
                                        gpui_kit::component::Icon::default()
                                            .data(highlight.as_bytes())
                                            .text_color(accent)
                                            .size_full(),
                                    ),
                                ),
                        ),
                )
                .into_any_element()
        } else {
            drawing.into_any_element()
        };
        div()
            .id(("mixer-control", index))
            .when(opposite_deck_control(index).is_some(), |control| {
                control.tooltip(move |window, cx| {
                    Tooltip::new(if control_deck(index).is_some_and(|d| index == channel_index(d) + 4) {
                        "Scroll to adjust. Ctrl-scroll moves the other deck's volume fader in the opposite direction."
                    } else {
                        "Scroll to adjust. Ctrl-scroll moves the other deck's matching EQ in the opposite direction."
                    }).build(window, cx)
                })
            })
            .when((20..=22).contains(&index), |control| control.tooltip(move |window, cx| {
                Tooltip::new(match index {
                    20 => "Sampler bus volume; individual pad levels remain relative. Double-click resets to unity.",
                    21 => "Headphone volume only. Double-click resets to 50% gain.",
                    _ => "Cue / master blend: centre is both at full volume; either end fades out the opposite source. Double-click resets to cue only.",
                }).build(window, cx)
            }))
            .when((41..=44).contains(&index), |control| control.tooltip(|window, cx| Tooltip::new("Colour FX macro: use the arrow to combine effects and noise generators. Centre is neutral; double-click to centre. Filter sweeps low-pass left and high-pass right.").build(window, cx)))
            .when(index == 45, |control| control.tooltip(|window, cx| Tooltip::new("Microphone input is not yet routed to the audio engine.").build(window, cx)))
            .when(!enabled, |this| this.opacity(0.35))
            .when(!knob && !horizontal, |this| this.h_full().min_h_0())
            .flex()
            .flex_col()
            .items_center()
            .w_full()
            .when(!label.is_empty(), |control| control.child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(10.))
                    .when(self.settings.four_decks || !matches!(index, 4 | 9 | 11), |label_box| {
                        let show_value = self.mixer_last_change[index].is_some_and(|at| at.elapsed().as_secs_f32() < 2.);
                        label_box.child(if show_value { value_label(index, value) } else { label.to_owned() })
                    }),
            ))
            .child(drawing)
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                if !enabled {
                    return;
                }
                let before = this.mixer_values[index];
                let value = (before + crate::wheel::steps(event) * 0.02).clamp(0., 1.);
                this.set_mixer_control(index, value, cx);
                if let Some(other) = opposite_deck_control(index).filter(|_| event.modifiers.control) {
                    let delta = this.mixer_values[index] - before;
                    this.set_mixer_control(other, this.mixer_values[other] - delta, cx);
                }
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if !enabled {
                        return;
                    }
                    if event.click_count == 2 {
                        this.set_mixer_control(
                            index,
                            if index == 20 || index == 10 {
                                1.
                            } else if index == 21 || index == 45 {
                                std::f32::consts::FRAC_1_SQRT_2
                            } else if index == 22 {
                                0.
                            } else if four_deck_volume {
                                1.
                            } else if is_knob(index) || index == 11 {
                                0.5
                            } else {
                                1.
                            },
                            cx,
                        );
                        this.mixer_drag = None;
                    } else {
                        this.mixer_drag =
                            Some((index, event.position.y.as_f32(), this.mixer_values[index]));
                        if !knob {
                            this.move_mixer_control(event.position, cx);
                        }
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
    fn meter(&self, index: usize) -> AnyElement {
        let peak = self.mixer_peaks[index];
        let level = ((20. * peak.max(0.00001).log10() + 48.) / 48.).clamp(0., 1.);
        let clip_lit = self.mixer_clip_until[index].is_some_and(|until| until > Instant::now());
        div()
            .flex()
            .flex_col()
            .items_center()
            .min_h_0()
            .when(index >= DECKS, |meter| meter.flex_1())
            .when(index < DECKS, |meter| meter.h_full().flex_shrink_0())
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let clip_space = 8.;
                        let step = (bounds.size.height.as_f32() - clip_space).max(0.) / 24.;
                        window.paint_quad(fill(
                            Bounds::new(bounds.origin, size(px(7.), px(5.))),
                            if clip_lit {
                                rgb(0xff5555)
                            } else {
                                rgb(0x3d2527)
                            },
                        ));
                        for segment in 0..24 {
                            let color = if segment as f32 / 24. >= level {
                                rgb(0x252b33)
                            } else if segment >= 21 {
                                rgb(0xf15b58)
                            } else if segment >= 17 {
                                rgb(0xe8c65c)
                            } else {
                                rgb(0xc8ff38)
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin
                                        + point(
                                            px(0.),
                                            px(clip_space + (23 - segment) as f32 * step),
                                        ),
                                    size(px(7.), px((step - 1.).max(1.))),
                                ),
                                color,
                            ));
                        }
                    },
                )
                .w(px(7.))
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }
    fn two_deck_eq_group(&self, deck: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let controls = match self.deck_eq_modes[deck] {
            EqMode::Classic => ["HIGH", "MID", "LOW"]
                .into_iter()
                .enumerate()
                .map(|(i, label)| self.mixer_control(channel_index(deck) + i + 1, label, cx))
                .collect::<Vec<_>>(),
            mode => {
                let labels: &[&'static str] = match mode {
                    EqMode::EzRemix => &["VOCAL", "INSTR", "BEAT"],
                    EqMode::Modern => &["DRUMS", "MEL/VOX", "BASS"],
                    _ => &["VOCAL", "INSTR", "BASS", "DRUMS"],
                };
                labels
                    .iter()
                    .enumerate()
                    .map(|(i, &label)| self.mixer_control(stem_index(deck) + i, label, cx))
                    .collect()
            }
        };
        div()
            .id(("two-deck-eq", deck))
            .flex()
            .flex_col()
            .w(px(52.))
            .h_full()
            .gap(px(4.))
            .p(px(4.))
            .border_t_2()
            .border_color(deck_colour(deck))
            .bg(cx.theme().secondary.opacity(0.5))
            .when(controls.len() > 3, |group| group.overflow_y_scroll())
            .children(controls)
    }

    fn two_deck_channel_group(&self, deck: usize, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .w(px(52.))
            .h_full()
            .min_h_0()
            .gap(px(4.))
            .p(px(4.))
            .border_t_2()
            .border_color(deck_colour(deck))
            .bg(cx.theme().secondary.opacity(0.5))
            .child(self.mixer_control(channel_index(deck), "TRIM", cx))
            .child(div().flex_1().min_h_0().w_full().child(self.mixer_control(
                channel_index(deck) + 4,
                "VOL",
                cx,
            )))
    }

    fn four_deck_controls(&self, deck: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let eq_controls = match self.deck_eq_modes[deck] {
            EqMode::Classic => ["HIGH", "MID", "LOW"]
                .into_iter()
                .enumerate()
                .map(|(i, label)| self.mixer_control(channel_index(deck) + i + 1, label, cx))
                .collect::<Vec<_>>(),
            mode => {
                let labels: &[&'static str] = match mode {
                    EqMode::EzRemix => &["VOCAL", "INSTR", "BEAT"],
                    EqMode::Modern => &["DRUMS", "MEL/VOX", "BASS"],
                    _ => &["VOCAL", "INSTR", "BASS", "DRUMS"],
                };
                labels
                    .iter()
                    .enumerate()
                    .map(|(i, &label)| self.mixer_control(stem_index(deck) + i, label, cx))
                    .collect()
            }
        };
        div()
            .id(("four-deck-controls", deck))
            .flex()
            .flex_col()
            .w(px(55.))
            .min_h_0()
            .gap(px(4.))
            .p(px(4.))
            .border_t_2()
            .border_color(deck_colour(deck))
            .bg(cx.theme().secondary.opacity(0.5))
            .when(eq_controls.len() > 3, |group| group.overflow_y_scroll())
            .child(self.mixer_control(channel_index(deck), "TRIM", cx))
            .children(eq_controls)
            .child(self.colour_fx_control(deck, cx))
            .child(self.mixer_control(channel_index(deck) + 4, "VOL", cx))
    }

    fn four_deck_mixer_content(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p(px(4.))
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .gap(px(4.))
                    .child(self.four_deck_controls(FOUR_DECK_ORDER[0], cx))
                    .child(self.four_deck_controls(FOUR_DECK_ORDER[1], cx))
                    .child(
                        div()
                            .flex()
                            .h_full()
                            .w(px(40.))
                            .gap(px(4.))
                            .children(FOUR_DECK_ORDER.into_iter().map(|deck| self.meter(deck))),
                    )
                    .child(self.four_deck_controls(FOUR_DECK_ORDER[2], cx))
                    .child(self.four_deck_controls(FOUR_DECK_ORDER[3], cx)),
            )
            .child(
                div()
                    .flex()
                    .w_full()
                    .gap(px(4.))
                    .items_center()
                    .child(self.crossfader_selectors(false, cx))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .h(px(62.))
                            .items_center()
                            .p(px(4.))
                            .bg(cx.theme().secondary.opacity(0.5))
                            .child(self.mixer_control(11, "CROSSFADE", cx)),
                    )
                    .child(self.crossfader_selectors(true, cx)),
            )
    }

    pub(super) fn mixer_panel(&self, cx: &mut Context<Self>) -> Div {
        let panel = div()
            .flex()
            .flex_col()
            .w(px(if self.settings.four_decks { 286. } else { 252. }))
            .flex_shrink_0()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .child(self.mixer_tabs(cx));
        if self.mixer_tab == 1 {
            return panel.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(4.))
                    .child(self.mixer_output_panel(cx)),
            );
        }
        if self.settings.four_decks {
            return panel.child(self.four_deck_mixer_content(cx));
        }
        let content = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p(px(4.))
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap(px(4.))
                    .child(self.two_deck_eq_group(0, cx))
                    .child(self.two_deck_channel_group(0, cx))
                    .child(
                        div()
                            .flex()
                            .w(px(18.))
                            .h_full()
                            .justify_center()
                            .gap(px(4.))
                            .child(self.meter(0))
                            .child(self.meter(1)),
                    )
                    .child(self.two_deck_channel_group(1, cx))
                    .child(self.two_deck_eq_group(1, cx)),
            )
            .child(
                div()
                    .flex()
                    .w_full()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .w(px(52.))
                            .h(px(64.))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .p(px(4.))
                            .bg(cx.theme().secondary.opacity(0.5))
                            .child(self.colour_fx_control(0, cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(64.))
                            .flex()
                            .items_center()
                            .p(px(4.))
                            .bg(cx.theme().secondary.opacity(0.5))
                            .child(self.mixer_control(11, "CROSSFADE", cx)),
                    )
                    .child(
                        div()
                            .w(px(52.))
                            .h(px(64.))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .p(px(4.))
                            .bg(cx.theme().secondary.opacity(0.5))
                            .child(self.colour_fx_control(1, cx)),
                    ),
            );
        panel.child(content)
    }
}

/// Four real sources, ordered vocals/instruments/bass/drums. Grouped controls
/// multiply gains; they never pretend filtered drums are isolated kick/hi-hat.
fn stem_members(mode: EqMode, control: usize) -> [bool; 4] {
    let groups = match mode {
        EqMode::Classic | EqMode::Stems => [0, 1, 2, 3],
        EqMode::EzRemix => [0, 1, 1, 2],
        EqMode::Modern => [1, 1, 2, 0],
    };
    groups.map(|group| group == control)
}
fn changed_stem_group(mode: EqMode, control: usize, value: f32, mut current: [f32; 4]) -> [f32; 4] {
    let gains = stem_gains(mode, [value; 4]);
    for (i, member) in stem_members(mode, control).into_iter().enumerate() {
        if member {
            current[i] = gains[i];
        }
    }
    current
}
pub(super) fn stem_controls_from_gains(mode: EqMode, gains: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|control| {
        let members = stem_members(mode, control);
        let count = members.iter().filter(|m| **m).count();
        let gain = if count == 0 {
            1.
        } else {
            gains
                .iter()
                .zip(members)
                .filter_map(|(g, m)| m.then_some(*g))
                .sum::<f32>()
                / count as f32
        };
        if gain <= 1. {
            gain / 2.
        } else {
            0.5 + gain.log10() * 20. / 12.
        }
    })
}
fn stem_gains(mode: EqMode, values: [f32; 4]) -> [f32; 4] {
    let gain = values.map(|v| {
        if v <= 0.5 {
            v * 2.
        } else {
            10f32.powf((v - 0.5) * 12. / 20.).min(2.)
        }
    });
    match mode {
        EqMode::Classic => [1.; 4],
        EqMode::EzRemix => [gain[0], gain[1], gain[1], gain[2]],
        EqMode::Modern => [gain[1], gain[1], gain[2], gain[0]],
        EqMode::Stems => gain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn linked_controls_pair_matching_deck_eq_and_volume() {
        for deck in 0..4 {
            let index = 41 + deck;
            assert_eq!(control_deck(index), Some(deck));
            assert!(is_knob(index));
            assert!(!is_stem(index));
            assert_eq!(opposite_deck_control(index), None);
            assert_eq!(initial_values()[index], 0.5);
            assert_eq!(value_label(index, 0.5), "Off");
        }
        for (left, right) in [
            (1, 6),
            (2, 7),
            (3, 8),
            (4, 9),
            (12, 16),
            (13, 17),
            (14, 18),
            (15, 19),
            (24, 29),
            (25, 30),
            (26, 31),
            (27, 32),
            (33, 37),
            (34, 38),
            (35, 39),
            (36, 40),
        ] {
            assert_eq!(opposite_deck_control(left), Some(right));
            assert_eq!(opposite_deck_control(right), Some(left));
        }
        for index in [0, 5, 10, 11, 20, 21, 22, 23, 28] {
            assert_eq!(opposite_deck_control(index), None);
        }
    }

    #[core::prelude::v1::test]
    fn extra_channel_controls_address_cd_without_changing_bus_controls() {
        for deck in 2..DECKS {
            assert_eq!(
                control_action(channel_index(deck) + 4, 0.5),
                Action::Deck {
                    deck: deck as u8,
                    action: DeckAction::SetVolume { level: 0.25 },
                }
            );
            assert_eq!(
                control_action(channel_index(deck) + 1, 0.),
                Action::Deck {
                    deck: deck as u8,
                    action: DeckAction::SetEq { band: 2, gain: 0. },
                }
            );
            assert!(!is_knob(channel_index(deck) + 4));
            assert!(is_knob(stem_index(deck)));
        }
        assert_eq!(
            control_action(10, 0.5),
            Action::Mixer {
                action: MixerAction::SetMasterGain { level: 0.25 },
            }
        );
    }

    #[core::prelude::v1::test]
    fn groups_mute_only_the_named_sources() {
        assert_eq!(
            changed_stem_group(EqMode::Modern, 2, 0.25, [0., 1., 1., 0.]),
            [0., 1., 0.5, 0.]
        );
        assert_eq!(
            stem_controls_from_gains(EqMode::Stems, [0., 1., 1., 0.]),
            [0., 0.5, 0.5, 0.]
        );
        assert_eq!(
            stem_gains(EqMode::EzRemix, [0., 0.5, 0.5, 0.5]),
            [0., 1., 1., 1.]
        );
        assert_eq!(
            stem_gains(EqMode::EzRemix, [0.5, 0., 0.5, 0.5]),
            [1., 0., 0., 1.]
        );
        assert_eq!(
            stem_gains(EqMode::Modern, [0.5, 0., 0.5, 0.5]),
            [0., 0., 1., 1.]
        );
        assert_eq!(
            stem_gains(EqMode::Stems, [0.5, 0.5, 0., 0.5]),
            [1., 1., 0., 1.]
        );
    }
}
