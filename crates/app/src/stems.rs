//! Blocking local preparation and cache IO. Never called on the audio/UI thread.
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

use djcraft_engine::Track;

// One CPU separator at a time, including when both decks are prepared together.
static PREPARATION: Mutex<()> = Mutex::new(());
const SOURCES: [&str; 4] = ["vocals", "other", "bass", "drums"];
const WORKER: &str = include_str!("../../../tools/prepare_stems.py");

fn cache_key(samples: &[f32], rate: u32) -> String {
    // Hash the exact decoded audio, not its name: edits, decoder/rate changes and
    // replacement files cannot accidentally reuse the old separation.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in rate
        .to_le_bytes()
        .into_iter()
        .chain(samples.iter().flat_map(|s| s.to_le_bytes()))
    {
        hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
    }
    format!("htdemucs-v1-{rate}-{}-{hash:016x}", samples.len())
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

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("DJCRAFT_STEM_RUNTIME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("DJCraft/stem-runtime")
        })
}

/// Returns false for a cache miss when preparation was not requested.
pub fn load_or_prepare(
    track: &Track,
    root: &Path,
    prepare: bool,
    progress: &AtomicU8,
) -> Result<bool, String> {
    if track.stems_ready() {
        return Ok(true);
    }
    if !track.is_complete() {
        return Err("Wait for the track to finish loading".into());
    }
    let samples = track.export_stereo();
    let directory = root.join(cache_key(&samples, track.sample_rate));
    if directory.join("complete").exists() && attach(track, &directory).is_ok() {
        return Ok(true);
    }
    if !prepare {
        return Ok(false);
    }
    let _guard = PREPARATION.lock().map_err(|e| e.to_string())?;
    progress.store(0, Ordering::Relaxed);
    if directory.join("complete").exists() && attach(track, &directory).is_ok() {
        return Ok(true);
    }
    let runtime = runtime_dir();
    let python = runtime.join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    });
    if !python.exists() {
        return Err("Install the local stem runtime using tools/setup-stems.ps1".into());
    }
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    // A marker is published only after all four sources have been validated.
    let _ = fs::remove_file(directory.join("complete"));
    let input = directory.join("input.f32");
    let mut file = BufWriter::new(File::create(&input).map_err(|e| e.to_string())?);
    for sample in samples {
        file.write_all(&sample.to_le_bytes())
            .map_err(|e| e.to_string())?;
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let script = directory.join("prepare.py");
    fs::write(&script, WORKER).map_err(|e| e.to_string())?;
    let log_path = directory.join("prepare.log");
    let log = File::create(&log_path).map_err(|e| e.to_string())?;
    let mut command = Command::new(python);
    command
        .arg(&script)
        .arg(&input)
        .arg(&directory)
        .arg(track.sample_rate.to_string())
        .env("TORCH_HOME", runtime.join("models"))
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // No console window while mixing.
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if let Some(percent) = progress_percent(&line) {
                progress.fetch_max(percent, Ordering::Relaxed);
            }
        }
    }
    let result = child.wait();
    let _ = fs::remove_file(input);
    let status = result.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("Preparation failed; see {}", log_path.display()));
    }
    attach(track, &directory)?;
    fs::write(directory.join("complete"), b"htdemucs-v1").map_err(|e| e.to_string())?;
    progress.store(100, Ordering::Relaxed);
    Ok(true)
}

fn progress_percent(line: &str) -> Option<u8> {
    line.strip_prefix("DJCRAFT_PROGRESS ")?
        .parse::<u8>()
        .ok()
        .filter(|p| *p <= 99)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_accepts_only_worker_messages_and_reserves_completion() {
        assert_eq!(progress_percent("DJCRAFT_PROGRESS 42"), Some(42));
        assert_eq!(progress_percent("DJCRAFT_PROGRESS 100"), None);
        assert_eq!(progress_percent("42% complete"), None);
        assert_eq!(progress_percent("DJCRAFT_PROGRESS -1"), None);
    }
    #[test]
    fn cache_identity_includes_audio_and_rate() {
        assert_ne!(cache_key(&[0., 1.], 44100), cache_key(&[0., 0.5], 44100));
        assert_ne!(cache_key(&[0., 1.], 44100), cache_key(&[0., 1.], 48000));
    }

    #[test]
    fn complete_cache_loads_but_partial_or_changed_audio_does_not() {
        let root = std::env::temp_dir().join(format!(
            "djcraft-stem-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let samples = vec![0.4; 100];
        let directory = root.join(cache_key(&samples, 48000));
        fs::create_dir_all(&directory).unwrap();
        let track = Track::from_samples("cache", 48000, samples.clone());
        for name in SOURCES {
            fs::write(
                directory.join(format!("{name}.f32")),
                [0.1f32.to_le_bytes(); 100].concat(),
            )
            .unwrap();
        }
        assert!(!load_or_prepare(&track, &root, false, &AtomicU8::new(0)).unwrap());
        fs::write(directory.join("complete"), b"htdemucs-v1").unwrap();
        assert!(load_or_prepare(&track, &root, false, &AtomicU8::new(0)).unwrap());
        assert!(track.stems_ready());
        let changed = Track::from_samples("cache", 48000, vec![0.2; 100]);
        assert!(!load_or_prepare(&changed, &root, false, &AtomicU8::new(0)).unwrap());
        fs::write(directory.join("vocals.f32"), b"partial").unwrap();
        let reloaded = Track::from_samples("cache", 48000, samples);
        assert!(!load_or_prepare(&reloaded, &root, false, &AtomicU8::new(0)).unwrap());
        assert!(!reloaded.stems_ready());
        fs::remove_dir_all(root).unwrap();
    }
}
