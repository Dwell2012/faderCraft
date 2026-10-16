use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub type SavedLoops = BTreeMap<String, [Option<[f64; 2]>; 16]>;

pub fn load(root: &Path) -> SavedLoops {
    std::fs::read(root.join("saved-loops.json"))
        .ok()
        .and_then(|bytes| {
            serde_json::from_slice::<BTreeMap<String, Vec<Option<[f64; 2]>>>>(&bytes).ok()
        })
        .map(|entries| {
            entries
                .into_iter()
                .map(|(key, values)| {
                    (
                        key,
                        std::array::from_fn(|i| values.get(i).copied().flatten()),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn save(root: &Path, loops: &SavedLoops) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let temporary = root.join("saved-loops.json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(loops).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, root.join("saved-loops.json")).map_err(|e| e.to_string())
}

/// Export the source selection at original speed. Never writes to user music.
pub fn export(
    track: &djcraft_engine::Track,
    region: [f64; 2],
    root: &Path,
) -> Result<PathBuf, String> {
    if region.iter().any(|v| !v.is_finite() || *v < 0.) {
        return Err("Invalid recording range".into());
    }
    let rate = track.sample_rate;
    let first = (region[0] * rate as f64).round() as usize * 2;
    let end = (region[1] * rate as f64).round() as usize * 2;
    if first >= end || end > track.decoded_frames() * 2 {
        return Err("Invalid recording range".into());
    }
    let bytes = u32::try_from((end - first) * 2)
        .ok()
        .filter(|v| *v <= u32::MAX - 36)
        .ok_or("Recording too large for WAV")?;
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = root.join(format!("Loop-{stamp}.wav"));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(file);
    let samples = track.export_stereo();
    let write = || -> std::io::Result<()> {
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
        for sample in &samples[first..end] {
            out.write_all(&((sample.clamp(-1., 1.) * 32767.).round() as i16).to_le_bytes())?;
        }
        out.flush()
    };
    let mut write = write;
    write().map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slots_survive_restart_and_recording_is_a_stereo_wav() {
        let root = std::env::temp_dir().join(format!(
            "djcraft-loop-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("saved-loops.json"),
            r#"{"legacy":[[1.0,3.0],null,null,null]}"#,
        )
        .unwrap();
        let old = load(&root);
        assert_eq!(old["legacy"][0], Some([1., 3.]));
        assert!(old["legacy"][4..].iter().all(Option::is_none));
        let mut loops = SavedLoops::new();
        loops.insert(
            "song.mp3".into(),
            std::array::from_fn(|i| (i == 0).then_some([1., 3.])),
        );
        save(&root, &loops).unwrap();
        assert_eq!(load(&root), loops);
        loops.get_mut("song.mp3").unwrap()[0] = None;
        save(&root, &loops).unwrap();
        assert_eq!(load(&root), loops);
        let track = djcraft_engine::Track::from_samples("test", 48000, vec![0.25; 9600]);
        let file = export(&track, [0.02, 0.07], &root).unwrap();
        let bytes = std::fs::read(file).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + 2400 * 4);
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 48000);
        assert!(export(&track, [0.1, 0.2], &root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
