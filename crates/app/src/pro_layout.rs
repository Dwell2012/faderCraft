use super::*;

fn deck_colour(index: usize) -> Rgba {
    if index == 0 {
        rgb(0xc8ff38)
    } else {
        rgb(0x3864e8)
    }
}

impl Shell {
    /// Both decks share the same width and centre playhead for beat comparison.
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
                    .take(2)
                    .enumerate()
                    .map(|(index, state)| {
                        let track = self.engine.as_ref().and_then(|e| e.track(index));
                        let grid = self.grids[index];
                        let colour = deck_colour(index);
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .w(px(30.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(colour)
                                    .font_weight(FontWeight::BOLD)
                                    .child(if index == 0 { "A" } else { "B" }),
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

    pub(super) fn deck_display(
        &self,
        index: usize,
        state: &DeckState,
        cx: &mut Context<Self>,
    ) -> Div {
        let colour = deck_colour(index);
        let position = state.position_seconds;
        let duration = state.track.as_ref().map_or(0., |t| t.duration_seconds);
        let loaded = state.track.is_some();
        let remaining = (duration - position).max(0.);
        let grid = self.grids[index];
        let tempo_text = grid
            .map(|g| format!("{:.2}", g.bpm * state.tempo))
            .unwrap_or_else(|| "—".into());
        let muted = cx.theme().muted_foreground;
        let ring = cx.theme().border;
        let beat = grid
            .filter(|g| position >= g.first_beat_seconds)
            .map(|g| ((position - g.first_beat_seconds) / g.beat_seconds()).floor() as usize % 4);
        let platter =
            div()
                .id(("platter", index))
                .relative()
                .w(px(120.))
                .h(px(120.))
                .flex_shrink_0()
                .rounded_full()
                .border_1()
                .border_color(ring)
                .bg(cx.theme().secondary.opacity(0.3))
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            let centre = bounds.origin
                                + point(bounds.size.width / 2., bounds.size.height / 2.);
                            let radius = bounds.size.width.as_f32() / 2. - 7.;
                            for mark in 0..120 {
                                let angle = mark as f32 / 120. * std::f32::consts::TAU;
                                let active =
                                    loaded && mark as f64 / 120. <= position / duration.max(0.001);
                                window.paint_quad(fill(
                                    Bounds::new(
                                        centre
                                            + point(
                                                px(angle.sin() * radius - 1.),
                                                px(-angle.cos() * radius - 1.),
                                            ),
                                        size(px(2.), px(2.)),
                                    ),
                                    if active { colour.into() } else { ring },
                                ));
                            }
                            if loaded {
                                let angle = (position * 0.5555556 * std::f64::consts::TAU) as f32;
                                for distance in 39..49 {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            centre
                                                + point(
                                                    px(angle.sin() * distance as f32 - 1.),
                                                    px(-angle.cos() * distance as f32 - 1.),
                                                ),
                                            size(px(2.), px(2.)),
                                        ),
                                        colour,
                                    ));
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
                                .text_size(px(28.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(colour)
                                .child(if index == 0 { "A" } else { "B" }),
                        )
                        .child(div().text_size(px(10.)).text_color(muted).child(
                            if state.playing {
                                "PLAYING"
                            } else if loaded {
                                "READY"
                            } else {
                                "EMPTY"
                            },
                        )),
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

        div()
            .relative()
            .flex()
            .when(index == 1, |d| d.flex_row_reverse())
            .h(px(150.))
            .flex_shrink_0()
            .items_center()
            .justify_end()
            .when(index == 1, |d| d.justify_start())
            .gap_4()
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(platter),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(90.))
                    .flex_shrink_0()
                    .when(index == 0, |d| d.items_end().text_right())
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .id(("tempo-display", index))
                            .on_scroll_wheel(cx.listener(
                                move |this, event: &ScrollWheelEvent, _, cx| {
                                    this.wheel_tempo(index, event, cx)
                                },
                            ))
                            .child(div().text_size(px(10.)).text_color(muted).child("BPM"))
                            .child(
                                div()
                                    .text_size(px(26.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(colour)
                                    .child(tempo_text),
                            ),
                    )
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(muted)
                                    .child("REMAINING"),
                            )
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if loaded && remaining < 30. {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().foreground
                                    })
                                    .child(if loaded {
                                        format!("−{}", clock(remaining / state.tempo))
                                    } else {
                                        "—:—".into()
                                    }),
                            ),
                    )
                    .child(
                        div().flex().gap_2().children(
                            (0..4)
                                .map(|i| {
                                    div().w(px(17.)).h(px(4.)).rounded_sm().bg(
                                        if loaded && beat == Some(i) {
                                            colour.into()
                                        } else {
                                            ring
                                        },
                                    )
                                })
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .child(div().text_size(px(10.)).text_color(muted).child(if loaded {
                        format!("ELAPSED  {}", clock(position / state.tempo))
                    } else {
                        "Load or drop a track".into()
                    })),
            )
            .child(self.pitch_control(index, state, cx))
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
                    self.pending_seeks[deck]
                        .map_or(state.position_seconds, |(position, _)| position)
                },
                |(position, _)| *position,
            );
        self.cue_anchor[deck] = Some((seconds, state.path.clone()));
        self.cue_preview = Some((deck, seconds, state.path));
        self.send(
            deck,
            DeckAction::CuePreview {
                seconds,
                play: true,
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
        let deck = self.active_deck;
        if let Some(state) = self.engine.as_mut().map(|e| e.state().decks[deck].clone()) {
            if state.track.is_some() {
                self.send(
                    deck,
                    if state.playing {
                        DeckAction::Pause
                    } else {
                        DeckAction::Play
                    },
                );
                cx.notify();
            }
        }
    }
    pub(super) fn move_pitch(&mut self, y: f32, cx: &mut Context<Self>) {
        let Some(deck) = self.pitch_drag else {
            return;
        };
        let bounds = self.pitch_areas[deck].get();
        let fraction = ((y - bounds.origin.y.as_f32() - 5.)
            / (bounds.size.height.as_f32() - 10.).max(1.))
        .clamp(0., 1.);
        self.send(
            deck,
            DeckAction::SetTempo {
                ratio: pitch_ratio(fraction as f64),
            },
        );
        cx.notify();
    }
    fn pitch_control(&self, deck: usize, state: &DeckState, cx: &mut Context<Self>) -> Div {
        let area = self.pitch_areas[deck].clone();
        let colour = deck_colour(deck);
        let tempo = state.tempo;
        let key_lock = state.key_lock;
        div()
            .flex()
            .flex_col()
            .items_center()
            .w(px(80.))
            .h_full()
            .gap_1()
            .child(
                Button::new(("key-lock", deck))
                    .outline()
                    .xsmall()
                    .icon(IconName::Lock)
                    .label("Key lock")
                    .selected(key_lock)
                    .tooltip("Keep the musical pitch unchanged when changing tempo.")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send(deck, DeckAction::SetKeyLock { on: !key_lock });
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id(("pitch-fader", deck))
                    .w(px(42.))
                    .flex_1()
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
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(x - px(8.), y + px(h * i as f32 / 10.)),
                                            size(px(16.), px(1.)),
                                        ),
                                        rgb(0x626b76),
                                    ));
                                }
                                let f = if tempo <= 1. {
                                    (tempo - MIN_TEMPO) / (1. - MIN_TEMPO) * 0.5
                                } else {
                                    0.5 + (tempo - 1.) / (MAX_TEMPO - 1.) * 0.5
                                };
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
                    .label(format!("{:+.1}%", (tempo - 1.) * 100.))
                    .tooltip("Gradually return tempo to zero adjustment.")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tempo_returns[deck] = Some((tempo, Instant::now()));
                        cx.notify();
                    })),
            )
    }
}
fn pitch_ratio(fraction: f64) -> f64 {
    if fraction <= 0.5 {
        MIN_TEMPO + fraction * 2. * (1. - MIN_TEMPO)
    } else {
        1. + (fraction - 0.5) * 2. * (MAX_TEMPO - 1.)
    }
}

#[cfg(test)]
mod pitch_tests {
    #[test]
    fn pitch_fader_has_unity_at_centre_and_preserves_full_tempo_range() {
        assert_eq!(super::pitch_ratio(0.), super::MIN_TEMPO);
        assert_eq!(super::pitch_ratio(0.5), 1.);
        assert_eq!(super::pitch_ratio(1.), super::MAX_TEMPO);
        assert!(super::pitch_ratio(0.49) < 1.);
        assert!(super::pitch_ratio(0.51) > 1.);
    }
}
