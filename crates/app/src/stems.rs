//! Blocking ONNX stem preparation and cache IO. Never called on the audio/UI thread.
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use djcraft_engine::Track;

pub const AUTO_TIMEOUT: &str = "Stopped: automatic stem separation exceeded 20% of track duration";
const SOURCES: [&str; 4] = ["vocals", "other", "bass", "drums"];

/// Paths whose preparation completed on this computer. Listing the library
/// never reads audio from its source folder to determine this status.
fn ready_index() -> BTreeMap<String, HashSet<PathBuf>> {
    fs::read(crate::storage::machine_dir().join("stem-ready-v2.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn ready_paths(model: &str) -> HashSet<PathBuf> {
    ready_index().remove(model).unwrap_or_default()
}

pub fn remember_ready(path: &Path, model: &str) -> Result<(), String> {
    let mut index = ready_index();
    if !index
        .entry(model.to_string())
        .or_default()
        .insert(path.to_path_buf())
    {
        return Ok(());
    }
    let directory = crate::storage::machine_dir();
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    fs::write(
        directory.join("stem-ready-v2.json"),
        serde_json::to_vec(&index).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn automatic_budget(seconds: f64) -> Duration {
    if seconds.is_finite() && seconds > 0. {
        Duration::from_secs_f64((seconds * 0.2).min(7200.))
    } else {
        Duration::ZERO
    }
}

fn check_deadline(deadline: Option<Instant>) -> Result<(), String> {
    if deadline.is_some_and(|at| Instant::now() >= at) {
        Err(AUTO_TIMEOUT.into())
    } else {
        Ok(())
    }
}

pub fn warm_up(model: &str) -> Result<(), String> {
    crate::onnx_stems::warm_up(model)
}

pub fn download_model(model: &str) -> Result<(), String> {
    crate::onnx_stems::download_model(model)
}

pub fn model_downloaded(model: &str) -> bool {
    crate::onnx_stems::model_downloaded(model)
}

pub fn shutdown() {
    crate::onnx_stems::shutdown();
}

fn cache_key(samples: &[f32], rate: u32, model: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in rate
        .to_le_bytes()
        .into_iter()
        .chain(samples.iter().flat_map(|s| s.to_le_bytes()))
    {
        hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
    }
    format!("onnx-{model}-v1-{rate}-{}-{hash:016x}", samples.len())
}

fn attach(track: &Track, directory: &Path) -> Result<(), String> {
    let mut sources = Vec::with_capacity(4);
    for name in SOURCES {
        let path = directory.join(format!("{name}.f32"));
        let expected = track.decoded_frames() as u64 * 2 * 4;
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() != expected {
            return Err("Cached stem length does not match this track".into());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        sources.push(
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect::<Box<[f32]>>(),
        );
    }
    if track.stems_ready() {
        return Ok(());
    }
    track.attach_stems(sources.try_into().map_err(|_| "Missing source")?)
}

pub fn load_or_prepare(
    track: &Track,
    root: &Path,
    prepare: bool,
    progress: &AtomicU8,
    model: &str,
) -> Result<bool, String> {
    load_or_prepare_with_limit(track, root, prepare, progress, model, None)
}

/// A completed cache bypasses the automatic processing budget.
pub fn load_or_prepare_with_limit(
    track: &Track,
    root: &Path,
    prepare: bool,
    progress: &AtomicU8,
    model: &str,
    limit: Option<Duration>,
) -> Result<bool, String> {
    if track.stems_ready() {
        return Ok(true);
    }
    if !track.is_complete() {
        return Err("Wait for the track to finish loading".into());
    }
    if !["htdemucs", "htdemucs_ft"].contains(&model) {
        return Err("Unknown ONNX stem model".into());
    }
    let samples = track.export_stereo();
    let directory = root.join(cache_key(&samples, track.sample_rate, model));
    if directory.join("complete").exists() && attach(track, &directory).is_ok() {
        return Ok(true);
    }
    if !prepare {
        return Ok(false);
    }
    let deadline = limit.map(|limit| Instant::now() + limit);
    check_deadline(deadline)?;
    progress.store(0, Ordering::Relaxed);
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(directory.join("complete"));
    if let Err(error) = crate::onnx_stems::separate(
        &samples,
        track.sample_rate,
        &directory,
        progress,
        deadline,
        model,
    ) {
        for source in SOURCES {
            let _ = fs::remove_file(directory.join(format!("{source}.f32")));
        }
        return Err(error);
    }
    check_deadline(deadline)?;
    attach(track, &directory)?;
    fs::write(directory.join("complete"), format!("onnx-{model}-v1")).map_err(|e| e.to_string())?;
    progress.store(100, Ordering::Relaxed);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_identity_includes_model_audio_and_rate() {
        assert_ne!(
            cache_key(&[0., 1.], 44100, "htdemucs"),
            cache_key(&[0., 1.], 44100, "htdemucs_ft")
        );
        assert_ne!(
            cache_key(&[0., 1.], 44100, "htdemucs"),
            cache_key(&[0., 0.5], 44100, "htdemucs")
        );
        assert_ne!(
            cache_key(&[0., 1.], 44100, "htdemucs"),
            cache_key(&[0., 1.], 48000, "htdemucs")
        );
    }

    #[test]
    fn cache_requires_complete_marker_and_valid_stem_lengths() {
        let root = std::env::temp_dir().join(format!("djcraft-stem-test-{}", std::process::id()));
        let samples = vec![0.4; 100];
        let directory = root.join(cache_key(&samples, 48000, "htdemucs"));
        fs::create_dir_all(&directory).unwrap();
        let track = Track::from_samples("cache", 48000, samples.clone());
        for name in SOURCES {
            fs::write(
                directory.join(format!("{name}.f32")),
                [0.1f32.to_le_bytes(); 100].concat(),
            )
            .unwrap();
        }
        assert!(!load_or_prepare(&track, &root, false, &AtomicU8::new(0), "htdemucs").unwrap());
        fs::write(directory.join("complete"), b"onnx-htdemucs-v1").unwrap();
        assert!(load_or_prepare(&track, &root, false, &AtomicU8::new(0), "htdemucs").unwrap());
        let changed = Track::from_samples("cache", 48000, vec![0.2; 100]);
        assert!(!load_or_prepare(&changed, &root, false, &AtomicU8::new(0), "htdemucs").unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn automatic_budget_and_zero_deadline() {
        assert_eq!(automatic_budget(300.), Duration::from_secs(60));
        assert_eq!(automatic_budget(1.), Duration::from_millis(200));
        let track = Track::from_samples("uncached", 48000, vec![0.; 100]);
        let root = std::env::temp_dir().join(format!("djcraft-zero-budget-{}", std::process::id()));
        assert_eq!(
            load_or_prepare_with_limit(
                &track,
                &root,
                true,
                &AtomicU8::new(0),
                "htdemucs",
                Some(Duration::ZERO)
            )
            .unwrap_err(),
            AUTO_TIMEOUT
        );
        assert!(!root.exists());
    }
}
