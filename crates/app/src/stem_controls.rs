use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use std::sync::Arc;
use std::sync::atomic::AtomicU8;

impl Shell {
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
            self.start_stem_job(deck, track, true, true, cx);
        }
    }

    fn stem_library_notice(&mut self, message: String, cx: &mut Context<Self>) {
        let view = if self.tab == Tab::Library {
            self.library.clone()
        } else {
            self.browser.clone()
        };
        view.update(cx, |view, cx| view.stem_notice(message, cx));
    }

    pub(super) fn prepare_library_stems(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(deck) = self
            .deck_paths
            .iter()
            .position(|p| p.as_ref() == Some(&path))
        {
            if let Some(track) = self
                .engine
                .as_ref()
                .and_then(|engine| engine.track(deck))
                .cloned()
            {
                if track.stems_ready() {
                    self.stem_library_notice("Stems already ready for this track.".into(), cx);
                    return;
                }
                if self.stem_busy[deck] {
                    self.stem_library_notice("This track's stems are already being prepared. You can retry manually after it finishes or stops.".into(), cx);
                    return;
                }
                if track.is_complete() {
                    self.start_stem_job(deck, track, true, false, cx);
                    self.stem_library_notice(
                        "Separating stems; follow progress on the loaded deck.".into(),
                        cx,
                    );
                    return;
                }
            }
        }
        if !self.library_stem_jobs.insert(path.clone()) {
            self.stem_library_notice(
                "This track is already queued for stem separation.".into(),
                cx,
            );
            return;
        }
        self.stem_library_notice(
            format!(
                "Separating stems for {}…",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            cx,
        );
        let worker_path = path.clone();
        let rate = self
            .engine
            .as_ref()
            .map_or(48000, |engine| engine.sample_rate());
        let root = data_dir().join("stems");
        let waves = crate::settings::waveform_dir();
        let prefer_gpu = self.settings.stem_gpu;
        let model = self.settings.stem_model.name();
        let work = cx.background_executor().spawn(async move {
            let track = Track::decode_cached(&worker_path, rate, Some(&waves))
                .map_err(|e| e.to_string())?;
            crate::stems::load_or_prepare(&track, &root, true, &AtomicU8::new(0), prefer_gpu, model)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.library_stem_jobs.remove(&path);
                if result.is_ok() {
                    // A deck may have loaded this file while the menu job ran.
                    // Recheck its cache without reloading or moving its transport.
                    for deck in 0..DECKS {
                        if this.deck_paths[deck].as_ref() == Some(&path) {
                            this.stem_tracks[deck] = None;
                        }
                    }
                }
                let message = match result {
                    Ok(_) => format!(
                        "Stems ready: {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    Err(error) => format!("Stem separation failed: {error}"),
                };
                this.stem_library_notice(message, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn prepare_stems(&mut self, cx: &mut Context<Self>) {
        for deck in 0..DECKS {
            let track = self.engine.as_ref().and_then(|e| e.track(deck)).cloned();
            if let Some(track) = track {
                if track.is_complete() && !track.stems_ready() && !self.stem_busy[deck] {
                    self.start_stem_job(deck, track, true, false, cx);
                }
            }
        }
    }

    pub(super) fn start_stem_job(
        &mut self,
        deck: usize,
        track: Arc<djcraft_engine::Track>,
        prepare: bool,
        automatic: bool,
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
        let completed_progress = progress.clone();
        let prefer_gpu = self.settings.stem_gpu;
        let model = self.settings.stem_model.name();
        let work = cx.background_executor().spawn(async move {
            let ready = crate::stems::load_or_prepare_with_limit(
                &worker_track,
                &root,
                prepare,
                &progress,
                prefer_gpu,
                model,
                automatic.then(|| crate::stems::automatic_budget(worker_track.duration_seconds())),
            )?;
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
                if Arc::ptr_eq(&this.stem_progress[deck], &completed_progress)
                    && this
                        .engine
                        .as_ref()
                        .and_then(|e| e.track(deck))
                        .is_some_and(|current| Arc::ptr_eq(current, &track))
                {
                    this.stem_busy[deck] = false;
                    this.stem_status[deck] = match result {
                        Ok(true) => "Stems ready · vocal map ready".into(),
                        Ok(false) => "Prepare stems".into(),
                        Err(error) if error.contains(crate::stems::AUTO_TIMEOUT) => error,
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
