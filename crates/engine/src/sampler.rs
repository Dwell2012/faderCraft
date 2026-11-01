//! Sample-bank disk work. Runs exclusively on a worker, never on the audio callback.
use crate::Track;
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};

pub(crate) const DEFAULTS: [&str; 12] = [
    "Kick",
    "Clap",
    "Snare",
    "Crash",
    "Air horn",
    "Siren",
    "Closed hat",
    "Open hat",
    "Low tom",
    "Rimshot",
    "Cowbell",
    "Riser",
];
pub(crate) enum Source {
    File(PathBuf),
    Capture {
        track: Arc<Track>,
        start: f64,
        end: f64,
        gains: [f32; 4],
    },
    Clear,
    Default,
    Restore,
    RestoreEmpty,
}
pub(crate) struct Request {
    pub slot: usize,
    pub revision: u64,
    pub source: Source,
    pub root: Option<PathBuf>,
}
pub(crate) struct Ready {
    pub slot: usize,
    pub revision: u64,
    pub result: Result<Option<(Arc<Track>, String)>, String>,
}
pub(crate) struct Worker {
    pub tx: mpsc::SyncSender<Vec<Request>>,
    pub rx: mpsc::Receiver<Ready>,
}
impl Worker {
    pub fn new(rate: u32) -> Self {
        let (tx, requests) = mpsc::sync_channel::<Vec<Request>>(32);
        let (results, rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(batch) = requests.recv() {
                for request in batch {
                    let result =
                        prepare(rate, request.root.as_deref(), request.slot, request.source);
                    if results
                        .send(Ready {
                            slot: request.slot,
                            revision: request.revision,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        Self { tx, rx }
    }
}
fn prepare(
    rate: u32,
    root: Option<&Path>,
    slot: usize,
    source: Source,
) -> Result<Option<(Arc<Track>, String)>, String> {
    if matches!(source, Source::Restore | Source::RestoreEmpty) {
        if let Some(root) = root {
            let entry = root.join(format!("slot-{slot:02}.txt"));
            match std::fs::read_to_string(entry) {
                Ok(text) if text.is_empty() => return Ok(None),
                Ok(text) => {
                    let (name, file) = text.split_once('\n').ok_or("Invalid sample-bank entry")?;
                    if !file.starts_with("sample-")
                        || !file.ends_with(".wav")
                        || file.contains(['/', '\\', ':'])
                    {
                        return Err("Invalid stored sample path".into());
                    }
                    let track = Track::decode(&root.join(file), rate).map_err(|e| e.to_string())?;
                    validate(&track)?;
                    return Ok(Some((track, name.to_string())));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    let sample = match source {
        Source::Clear | Source::RestoreEmpty => None,
        Source::Default | Source::Restore => DEFAULTS
            .get(slot)
            .map(|name| (Arc::new(synthesize(slot, rate)), name.to_string())),
        Source::File(path) => {
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
                return Err(
                    "Choose a sample file smaller than 64 MB (maximum 120 seconds).".into(),
                );
            }
            let track = Track::decode(&path, rate).map_err(|e| e.to_string())?;
            validate(&track)?;
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .replace(['\r', '\n'], " ");
            Some((track, name))
        }
        Source::Capture {
            track,
            start,
            end,
            gains,
        } => {
            let first = (start * rate as f64) as usize;
            let last = (end * rate as f64) as usize;
            let samples = (first * 2..last * 2)
                .map(|i| track.mixed_sample(i, gains))
                .collect();
            let name = capture_name(&track.title, start, gains);
            Some((
                Arc::new(Track::from_samples(name.clone(), rate, samples)),
                name,
            ))
        }
    };
    if let Some(root) = root {
        persist(root, slot, sample.as_ref())?;
    }
    Ok(sample)
}
pub(crate) fn capture_name(title: &str, start: f64, gains: [f32; 4]) -> String {
    let selection = ["Vocals", "Instruments", "Bass", "Drums"]
        .into_iter()
        .zip(gains)
        .filter(|(_, gain)| *gain > 0.)
        .map(|(name, _)| name)
        .collect::<Vec<_>>()
        .join("+");
    if gains == [1.; 4] {
        format!("{} {start:.1}s", title.replace(['\r', '\n'], " "))
    } else {
        format!(
            "{} [{selection}] {start:.1}s",
            title.replace(['\r', '\n'], " ")
        )
    }
}

fn validate(track: &Track) -> Result<(), String> {
    if track.decoded_seconds() < 0.01 || track.decoded_seconds() > 120. {
        return Err("Samples must be between 0.01 and 120 seconds.".into());
    }
    if (0..track.decoded_frames() * 2).any(|i| !track.mixed_sample(i, [1.; 4]).is_finite()) {
        return Err("Sample contains invalid audio values.".into());
    }
    Ok(())
}
fn persist(root: &Path, slot: usize, sample: Option<&(Arc<Track>, String)>) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let entry = root.join(format!("slot-{slot:02}.txt"));
    let old = std::fs::read_to_string(&entry).ok();
    let mut text = String::new();
    if let Some((track, name)) = sample {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let filename = format!("sample-{slot:02}-{stamp}.wav");
        write_wav(track, &root.join(&filename))?;
        text = format!("{}\n{filename}", name.replace(['\r', '\n'], " "));
    }
    let temporary = root.join(format!("slot-{slot:02}.tmp"));
    std::fs::write(&temporary, text).map_err(|e| e.to_string())?;
    std::fs::rename(temporary, &entry).map_err(|e| e.to_string())?;
    if let Some(old) = old.and_then(|s| s.split_once('\n').map(|(_, p)| p.to_string())) {
        if old.starts_with("sample-") && old.ends_with(".wav") && !old.contains(['/', '\\', ':']) {
            let _ = std::fs::remove_file(root.join(old));
        }
    }
    Ok(())
}
fn write_wav(track: &Track, path: &Path) -> Result<(), String> {
    let mut out = std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?,
    );
    let bytes = track.decoded_frames() as u32 * 4;
    let rate = track.sample_rate;
    let result = (|| -> std::io::Result<()> {
        out.write_all(b"RIFF")?;
        out.write_all(&(bytes + 36).to_le_bytes())?;
        out.write_all(b"WAVEfmt ")?;
        out.write_all(&16u32.to_le_bytes())?;
        out.write_all(&1u16.to_le_bytes())?;
        out.write_all(&2u16.to_le_bytes())?;
        out.write_all(&rate.to_le_bytes())?;
        out.write_all(&(rate * 4).to_le_bytes())?;
        out.write_all(&4u16.to_le_bytes())?;
        out.write_all(&16u16.to_le_bytes())?;
        out.write_all(b"data")?;
        out.write_all(&bytes.to_le_bytes())?;
        for i in 0..track.decoded_frames() * 2 {
            let s = track.mixed_sample(i, [1.; 4]);
            let pcm = ((if s.is_finite() { s.clamp(-1., 1.) } else { 0. }) * 32767.).round() as i16;
            out.write_all(&pcm.to_le_bytes())?;
        }
        out.flush()?;
        out.get_ref().sync_all()
    })();
    result.map_err(|e| e.to_string())
}
// Original procedural sounds, distributed under the project's MIT licence.
fn synthesize(kind: usize, rate: u32) -> Track {
    use std::f64::consts::TAU;
    let seconds = [0.65, 0.35, 0.4, 2.4, 1.8, 3., 0.13, 0.7, 0.6, 0.15, 0.5, 3.][kind];
    let mut seed = 0x532196ab_u32;
    let mut low = 0.;
    let mut phase = 0.;
    let samples = (0..(seconds * rate as f64) as usize)
        .flat_map(|i| {
            let t = i as f64 / rate as f64;
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = seed as f64 / u32::MAX as f64 * 2. - 1.;
            low += 0.12 * (noise - low);
            let high = noise - low;
            let freq = match kind {
                0 => 48. + 130. * (-t * 35.).exp(),
                8 => 95. + 65. * (-t * 20.).exp(),
                5 => 480. + 330. * (TAU * t * 1.4).sin(),
                _ => 100.,
            };
            phase += TAU * freq / rate as f64;
            let sine = |f: f64| (TAU * f * t).sin();
            let sound = match kind {
                0 => phase.sin() * (-t * 9.).exp() + high * 0.2 * (-t * 140.).exp(),
                1 => {
                    high * ((-(t % 0.025) * 110.).exp()
                        * if t < 0.09 { 1. } else { (-t * 18.).exp() })
                }
                2 => 0.65 * high * (-t * 17.).exp() + 0.35 * sine(185.) * (-t * 25.).exp(),
                3 => (0.65 * high + 0.35 * (sine(3201.) * sine(4597.))) * (-t * 2.2).exp(),
                4 => {
                    (sine(233.) + 0.6 * sine(349.5) + 0.35 * sine(466.) + 0.2 * sine(699.))
                        * 0.45
                        * (1. - (-t * 45.).exp())
                        * (-t * 0.65).exp()
                }
                5 => phase.sin() * 0.6 + (phase * 2.).sin() * 0.15,
                6 | 7 => high * (-t * if kind == 6 { 48. } else { 7. }).exp(),
                8 => phase.sin() * (-t * 9.).exp(),
                9 => (sine(1800.) + 0.5 * sine(850.)) * 0.6 * (-t * 55.).exp(),
                10 => (sine(540.).signum() + sine(800.).signum()) * 0.3 * (-t * 8.).exp(),
                _ => {
                    high * (t / seconds).powi(2) * 0.65 + sine(200. + 300. * t) * 0.15 * t / seconds
                }
            };
            let edge = (t / 0.002).min((seconds - t) / 0.04).clamp(0., 1.);
            let v = (sound * edge * 0.75).clamp(-0.85, 0.85) as f32;
            [v, v]
        })
        .collect();
    Track::from_samples(DEFAULTS[kind], rate, samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_audible_finite_faded_and_below_full_scale() {
        for kind in 0..DEFAULTS.len() {
            let track = synthesize(kind, 24000);
            let samples = track.export_stereo();
            assert!(samples.iter().all(|v| v.is_finite() && v.abs() <= 0.85));
            assert!(
                samples.iter().any(|v| v.abs() > 0.1),
                "{} is silent",
                DEFAULTS[kind]
            );
            assert!(samples[0].abs() < 0.0001);
            assert!(samples.last().unwrap().abs() < 0.001);
        }
    }
    #[test]
    fn bank_restores_imports_captures_and_deleted_defaults() {
        let root = std::env::temp_dir().join(format!(
            "djcraft-sampler-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let original = prepare(24000, Some(&root), 0, Source::Restore)
            .unwrap()
            .unwrap();
        assert_eq!(original.1, "Kick");
        let restored = prepare(48000, Some(&root), 0, Source::Restore)
            .unwrap()
            .unwrap();
        assert_eq!(restored.1, "Kick");
        assert_eq!(restored.0.sample_rate, 48000);
        let source = root.join("import.wav");
        write_wav(&synthesize(1, 24000), &source).unwrap();
        prepare(48000, Some(&root), 0, Source::File(source.clone())).unwrap();
        std::fs::remove_file(source).unwrap();
        assert_eq!(
            prepare(24000, Some(&root), 0, Source::Restore)
                .unwrap()
                .unwrap()
                .1,
            "import"
        );
        assert!(
            prepare(
                24000,
                Some(&root),
                0,
                Source::File(root.join("missing.wav"))
            )
            .is_err()
        );
        assert_eq!(
            prepare(24000, Some(&root), 0, Source::Restore)
                .unwrap()
                .unwrap()
                .1,
            "import"
        );
        let track = Arc::new(Track::from_samples("capture", 24000, vec![0.2; 48000]));
        prepare(
            24000,
            Some(&root),
            0,
            Source::Capture {
                track,
                start: 0.2,
                end: 0.4,
                gains: [1.; 4],
            },
        )
        .unwrap();
        let captured = prepare(24000, Some(&root), 0, Source::Restore)
            .unwrap()
            .unwrap();
        assert!((captured.0.decoded_seconds() - 0.2).abs() < 0.001);
        prepare(24000, Some(&root), 0, Source::Clear).unwrap();
        assert!(
            prepare(24000, Some(&root), 0, Source::Restore)
                .unwrap()
                .is_none()
        );
        prepare(24000, Some(&root), 0, Source::Default).unwrap();
        assert_eq!(
            prepare(24000, Some(&root), 0, Source::Restore)
                .unwrap()
                .unwrap()
                .1,
            "Kick"
        );
        for entry in std::fs::read_dir(&root).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
}
