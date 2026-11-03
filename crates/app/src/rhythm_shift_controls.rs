use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct PackInfo {
    name: String,
    bpm: f64,
    loop_beats: f64,
    return_beats: [f64; 4],
}

fn pack_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("DJCraft")
        .join("Rhythm Shift")
}

fn packs() -> Vec<(PathBuf, PackInfo)> {
    let Ok(entries) = std::fs::read_dir(pack_root()) else {
        return Vec::new();
    };
    let mut packs: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let folder = entry.path();
            let bytes = std::fs::read(folder.join("pack.json")).ok()?;
            let info = serde_json::from_slice::<PackInfo>(&bytes).ok()?;
            Some((folder, info))
        })
        .collect();
    packs.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    packs
}

impl Shell {
    fn rhythm_pack_load(&mut self, folder: &Path, info: &PackInfo, cx: &mut Context<Self>) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        self.rhythm_notice = match engine.load_rhythm_shift_pack(
            folder,
            info.bpm,
            info.loop_beats,
            info.return_beats,
        ) {
            Ok(()) => {
                self.rhythm_pack = Some(info.name.clone());
                format!("Loaded {}", info.name)
            }
            Err(error) => format!("Pack error: {error}"),
        };
        cx.notify();
    }

    fn rhythm_send(&mut self, deck: usize, action: DeckAction, cx: &mut Context<Self>) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        if matches!(&action, DeckAction::SetRhythmShift { enabled: true }) {
            if let Err(reason) = engine.rhythm_shift_readiness(deck) {
                self.rhythm_notice = format!("Deck {}: {reason}", deck_label(deck));
                cx.notify();
                return;
            }
        }
        self.rhythm_notice = match engine.apply(
            Role::Host,
            Action::Deck {
                deck: deck as u8,
                action,
            },
        ) {
            Ok(()) => String::new(),
            Err(error) => format!(
                "Deck {}: Rhythm Shift action failed ({error:?}).",
                deck_label(deck)
            ),
        };
        cx.notify();
    }

    pub(super) fn rhythm_shift_panel(&self, decks: &[DeckState], cx: &mut Context<Self>) -> Div {
        let mut panel = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_1()
            .p(px(4.))
            .child(div().text_sm().font_weight(FontWeight::BOLD).child("RHYTHM SHIFT"))
            .child(div().text_size(px(10.)).child("Replace a deck's drums with a beat-synced groove. Rolls return at their pack's saved point."));
        for (folder, info) in packs() {
            let label = info.name.clone();
            panel =
                panel.child(
                    Button::new(format!("rhythm-pack-{}", folder.display()))
                        .small()
                        .label(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.rhythm_pack_load(&folder, &info, cx)
                        })),
                );
        }
        if self.rhythm_pack.is_none() {
            panel = panel.child(
                div()
                    .text_size(px(10.))
                    .child(format!("Add packs to {}", pack_root().display())),
            );
        }
        for (deck, state) in decks.iter().enumerate() {
            if deck >= if self.settings.four_decks { 4 } else { 2 } {
                break;
            }
            let enabled = state.rhythm_shift;
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w(px(48.))
                            .text_size(px(11.))
                            .child(format!("Deck {}", deck_label(deck))),
                    )
                    .child(
                        Button::new(format!("rhythm-enable-{deck}"))
                            .small()
                            .label(if enabled { "On" } else { "Off" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.rhythm_send(
                                    deck,
                                    DeckAction::SetRhythmShift { enabled: !enabled },
                                    cx,
                                )
                            })),
                    )
                    .children((0..4).map(|slot| {
                        Button::new(format!("rhythm-roll-{deck}-{slot}"))
                            .small()
                            .label(format!("R{}", slot + 1))
                            .disabled(!enabled)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.rhythm_send(
                                    deck,
                                    DeckAction::TriggerRhythmRoll { slot: slot as u8 },
                                    cx,
                                )
                            }))
                    })),
            );
            let gain = state.rhythm_gain;
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().w(px(48.)).text_size(px(10.)).child("Gain"))
                    .child(
                        Button::new(format!("rhythm-gain-down-{deck}"))
                            .small()
                            .label("−")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.rhythm_send(
                                    deck,
                                    DeckAction::SetRhythmGain {
                                        level: ((gain * 10.).round() / 10. - 0.1).max(0.),
                                    },
                                    cx,
                                )
                            })),
                    )
                    .child(
                        div()
                            .w(px(45.))
                            .text_size(px(10.))
                            .child(format!("{}%", (gain * 100.).round() as i32)),
                    )
                    .child(
                        Button::new(format!("rhythm-gain-up-{deck}"))
                            .small()
                            .label("+")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.rhythm_send(
                                    deck,
                                    DeckAction::SetRhythmGain {
                                        level: ((gain * 10.).round() / 10. + 0.1).min(2.),
                                    },
                                    cx,
                                )
                            })),
                    ),
            );
        }
        panel.child(div().text_size(px(10.)).child(self.rhythm_notice.clone()))
    }
}
