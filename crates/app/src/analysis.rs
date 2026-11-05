//! Background analysis of library tracks.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, mpsc};

use djcraft_analysis::{TempoRange, detect_key, detect_structure, detect_tempo};
use djcraft_engine::Track;
use djcraft_library::{Camelot, CuePoint, KeyResult, Library, TempoResult};

/// Raise this when the analysis changes enough that every track should be
/// analysed again. Version 2 added energy and cue points; version 3, key;
/// version 4, bars and waveforms; version 5, bass phase; version 6,
/// actual opening attacks and opening-bar anchoring.
// Version 7 retains reliable kick phase when log-flux confidence is weak.
// Version 8 retains a strong first kick with modest attack-shape variation.
// Version 9 anchors phase on repeated pitch-sweeping kicks before bass fallback.
// Version 10 uses the first confirmed run and refines it locally, not the intro.
// Version 11 anchors quiet-lead-in bar starts on the confirmed kick entrance,
// rejects unstable/rolling references and accepts half-time kick cadence.
// Version 12 adds conservative modal estimates alongside the legacy key.
const ANALYSIS_VERSION: i64 = 12;
/// Tempo and key do not need treble, and a lower rate is quicker to decode.
const ANALYSIS_RATE: u32 = 22_050;
const MAX_WORKERS: usize = 4;

#[derive(Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    /// Set to stop the pass early. Tracks already analysed keep their
    /// results; the rest stay pending.
    pub cancel: AtomicBool,
    queue: Mutex<Queue>,
}

type Job = (i64, PathBuf);

#[derive(Default)]
struct Queue {
    pending: VecDeque<Job>,
    active: Vec<Job>,
    priority: Vec<PathBuf>,
}

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .replace('\\', "/")
            .eq_ignore_ascii_case(&b.to_string_lossy().replace('\\', "/"))
    } else {
        a == b
    }
}

impl Queue {
    fn urgent(&self, path: &Path) -> bool {
        self.priority.iter().any(|p| same_path(p, path))
    }

    fn take(&mut self, priority_only: bool) -> Option<Job> {
        let urgent = self.pending.iter().position(|(_, path)| self.urgent(path));
        let index = urgent.or_else(|| {
            (!priority_only && self.priority.is_empty() && !self.pending.is_empty()).then_some(0)
        })?;
        let job = self.pending.remove(index)?;
        self.active.push(job.clone());
        Some(job)
    }

    fn finish(&mut self, id: i64) {
        if let Some(index) = self.active.iter().position(|(active, _)| *active == id) {
            let (_, path) = self.active.remove(index);
            self.priority.retain(|p| !same_path(p, &path));
        }
    }
}

impl Progress {
    /// Only pending/in-flight library tracks need promotion. Cached tracks do nothing.
    pub fn prioritize(&self, path: &Path) {
        let mut queue = self.queue.lock().unwrap();
        if !queue.urgent(path)
            && queue
                .pending
                .iter()
                .chain(&queue.active)
                .any(|(_, p)| same_path(p, path))
        {
            queue.priority.push(path.to_owned());
        }
    }

    pub fn status(&self) -> String {
        let done = self.done.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        let queue = self.queue.lock().unwrap();
        if let Some(path) = queue.priority.first() {
            format!(
                "Deck analysis · {} · library {done}/{total} paused",
                path.file_stem().unwrap_or_default().to_string_lossy()
            )
        } else if done < total {
            format!("Analysing {done}/{total} · {}%", done * 100 / total)
        } else {
            "Analysis idle".into()
        }
    }

    /// DSP stages are indivisible; pause between stages without discarding work.
    fn checkpoint(&self, path: &Path) -> bool {
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return false;
            }
            let queue = self.queue.lock().unwrap();
            if queue.priority.is_empty() || queue.urgent(path) {
                return true;
            }
            drop(queue);
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

/// Analyses every track that needs it and stores the results, saving each
/// track's waveform in `waveform_dir` on the way. Blocking, and busy on
/// several threads; run it off the UI thread.
pub fn analyse_pending(
    db_path: &Path,
    waveform_dir: &Path,
    range: TempoRange,
    progress: &Progress,
) -> djcraft_library::Result<()> {
    let mut library = Library::open(db_path)?;
    let pending = library.pending_tempo_analysis(ANALYSIS_VERSION)?;
    progress.done.store(0, Ordering::Relaxed);
    progress.total.store(pending.len(), Ordering::Relaxed);
    if pending.is_empty() {
        return Ok(());
    }

    // Leave half the machine free: the user may be mixing meanwhile.
    let workers = std::thread::available_parallelism()
        .map(|cores| cores.get() / 2)
        .unwrap_or(1)
        .clamp(1, MAX_WORKERS);
    *progress.queue.lock().unwrap() = Queue {
        pending: pending.into(),
        ..Default::default()
    };
    let (results, received) = mpsc::channel::<(i64, Analysis)>();

    std::thread::scope(|scope| {
        // A reserved worker prevents deadlock when every background worker is paused.
        for worker in 0..=workers {
            let results = results.clone();
            let priority_only = worker == workers;
            scope.spawn(move || {
                loop {
                    if progress.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let next = {
                        let mut queue = progress.queue.lock().unwrap();
                        if queue.pending.is_empty() && queue.active.is_empty() {
                            break;
                        }
                        queue.take(priority_only)
                    };
                    let Some((id, path)) = next else {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                        continue;
                    };
                    let Some(analysis) = analyse(&path, waveform_dir, range, progress) else {
                        break;
                    };
                    if results.send((id, analysis)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(results);
        // Results are written from this one thread as they arrive.
        for (id, analysis) in received {
            let stored = (|| -> djcraft_library::Result<()> {
                library.store_structure(id, analysis.energy, &analysis.cues)?;
                library.store_key(id, analysis.key)?;
                library.store_modal_key(id, analysis.modal)?;
                // Last, as this is what marks the track as analysed.
                library.store_tempo(id, ANALYSIS_VERSION, analysis.tempo)?;
                Ok(())
            })();
            if let Err(error) = stored {
                progress.cancel.store(true, Ordering::Relaxed);
                return Err(error);
            }
            progress.done.fetch_add(1, Ordering::Relaxed);
            progress.queue.lock().unwrap().finish(id);
        }
        Ok(())
    })
}

#[derive(Default)]
struct Analysis {
    modal: Option<djcraft_library::ModalKeyResult>,
    tempo: Option<TempoResult>,
    key: Option<KeyResult>,
    energy: Option<u8>,
    cues: Vec<CuePoint>,
}

/// A file that cannot be decoded simply has nothing to report.
fn analyse(
    path: &Path,
    waveform_dir: &Path,
    range: TempoRange,
    progress: &Progress,
) -> Option<Analysis> {
    if !progress.checkpoint(path) {
        return None;
    }
    let Ok(track) = Track::decode_cached(path, ANALYSIS_RATE, Some(waveform_dir)) else {
        return Some(Analysis::default());
    };
    if !progress.checkpoint(path) {
        return None;
    }
    let samples = track.mono();
    // Persist waveform events while this track is already decoded for analysis.
    let _ = crate::attack_cache::load_or_analyse(&track);
    if !progress.checkpoint(path) {
        return None;
    }
    let tempo = detect_tempo(&samples, ANALYSIS_RATE, range);
    // Cue points sit on the bar lines of a tempo worth trusting.
    if !progress.checkpoint(path) {
        return None;
    }
    let structure = detect_structure(
        &samples,
        ANALYSIS_RATE,
        tempo.filter(|t| t.confidence >= 0.1),
    );
    if !progress.checkpoint(path) {
        return None;
    }
    let key = detect_key(&samples, ANALYSIS_RATE);
    Some(Analysis {
        modal: key.and_then(|key| {
            key.modal.map(|mode| djcraft_library::ModalKeyResult {
                tonic: key.tonic,
                mode: mode.mode.label().into(),
                score: f64::from(mode.score),
                margin: f64::from(mode.margin),
            })
        }),
        tempo: tempo.map(|tempo| TempoResult {
            bpm: tempo.bpm,
            first_beat_seconds: tempo.first_beat_seconds,
            downbeat: tempo.downbeat,
            confidence: tempo.confidence as f64,
        }),
        key: key.map(|key| KeyResult {
            key: Camelot::from_tonic(key.tonic, key.major),
            confidence: key.confidence as f64,
        }),
        energy: structure.as_ref().map(|structure| structure.energy),
        cues: structure
            .map(|structure| structure.cues)
            .unwrap_or_default()
            .into_iter()
            .map(|cue| CuePoint {
                seconds: cue.seconds,
                energy: cue.energy,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress() -> Progress {
        Progress {
            queue: Mutex::new(Queue {
                pending: [
                    (1, "background".into()),
                    (2, "deck".into()),
                    (3, "next".into()),
                ]
                .into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn deck_bypasses_queue_and_background_resumes_after_store() {
        let p = progress();
        assert_eq!(p.queue.lock().unwrap().take(false).unwrap().0, 1);
        p.prioritize(Path::new("deck"));
        p.prioritize(Path::new("deck"));
        let mut q = p.queue.lock().unwrap();
        assert_eq!(q.priority.len(), 1);
        assert_eq!(q.take(true).unwrap().0, 2);
        assert!(q.take(false).is_none());
        q.finish(2);
        assert_eq!(q.take(false).unwrap().0, 3);
        assert!(q.take(true).is_none());
    }

    #[test]
    fn in_flight_deck_is_promoted_without_duplicate_work() {
        let p = progress();
        let job = p.queue.lock().unwrap().take(false).unwrap();
        p.prioritize(&job.1);
        assert!(p.checkpoint(&job.1));
        assert!(p.queue.lock().unwrap().take(true).is_none());
        p.queue.lock().unwrap().finish(job.0);
        p.prioritize(&job.1);
        p.prioritize(Path::new("already analysed or outside library"));
        assert!(p.queue.lock().unwrap().priority.is_empty());
    }

    #[test]
    fn paused_background_unblocks_on_completion_or_cancel() {
        let p = progress();
        p.prioritize(Path::new("deck"));
        std::thread::scope(|scope| {
            let (tx, rx) = mpsc::channel();
            let progress = &p;
            scope.spawn(move || {
                tx.send(progress.checkpoint(Path::new("background")))
                    .unwrap();
            });
            assert!(
                rx.recv_timeout(std::time::Duration::from_millis(60))
                    .is_err()
            );
            let mut q = p.queue.lock().unwrap();
            let job = q.take(true).unwrap();
            q.finish(job.0);
            drop(q);
            assert!(rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap());
        });
        p.prioritize(Path::new("next"));
        p.cancel.store(true, Ordering::Relaxed);
        assert!(!p.checkpoint(Path::new("background")));
    }
}
