use super::*;
use djcraft_protocol::{PhraseLabel, PhraseMarker, TrackPreparation};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
#[derive(Clone, Copy)]
pub(super) enum Marker {
    In,
    Out,
    Phrase(usize),
}
fn colour(label: PhraseLabel) -> Rgba {
    rgb(match label {
        PhraseLabel::Intro => 0x3864e8,
        PhraseLabel::Build => 0xc8ff38,
        PhraseLabel::Breakdown => 0x8464c8,
        PhraseLabel::Drop => 0x28c8a0,
        PhraseLabel::Outro => 0x387098,
        PhraseLabel::Section => 0x586878,
    })
}
impl Shell {
    pub(super) fn ensure_preparation(
        &mut self,
        deck: usize,
        state: &DeckState,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = state.path.clone() else {
            return;
        };
        if state.loading
            || self
                .preparations
                .contains_key(&crate::track_identity::key(&path))
            || self.preparation_pending.contains(&path)
        {
            return;
        }
        let Some(track) = self
            .engine
            .as_ref()
            .and_then(|e| e.track(deck))
            .filter(|t| t.is_complete())
            .cloned()
        else {
            return;
        };
        let tempo = self.grids[deck].map(|g| djcraft_analysis::Tempo {
            bpm: g.bpm,
            first_beat_seconds: g.first_beat_seconds,
            downbeat: g.downbeat,
            confidence: g.confidence as f32,
        });
        self.preparation_pending.insert(path.clone());
        let task = cx
            .background_executor()
            .spawn(async move { crate::preparation::suggest(&track, tempo) });
        cx.spawn(async move |this, cx| {
            let preparation = task.await;
            let _ = this.update(cx, |this, cx| {
                this.preparation_pending.remove(&path);
                // Manual edits made while analysis ran always win.
                if !this
                    .preparations
                    .contains_key(&crate::track_identity::key(&path))
                {
                    this.commit_preparation(path, preparation, true);
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn commit_preparation(&mut self, path: String, preparation: TrackPreparation, save: bool) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        if engine
            .apply(
                Role::Host,
                Action::Mixer {
                    action: MixerAction::SetTrackPreparation {
                        path: path.clone(),
                        preparation: preparation.clone(),
                    },
                },
            )
            .is_err()
        {
            self.preparation_notice = "Invalid marker order or transition setting".into();
            return;
        }
        self.preparations
            .insert(crate::track_identity::key(&path), preparation);
        self.preparation_notice.clear();
        if save {
            self.save_preparations();
        }
    }
    fn save_preparations(&mut self) {
        if let Err(error) = crate::preparation::save(&data_dir(), &self.preparations) {
            self.preparation_notice = format!("Preparation not saved: {error}");
        }
    }
    fn edit_preparation(
        &mut self,
        deck: usize,
        change: impl FnOnce(&mut TrackPreparation, f64, f64),
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.loop_state(deck).filter(|s| !s.loading) else {
            return;
        };
        let (Some(path), Some(track)) = (state.path, state.track) else {
            return;
        };
        let mut prep = self
            .preparations
            .get(&crate::track_identity::key(&path))
            .cloned()
            .unwrap_or_default();
        change(&mut prep, state.position_seconds, track.duration_seconds);
        self.commit_preparation(path, prep, true);
        cx.notify();
    }
    fn begin_preparation_drag(&mut self, deck: usize, marker: Marker, cx: &mut Context<Self>) {
        let Some(state) = self.loop_state(deck).filter(|s| !s.loading) else {
            return;
        };
        if let Some(path) = state.path {
            if let Marker::Phrase(i) = marker {
                self.preparation_selected[deck] = Some(i);
            }
            self.preparation_drag = Some((deck, path, marker));
            cx.notify();
        }
    }
    pub(super) fn move_preparation_drag(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some((deck, path, marker)) = self.preparation_drag.clone() else {
            return;
        };
        let Some(state) = self
            .loop_state(deck)
            .filter(|s| !s.loading && s.path.as_ref() == Some(&path))
        else {
            self.preparation_drag = None;
            return;
        };
        let Some(track) = state.track else {
            return;
        };
        let area = self.overview_areas[deck].get();
        if area.size.width.as_f32() <= 0. {
            return;
        }
        let mut t = ((x - area.origin.x.as_f32()) / area.size.width.as_f32()) as f64
            * track.duration_seconds;
        if let Some(g) = self.grids[deck] {
            t = g.first_beat_seconds
                + ((t - g.first_beat_seconds) / g.beat_seconds()).round() * g.beat_seconds();
        }
        let mut prep = self
            .preparations
            .get(&crate::track_identity::key(&path))
            .cloned()
            .unwrap_or_default();
        move_marker(&mut prep, marker, t, track.duration_seconds);
        self.commit_preparation(path, prep, false);
        cx.notify();
    }
    pub(super) fn end_preparation_drag(&mut self, cx: &mut Context<Self>) {
        if self.preparation_drag.take().is_some() {
            self.save_preparations();
            cx.notify();
        }
    }
    fn wheel_preparation(
        &mut self,
        deck: usize,
        marker: Marker,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        let step =
            self.grids[deck].map_or(0.1, |g| g.beat_seconds()) * crate::wheel::steps(event) as f64;
        self.edit_preparation(
            deck,
            move |p, _, duration| {
                let current = match marker {
                    Marker::In => p.mix_in,
                    Marker::Out => p.mix_out,
                    Marker::Phrase(i) => p.phrases.get(i).map(|m| m.seconds),
                };
                if let Some(t) = current {
                    move_marker(p, marker, t + step, duration);
                }
            },
            cx,
        );
        cx.stop_propagation();
    }
    pub(super) fn preparation_panel(
        &self,
        deck: usize,
        state: &DeckState,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(path) = &state.path else {
            return div();
        };
        let duration = state
            .track
            .as_ref()
            .map_or(1., |t| t.duration_seconds.max(0.01));
        let prep = self
            .preparations
            .get(&crate::track_identity::key(path))
            .cloned()
            .unwrap_or_default();
        let selected = self.preparation_selected[deck].filter(|i| *i < prep.phrases.len());
        let mut strip = div()
            .relative()
            .h(px(22.))
            .w_full()
            .overflow_hidden()
            .bg(rgb(0x15191e));
        for (i, phrase) in prep.phrases.iter().enumerate() {
            let end = prep.phrases.get(i + 1).map_or(duration, |p| p.seconds);
            strip = strip.child(
                div()
                    .id(("phrase", deck * 128 + i))
                    .absolute()
                    .left(relative((phrase.seconds / duration) as f32))
                    .top_0()
                    .h_full()
                    .w(relative(((end - phrase.seconds) / duration).max(0.) as f32))
                    .overflow_hidden()
                    .bg(colour(phrase.label).opacity(if selected == Some(i) { 0.65 } else { 0.3 }))
                    .border_l_1()
                    .border_color(colour(phrase.label))
                    .text_size(px(10.))
                    .px_1()
                    .child(phrase.label.text())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.begin_preparation_drag(deck, Marker::Phrase(i), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_scroll_wheel(cx.listener(move |this, e: &ScrollWheelEvent, _, cx| {
                        this.wheel_preparation(deck, Marker::Phrase(i), e, cx)
                    })),
            );
        }
        let mut vocal_strip = div().relative().h(px(5.)).w_full().overflow_hidden();
        if let Some(regions) = self
            .engine
            .as_ref()
            .and_then(|e| e.track(deck))
            .and_then(|t| t.vocal_regions())
        {
            for region in regions {
                vocal_strip = vocal_strip.child(
                    div()
                        .absolute()
                        .top_0()
                        .h_full()
                        .left(relative((region[0] / duration) as f32))
                        .w(relative(((region[1] - region[0]) / duration) as f32))
                        .bg(rgb(0xd875dc)),
                );
            }
        }
        let mut mix = div().relative().h(px(21.)).w_full();
        for (id, t, label, marker, c) in [
            (0, prep.mix_in, "MIX IN", Marker::In, 0xc8ff38),
            (1, prep.mix_out, "MIX OUT", Marker::Out, 0x5785ff),
        ] {
            if let Some(t) = t {
                mix = mix.child(
                    div()
                        .id(("mix-marker", deck * 2 + id))
                        .absolute()
                        .left(relative((t / duration) as f32))
                        .h_full()
                        .w(px(2.))
                        .bg(rgb(c))
                        .child(
                            div()
                                .absolute()
                                .when(t > duration * 0.8, |d| d.right_0())
                                .w(px(58.))
                                .text_size(px(10.))
                                .text_color(rgb(c))
                                .child(label),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.begin_preparation_drag(deck, marker, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_scroll_wheel(cx.listener(move |this, e: &ScrollWheelEvent, _, cx| {
                            this.wheel_preparation(deck, marker, e, cx)
                        })),
                );
            }
        }
        let mut tools = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .child(
                Button::new(("set-mix-in", deck))
                    .tooltip("Set the automix entry point at the current playhead.")
                    .xsmall()
                    .outline()
                    .label("IN here")
                    .disabled(state.loading)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_preparation(deck, |p, t, d| move_marker(p, Marker::In, t, d), cx)
                    })),
            )
            .child(
                Button::new(("set-mix-out", deck))
                    .tooltip("Start the outgoing automix transition at the current playhead.")
                    .xsmall()
                    .outline()
                    .label("OUT here")
                    .disabled(state.loading)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_preparation(deck, |p, t, d| move_marker(p, Marker::Out, t, d), cx)
                    })),
            )
            .child(
                Button::new(("add-phrase", deck))
                    .tooltip("Add a section boundary at the current playhead.")
                    .xsmall()
                    .outline()
                    .label("+ Section")
                    .disabled(state.loading)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_preparation(
                            deck,
                            |p, t, _| {
                                if p.phrases.len() < 128
                                    && p.phrases.iter().all(|m| (m.seconds - t).abs() > 0.01)
                                {
                                    p.phrases.push(PhraseMarker {
                                        seconds: t,
                                        label: PhraseLabel::Section,
                                    });
                                    p.phrases.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
                                }
                            },
                            cx,
                        )
                    })),
            );
        let view = cx.weak_entity();
        tools = tools
            .child(
                Button::new(("phrase-label", deck))
                    .tooltip("Choose a label for the selected section.")
                    .xsmall()
                    .outline()
                    .label(selected.map_or("Label", |i| prep.phrases[i].label.text()))
                    .disabled(selected.is_none())
                    .dropdown_caret(true)
                    .dropdown_menu(move |mut menu, _, _| {
                        for label in [
                            PhraseLabel::Intro,
                            PhraseLabel::Build,
                            PhraseLabel::Breakdown,
                            PhraseLabel::Drop,
                            PhraseLabel::Outro,
                            PhraseLabel::Section,
                        ] {
                            let view = view.clone();
                            menu = menu.item(PopupMenuItem::new(label.text()).on_click(
                                move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.edit_preparation(
                                            deck,
                                            |p, _, _| {
                                                if let Some(m) =
                                                    selected.and_then(|i| p.phrases.get_mut(i))
                                                {
                                                    m.label = label;
                                                }
                                            },
                                            cx,
                                        )
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            )
            .child(
                Button::new(("delete-phrase", deck))
                    .tooltip("Remove the selected section boundary.")
                    .xsmall()
                    .outline()
                    .label("Remove")
                    .disabled(selected.is_none())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_preparation(
                            deck,
                            |p, _, _| {
                                if let Some(i) = selected.filter(|i| *i < p.phrases.len()) {
                                    p.phrases.remove(i);
                                }
                            },
                            cx,
                        )
                    })),
            );
        let view = cx.weak_entity();
        tools = tools.child(
            div()
                .id(("mix-length-wheel", deck))
                .on_scroll_wheel(cx.listener(move |this, e: &ScrollWheelEvent, _, cx| {
                    let direction = crate::wheel::steps(e).signum() as i32;
                    if direction != 0 {
                        this.edit_preparation(
                            deck,
                            |p, _, _| {
                                let options = [None, Some(8), Some(16), Some(32)];
                                let index = options
                                    .iter()
                                    .position(|v| *v == p.transition_bars)
                                    .unwrap_or(0)
                                    as i32;
                                p.transition_bars =
                                    options[(index + direction).clamp(0, 3) as usize];
                            },
                            cx,
                        );
                    }
                    cx.stop_propagation();
                }))
                .child(
                    Button::new(("mix-length", deck))
                        .tooltip("Choose the transition length in bars. Wheel to adjust.")
                        .xsmall()
                        .outline()
                        .label(
                            prep.transition_bars
                                .map_or("Auto length".into(), |n| format!("{n} bars")),
                        )
                        .dropdown_caret(true)
                        .dropdown_menu(move |mut menu, _, _| {
                            for bars in [None, Some(8), Some(16), Some(32)] {
                                let view = view.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(
                                        bars.map_or("Auto length".into(), |n| format!("{n} bars")),
                                    )
                                    .on_click(
                                        move |_, _, cx| {
                                            let _ = view.update(cx, |this, cx| {
                                                this.edit_preparation(
                                                    deck,
                                                    |p, _, _| p.transition_bars = bars,
                                                    cx,
                                                )
                                            });
                                        },
                                    ),
                                );
                            }
                            menu
                        }),
                ),
        );
        tools = tools
            .child(
                div()
                    .id(("bass-swap", deck))
                    .on_scroll_wheel(cx.listener(move |this, e: &ScrollWheelEvent, _, cx| {
                        let delta = crate::wheel::steps(e) * 0.05;
                        this.edit_preparation(
                            deck,
                            |p, _, _| {
                                p.bass_swap =
                                    Some((p.bass_swap.unwrap_or(0.5) + delta).clamp(0.1, 0.9))
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }))
                    .child(
                        Button::new(("bass-swap-reset", deck))
.tooltip("Wheel to change when bass swaps during the mix; click to restore automatic timing.")
                            .xsmall()
                            .outline()
                            .label(format!("Bass {:.0}%", prep.bass_swap.unwrap_or(0.5) * 100.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_preparation(deck, |p, _, _| p.bass_swap = None, cx)
                            })),
                    ),
            )
            .child(
                Button::new(("clear-mix", deck))
.tooltip("Clear saved entry and exit points so automix can choose them.")
                    .xsmall()
                    .outline()
                    .label("Clear IN/OUT")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_preparation(
                            deck,
                            |p, _, _| {
                                p.mix_in = None;
                                p.mix_out = None;
                            },
                            cx,
                        )
                    })),
            );
        div().flex().flex_col().gap_1().child(strip).child(vocal_strip).child(mix).child(tools)
            .child(div().text_size(px(9.)).text_color(cx.theme().muted_foreground).child(
                if !self.preparation_notice.is_empty(){self.preparation_notice.clone()}
                else if self.preparation_pending.contains(path){"Finding energy-based sections…".into()}
                else {"Purple: vocal estimate · drag/wheel boundaries · click section to label · Bass: wheel / click resets".into()}))
    }
}
fn move_marker(p: &mut TrackPreparation, marker: Marker, t: f64, duration: f64) {
    let t = t.clamp(0., (duration - 0.01).max(0.));
    match marker {
        Marker::In => p.mix_in = Some(t.min(p.mix_out.map_or(duration, |v| (v - 0.01).max(0.)))),
        Marker::Out => p.mix_out = Some(t.max(p.mix_in.map_or(0., |v| v + 0.01))),
        Marker::Phrase(i) => {
            if i >= p.phrases.len() {
                return;
            }
            let low = if i == 0 {
                0.
            } else {
                p.phrases[i - 1].seconds + 0.01
            };
            let high = p
                .phrases
                .get(i + 1)
                .map_or(duration - 0.01, |m| m.seconds - 0.01);
            if high >= low {
                p.phrases[i].seconds = t.clamp(low, high);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Marker, move_marker};
    use djcraft_protocol::{PhraseLabel, PhraseMarker, TrackPreparation};
    #[test]
    fn dragging_clamps_to_neighbours_and_keeps_mix_points_ordered() {
        let mut p = TrackPreparation {
            phrases: vec![
                PhraseMarker {
                    seconds: 0.,
                    label: PhraseLabel::Intro,
                },
                PhraseMarker {
                    seconds: 10.,
                    label: PhraseLabel::Drop,
                },
                PhraseMarker {
                    seconds: 20.,
                    label: PhraseLabel::Outro,
                },
            ],
            mix_in: Some(2.),
            mix_out: Some(25.),
            ..Default::default()
        };
        move_marker(&mut p, Marker::Phrase(1), 50., 30.);
        assert!(p.phrases[1].seconds < 20.);
        move_marker(&mut p, Marker::In, 28., 30.);
        assert!(p.mix_in.unwrap() < p.mix_out.unwrap());
        move_marker(&mut p, Marker::Out, -5., 30.);
        assert!(p.valid());
        move_marker(&mut p, Marker::Phrase(0), -5., 30.);
        assert_eq!(p.phrases[0].seconds, 0.);
    }
}
