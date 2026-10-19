use super::*;
use gpui_kit::component::tooltip::Tooltip;

fn curve_icon(curve: CrossfaderCurve) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for (deck, color) in [0xc8ff38, 0x3864e8].into_iter().enumerate() {
                let mut path = gpui_kit::PathBuilder::stroke(px(1.3));
                for step in 0..=40 {
                    let position = step as f32 / 40.;
                    let gain = djcraft_engine::crossfader_gains(position, curve)[deck];
                    let p = bounds.origin + point(px(7. + position * 26.), px(15. - gain * 10.));
                    if step == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(color));
                }
            }
        },
    )
    .w(px(40.))
    .h(px(20.))
}

fn is_knob(index: usize) -> bool {
    (index < 10 && index % 5 < 4) || index >= 12
}
fn control_action(index: usize, value: f32) -> Action {
    if index == 10 {
        return Action::Mixer {
            action: MixerAction::SetMasterGain {
                level: value * value,
            },
        };
    }
    if index == 11 {
        return Action::Mixer {
            action: MixerAction::SetCrossfader { position: value },
        };
    }
    let action = match index % 5 {
        0 => DeckAction::SetTrim {
            db: value * 24. - 12.,
        },
        4 => DeckAction::SetVolume {
            level: value * value,
        },
        band => DeckAction::SetEq {
            band: (3 - band) as u8,
            gain: if value <= 0.5 {
                value * 2.
            } else {
                10f32.powf((value - 0.5) * 12. / 20.).min(2.)
            },
        },
    };
    Action::Deck {
        deck: (index / 5) as u8,
        action,
    }
}
fn value_label(index: usize, value: f32) -> String {
    if index == 22 {
        let [cue, master] = djcraft_engine::cue_mix_gains(value);
        return format!("{:.0}/{:.0}", cue * 100., master * 100.);
    }
    if index == 20 || index == 21 {
        return if value <= 0.001 {
            "−∞".into()
        } else {
            format!("{:+.1}", 20. * (value * value).log10())
        };
    }
    if index == 11 {
        return format!("L  {:3.0}%  R", value * 100.);
    }
    if index < 10 && index % 5 == 0 {
        return format!("{:+.1}", value * 24. - 12.);
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
    format!("{:+.1}", 20. * gain.log10())
}

impl Shell {
    fn set_mixer_control(&mut self, index: usize, value: f32, cx: &mut Context<Self>) {
        let value = value.clamp(0., 1.);
        let action = if index == 20 {
            Action::Mixer {
                action: MixerAction::SetSamplerGain {
                    level: value * value,
                },
            }
        } else if index == 21 || index == 22 {
            let mut monitor = self
                .engine
                .as_mut()
                .map(|e| e.state().monitor)
                .unwrap_or_default();
            monitor.deck = self.active_deck as u8;
            if index == 21 {
                monitor.gain = value * value;
            } else {
                monitor.mix = value;
            }
            Action::Mixer {
                action: MixerAction::SetMonitor { monitor },
            }
        } else if index >= 12 {
            let deck = (index - 12) / 4;
            let current = self
                .engine
                .as_mut()
                .map(|engine| engine.state().decks[deck].stem_gains)
                .unwrap_or([1.; 4]);
            Action::Deck {
                deck: deck as u8,
                action: DeckAction::SetStemGains {
                    gains: changed_stem_group(self.eq_mode, (index - 12) % 4, value, current),
                },
            }
        } else {
            control_action(index, value)
        };
        if let Some(engine) = &mut self.engine {
            match engine.apply(Role::Host, action) {
                Ok(()) => self.mixer_values[index] = value,
                Err(reason) => eprintln!("mixer action rejected: {reason:?}"),
            }
        }
        cx.notify();
    }
    pub(super) fn move_mixer_control(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((index, start_y, start_value)) = self.mixer_drag else {
            return;
        };
        let area = self.mixer_areas[index].get();
        let value = if is_knob(index) {
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
        let knob = is_knob(index);
        let enabled = index < 12
            || index >= 20
            || self
                .engine
                .as_ref()
                .and_then(|e| e.track((index - 12) / 4))
                .is_some_and(|t| t.stems_ready());
        let horizontal = index == 11;
        let accent = if index >= 20 {
            rgb(0x79d9aa)
        } else if index < 5 || (12..16).contains(&index) {
            rgb(0xc8ff38)
        } else if index < 10 || index >= 16 {
            rgb(0x3864e8)
        } else {
            rgb(0x79d9aa)
        };
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
                    // Dial outline and a radial pointer, with neutral at twelve o'clock.
                    let radius = 11.;
                    for step in 0..48 {
                        let angle = step as f32 / 48. * std::f32::consts::TAU;
                        rect(
                            w / 2. + angle.sin() * radius - 1.,
                            h / 2. - angle.cos() * radius - 1.,
                            2.,
                            2.,
                            rgb(0x646b75),
                        );
                    }
                    let angle = (value.clamp(0., 1.) - 0.5) * std::f32::consts::PI * 1.5;
                    for distance in 2..10 {
                        rect(
                            w / 2. + angle.sin() * distance as f32 - 1.,
                            h / 2. - angle.cos() * distance as f32 - 1.,
                            2.,
                            2.,
                            accent,
                        );
                    }
                    rect(w / 2. - 1., 0., 2., 2., rgb(0xbac1ca));
                } else if horizontal {
                    rect(8., h / 2. - 2., w - 16., 4., rgb(0x303640));
                    rect(w / 2. - 1., 3., 2., h - 6., rgb(0x747b85));
                    rect(8. + value * (w - 16.) - 5., 2., 10., h - 4., accent);
                } else {
                    rect(w / 2. - 2., 8., 4., h - 16., rgb(0x303640));
                    for step in 0..5 {
                        rect(
                            w / 2. - 10.,
                            8. + step as f32 * (h - 16.) / 4.,
                            20.,
                            1.,
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
        .when(knob, |this| this.h(px(28.)))
        .when(horizontal, |this| this.h(px(22.)))
        .when(!knob && !horizontal, |this| this.flex_1().min_h(px(80.)));
        div()
            .id(("mixer-control", index))
            .when(index >= 20, |control| control.tooltip(move |window, cx| {
                Tooltip::new(match index {
                    20 => "Sampler bus volume; individual pad levels remain relative. Double-click resets to unity.",
                    21 => "Headphone volume only. Double-click resets to 50% gain.",
                    _ => "Cue / master blend: centre is both at full volume; either end fades out the opposite source. Double-click resets to cue only.",
                }).build(window, cx)
            }))
            .when(!enabled, |this| this.opacity(0.35))
            .when(!knob && !horizontal, |this| this.h_full().min_h_0())
            .flex()
            .flex_col()
            .items_center()
            .w_full()
            .child(
                div()
                    .text_size(px(10.))
                    .child(format!("{label} {}", value_label(index, value))),
            )
            .child(drawing)
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                if !enabled {
                    return;
                }
                let value = this.mixer_values[index] + crate::wheel::steps(event) * 0.02;
                this.set_mixer_control(index, value, cx);
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
                            if index == 20 {
                                1.
                            } else if index == 21 {
                                std::f32::consts::FRAC_1_SQRT_2
                            } else if index == 22 {
                                0.
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
                        if !is_knob(index) {
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
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .min_h_0()
            .when(index >= 2, |meter| meter.flex_1())
            .when(index < 2, |meter| meter.h_full().flex_shrink_0())
            .child(
                div()
                    .text_size(px(8.))
                    .text_color(if peak >= 1. {
                        rgb(0xff5555)
                    } else {
                        rgb(0x69717d)
                    })
                    .child("CLIP"),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let step = bounds.size.height.as_f32() / 24.;
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
                                    bounds.origin + point(px(0.), px((23 - segment) as f32 * step)),
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
    pub(super) fn mixer_panel(&self, cx: &mut Context<Self>) -> Div {
        let knobs = |deck: usize, cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .w(px(78.))
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .child(if deck == 0 { "LEFT" } else { "RIGHT" }),
                )
                .child(self.mixer_control(deck * 5, "TRIM", cx))
                .children(match self.eq_mode {
                    EqMode::Classic => ["HIGH", "MID", "LOW"]
                        .into_iter()
                        .enumerate()
                        .map(|(i, label)| self.mixer_control(deck * 5 + i + 1, label, cx))
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
                            .map(|(i, &label)| self.mixer_control(12 + deck * 4 + i, label, cx))
                            .collect()
                    }
                })
        };
        div()
            .flex()
            .flex_col()
            .w(px(398.))
            .flex_shrink_0()
            .p_2()
            .gap_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .h(px(if self.eq_mode == EqMode::Stems {
                        258.
                    } else {
                        210.
                    }))
                    .flex_shrink_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .w(px(64.))
                            .gap_1()
                            .child(div().text_size(px(10.)).child("SAMPLER"))
                            .child(self.mixer_control(20, "VOL", cx)),
                    )
                    .child(knobs(0, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .w(px(60.))
                            .h_full()
                            .gap_1()
                            .child(div().text_size(px(10.)).child("MASTER"))
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_h_0()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .h_full()
                                            .child(div().text_size(px(9.)).child("L"))
                                            .child(self.meter(2)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .h_full()
                                            .child(div().text_size(px(9.)).child("R"))
                                            .child(self.meter(3)),
                                    ),
                            ),
                    )
                    .child(knobs(1, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .w(px(64.))
                            .gap_1()
                            .child(div().text_size(px(10.)).child("CUE"))
                            .child(self.mixer_control(21, "VOL", cx))
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
                    .flex_1()
                    .min_h(px(100.))
                    .px(px(72.))
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .w(px(78.))
                            .h_full()
                            .gap_1()
                            .child(self.meter(0))
                            .child(self.mixer_control(4, "VOL", cx)),
                    )
                    .child(
                        div()
                            .w(px(60.))
                            .h_full()
                            .child(self.mixer_control(10, "dB", cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .w(px(78.))
                            .h_full()
                            .gap_1()
                            .child(self.mixer_control(9, "VOL", cx))
                            .child(self.meter(1)),
                    ),
            )
            .child(
                div().flex().justify_between().children(
                    (0..2)
                        .map(|deck| {
                            let ready = self
                                .engine
                                .as_ref()
                                .and_then(|e| e.track(deck))
                                .is_some_and(|t| t.stems_ready());
                            let label = if self.stem_busy[deck] {
                                match self.stem_progress[deck]
                                    .load(std::sync::atomic::Ordering::Relaxed)
                                {
                                    255 => "Queued".to_string(),
                                    254 => "Loading stems…".to_string(),
                                    percent => format!("Stems {percent}%"),
                                }
                            } else if ready {
                                "Stems ready".to_string()
                            } else {
                                "Prepare stems".to_string()
                            };
                            Button::new(("prepare-deck", deck))
                                .xsmall()
                                .outline()
                                .label(label)
                                .tooltip(if self.stem_status[deck].is_empty() {
                                    "Prepare and cache stems locally for stem mixing.".to_string()
                                } else {
                                    self.stem_status[deck].clone()
                                })
                                .disabled(
                                    self.stem_busy[deck]
                                        || ready
                                        || !self
                                            .engine
                                            .as_ref()
                                            .and_then(|e| e.track(deck))
                                            .is_some_and(|t| t.is_complete()),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(track) =
                                        this.engine.as_ref().and_then(|e| e.track(deck)).cloned()
                                    {
                                        this.start_stem_job(deck, track, true, cx);
                                    }
                                }))
                        })
                        .collect::<Vec<_>>(),
                ),
            )
            .child(self.mixer_control(11, "CROSSFADE", cx))
            .child(
                div().flex().justify_center().gap_1().children(
                    [
                        (CrossfaderCurve::Smooth, "Smooth"),
                        (CrossfaderCurve::EqualPower, "Equal power"),
                        (CrossfaderCurve::Cut, "Cut"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (curve, label))| {
                        Button::new(("cross-curve", index))
                            .tooltip(label)
                            .xsmall()
                            .outline()
                            .child(curve_icon(curve))
                            .selected(self.mixer_curve == curve)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(engine) = &mut this.engine {
                                    if engine
                                        .apply(
                                            Role::Host,
                                            Action::Mixer {
                                                action: MixerAction::SetCrossfaderCurve { curve },
                                            },
                                        )
                                        .is_ok()
                                    {
                                        this.mixer_curve = curve;
                                    }
                                }
                                cx.notify();
                            }))
                    })
                    .collect::<Vec<_>>(),
                ),
            )
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

    fn curve_icon(curve: CrossfaderCurve) -> impl IntoElement {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                for (deck, color) in [0xc8ff38, 0x3864e8].into_iter().enumerate() {
                    let mut path = gpui_kit::PathBuilder::stroke(px(1.3));
                    for step in 0..=40 {
                        let position = step as f32 / 40.;
                        let gain = djcraft_engine::crossfader_gains(position, curve)[deck];
                        let p =
                            bounds.origin + point(px(7. + position * 26.), px(15. - gain * 10.));
                        if step == 0 {
                            path.move_to(p);
                        } else {
                            path.line_to(p);
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(color));
                    }
                }
            },
        )
        .w(px(40.))
        .h(px(20.))
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
