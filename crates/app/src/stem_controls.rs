use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use std::sync::Arc;
use std::sync::atomic::AtomicU8;

impl Shell {
    pub(super) fn prepare_stems_button(&self, deck: usize, cx: &Context<Self>) -> Button {
        let track = self.engine.as_ref().and_then(|e| e.track(deck));
        let ready = track.is_some_and(|track| track.stems_ready());
        let label = if self.stem_busy[deck] {
            match self.stem_progress[deck].load(std::sync::atomic::Ordering::Relaxed) {
                255 => "Queued".to_owned(),
                254 => "Loading…".to_owned(),
                percent if self.settings.four_decks => format!("{percent}%"),
                percent => format!("Stems {percent}%"),
            }
        } else if ready {
            if self.settings.four_decks {
                "Ready"
            } else {
                "Stems ready"
            }
            .to_owned()
        } else {
            "Stems".to_owned()
        };
        Button::new(("prepare-deck", deck))
            .xsmall()
            .outline()
            .label(label)
            .tooltip(if self.stem_status[deck].is_empty() {
                "Prepare and cache stems locally for this deck.".to_owned()
            } else {
                self.stem_status[deck].clone()
            })
            .disabled(
                self.stem_busy[deck] || ready || !track.is_some_and(|track| track.is_complete()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(track) = this.engine.as_ref().and_then(|e| e.track(deck)).cloned() {
                    this.start_stem_job(deck, track, true, cx);
                }
            }))
    }

    pub(super) fn eq_mode_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let view = cx.weak_entity();
        let current = self.eq_mode;
        Button::new("eq-mode")
            .tooltip("Choose Classic EQ or a stem-based mixing mode.")
            .small()
            .outline()
            .label(if self.deck_eq_modes.iter().any(|mode| *mode != current) {
                "Mixed EQ"
            } else {
                current.label()
            })
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for mode in [
                    EqMode::Classic,
                    EqMode::EzRemix,
                    EqMode::Modern,
                    EqMode::Stems,
                ] {
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new(mode.label())
                            .checked(current == mode)
                            .on_click(move |_, _, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    if mode != this.eq_mode
                                        || this.deck_eq_modes.iter().any(|m| *m != mode)
                                    {
                                        for deck in 0..DECKS {
                                            this.send(deck, DeckAction::SetEqMode { mode });
                                        }
                                        this.eq_mode = mode;
                                        for deck in 0..DECKS {
                                            let start = mixer_controls::stem_index(deck);
                                            this.mixer_values[start..start + 4].fill(0.5);
                                        }
                                        this.mixer_drag = None;
                                        cx.notify();
                                    }
                                });
                            }),
                    );
                }
                menu
            })
    }

    pub(super) fn check_stem_cache(&mut self, cx: &mut Context<Self>) {
        for deck in 0..DECKS {
            let track = self.engine.as_ref().and_then(|e| e.track(deck)).cloned();
            let Some(track) = track else {
                continue;
            };
            if !track.is_complete()
                || self.stem_tracks[deck]
                    .as_ref()
                    .is_some_and(|old| Arc::ptr_eq(old, &track))
            {
                continue;
            }
            self.stem_tracks[deck] = Some(track.clone());
            self.mixer_values
                [mixer_controls::stem_index(deck)..mixer_controls::stem_index(deck) + 4]
                .fill(0.5);
            self.start_stem_job(deck, track, false, cx);
        }
    }

    pub(super) fn prepare_stems(&mut self, cx: &mut Context<Self>) {
        for deck in 0..DECKS {
            let track = self.engine.as_ref().and_then(|e| e.track(deck)).cloned();
            if let Some(track) = track {
                if track.is_complete() && !track.stems_ready() && !self.stem_busy[deck] {
                    self.start_stem_job(deck, track, true, cx);
                }
            }
        }
    }

    pub(super) fn start_stem_job(
        &mut self,
        deck: usize,
        track: Arc<djcraft_engine::Track>,
        prepare: bool,
        cx: &mut Context<Self>,
    ) {
        self.stem_busy[deck] = true;
        self.stem_status[deck] = if prepare {
            "Preparing / queued…"
        } else {
            "Checking cache…"
        }
        .into();
        let root = data_dir().join("stems");
        let progress = Arc::new(AtomicU8::new(if prepare { 255 } else { 254 }));
        self.stem_progress[deck] = progress.clone();
        let watched = progress.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let active = this
                    .update(cx, |this, cx| {
                        if this.stem_busy[deck] && Arc::ptr_eq(&this.stem_progress[deck], &watched)
                        {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
        let worker_track = track.clone();
        let work = cx.background_executor().spawn(async move {
            let ready = crate::stems::load_or_prepare(&worker_track, &root, prepare, &progress)?;
            if ready && worker_track.vocal_regions().is_none() {
                if let Some(vocals) = worker_track.vocal_stem() {
                    let regions =
                        djcraft_analysis::detect_vocal_regions(vocals, worker_track.sample_rate);
                    worker_track.set_vocal_regions(regions)?;
                }
            }
            Ok::<bool, String>(ready)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .engine
                    .as_ref()
                    .and_then(|e| e.track(deck))
                    .is_some_and(|current| Arc::ptr_eq(current, &track))
                {
                    this.stem_busy[deck] = false;
                    this.stem_status[deck] = match result {
                        Ok(true) => "Stems ready · vocal map ready".into(),
                        Ok(false) => "Prepare stems".into(),
                        Err(error) => {
                            eprintln!("Stem preparation: {error}");
                            format!("Retry stems: {error}")
                        }
                    };
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}
