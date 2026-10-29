use super::*;

impl Shell {
    pub(super) fn refresh_deck_artwork(&mut self, cx: &mut Context<Self>) {
        for deck in 0..DECKS {
            let path = self.deck_paths[deck].clone();
            if self.artwork_paths[deck] == path {
                continue;
            }
            self.artwork_paths[deck] = path.clone();
            self.deck_artwork[deck] = None;
            let Some(path) = path else {
                continue;
            };
            let cover = self.library.read(cx).deck_artwork_path(&path);
            cx.spawn(async move |this, cx| {
                let file = path.clone();
                let artwork = cx
                    .background_spawn(async move {
                        cover
                            .and_then(|path| std::fs::read(path).ok())
                            .map(|data| djcraft_library::Artwork {
                                data,
                                mime_type: None,
                            })
                            .or_else(|| djcraft_library::read_artwork(&file))
                    })
                    .await
                    .and_then(|artwork| {
                        let format = match artwork.data.as_slice() {
                            [0xff, 0xd8, ..] => ImageFormat::Jpeg,
                            [0x89, b'P', b'N', b'G', ..] => ImageFormat::Png,
                            _ => return None,
                        };
                        Some(Arc::new(Image::from_bytes(format, artwork.data)))
                    });
                let _ = this.update(cx, |this, cx| {
                    if this.artwork_paths[deck].as_ref() == Some(&path) {
                        this.deck_artwork[deck] = artwork;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    pub(super) fn compact_deck_header(
        &self,
        deck: usize,
        state: &DeckState,
        fallback: SharedString,
        cx: &Context<Self>,
    ) -> Div {
        let metadata = self.deck_metadata[deck]
            .as_ref()
            .filter(|_| state.track.is_some() && !state.loading && state.error.is_none());
        let title: SharedString = metadata
            .filter(|m| !m.0.is_empty())
            .map(|m| m.0.clone().into())
            .unwrap_or(fallback);
        let artist = metadata
            .map(|m| m.1.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if state.track.is_some() {
                    "Unknown artist".into()
                } else {
                    "Drop a track to load".into()
                }
            });
        let accent = deck_colour(deck);
        let bpm = self.grids[deck]
            .map(|g| format!("{:.2}", g.bpm * state.tempo))
            .unwrap_or_else(|| "—".into());
        let key = self.deck_keys[deck]
            .map(|k| {
                let offset = state.key_shift as f64
                    + if state.key_lock {
                        0.
                    } else {
                        12. * state.tempo.log2()
                    };
                k.shifted(offset.round() as i32).to_string()
            })
            .unwrap_or_else(|| "—".into());
        div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .h(px(58.))
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(22.))
                    .h_full()
                    .flex_shrink_0()
                    .bg(Hsla::from(accent).opacity(0.18))
                    .text_color(accent)
                    .font_weight(FontWeight::BOLD)
                    .child(deck_label(deck)),
            )
            .child(
                div()
                    .w(px(48.))
                    .h(px(48.))
                    .flex_shrink_0()
                    .rounded_sm()
                    .overflow_hidden()
                    .bg(cx.theme().secondary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(self.deck_artwork[deck].clone(), |art, image| {
                        art.child(img(image).w_full().h_full().object_fit(ObjectFit::Contain))
                    })
                    .when(self.deck_artwork[deck].is_none(), |art| {
                        art.child(div().text_color(cx.theme().muted_foreground).child("♪"))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(artist),
                    ),
            )
            .child(
                div()
                    .w(px(146.))
                    .flex_shrink_0()
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .pl_2()
                    .flex()
                    .flex_col()
                    .items_end()
                    .child(
                        div()
                            .id(("header-bpm", deck))
                            .text_size(px(26.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(accent)
                            .child(bpm)
                            .on_scroll_wheel(cx.listener(
                                move |this, event: &ScrollWheelEvent, _, cx| {
                                    this.wheel_tempo(deck, event, cx)
                                },
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(div().text_xs().child(format!("KEY {key}")))
                            .children([-1., 1.].into_iter().enumerate().map(|(direction, step)| {
                                Button::new(("header-key-step", deck * 2 + direction))
                                    .xsmall()
                                    .outline()
                                    .w(px(16.))
                                    .p_0()
                                    .label(if direction == 0 { "−" } else { "+" })
                                    .disabled(state.track.is_none())
                                    .tooltip(if direction == 0 {
                                        "Lower key by one semitone."
                                    } else {
                                        "Raise key by one semitone."
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(engine) = this.engine.as_mut() {
                                            let semitones = (engine.state().decks[deck].key_shift
                                                + step)
                                                .clamp(-12., 12.);
                                            this.send(deck, DeckAction::SetKeyShift { semitones });
                                            cx.notify();
                                        }
                                    }))
                            }))
                            .child(
                                Button::new(("header-key-lock", deck))
                                    .xsmall()
                                    .outline()
                                    .label("MT")
                                    .selected(state.key_lock)
                                    .tooltip(
                                        "Master tempo: preserve musical key when changing speed.",
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let on = this
                                            .engine
                                            .as_mut()
                                            .map(|e| !e.state().decks[deck].key_lock)
                                            .unwrap_or(true);
                                        this.send(deck, DeckAction::SetKeyLock { on });
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    pub(super) fn deck_header_times(&self, state: &DeckState, cx: &Context<Self>) -> Div {
        let duration = state.track.as_ref().map(|t| t.duration_seconds);
        div()
            .w(px(146.))
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .pl_2()
            .flex()
            .justify_between()
            .children(
                [
                    (
                        "ELAPSED",
                        duration.map(|_| clock(state.position_seconds / state.tempo)),
                    ),
                    (
                        "REMAIN",
                        duration.map(|d| clock((d - state.position_seconds).max(0.) / state.tempo)),
                    ),
                ]
                .into_iter()
                .map(|(label, value)| {
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(9.))
                                .text_color(cx.theme().muted_foreground)
                                .child(label),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(value.unwrap_or_else(|| "—".into())),
                        )
                }),
            )
    }
}
