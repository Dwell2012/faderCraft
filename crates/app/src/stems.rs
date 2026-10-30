//! Blocking local preparation and cache IO. Never called on the audio/UI thread.
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use djcraft_engine::Track;

// One separator at a time, avoiding GPU-memory contention between queued decks.
static PREPARATION: Mutex<Option<Worker>> = Mutex::new(None);
// Separate from the job lock so quitting can stop an active separation immediately.
static PROCESS: Mutex<Option<Child>> = Mutex::new(None);
const SOURCES: [&str; 4] = ["vocals", "other", "bass", "drums"];
const WORKER: &str = include_str!("../../../tools/prepare_stems.py");

struct Worker {
    input: ChildStdin,
    messages: Receiver<String>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        shutdown();
    }
}

impl Worker {
    fn start(prefer_gpu: bool, model: &str) -> Result<Self, String> {
        let runtime = runtime_dir();
        let python = runtime.join(if cfg!(windows) {
            "Scripts/python.exe"
        } else {
            "bin/python"
        });
        if !python.exists() {
            return Err("Install the local stem runtime using tools/setup-stems.ps1".into());
        }
        let script = runtime.join("prepare-worker.py");
        fs::write(&script, WORKER).map_err(|e| e.to_string())?;
        let log_path = runtime.join("worker.log");
        let log = File::create(&log_path).map_err(|e| e.to_string())?;
        let mut command = Command::new(python);
        command
            .arg(script)
            .arg("--serve")
            .arg("--model")
            .arg(model)
            .arg("--device")
            .arg(if prefer_gpu { "auto" } else { "cpu" })
            .env("TORCH_HOME", model_cache_dir(&runtime))
            .env("PYTHONUTF8", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let input = child.stdin.take().ok_or("Missing worker input")?;
        let stdout = child.stdout.take().ok_or("Missing worker output")?;
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        *PROCESS.lock().map_err(|e| e.to_string())? = Some(child);
        let worker = Self { input, messages };
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() > Duration::from_secs(1800) {
                return Err("Stem startup exceeded 30 minutes".into());
            }
            let line = worker
                .messages
                .recv_timeout(Duration::from_secs(120))
                .map_err(|e| format!("Stem startup failed ({e}); see {}", log_path.display()))?;
            if line == "DJCRAFT_READY" {
                return Ok(worker);
            }
        }
    }

    fn prepare(&mut self, request: serde_json::Value, progress: &AtomicU8) -> Result<(), String> {
        writeln!(self.input, "{request}").map_err(|e| e.to_string())?;
        self.input.flush().map_err(|e| e.to_string())?;
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() > Duration::from_secs(7200) {
                return Err("Stem preparation exceeded two hours".into());
            }
            let line = self
                .messages
                .recv_timeout(Duration::from_secs(180))
                .map_err(|e| format!("Stem worker stopped responding: {e}"))?;
            match line.as_str() {
                "DJCRAFT_DONE" => return Ok(()),
                "DJCRAFT_ERROR" => {
                    return Err(format!(
                        "Preparation failed; see prepare.log and {}",
                        runtime_dir().join("worker.log").display()
                    ));
                }
                _ => {
                    if let Some(percent) = progress_percent(&line) {
                        progress.fetch_max(percent, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}

/// Called on a background executor. A missing/broken runtime never prevents DJ use.
pub fn warm_up(prefer_gpu: bool, model: &str) -> Result<(), String> {
    let mut worker = PREPARATION.lock().map_err(|e| e.to_string())?;
    if worker.is_none() {
        *worker = Some(Worker::start(prefer_gpu, model)?);
    }
    Ok(())
}

/// Download/verify a model without touching the persistent inference worker.
pub fn download_model(model: &str) -> Result<(), String> {
    if !["htdemucs", "htdemucs_ft"].contains(&model) {
        return Err("Unknown model".into());
    }
    let runtime = runtime_dir();
    let python = runtime.join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    });
    let script = runtime.join("download-model.py");
    fs::write(&script, WORKER).map_err(|e| e.to_string())?;
    let log = File::create(runtime.join("model-download.log")).map_err(|e| e.to_string())?;
    let mut command = Command::new(python);
    command
        .arg(script)
        .arg("--download")
        .arg(model)
        .env("TORCH_HOME", model_cache_dir(&runtime))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    if command.status().map_err(|e| e.to_string())?.success() {
        Ok(())
    } else {
        Err(format!(
            "Download failed; see {}. Retry when connected.",
            runtime.join("model-download.log").display()
        ))
    }
}

pub fn model_downloaded(model: &str) -> bool {
    let runtime = runtime_dir();
    model_cache_dir(&runtime)
        .parent()
        .unwrap_or(&runtime)
        .join(format!("model-{model}.ready"))
        .is_file()
}

pub fn shutdown() {
    if let Ok(mut process) = PROCESS.lock() {
        if let Some(mut child) = process.take() {
            #[cfg(windows)]
            {
                // A venv python.exe can be a launcher for a second Python process.
                // Kill its tree as well, including during an active separation.
                use std::os::windows::process::CommandExt;
                let _ = Command::new("taskkill.exe")
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .creation_flags(0x08000000)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn cache_key(samples: &[f32], rate: u32, model: &str) -> String {
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
    format!("{model}-v1-{rate}-{}-{hash:016x}", samples.len())
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
            let root = dirs::data_local_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("DJCraft");
            let choice =
                fs::read_to_string(root.join("stem-runtime-choice.txt")).unwrap_or_default();
            root.join(runtime_name(&choice))
        })
}

fn runtime_name(choice: &str) -> &'static str {
    if choice.trim() == "stem-runtime-rocm" {
        "stem-runtime-rocm"
    } else {
        "stem-runtime"
    }
}

fn model_cache_dir(runtime: &Path) -> PathBuf {
    fs::read_to_string(runtime.join("model-cache-path.txt"))
        .ok()
        .filter(|path| !path.trim().is_empty())
        .map(|path| PathBuf::from(path.trim()))
        .unwrap_or_else(|| runtime.join("models"))
}

#[cfg(test)]
mod runtime_selection_tests {
    #[test]
    fn runtime_choice_only_accepts_supported_local_backends() {
        assert_eq!(
            super::runtime_name("stem-runtime-rocm\n"),
            "stem-runtime-rocm"
        );
        for value in ["", "stem-runtime", "../../other", "C:\\other", "unverified"] {
            assert_eq!(super::runtime_name(value), "stem-runtime");
        }
    }
}

/// Returns false for a cache miss when preparation was not requested.
pub fn load_or_prepare(
    track: &Track,
    root: &Path,
    prepare: bool,
    progress: &AtomicU8,
    prefer_gpu: bool,
    model: &str,
) -> Result<bool, String> {
    if track.stems_ready() {
        return Ok(true);
    }
    if !track.is_complete() {
        return Err("Wait for the track to finish loading".into());
    }
    let samples = track.export_stereo();
    let directory = root.join(cache_key(&samples, track.sample_rate, model));
    if directory.join("complete").exists() && attach(track, &directory).is_ok() {
        return Ok(true);
    }
    if !prepare {
        return Ok(false);
    }
    let mut worker = PREPARATION.lock().map_err(|e| e.to_string())?;
    progress.store(0, Ordering::Relaxed);
    if directory.join("complete").exists() && attach(track, &directory).is_ok() {
        return Ok(true);
    }
    if worker.is_none() {
        *worker = Some(Worker::start(prefer_gpu, model)?);
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
    let result = worker.as_mut().unwrap().prepare(
        serde_json::json!({
            "input": input, "output": directory, "sample_rate": track.sample_rate,
            "device": if prefer_gpu { "auto" } else { "cpu" },
            "log": directory.join("prepare.log"), "model": model,
        }),
        progress,
    );
    let _ = fs::remove_file(input);
    if let Err(error) = result {
        // Discard broken processes; the next request starts a clean worker.
        *worker = None;
        return Err(error);
    }
    attach(track, &directory)?;
    fs::write(directory.join("complete"), format!("{model}-v1")).map_err(|e| e.to_string())?;
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
        assert!(
            !load_or_prepare(&track, &root, false, &AtomicU8::new(0), true, "htdemucs").unwrap()
        );
        fs::write(directory.join("complete"), b"htdemucs-v1").unwrap();
        assert!(
            load_or_prepare(&track, &root, false, &AtomicU8::new(0), true, "htdemucs").unwrap()
        );
        assert!(track.stems_ready());
        let changed = Track::from_samples("cache", 48000, vec![0.2; 100]);
        assert!(
            !load_or_prepare(&changed, &root, false, &AtomicU8::new(0), true, "htdemucs").unwrap()
        );
        fs::write(directory.join("vocals.f32"), b"partial").unwrap();
        let reloaded = Track::from_samples("cache", 48000, samples);
        assert!(
            !load_or_prepare(&reloaded, &root, false, &AtomicU8::new(0), true, "htdemucs").unwrap()
        );
        assert!(!reloaded.stems_ready());
        fs::remove_dir_all(root).unwrap();
    }
}
