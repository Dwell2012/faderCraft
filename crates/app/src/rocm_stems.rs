//! Persistent ROCm inference process. Called from background jobs.
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use super::check_deadline;

static STOPPING: AtomicBool = AtomicBool::new(false);

fn worker_message(
    messages: &Receiver<String>,
    deadline: Option<Instant>,
    idle: Duration,
) -> Result<String, String> {
    check_deadline(deadline)?;
    let wait = deadline.map_or(idle, |at| {
        at.saturating_duration_since(Instant::now()).min(idle)
    });
    let result = messages.recv_timeout(wait);
    check_deadline(deadline)?;
    result.map_err(|e| format!("Stem worker stopped responding: {e}"))
}

// One separator at a time, avoiding GPU-memory contention between queued decks.
static PREPARATION: Mutex<Option<(String, Worker)>> = Mutex::new(None);
// Separate from the job lock so quitting can stop an active separation immediately.
static PROCESS: Mutex<Option<Child>> = Mutex::new(None);
const WORKER: &str = include_str!("../../../tools/prepare_stems.py");

struct Worker {
    input: ChildStdin,
    messages: Receiver<String>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        stop_worker();
    }
}

impl Worker {
    fn start(model: &str, deadline: Option<Instant>) -> Result<Self, String> {
        check_deadline(deadline)?;
        let runtime = runtime_dir();
        let python = runtime.join(if cfg!(windows) {
            "Scripts/python.exe"
        } else {
            "bin/python"
        });
        if !python.exists() {
            return Err(format!(
                "Install the ROCm PyTorch stem runtime at {}",
                runtime.display()
            ));
        }
        let script = runtime.join("djcraft-rocm-worker.py");
        fs::write(&script, WORKER).map_err(|e| e.to_string())?;
        let log_path = runtime.join("rocm-worker.log");
        let log = File::create(&log_path).map_err(|e| e.to_string())?;
        let mut command = Command::new(python);
        command
            .arg(script)
            .arg("--serve")
            .arg("--model")
            .arg(model)
            .arg("--device")
            .arg("rocm")
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
        let mut process = PROCESS.lock().map_err(|e| e.to_string())?;
        if STOPPING.load(Ordering::Relaxed) {
            return Err("Stem separation stopped because the app is closing".into());
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
        *process = Some(child);
        drop(process);
        let worker = Self { input, messages };
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() > Duration::from_secs(1800) {
                return Err("Stem startup exceeded 30 minutes".into());
            }
            let line = worker_message(&worker.messages, deadline, Duration::from_secs(120))
                .map_err(|e| format!("{e}; see {}", log_path.display()))?;
            if line == "DJCRAFT_READY" {
                return Ok(worker);
            }
        }
    }

    fn prepare(
        &mut self,
        request: serde_json::Value,
        progress: &AtomicU8,
        deadline: Option<Instant>,
    ) -> Result<(), String> {
        check_deadline(deadline)?;
        writeln!(self.input, "{request}").map_err(|e| e.to_string())?;
        self.input.flush().map_err(|e| e.to_string())?;
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() > Duration::from_secs(7200) {
                return Err("Stem preparation exceeded two hours".into());
            }
            let line = worker_message(&self.messages, deadline, Duration::from_secs(180))?;
            match line.as_str() {
                "DJCRAFT_DONE" => return Ok(()),
                "DJCRAFT_ERROR" => {
                    return Err(format!(
                        "Preparation failed; see prepare.log and {}",
                        runtime_dir().join("rocm-worker.log").display()
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
pub fn warm_up(model: &str) -> Result<(), String> {
    let mut worker = PREPARATION.lock().map_err(|e| e.to_string())?;
    ensure_worker(&mut worker, model, None)
}

fn ensure_worker(
    worker: &mut Option<(String, Worker)>,
    model: &str,
    deadline: Option<Instant>,
) -> Result<(), String> {
    if worker.as_ref().is_none_or(|(loaded, _)| loaded != model) {
        *worker = None;
        *worker = Some((model.to_owned(), Worker::start(model, deadline)?));
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
    if !python.is_file() {
        return Err(format!(
            "Install the ROCm PyTorch stem runtime at {}",
            runtime.display()
        ));
    }
    let script = runtime.join("download-rocm-model.py");
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
    STOPPING.store(true, Ordering::Relaxed);
    stop_worker();
}

fn stop_worker() {
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

fn runtime_dir() -> PathBuf {
    std::env::var_os("DJCRAFT_ROCM_RUNTIME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("DJCraft/stem-runtime-rocm")
        })
}

fn model_cache_dir(runtime: &Path) -> PathBuf {
    fs::read_to_string(runtime.join("model-cache-path.txt"))
        .ok()
        .filter(|path| !path.trim().is_empty())
        .map(|path| PathBuf::from(path.trim()))
        .unwrap_or_else(|| runtime.join("models"))
}

pub fn separate(
    samples: &[f32],
    rate: u32,
    directory: &Path,
    progress: &AtomicU8,
    deadline: Option<Instant>,
    model: &str,
) -> Result<(), String> {
    let mut worker = PREPARATION.lock().map_err(|e| e.to_string())?;
    ensure_worker(&mut worker, model, deadline)?;
    let input = directory.join("input.f32");
    let result = (|| {
        let mut file = BufWriter::new(File::create(&input).map_err(|e| e.to_string())?);
        for (index, sample) in samples.iter().enumerate() {
            if index % 65536 == 0 {
                check_deadline(deadline)?;
            }
            file.write_all(&sample.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        file.flush().map_err(|e| e.to_string())?;
        drop(file);
        let (_, worker) = worker.as_mut().ok_or("ROCm worker unavailable")?;
        worker.prepare(
            serde_json::json!({
                "input": input, "output": directory, "sample_rate": rate,
                "device": "rocm", "log": directory.join("prepare.log"), "model": model,
            }),
            progress,
            deadline,
        )
    })();
    if result.is_err() {
        *worker = None;
    }
    let _ = fs::remove_file(input);
    result
}

fn progress_percent(line: &str) -> Option<u8> {
    line.strip_prefix("DJCRAFT_PROGRESS ")?
        .parse::<u8>()
        .ok()
        .filter(|p| *p <= 99)
}
