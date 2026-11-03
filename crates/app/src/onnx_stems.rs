//! Native ONNX stem inference. Called only from background jobs.
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use ort::{session::Session, value::Tensor};
use sha2::{Digest, Sha256};

use crate::stems::AUTO_TIMEOUT;

const MODEL_RATE: u32 = 44_100;
const SEGMENT: usize = 343_980;
const OVERLAP: usize = SEGMENT / 4;
const STRIDE: usize = SEGMENT - OVERLAP;
// ONNX output order; the app's cache order is handled by file names.
const SOURCES: [&str; 4] = ["drums", "bass", "other", "vocals"];
static SESSION: Mutex<Option<(String, Session)>> = Mutex::new(None);
static ENVIRONMENT: OnceLock<ort::environment::Environment> = OnceLock::new();
static STOPPING: AtomicBool = AtomicBool::new(false);
const DETAILED: [&str; 4] = ["drums", "bass", "other", "vocals"];
const DETAILED_SHA256: [&str; 4] = [
    "f76b68af36066e38885b369299b5032a861038f9b49da5aa6cf1c31cfa69cf27",
    "2a74d9283fc2336fcc58d50f87a7080aff57aea372f65cfe3f0211ea1ff16182",
    "90e11806c1bb558ca9d9c7e909d28a2854f7f217982e90482dbed6442513daad",
    "8c5d5e2da1f27050240bb80236673307ee3b40d4b064066d9350f4d64bfd544d",
];
const DETAILED_BYTES: u64 = 316_446_953;
const DETAILED_REVISION: &str = "8616370ed541bc183dbe15fb0d54d5f49918f47e";

fn model_dir() -> PathBuf {
    std::env::var_os("DJCRAFT_ONNX_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("DJCraft/onnx-models")
        })
}

fn model_path(name: &str) -> PathBuf {
    if name == "htdemucs" {
        if let Some(path) = std::env::var_os("DJCRAFT_ONNX_MODEL") {
            return PathBuf::from(path);
        }
    }
    model_dir().join(format!("{name}.onnx"))
}

pub fn model_downloaded(model: &str) -> bool {
    match model {
        "htdemucs" => fs::metadata(model_path(model)).is_ok_and(|m| m.len() > 100_000_000),
        "htdemucs_ft" => DETAILED.iter().all(|source| {
            fs::metadata(model_path(&format!("htdemucs_ft_{source}")))
                .is_ok_and(|m| m.len() == DETAILED_BYTES)
        }),
        _ => false,
    }
}

fn download_file(
    name: &str,
    url: &str,
    expected_bytes: Option<u64>,
    expected_sha: Option<&str>,
) -> Result<(), String> {
    let target = model_path(name);
    if fs::metadata(&target)
        .is_ok_and(|m| expected_bytes.map_or(m.len() > 100_000_000, |size| m.len() == size))
    {
        return Ok(());
    }
    fs::create_dir_all(target.parent().ok_or("Invalid ONNX model path")?)
        .map_err(|e| e.to_string())?;
    let partial = target.with_extension("onnx.partial");
    let response = ureq::get(url).call().map_err(|e| e.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut file = File::create(&partial).map_err(|e| e.to_string())?;
    let bytes = std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    if !expected_bytes.map_or(bytes > 100_000_000, |size| bytes == size) {
        let _ = fs::remove_file(&partial);
        return Err("ONNX model download was incomplete".into());
    }
    if let Some(expected) = expected_sha {
        let mut reader = File::open(&partial).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1 << 20];
        loop {
            let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        if format!("{:x}", hasher.finalize()) != expected {
            let _ = fs::remove_file(&partial);
            return Err(format!("{name} download failed SHA-256 verification"));
        }
    }
    if target.exists() {
        fs::remove_file(&target).map_err(|e| e.to_string())?;
    }
    fs::rename(&partial, &target).map_err(|e| e.to_string())
}

pub fn download_model(model: &str) -> Result<(), String> {
    match model {
        "htdemucs" => download_file(
            "htdemucs",
            "https://huggingface.co/StemSplitio/htdemucs-onnx/resolve/d54ed9eb60e258ea82131c6ee14578628816456a/htdemucs.onnx",
            None,
            None,
        ),
        "htdemucs_ft" => {
            for (index, source) in DETAILED.iter().enumerate() {
                let name = format!("htdemucs_ft_{source}");
                let url = format!(
                    "https://huggingface.co/StemSplitio/htdemucs-ft-onnx/resolve/{DETAILED_REVISION}/{name}.onnx"
                );
                download_file(
                    &name,
                    &url,
                    Some(DETAILED_BYTES),
                    Some(DETAILED_SHA256[index]),
                )?;
            }
            Ok(())
        }
        _ => Err("Unknown ONNX stem model".into()),
    }
}

fn session(
    name: &str,
) -> Result<std::sync::MutexGuard<'static, Option<(String, Session)>>, String> {
    let mut guard = SESSION.lock().map_err(|e| e.to_string())?;
    if guard.as_ref().is_none_or(|(loaded, _)| loaded != name) {
        *guard = None;
        let path = model_path(name);
        if !fs::metadata(&path).is_ok_and(|m| m.len() > 100_000_000) {
            return Err(format!(
                "Download the {name} ONNX model in Settings > Audio first"
            ));
        }
        if ENVIRONMENT.get().is_none() {
            let environment = ort::init().build().map_err(|e| e.to_string())?;
            let _ = ENVIRONMENT.set(environment);
        }
        let environment = ENVIRONMENT.get().ok_or("ONNX environment unavailable")?;
        *guard = Some((
            name.to_owned(),
            Session::builder(environment)
                .map_err(|e| e.to_string())?
                .with_intra_threads(4)
                .map_err(|e| e.to_string())?
                .commit_from_file(path)
                .map_err(|e| e.to_string())?,
        ));
    }
    Ok(guard)
}

pub fn warm_up(model: &str) -> Result<(), String> {
    let name = match model {
        "htdemucs" => "htdemucs".to_string(),
        "htdemucs_ft" => "htdemucs_ft_drums".to_string(),
        _ => return Err("Unknown ONNX stem model".into()),
    };
    let _session = session(&name)?;
    Ok(())
}

pub fn shutdown() {
    STOPPING.store(true, Ordering::Relaxed);
    if let Ok(mut guard) = SESSION.lock() {
        *guard = None;
    }
}

fn resample(input: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if source_rate == target_rate {
        return input.to_vec();
    }
    let frames = input.len() / 2;
    let out_frames = ((frames as u64 * target_rate as u64 + source_rate as u64 / 2)
        / source_rate as u64) as usize;
    let mut out = vec![0.0; out_frames * 2];
    for i in 0..out_frames {
        let pos = i as f64 * source_rate as f64 / target_rate as f64;
        let a = (pos.floor() as usize).min(frames.saturating_sub(1));
        let b = (a + 1).min(frames.saturating_sub(1));
        let frac = (pos - a as f64) as f32;
        for channel in 0..2 {
            out[2 * i + channel] =
                input[2 * a + channel] * (1.0 - frac) + input[2 * b + channel] * frac;
        }
    }
    out
}

fn deadline_check(deadline: Option<Instant>) -> Result<(), String> {
    if STOPPING.load(Ordering::Relaxed) {
        return Err("Stem separation stopped because the app is closing".into());
    }
    if deadline.is_some_and(|d| Instant::now() >= d) {
        Err(AUTO_TIMEOUT.into())
    } else {
        Ok(())
    }
}

pub fn separate(
    input: &[f32],
    rate: u32,
    output: &Path,
    progress: &AtomicU8,
    deadline: Option<Instant>,
    model: &str,
) -> Result<(), String> {
    if input.len() < 2 || input.len() % 2 != 0 || rate == 0 {
        return Err("Invalid stereo input for ONNX separation".into());
    }
    deadline_check(deadline)?;
    let mix = resample(input, rate, MODEL_RATE);
    let frames = mix.len() / 2;
    let chunks = frames.div_ceil(STRIDE);
    let mut stems: [Vec<f32>; 4] = std::array::from_fn(|_| vec![0.; frames * 2]);
    let mut weights = vec![0.0f32; frames];
    let passes = match model {
        "htdemucs" => 1,
        "htdemucs_ft" => 4,
        _ => return Err("Unknown ONNX stem model".into()),
    };
    for pass in 0..passes {
        let name = if passes == 1 {
            "htdemucs".to_string()
        } else {
            format!("htdemucs_ft_{}", DETAILED[pass])
        };
        let mut guard = session(&name)?;
        let session = &mut guard.as_mut().ok_or("ONNX model unavailable")?.1;
        for chunk in 0..chunks {
            deadline_check(deadline)?;
            let start = chunk * STRIDE;
            let len = (frames - start).min(SEGMENT);
            let mut data = vec![0.0f32; SEGMENT * 2];
            for channel in 0..2 {
                for i in 0..len {
                    data[channel * SEGMENT + i] = mix[(start + i) * 2 + channel];
                }
            }
            let tensor =
                Tensor::from_array(([1usize, 2, SEGMENT], data)).map_err(|e| e.to_string())?;
            let outputs = session
                .run(ort::inputs![tensor])
                .map_err(|e| e.to_string())?;
            let (shape, prediction) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            if shape.as_ref() != [1, 4, 2, SEGMENT as i64] {
                return Err(format!("Unexpected ONNX output shape: {shape:?}"));
            }
            for i in 0..len {
                let weight = if i < OVERLAP {
                    i as f32 / (OVERLAP - 1) as f32
                } else if i >= SEGMENT - OVERLAP {
                    (SEGMENT - 1 - i) as f32 / (OVERLAP - 1) as f32
                } else {
                    1.0
                };
                if pass == 0 {
                    weights[start + i] += weight;
                }
                for source in 0..4 {
                    if passes == 4 && source != pass {
                        continue;
                    }
                    for channel in 0..2 {
                        stems[source][(start + i) * 2 + channel] +=
                            prediction[(source * 2 + channel) * SEGMENT + i] * weight;
                    }
                }
            }
            progress.store(
                ((pass * chunks + chunk + 1) * 90 / (passes * chunks)).min(99) as u8,
                Ordering::Relaxed,
            );
        }
    }
    for (index, stem) in stems.into_iter().enumerate() {
        deadline_check(deadline)?;
        let mut normalized = stem;
        for (i, pair) in normalized.chunks_exact_mut(2).enumerate() {
            let weight = weights[i].max(1e-8);
            pair[0] /= weight;
            pair[1] /= weight;
        }
        let native = resample(&normalized, MODEL_RATE, rate);
        let mut file = BufWriter::new(
            File::create(output.join(format!("{}.f32", SOURCES[index])))
                .map_err(|e| e.to_string())?,
        );
        for value in native.iter().take(input.len()) {
            file.write_all(&value.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        for _ in native.len()..input.len() {
            file.write_all(&0.0f32.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        file.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
