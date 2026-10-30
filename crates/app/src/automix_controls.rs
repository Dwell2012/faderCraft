use super::*;
use djcraft_protocol::QueueTrack;
impl Shell {
    pub(super) fn library_side_panel(&self, cx: &mut Context<Self>) -> Div {
        div()
            .w(px(310.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                div().flex().w_full().h(px(29.)).flex_shrink_0().children(
                    ["AUTOMIX", "SAVED LOOPS"]
                        .into_iter()
                        .enumerate()
                        .map(|(tab, label)| {
                            div()
                                .id(("library-side-tab", tab))
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .h(px(29.))
                                .items_center()
                                .justify_center()
                                .text_size(px(11.))
                                .when(tab == 0, |button| button.border_r_1())
                                .border_color(cx.theme().border)
                                .when(self.library_side_tab == tab, |button| {
                                    button.bg(cx.theme().background)
                                })
                                .when(self.library_side_tab != tab, |button| {
                                    button.bg(cx.theme().secondary.opacity(0.5)).border_b_1()
                                })
                                .child(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.library_side_tab = tab;
                                    if tab == 1 && this.saved_loops_deck.is_none() {
                                        this.saved_loops_deck = Some(this.active_deck);
                                    }
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .child(if self.library_side_tab == 0 {
                self.automix_panel(cx)
            } else {
                self.saved_loops_panel(self.saved_loops_deck.unwrap_or(self.active_deck), cx)
            })
    }
    fn automix_send(&mut self, action: MixerAction, cx: &mut Context<Self>) {
        if let Some(engine) = &mut self.engine {
            match engine.apply(Role::Host, Action::Mixer { action }) {
                Ok(()) => {
                    self.automix_notice.clear();
                    self.tempo_returns = [None; DECKS];
                }
                Err(error) => {
                    self.automix_notice = format!(
                        "Automix: {error:?}. Queue tracks or play a deck first; wait for any loading to finish."
                    )
                }
            }
        }
        cx.notify();
    }
    pub(super) fn enqueue_automix(&mut self, mut tracks: Vec<QueueTrack>, cx: &mut Context<Self>) {
        for track in &mut tracks {
            track.grid = self
                .library
                .read(cx)
                .beat_grid_at(Path::new(&track.path))
                .map(|g| djcraft_protocol::BeatGrid {
                    bpm: g.bpm,
                    first_beat: g.first_beat_seconds,
                    downbeat: g.downbeat,
                    confidence: g.confidence,
                });
        }
        self.automix_send(MixerAction::AutomixEnqueue { tracks }, cx);
    }
    pub(super) fn restore_automix(&mut self, cx: &mut Context<Self>) {
        match std::fs::read(data_dir().join("automix-queue.json")) {
            Ok(bytes) => match serde_json::from_slice::<Vec<QueueTrack>>(&bytes) {
                Ok(tracks) => {
                    let tracks: Vec<_> = tracks
                        .into_iter()
                        .filter_map(|mut track| {
                            track.path = crate::track_identity::resolve(&track.path)?;
                            Some(track)
                        })
                        .collect();
                    self.saved_queue = tracks.clone();
                    self.enqueue_automix(tracks, cx);
                }
                Err(error) => self.automix_notice = format!("Could not restore queue: {error}"),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => self.automix_notice = format!("Could not restore queue: {error}"),
        }
    }
    pub(super) fn persist_automix(&mut self) {
        if self.engine.is_none() {
            return;
        }
        let mut remaining = Vec::new();
        if !self.automix.transitioning {
            remaining.extend(self.automix.next.clone());
        }
        remaining.extend(self.automix.queue.clone());
        if self.saved_queue == remaining {
            return;
        }
        let result = (|| -> Result<(), String> {
            std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
            let tmp = data_dir().join("automix-queue.json.tmp");
            std::fs::write(
                &tmp,
                serde_json::to_vec_pretty(
                    &remaining
                        .iter()
                        .cloned()
                        .map(|mut track| {
                            track.path = crate::track_identity::key(&track.path);
                            track
                        })
                        .collect::<Vec<_>>(),
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            std::fs::rename(tmp, data_dir().join("automix-queue.json")).map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => self.saved_queue = remaining,
            Err(e) => self.automix_notice = format!("Could not save queue: {e}"),
        }
    }
    fn save_automix_playlist(&mut self, cx: &mut Context<Self>) {
        let mut tracks: Vec<_> = self.automix.next.clone().into_iter().collect();
        tracks.extend(self.automix.queue.clone());
        let result = self
            .library
            .update(cx, |view, cx| view.save_automix_playlist(&tracks, cx));
        self.automix_notice = match result {
            Ok(name) => format!("Saved playlist: {name}"),
            Err(error) => error,
        };
        self.browser.update(cx, |view, cx| view.refresh(cx));
        cx.notify();
    }
    pub(super) fn automix_panel(&self, cx: &mut Context<Self>) -> Div {
        let enabled = self.automix.enabled;
        let smart = self.automix.smart;
        let seconds = self.automix.fade_seconds;
        let mut panel =
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .gap_1()
                .p(px(4.))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .child(format!("AUTOMIX · {} queued", self.automix.queue.len())),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .child("Uses both decks and the crossfader. Master stays yours."),
                )
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            Button::new("auto-start")
.tooltip("Resume automix on a non-playing deck, or take over the current mix manually.")
                                .small()
                                .label(if enabled { "Take over" } else { "Resume" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.automix_send(
                                        if enabled {
                                            MixerAction::AutomixStop
                                        } else {
                                            MixerAction::AutomixStart
                                        },
                                        cx,
                                    )
                                })),
                        )
                        .child(
                            Button::new("auto-next")
.tooltip("Start the transition to the next queued track now.")
                                .small()
                                .label("Next")
                                .disabled(!enabled || self.automix.transitioning)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.automix_send(MixerAction::AutomixNext, cx)
                                })),
                        )
                        .child(
                            Button::new("auto-clear")
.tooltip("Clear the remaining automix queue.")
                                .small()
                                .label("Clear queue")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.automix_send(MixerAction::AutomixClear, cx)
                                })),
                        ),
                )
                .child(
                    div()
                        .id("auto-duration")
                        .flex()
                        .items_center()
                        .gap_1()
                        .on_scroll_wheel(cx.listener(
                            move |this, event: &ScrollWheelEvent, _, cx| {
                                let seconds =
                                    (seconds + crate::wheel::steps(event) as f64).clamp(0.1, 30.);
                                this.automix_send(MixerAction::AutomixFadeTime { seconds }, cx);
                                cx.stop_propagation();
                            },
                        ))
                        .child(Button::new("auto-shorter")
.tooltip("Shorten the timed crossfade by one second.").xsmall().label("−").on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.automix_send(
                                    MixerAction::AutomixFadeTime {
                                        seconds: (seconds - 1.).max(0.1),
                                    },
                                    cx,
                                )
                            }),
                        ))
                        .child(format!("Fade {seconds:.1}s"))
                        .child(Button::new("auto-longer")
.tooltip("Lengthen the timed crossfade by one second.").xsmall().label("+").on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.automix_send(
                                    MixerAction::AutomixFadeTime {
                                        seconds: (seconds + 1.).min(30.),
                                    },
                                    cx,
                                )
                            }),
                        )),
                )
                .child(
                    Button::new("auto-smart")
.tooltip("Switch between beat-matched EQ transitions and timed crossfades.")
                        .small()
                        .label(if smart { "Beat + EQ" } else { "Timed fade" })
                        .disabled(enabled)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.automix_send(MixerAction::AutomixSmart { enabled: !smart }, cx)
                        })),
                )
                .child(div().text_size(px(10.)).child(if smart {
                    "Classic EQ · ±8% varispeed (pitch follows tempo)"
                } else {
                    "Timed crossfade only"
                }))
                .child(div().text_size(px(10.)).child(self.automix.status.clone()));
        if let Some(plan) = &self.automix.plan {
            panel = panel.child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0x152432))
                    .text_size(px(11.))
                    .child(div().child(plan.description.clone()))
                    .child(div().child(plan.vocal_note.clone()))
                    .child(div().child(format!(
                        "OUT {} → IN {}",
                        clock(plan.outgoing_seconds),
                        clock(plan.incoming_seconds)
                    )))
                    .child(div().child(format!(
                        "{} · {:.1}s · EQ in +{:.1}s · bass +{:.1}s",
                        plan.bars.map_or("Timed".into(), |n| format!("{n} bars")),
                        plan.duration_seconds,
                        plan.duration_seconds * plan.eq_start as f64,
                        plan.duration_seconds * plan.bass_swap as f64
                    ))),
            );
        }
        if let Some(next) = &self.automix.next {
            panel = panel.child(
                div()
                    .text_size(px(11.))
                    .truncate()
                    .child(format!("Next: {}", next.title)),
            );
        }
        let mut rows = div()
            .id("automix-queue")
            .flex_1()
            .min_h(px(100.))
            .overflow_y_scroll()
            .drag_over::<DraggedTrack>(|s, _, _, _| s.bg(rgb(0x183849)))
            .on_drop(cx.listener(|this, track: &DraggedTrack, _, cx| {
                this.enqueue_automix(
                    vec![QueueTrack {
                        grid: None,
                        path: track.path.to_string_lossy().into_owned(),
                        title: track.title.to_string(),
                    }],
                    cx,
                );
                cx.stop_propagation();
            }));
        if self.automix.queue.is_empty() {
            rows = rows.child(
                div()
                    .p_2()
                    .text_xs()
                    .child("Drop tracks here · or use Queue all in a playlist"),
            );
        }
        for (index, track) in self.automix.queue.iter().enumerate() {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.))
                            .child(format!("{}. {}", index + 1, track.title)),
                    )
                    .child(
                        Button::new(("auto-up", index))
                            .tooltip("Move this track earlier in the queue.")
                            .xsmall()
                            .label("↑")
                            .disabled(index == 0)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.automix_send(
                                    MixerAction::AutomixMove {
                                        from: index,
                                        to: index.saturating_sub(1),
                                    },
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new(("auto-down", index))
                            .tooltip("Move this track later in the queue.")
                            .xsmall()
                            .label("↓")
                            .disabled(index + 1 >= self.automix.queue.len())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.automix_send(
                                    MixerAction::AutomixMove {
                                        from: index,
                                        to: index + 1,
                                    },
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new(("auto-remove", index))
                            .tooltip("Remove this track from the queue.")
                            .xsmall()
                            .label("×")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.automix_send(MixerAction::AutomixRemove { index }, cx)
                            })),
                    ),
            );
        }
        panel = panel.child(rows).child(
            Button::new("auto-save-playlist")
                .tooltip("Save the remaining queue as a playlist.")
                .small()
                .label("Save queue as playlist")
                .on_click(cx.listener(|this, _, _, cx| this.save_automix_playlist(cx))),
        );
        if !self.automix_notice.is_empty() {
            panel = panel.child(div().text_size(px(10.)).child(self.automix_notice.clone()));
        }
        panel
    }
}
