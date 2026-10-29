use super::*;

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
    pub(super) fn transport_anchor_for(
        &mut self,
        deck: usize,
        action: &DeckAction,
    ) -> Option<(f64, Option<String>)> {
        if !matches!(
            action,
            DeckAction::Play
                | DeckAction::Load { .. }
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
    /// Both decks share the same width and centre playhead for beat comparison.
    pub(super) fn rhythm_panel(&self, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h(px(if self.settings.four_decks { 100. } else { 168. }))
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
                                            if state.eq_mode == EqMode::Classic { state.eq } else { [1.; 3] },
                                            state.stem_gains,
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
        controls: Option<AnyElement>,
        identity: Option<AnyElement>,
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
        let platter = div()
            .id(("platter", index))
            .relative()
            .w(px(if self.settings.four_decks { 64. } else { 140. }))
            .h(px(if self.settings.four_decks { 64. } else { 140. }))
            .flex_shrink_0()
            .rounded_full()
            .border_1()
            .border_color(ring)
            .bg(cx.theme().secondary.opacity(0.3))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let centre =
                            bounds.origin + point(bounds.size.width / 2., bounds.size.height / 2.);
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
            return div()
                .flex()
                .h(px(120.))
                .flex_shrink_0()
                .items_center()
                .gap_3()
                .when(index % 2 == 1, |row| row.flex_row_reverse())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .h_full()
                        .items_center()
                        .gap_3()
                        .when(index % 2 == 1, |group| group.flex_row_reverse())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .justify_start()
                                .gap_3()
                                .when(index % 2 == 1, |group| group.flex_row_reverse())
                                .child(platter)
                                .child(
                                    div()
                                        .id(("compact-tempo", index))
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .when(index % 2 == 1, |info| info.items_end().text_right())
                                        .on_scroll_wheel(cx.listener(
                                            move |this, event: &ScrollWheelEvent, _, cx| {
                                                this.wheel_tempo(index, event, cx)
                                            },
                                        ))
                                        .child(
                                            div().text_size(px(10.)).text_color(muted).child("BPM"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(22.))
                                                .text_color(colour)
                                                .child(tempo_text),
                                        )
                                        .child(div().text_size(px(10.)).child(format!(
                                            "−{} remaining",
                                            clock(remaining / state.tempo)
                                        )))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .text_size(px(11.))
                                                .text_color(colour)
                                                .whitespace_nowrap()
                                                .children(
                                                    current_key_label(self.deck_keys[index], state)
                                                        .split(" · ")
                                                        .enumerate()
                                                        .map(|(part, line)| {
                                                            div()
                                                                .when(part > 0, |label| {
                                                                    label
                                                                        .ml(px(7.))
                                                                        .pl(px(7.))
                                                                        .border_l_1()
                                                                        .border_color(
                                                                            muted.opacity(0.3),
                                                                        )
                                                                })
                                                                .child(line.to_owned())
                                                        })
                                                        .collect::<Vec<_>>(),
                                                ),
                                        ),
                                ),
                        ),
                )
                .child(self.pitch_control(index, state, cx))
                .children(controls)
                .children(identity);
        }

        div()
            .flex()
            .flex_col()
            .gap_2()
            .h(px(198.))
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .gap_3()
                    .when(index % 2 == 1, |row| row.flex_row_reverse())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(platter)
                            .child(div().flex().gap_2().children((0..4).map(|i| {
                                div().w(px(22.)).h(px(3.)).rounded_sm().bg(
                                    if loaded && beat == Some(i) {
                                        colour.into()
                                    } else {
                                        ring
                                    },
                                )
                            }))),
                    )
                    .child(self.pitch_control(index, state, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(ring)
                    .pt_2()
                    .children(controls),
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
    fn pitch_control(&self, deck: usize, state: &DeckState, cx: &mut Context<Self>) -> Div {
        let area = self.pitch_areas[deck].clone();
        let colour = deck_colour(deck);
        let tempo = state.tempo;
        let key_lock = state.key_lock;
        let range = state.tempo_range;
        div()
            .flex()
            .flex_col()
            .items_center()
            .w(px(80.))
            .h_full()
            .gap_1()
            .child(
                Button::new(("pitch-range", deck))
                    .outline()
                    .xsmall()
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
    if matches!(action, DeckAction::Load { .. }) {
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
