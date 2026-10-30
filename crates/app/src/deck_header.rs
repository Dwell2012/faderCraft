use super::*;
use gpui_kit::component::tooltip::Tooltip;

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
        let key_hover = self.key_hover_text(deck, state);
        div()
            .flex()
            .when(deck % 2 == 1, |row| row.flex_row_reverse())
            .items_center()
            .min_w_0()
            .h(px(72.))
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .when(deck % 2 == 1, |row| row.flex_row_reverse())
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .gap(px(4.))
                    .bg(cx.theme().secondary.opacity(0.5))
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
                            .w(px(72.))
                            .h_full()
                            .flex_shrink_0()
                            .overflow_hidden()
                            .bg(cx.theme().secondary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .when_some(self.deck_artwork[deck].clone(), |art, image| {
                                art.child(
                                    img(image).w_full().h_full().object_fit(ObjectFit::Contain),
                                )
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
                            .when(deck % 2 == 1, |text| text.text_right())
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
                    ),
            )
            .child(
                div()
                    .w(px(146.))
                    .flex_shrink_0()
                    .px(px(4.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .items_end()
                    .when(deck % 2 == 1, |controls| controls.items_start())
                    .child(
                        div()
                            .id(("header-bpm", deck))
                            .w_full()
                            .h(px(40.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_center()
                            .bg(cx.theme().secondary.opacity(0.5))
                            .p(px(4.))
                            .text_size(px(26.))
                            .line_height(px(26.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(accent)
                            .child(div().h(px(26.)).relative().top(px(-1.)).child(bpm))
                            .on_scroll_wheel(cx.listener(
                                move |this, event: &ScrollWheelEvent, _, cx| {
                                    this.wheel_tempo(deck, event, cx)
                                },
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .h(px(28.))
                            .flex_shrink_0()
                            .justify_center()
                            .bg(cx.theme().secondary.opacity(0.5))
                            .p(px(4.))
                            .items_center()
                            .child(div().id(("header-key-info", deck)).w(relative(0.3)).flex_shrink_0().text_center().text_xs().child("KEY"))
                            .child(div().flex().w(relative(0.4)).flex_shrink_0().min_w_0().items_center().justify_center().gap(px(2.))
                            .children((0..3).map(|position| {
                                if position == 1 {
                                    let hover = key_hover.clone();
                                    return div()
                                        .id(("header-key-value", deck))
                                        .text_xs()
                                        .child(key.clone())
                                        .tooltip(move |window, cx| {
                                            Tooltip::new(hover.clone()).build(window, cx)
                                        })
                                        .into_any_element();
                                }
                                let direction = usize::from(position == 2);
                                let step = if direction == 0 { -1. } else { 1. };
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
                                    .into_any_element()
                            })))
                            .child(
                                div().flex().w(relative(0.3)).flex_shrink_0().justify_center().child(
                                Button::new(("header-key-lock", deck))
                                    .xsmall()
                                    .outline()
                                    .w(px(26.))
                                    .p_0()
                                    .icon(IconName::Lock)
                                    .selected(state.key_lock)
                                    .when(state.key_lock, |button| button.bg(Hsla::from(accent)).text_color(rgb(0x070b12)))
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
                                    }))),
                            ),
                    ),
            )
    }

    pub(super) fn deck_header_times(
        &self,
        _deck: usize,
        state: &DeckState,
        cx: &Context<Self>,
    ) -> Div {
        let duration = state.track.as_ref().map(|t| t.duration_seconds);
        div()
            .w(px(146.))
            .flex_shrink_0()
            .px(px(4.))
            .h_full()
            .flex()
            .gap(px(4.))
            .justify_between()
            .child(
                div()
                    .flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .bg(cx.theme().secondary.opacity(0.5))
                    .children(
                        [
                            (
                                "ELAPSED",
                                duration.map(|_| clock(state.position_seconds / state.tempo)),
                            ),
                            (
                                "REMAIN",
                                duration.map(|d| {
                                    clock((d - state.position_seconds).max(0.) / state.tempo)
                                }),
                            ),
                        ]
                        .into_iter()
                        .map(|(label, value)| {
                            div()
                                .flex()
                                .flex_1()
                                .p(px(4.))
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
                    ),
            )
    }
}
