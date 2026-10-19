use super::*;
use djcraft_protocol::{AutomixState, EffectState};
#[derive(Default)]
pub(super) struct Automix {
    pub state: AutomixState,
    incoming: Option<usize>,
    fade: Option<(u64, usize)>,
    serial: u64,
    vocal_ready: (bool, bool),
    next_now: bool,
    pub waiting_for_deck: bool,
}
impl Engine {
    pub(super) fn stop_automix(&mut self, reason: &str) -> Result<(), Rejection> {
        self.send(Command::CancelFade)?;
        if let Some((_, to)) = self.automix.fade {
            // A scheduled handover has not consumed the queue item until audible.
            if !self.shared.decks[to].playing() {
                if let Some(next) = self.automix.state.next.take() {
                    self.automix.state.queue.insert(0, next);
                }
            }
            self.crossfader = self.shared.cross_position();
            if self.automix.state.smart {
                for deck in 0..2 {
                    self.decks[deck].eq = self.shared.decks[deck].eq_gains();
                }
            }
        } else if let Some(next) = self.automix.state.next.take() {
            self.automix.state.queue.insert(0, next);
        }
        self.automix.state.plan = None;
        self.automix.state.enabled = false;
        self.automix.state.transitioning = false;
        self.automix.state.next = None;
        self.automix.state.status = reason.into();
        self.automix.incoming = None;
        self.automix.fade = None;
        self.automix.next_now = false;
        Ok(())
    }
    pub(super) fn automix_action(&mut self, action: MixerAction) -> Result<(), Rejection> {
        match action {
            MixerAction::AutomixSmart { enabled } => {
                if self.automix.state.enabled {
                    return Err(Rejection::Busy);
                }
                self.automix.state.smart = enabled;
            }
            MixerAction::AutomixEnqueue { tracks } => {
                if tracks
                    .iter()
                    .any(|t| t.path.is_empty() || t.grid.is_some_and(|g| !g.valid()))
                    || self.automix.state.queue.len() + tracks.len() > 1000
                {
                    return Err(Rejection::InvalidValue);
                }
                self.automix.state.queue.extend(tracks);
            }
            MixerAction::AutomixRemove { index } => {
                if index >= self.automix.state.queue.len() {
                    return Err(Rejection::InvalidValue);
                }
                self.automix.state.queue.remove(index);
            }
            MixerAction::AutomixMove { from, to } => {
                let queue = &mut self.automix.state.queue;
                if from >= queue.len() || to >= queue.len() {
                    return Err(Rejection::InvalidValue);
                }
                let item = queue.remove(from);
                queue.insert(to, item);
            }
            MixerAction::AutomixClear => {
                self.automix.state.queue.clear();
                if self.automix.fade.is_none() {
                    self.automix.state.plan = None;
                }
                if self.automix.fade.is_none() {
                    self.automix.incoming = None;
                    self.automix.state.next = None;
                    if self.automix.state.active_deck.is_none() {
                        self.automix.state.enabled = false;
                    }
                }
            }
            MixerAction::AutomixFadeTime { seconds } => {
                if !seconds.is_finite() || !(0.1..=30.).contains(&seconds) {
                    return Err(Rejection::InvalidValue);
                }
                self.automix.state.fade_seconds = seconds;
                if !self.automix.state.transitioning {
                    self.automix.state.plan = None;
                }
            }
            MixerAction::AutomixStart => {
                if self.automix.state.enabled {
                    return Ok(());
                }
                if self.decks.iter().any(|d| d.loading.is_some()) {
                    return Err(Rejection::Busy);
                }
                let playing: Vec<_> = self
                    .shared
                    .decks
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| d.playing())
                    .map(|(i, _)| i)
                    .collect();
                if playing.is_empty() && self.automix.state.queue.is_empty() {
                    return Err(Rejection::InvalidValue);
                }
                self.crossfader = self.shared.cross_position();
                let active = playing.first().copied();
                self.automix.state.active_deck = if playing.len() == 1 { active } else { None };
                self.automix.state.status = if playing.len() == 2 {
                    "Waiting for a non-playing deck"
                } else {
                    "Resuming playlist…"
                }
                .into();
                if playing.is_empty() {
                    self.crossfader = 0.;
                    self.send(Command::Crossfader { position: 0. })?;
                }
                self.automix.waiting_for_deck = playing.len() == 2;
                self.automix.state.enabled = true;
            }
            MixerAction::AutomixStop => {
                return self.stop_automix("Manual control · playlist retained; click Resume");
            }
            MixerAction::AutomixNext => {
                if self.automix.state.enabled {
                    self.automix.next_now = true;
                }
            }
            _ => return Err(Rejection::InvalidValue),
        }
        Ok(())
    }
    fn prepare_auto_deck(&mut self, deck: usize) -> Result<(), Rejection> {
        self.apply_deck(deck, DeckAction::Pause)?;
        if self.automix.state.smart {
            self.apply_deck(
                deck,
                DeckAction::SetEqMode {
                    mode: EqMode::Classic,
                },
            )?;
        }
        self.apply_deck(deck, DeckAction::ClearLoop)?;
        self.apply_deck(deck, DeckAction::SetTempo { ratio: 1. })?;
        self.apply_deck(deck, DeckAction::SetVolume { level: 1. })?;
        self.apply_deck(deck, DeckAction::SetTrim { db: 0. })?;
        for band in 0..3 {
            self.apply_deck(deck, DeckAction::SetEq { band, gain: 1. })?;
        }
        for slot in 0..6 {
            self.apply_deck(
                deck,
                DeckAction::SetEffect {
                    slot,
                    effect: EffectState {
                        tails: false,
                        ..Default::default()
                    },
                },
            )?;
        }
        Ok(())
    }
    pub(super) fn tick_automix(&mut self) {
        if !self.automix.state.enabled {
            return;
        }
        if self.automix.waiting_for_deck {
            let playing: Vec<_> = self
                .shared
                .decks
                .iter()
                .enumerate()
                .filter(|(_, d)| d.playing())
                .map(|(i, _)| i)
                .collect();
            if playing.len() == 2 {
                return;
            }
            self.automix.waiting_for_deck = false;
            self.automix.state.active_deck = playing.first().copied();
        }
        if let Some((id, to)) = self.automix.fade {
            if self
                .automix
                .state
                .plan
                .as_ref()
                .is_some_and(|p| p.phrase_aligned)
                && self.shared.decks[to].playing()
            {
                self.automix.state.status = "Phrase transition · EQ bass handover".into();
            }
            self.crossfader = self.shared.cross_position();
            if self.automix.state.smart {
                for deck in 0..2 {
                    self.decks[deck].eq = self.shared.decks[deck].eq_gains();
                }
            }
            if self.shared.fade_done() != id {
                return;
            }
            self.crossfader = self.shared.cross_position();
            self.automix.state.active_deck = Some(to);
            self.automix.state.next = None;
            self.automix.state.transitioning = false;
            self.automix.fade = None;
            self.automix.incoming = None;
            self.automix.state.plan = None;
            self.automix.state.status = "Playing".into();
        }
        if self.automix.incoming.is_none() && !self.automix.state.queue.is_empty() {
            let deck = self.automix.state.active_deck.map_or(0, |d| 1 - d);
            let item = self.automix.state.queue[0].clone();
            if self
                .prepare_auto_deck(deck)
                .and_then(|_| {
                    self.apply_deck(
                        deck,
                        DeckAction::Load {
                            path: item.path.clone(),
                        },
                    )
                })
                .is_err()
            {
                return;
            }
            self.automix.state.queue.remove(0);
            self.automix.state.next = Some(item);
            self.automix.incoming = Some(deck);
        }
        if let Some(deck) = self.automix.incoming {
            let control = &self.decks[deck];
            if control.loading.is_some() {
                return;
            }
            if let Some(error) = &control.error {
                self.automix.state.status = format!("Skipped unreadable track: {error}");
                self.automix.incoming = None;
                self.automix.state.plan = None;
                self.automix.state.next = None;
                if self.automix.state.queue.is_empty() && self.automix.state.active_deck.is_none() {
                    self.automix.state.enabled = false;
                }
                return;
            }
            if !control.track.as_ref().is_some_and(|t| t.is_complete()) {
                return;
            }
            self.decks[deck].grid = self
                .automix
                .state
                .next
                .as_ref()
                .and_then(|t| t.grid)
                .or(self.decks[deck].grid);
            let incoming_prep = self.decks[deck]
                .path
                .as_ref()
                .and_then(|p| self.preparations.get(p))
                .cloned()
                .unwrap_or_default();
            let incoming_duration = self.decks[deck].track.as_ref().unwrap().duration_seconds();
            let mut incoming_start = incoming_prep
                .mix_in
                .filter(|t| *t < incoming_duration - 0.01);
            if let Some(from) = self.automix.state.active_deck {
                let Some(track) = &self.decks[from].track else {
                    return;
                };
                let remaining = (track.duration_seconds()
                    - self.shared.decks[from].position() / self.sample_rate as f64)
                    / self.decks[from].tempo;
                let outgoing_prep = self.decks[from]
                    .path
                    .as_ref()
                    .and_then(|p| self.preparations.get(p))
                    .cloned()
                    .unwrap_or_default();
                let out_vocals = track.vocal_regions();
                let in_vocals = self.decks[deck]
                    .track
                    .as_ref()
                    .and_then(|t| t.vocal_regions());
                let ready = (out_vocals.is_some(), in_vocals.is_some());
                if ready != self.automix.vocal_ready {
                    self.automix.vocal_ready = ready;
                    self.automix.state.plan = None;
                }
                let incoming_seconds = incoming_duration - incoming_start.unwrap_or(0.);
                let mut matching = if self.automix.state.smart {
                    matching(
                        self.decks[from].grid,
                        self.decks[deck].grid,
                        self.decks[from].tempo,
                    )
                } else {
                    None
                };
                let mut match_reason = match_failure(
                    self.decks[from].grid,
                    self.decks[deck].grid,
                    self.decks[from].tempo,
                );
                let mut target = self
                    .automix
                    .state
                    .fade_seconds
                    .min(incoming_seconds * 0.5)
                    .max(0.01);
                if let (Some(bars), Some(grid)) =
                    (outgoing_prep.transition_bars, self.decks[from].grid)
                {
                    target = (bars as f64 * 4. * 60. / (grid.bpm * self.decks[from].tempo))
                        .min(incoming_seconds * 0.5);
                }
                if let Some((_, grid, ratio)) = matching {
                    let beat = 60. / (grid.bpm * ratio);
                    let usable = (incoming_seconds
                        - grid.first_beat
                        - (grid.downbeat as f64 + 4.) * 60. / grid.bpm)
                        / ratio;
                    let bars = (target.min(usable * 0.5) / (beat * 4.)).floor();
                    if bars >= 1. && remaining >= beat * 4. {
                        target = bars * beat * 4.;
                    } else {
                        matching = None;
                        match_reason = Some("too little audio for a complete beatmatched bar");
                    }
                }
                let position = self.shared.decks[from].position() / self.sample_rate as f64;
                let phrase_plan = if self.automix.next_now {
                    None
                } else {
                    self.automix
                        .state
                        .plan
                        .clone()
                        .filter(|p| {
                            p.phrase_aligned
                                && matching.is_some()
                                && position
                                    < p.outgoing_seconds
                                        + p.duration_seconds * self.decks[from].tempo
                        })
                        .or_else(|| {
                            let (out_grid, in_grid, _) = matching?;
                            crate::transition::plan_vocals(
                                crate::transition::TrackContext {
                                    preparation: &outgoing_prep,
                                    grid: out_grid,
                                    duration: track.duration_seconds(),
                                },
                                crate::transition::TrackContext {
                                    preparation: &incoming_prep,
                                    grid: in_grid,
                                    duration: incoming_duration,
                                },
                                position,
                                self.decks[from].tempo,
                                out_vocals,
                                in_vocals,
                            )
                        })
                };
                let mut schedule = None;
                let bass_swap;
                let eq_start;
                let duration;
                if let Some(plan) = phrase_plan {
                    incoming_start = Some(plan.incoming_seconds);
                    duration = plan.duration_seconds;
                    bass_swap = plan.bass_swap;
                    eq_start = plan.eq_start;
                    schedule = Some((plan.outgoing_seconds, plan.incoming_seconds));
                    self.automix.state.plan = Some(plan.clone());
                    self.automix.state.status = format!(
                        "Planned · {} bars · starts in {:.1}s",
                        plan.bars.unwrap_or(0),
                        ((plan.outgoing_seconds - position) / self.decks[from].tempo).max(0.)
                    );
                    // Arm near the boundary; the audio callback starts it independently
                    // of the UI timer and compensates its sub-block start latency.
                    if position + self.decks[from].tempo * 0.2 < plan.outgoing_seconds {
                        return;
                    }
                } else {
                    let out_at = outgoing_prep
                        .mix_out
                        .filter(|t| *t < track.duration_seconds())
                        .unwrap_or(
                            (track.duration_seconds() - target * self.decks[from].tempo).max(0.),
                        );
                    bass_swap = outgoing_prep.bass_swap.unwrap_or(0.5);
                    eq_start = 0.;
                    duration = target.min(remaining.max(0.1));
                    self.automix.state.plan = Some(djcraft_protocol::AutomixPlan {
                        vocal_overlap_seconds: None,
                        vocal_note: String::new(),
                        outgoing_seconds: out_at,
                        incoming_seconds: incoming_start.unwrap_or(0.),
                        duration_seconds: duration,
                        bars: None,
                        bass_swap,
                        eq_start,
                        phrase_aligned: false,
                        description: if self.automix.next_now {
                            "Next requested · immediate fade"
                        } else if !self.automix.state.smart {
                            "Timed fade"
                        } else if matching.is_none() {
                            match_reason.unwrap_or("Beatmatching unavailable")
                        } else {
                            "Fallback · no compatible section boundaries / manual markers"
                        }
                        .into(),
                    });
                    if let Some(plan) = &mut self.automix.state.plan {
                        crate::transition::annotate_vocals(
                            plan,
                            out_vocals,
                            in_vocals,
                            self.decks[from].tempo,
                            matching.map_or(self.decks[deck].tempo, |(_, _, r)| r),
                        );
                    }
                    if !self.automix.next_now && position < out_at {
                        return;
                    }
                }
                let id = self.automix.serial + 1;
                if self
                    .send(Command::AutomixFade {
                        from,
                        to: deck,
                        frames: (duration * self.sample_rate as f64) as u64,
                        id,
                        smart: self.automix.state.smart,
                        incoming_start,
                        bass_swap,
                        eq_start,
                        schedule,
                        preserve_entry: incoming_prep.mix_in.is_some(),
                        matching,
                    })
                    .is_err()
                {
                    return;
                }
                if let Some((_, _, ratio)) = matching {
                    self.decks[deck].tempo = ratio;
                    self.decks[deck].key_lock = false;
                }
                self.automix.serial = id;
                self.automix.fade = Some((id, deck));
                self.automix.next_now = false;
                self.automix.state.transitioning = true;
                self.automix.state.status = if schedule.is_some() {
                    "Phrase handover armed · EQ bass swap"
                } else if matching.is_some() {
                    "Beatmatched · EQ bass handover"
                } else if self.automix.state.smart {
                    "EQ fade · no compatible grids/tempo or too little audio"
                } else {
                    "Crossfading…"
                }
                .into();
            } else {
                if let Some(seconds) = incoming_start {
                    if self.apply_deck(deck, DeckAction::Seek { seconds }).is_err() {
                        return;
                    }
                }
                if self.apply_deck(deck, DeckAction::Play).is_err() {
                    return;
                }
                self.automix.state.active_deck = Some(deck);
                self.automix.incoming = None;
                self.automix.state.next = None;
                self.automix.state.status = "Playing".into();
            }
        } else if let Some(deck) = self.automix.state.active_deck {
            if self.decks[deck].track.as_ref().is_some_and(|t| {
                self.shared.decks[deck].position() >= t.decoded_frames() as f64 && t.is_complete()
            }) {
                self.automix.state.enabled = false;
                self.automix.state.status = "Queue finished".into();
            }
        }
    }
}

fn matching(
    out: Option<djcraft_protocol::BeatGrid>,
    incoming: Option<djcraft_protocol::BeatGrid>,
    tempo: f64,
) -> Option<(djcraft_protocol::BeatGrid, djcraft_protocol::BeatGrid, f64)> {
    let (out, incoming) = (out?, incoming?);
    if match_failure(Some(out), Some(incoming), tempo).is_some() {
        return None;
    }
    let ratio = out.bpm * tempo / incoming.bpm;
    (0.92..=1.08)
        .contains(&ratio)
        .then_some((out, incoming, ratio))
}

fn match_failure(
    out: Option<djcraft_protocol::BeatGrid>,
    incoming: Option<djcraft_protocol::BeatGrid>,
    tempo: f64,
) -> Option<&'static str> {
    let (Some(out), Some(incoming)) = (out, incoming) else {
        return Some("Fallback · beat grid missing on one or both decks");
    };
    if !out.valid() || !incoming.valid() {
        return Some("Fallback · invalid beat grid");
    }
    // Match the analysis/library acceptance threshold. A second, stricter
    // gate rejected grids already displayed and used successfully by Sync.
    if out.confidence < 0.1 || incoming.confidence < 0.1 {
        return Some("Fallback · beat-grid confidence below 0.10");
    }
    if !(0.92..=1.08).contains(&(out.bpm * tempo / incoming.bpm)) {
        return Some("Fallback · tempo match exceeds ±8% pitch range");
    }
    None
}

#[cfg(test)]
mod matching_tests {
    use super::*;
    #[test]
    fn accepted_lizard_motorcycle_grids_can_beatmatch() {
        let a = djcraft_protocol::BeatGrid {
            bpm: 135.035,
            first_beat: 0.3016818265,
            downbeat: 0,
            confidence: 0.29602849,
        };
        let b = djcraft_protocol::BeatGrid {
            bpm: 134.291,
            first_beat: 15.2400773458,
            downbeat: 0,
            confidence: 0.28559553,
        };
        let (_, _, ratio) = matching(Some(a), Some(b), 1.).unwrap();
        assert!((ratio - 1.00554).abs() < 0.00001);
        assert!(matching(Some(a), Some(b), 1.2).is_none());
        assert!(
            match_failure(None, Some(b), 1.)
                .unwrap()
                .contains("missing")
        );
        assert!(
            matching(
                Some(a),
                Some(djcraft_protocol::BeatGrid {
                    confidence: 0.09,
                    ..b
                }),
                1.
            )
            .is_none()
        );
    }
}
