use super::*;
impl Shell {
    pub(super) fn ensure_loudness(&mut self, cx: &mut Context<Self>) {
        for deck in 0..DECKS {
            let Some(track) = self
                .engine
                .as_ref()
                .and_then(|e| e.track(deck))
                .filter(|t| t.is_complete())
                .cloned()
            else {
                continue;
            };
            let state = self.engine.as_mut().unwrap().state();
            let ratio = state.decks[deck].tempo;
            let key_lock = state.decks[deck].key_lock;
            if self.loudness_tracks[deck]
                .as_ref()
                .is_some_and(|(t, r, k)| Arc::ptr_eq(t, &track) && *r == ratio && *k == key_lock)
            {
                continue;
            }
            self.loudness_tracks[deck] = Some((track.clone(), ratio, key_lock));
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(350))
                    .await;
                let current = this
                    .update(cx, |this, _| {
                        this.loudness_tracks[deck]
                            .as_ref()
                            .is_some_and(|(t, r, k)| {
                                Arc::ptr_eq(t, &track) && *r == ratio && *k == key_lock
                            })
                    })
                    .unwrap_or(false);
                if !current {
                    return;
                }
                let worker = track.clone();
                let peak = cx
                    .background_executor()
                    .spawn(async move {
                        if worker.loudness().is_none() {
                            let measured = djcraft_analysis::measure_loudness(
                                &worker.export_stereo(),
                                worker.sample_rate,
                            )
                            .map(|(rms_db, peak_db)| {
                                djcraft_protocol::LoudnessInfo { rms_db, peak_db }
                            });
                            let _ = worker.set_loudness(measured);
                        }
                        if key_lock && (ratio - 1.).abs() > 1e-3 {
                            djcraft_engine::measure_key_lock_peak(&worker, ratio)
                        } else {
                            None
                        }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    let current = this
                        .engine
                        .as_ref()
                        .and_then(|e| e.track(deck))
                        .is_some_and(|t| Arc::ptr_eq(t, &track))
                        && this.loudness_tracks[deck]
                            .as_ref()
                            .is_some_and(|(_, r, k)| *r == ratio && *k == key_lock);
                    if current {
                        if let Some(peak_db) = peak {
                            this.send(deck, DeckAction::SetProcessedPeak { ratio, peak_db });
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }
    pub(super) fn suggestions_panel(
        &self,
        deck: usize,
        state: &DeckState,
        cx: &mut Context<Self>,
    ) -> Div {
        let preparation = state
            .path
            .as_ref()
            .and_then(|p| self.preparations.get(&crate::track_identity::key(p)))
            .cloned()
            .unwrap_or_default();
        let duration = state.track.as_ref().map_or(0., |t| t.duration_seconds);
        if duration <= 0. || state.loading {
            return div();
        }
        let vocals = self
            .engine
            .as_ref()
            .and_then(|e| e.track(deck))
            .and_then(|t| t.vocal_regions());
        let suggestions: Vec<_> = crate::preparation::cue_suggestions(
            &preparation,
            vocals,
            self.grids[deck].map(|g| djcraft_analysis::Tempo {
                bpm: g.bpm,
                first_beat_seconds: g.first_beat_seconds,
                downbeat: g.downbeat,
                confidence: g.confidence as f32,
            }),
            duration,
        )
        .into_iter()
        .filter(|s| {
            state
                .hot_cues
                .iter()
                .flatten()
                .all(|c| (c.seconds - s.seconds).abs() > 0.25)
        })
        .collect();
        let mut ghosts = div().relative().h(px(7.)).w_full();
        let mut row = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .items_center()
            .child(div().text_size(px(10.)).child("Suggested cues:"));
        for (i, suggestion) in suggestions.iter().enumerate() {
            ghosts = ghosts.child(
                div()
                    .absolute()
                    .left(relative((suggestion.seconds / duration) as f32))
                    .h_full()
                    .w(px(2.))
                    .bg(rgb(0xc8ff38).opacity(0.45)),
            );
            let cue = suggestion.clone();
            row = row.child(
                Button::new(("accept-cue", deck * 16 + i))
                    .tooltip("Save this bar-aligned suggestion in the next empty hot cue slot.")
                    .xsmall()
                    .outline()
                    .label(format!(
                        "{} {}",
                        suggestion.label,
                        clock(suggestion.seconds)
                    ))
                    .disabled(state.hot_cues.iter().all(Option::is_some))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.accept_suggested_cues(deck, std::slice::from_ref(&cue), cx)
                    })),
            );
        }
        let empty = state.hot_cues.iter().any(Option::is_none);
        let has_suggestions = !suggestions.is_empty();
        row = row.child(
            Button::new(("fill-cues", deck))
                .tooltip("Fill empty hot cue slots with suggestions; keep existing cues.")
                .xsmall()
                .outline()
                .label("Fill empty slots")
                .disabled(!empty || !has_suggestions)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.accept_suggested_cues(deck, &suggestions, cx)
                })),
        );
        let text = if let Some(level) = state.loudness {
            format!(
                "Peak {:.1} dBFS · suggested trim {:+.1} dB · {}{}",
                level.peak_db,
                level.recommended_trim(),
                if !state.auto_trim {
                    "manual"
                } else if state.loudness_applied {
                    "peak normalized"
                } else {
                    "pending pause / Apply trim"
                },
                if level.recommended_trim() >= 12. {
                    " · boost limited to +12 dB"
                } else {
                    ""
                }
            )
        } else if self
            .engine
            .as_ref()
            .and_then(|e| e.track(deck))
            .is_some_and(|t| t.loudness().is_some())
        {
            "Silent track · trim unchanged".into()
        } else {
            "Measuring loudness…".into()
        };
        let text = if state.key_lock_protection_db < -0.05 {
            format!(
                "{text} · Key-lock peak protection {:+.1} dB",
                state.key_lock_protection_db
            )
        } else {
            text
        };
        let auto = state.auto_trim;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(ghosts)
            .child(row)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .items_center()
                    .child(div().text_size(px(10.)).child(text))
                    .child(
                        Button::new(("match-trim", deck))
.tooltip("Normalize the full track's peak, including cubic playback interpolation, to -0.1 dBFS, up to +12 dB boost. EQ, effects and mixing can add gain.")
                            .xsmall()
                            .outline()
                            .label("Apply trim")
                            .disabled(state.loudness.is_none())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.send(deck, DeckAction::SetAutoTrim { enabled: true });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(("auto-trim", deck))
.tooltip("Automatically normalize the full track's playback peak to -0.1 dBFS. Manual trim adjustments disable it.")
                            .xsmall()
                            .outline()
                            .label(if auto {
                                "Auto trim on"
                            } else {
                                "Auto trim off"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.send(deck, DeckAction::SetAutoTrim { enabled: !auto });
                                cx.notify();
                            })),
                    ),
            )
    }
}
