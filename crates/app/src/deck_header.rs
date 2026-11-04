use super::*;
use gpui_kit::component::tooltip::Tooltip;

fn clock_hundredths(seconds: f64) -> String {
    let hundredths = (seconds.max(0.) * 100.).floor() as u64;
    format!(
        "{}:{:02}.{:02}",
        hundredths / 6000,
        (hundredths / 100) % 60,
        hundredths % 100
    )
}

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

    pub(super) fn deck_header_key_controls(
        &self,
        deck: usize,
        state: &DeckState,
        cx: &Context<Self>,
    ) -> Div {
        let accent = deck_colour(deck);
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
            .unwrap_or_else(|| "\u{2014}".into());
        let key_hover = self.key_hover_text(deck, state);
        div()
            .flex()
            .w(px(132.))
            .h(px(29.))
            .flex_shrink_0()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .gap(px(4.))
                    .children((0..3).map(|position| {
                        if position == 1 {
                            let hover = key_hover.clone();
                            return div()
                                .id(("header-key-value", deck))
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .w(px(33.))
                                .flex_shrink_0()
                                .min_w_0()
                                .h(px(29.))
                                .bg(cx.theme().secondary.opacity(0.5))
                                .child(
                                    div()
                                        .id(("header-key-info", deck))
                                        .text_size(px(9.))
                                        .line_height(px(9.))
                                        .text_color(cx.theme().muted_foreground)
                                        .child("KEY"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .line_height(px(12.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(key.clone()),
                                )
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
                            .rounded(ButtonRounded::None)
                            .w(px(29.))
                            .h(px(29.))
                            .min_w_0()
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
                                    let semitones = (engine.state().decks[deck].key_shift + step)
                                        .clamp(-12., 12.);
                                    this.send(deck, DeckAction::SetKeyShift { semitones });
                                    cx.notify();
                                }
                            }))
                            .into_any_element()
                    })),
            )
            .child(
                div().flex().flex_shrink_0().child(
                    Button::new(("header-key-lock", deck))
                        .xsmall()
                        .outline()
                        .rounded(ButtonRounded::None)
                        .w(px(29.))
                        .h(px(29.))
                        .min_w_0()
                        .p_0()
                        .icon(IconName::Lock)
                        .selected(state.key_lock)
                        .when(state.key_lock, |button| {
                            button.bg(Hsla::from(accent)).text_color(rgb(0x070b12))
                        })
                        .tooltip("Master tempo: preserve musical key when changing speed.")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.run_skin_button(
                                deck,
                                djcraft_protocol::script::ButtonAction::ToggleKeyLock,
                                cx,
                            );
                        })),
                ),
            )
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
        div()
            .flex()
            .gap(px(4.))
            .when(deck % 2 == 1, |row| row.flex_row_reverse())
            .items_center()
            .min_w_0()
            .h(px(if self.settings.four_decks { 40. } else { 73. }))
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
                            .when(self.settings.four_decks, |text| text.items_center())
                            .when(!self.settings.four_decks, |text| text.flex_col().gap_1())
                            .when(deck % 2 == 1, |text| text.text_right().pr(px(4.)))
                            .when(deck % 2 == 0, |text| text.pl(px(4.)))
                            .child(
                                div()
                                    .min_w_0()
                                    .when(self.settings.four_decks, |line| line.w_full())
                                    .when(self.settings.four_decks && deck % 2 == 1, |line| {
                                        line.text_right()
                                    })
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .child(if self.settings.four_decks {
                                        format!("{title} — {artist}").into()
                                    } else {
                                        title
                                    }),
                            )
                            .when(!self.settings.four_decks, |text| {
                                text.child(
                                    div()
                                        .truncate()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(artist),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .w(px(132.))
                    .flex_shrink_0()
                    .h(px(if self.settings.four_decks { 40. } else { 73. }))
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
                            .py(px(4.))
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
                    .when(!self.settings.four_decks, |controls| {
                        controls.child(self.deck_header_key_controls(deck, state, cx))
                    }),
            )
    }

    pub(super) fn deck_header_times(
        &self,
        deck: usize,
        state: &DeckState,
        cx: &Context<Self>,
    ) -> Div {
        let duration = state.track.as_ref().map(|t| t.duration_seconds);
        let beat = self.grids[deck]
            .filter(|grid| state.position_seconds >= grid.first_beat_seconds)
            .map(|grid| {
                ((state.position_seconds - grid.first_beat_seconds) / grid.beat_seconds()).floor()
                    as usize
                    % 4
            });
        div()
            .w(px(132.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .gap(px(4.))
            .justify_between()
            .child(
                div()
                    .flex()
                    .w_full()
                    .h_full()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(2.))
                    .bg(cx.theme().secondary.opacity(0.5))
                    .child(
                        div().flex().w_full().children(
                            [
                                (
                                    "ELAPSED",
                                    duration.map(|_| {
                                        clock_hundredths(state.position_seconds / state.tempo)
                                    }),
                                ),
                                (
                                    "REMAIN",
                                    duration.map(|d| {
                                        clock_hundredths(
                                            (d - state.position_seconds).max(0.) / state.tempo,
                                        )
                                    }),
                                ),
                            ]
                            .into_iter()
                            .map(|(label, value)| {
                                let (main_time, decimals) = value
                                    .and_then(|time| {
                                        time.split_once('.').map(|(main, fraction)| {
                                            (main.to_owned(), format!(".{fraction}"))
                                        })
                                    })
                                    .unwrap_or_else(|| ("—".into(), String::new()));
                                div()
                                    .flex()
                                    .flex_1()
                                    .py(px(4.))
                                    .flex_col()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(9.))
                                            .line_height(px(9.))
                                            .text_color(cx.theme().muted_foreground)
                                            .child(label),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_end()
                                            .line_height(px(12.))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(main_time),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .relative()
                                                    .top(px(2.))
                                                    .text_color(
                                                        cx.theme().muted_foreground.opacity(0.7),
                                                    )
                                                    .child(decimals),
                                            ),
                                    )
                            }),
                        ),
                    )
                    .when(!self.settings.four_decks, |times| {
                        times.child(div().flex().justify_center().gap_2().children((0..4).map(
                            |i| {
                                div().w(px(22.)).h(px(3.)).rounded_sm().bg(
                                    if state.track.is_some() && beat == Some(i) {
                                        deck_colour(deck).into()
                                    } else {
                                        cx.theme().border
                                    },
                                )
                            },
                        )))
                    }),
            )
    }
}
