//! Versioned display-analysis sidecar. I/O is restricted to background workers.
use djcraft_analysis::BassAttack;
use djcraft_engine::Track;
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

const MAGIC: &[u8; 8] = b"DJCA0001";
const MAX_BYTES: u64 = 16 * 1024 * 1024;
static SERIAL: AtomicU64 = AtomicU64::new(0);

fn encode(attacks: &[BassAttack], dt: f64, duration: f64) -> Vec<u8> {
    let mut data = Vec::with_capacity(16 + attacks.len() * 12);
    data.extend_from_slice(MAGIC);
    data.extend_from_slice(&duration.to_le_bytes());
    for attack in attacks {
        data.extend_from_slice(&(attack.index as f64 * dt).to_le_bytes());
        data.extend_from_slice(&attack.strength.to_le_bytes());
    }
    data
}

fn decode(data: &[u8], dt: f64, duration: f64) -> Option<Vec<BassAttack>> {
    if data.len() < 16 || &data[..8] != MAGIC || (data.len() - 16) % 12 != 0 {
        return None;
    }
    let stored_duration = f64::from_le_bytes(data[8..16].try_into().ok()?);
    if !stored_duration.is_finite() || (stored_duration - duration).abs() > 0.05 {
        return None;
    }
    let mut result = Vec::new();
    let mut previous = -1.;
    for record in data[16..].chunks_exact(12) {
        let seconds = f64::from_le_bytes(record[..8].try_into().ok()?);
        let strength = f32::from_le_bytes(record[8..].try_into().ok()?);
        if !seconds.is_finite()
            || seconds < 0.
            || seconds > duration
            || seconds <= previous
            || !strength.is_finite()
            || !(0. ..=1.).contains(&strength)
        {
            return None;
        }
        previous = seconds;
        result.push(BassAttack {
            index: (seconds / dt).round() as usize,
            strength,
        });
    }
    Some(result)
}

fn read(path: &Path, dt: f64, duration: f64) -> Option<Vec<BassAttack>> {
    if fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    decode(&fs::read(path).ok()?, dt, duration)
}

pub(crate) fn load_or_analyse(track: &Track) -> Vec<BassAttack> {
    let dt = track.frames_per_peak() as f64 / track.sample_rate as f64;
    let duration = track.frames() as f64 / track.sample_rate as f64;
    let path = track
        .waveform_cache_path()
        .map(|path| path.with_extension("bass-attacks-v1"));
    if let Some(cached) = path.as_ref().and_then(|path| read(path, dt, duration)) {
        return cached;
    }
    let levels: Vec<_> = (0..track.peak_count()).map(|i| track.bands(i)[0]).collect();
    let attacks = djcraft_analysis::bass_attacks(&levels, dt);
    if let Some(path) = &path {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let temporary = path.with_extension(format!(
            "{}-{}.partial",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        if fs::write(&temporary, encode(&attacks, dt, duration)).is_ok() {
            if fs::rename(&temporary, path).is_err() {
                let _ = fs::remove_file(&temporary);
            }
            // Concurrent analysis and loading converge on the published sidecar.
            if let Some(cached) = read(path, dt, duration) {
                return cached;
            }
        }
    }
    attacks
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reload_reads_saved_events_and_repairs_a_damaged_sidecar() {
        let folder = std::env::temp_dir().join(format!(
            "fadercraft-attacks-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&folder).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/tests/fixtures/silence.flac");
        let first = Track::decode_cached(&fixture, 48000, Some(&folder)).unwrap();
        assert!(load_or_analyse(&first).is_empty());
        let path = first
            .waveform_cache_path()
            .unwrap()
            .with_extension("bass-attacks-v1");
        let duration = first.frames() as f64 / first.sample_rate as f64;
        // A supplied valid event proves reload consumes the sidecar rather than
        // re-detecting the silent fixture. No source media is modified.
        fs::write(
            &path,
            encode(
                &[BassAttack {
                    index: 0,
                    strength: 0.75,
                }],
                0.01,
                duration,
            ),
        )
        .unwrap();
        let reloaded = Track::decode_cached(&fixture, 22050, Some(&folder)).unwrap();
        assert_eq!(
            load_or_analyse(&reloaded),
            vec![BassAttack {
                index: 0,
                strength: 0.75
            }]
        );
        fs::write(&path, b"damaged").unwrap();
        assert!(load_or_analyse(&reloaded).is_empty());
        assert!(fs::metadata(&path).unwrap().len() >= 16);
        for file in fs::read_dir(&folder).unwrap() {
            fs::remove_file(file.unwrap().path()).unwrap();
        }
        fs::remove_dir(&folder).unwrap();
    }
    #[test]
    fn persistent_events_keep_source_time_across_rates_and_reject_stale_data() {
        let events = vec![
            BassAttack {
                index: 100,
                strength: 0.6,
            },
            BassAttack {
                index: 200,
                strength: 0.9,
            },
        ];
        let data = encode(&events, 0.01, 10.);
        assert_eq!(
            decode(&data, 0.005, 10.).unwrap(),
            vec![
                BassAttack {
                    index: 200,
                    strength: 0.6
                },
                BassAttack {
                    index: 400,
                    strength: 0.9
                }
            ]
        );
        assert!(decode(&data, 0.01, 11.).is_none());
        assert!(decode(&data[..data.len() - 1], 0.01, 10.).is_none());
        let mut corrupt = data.clone();
        corrupt[8..16].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(decode(&corrupt, 0.01, 10.).is_none());
        let invalid = encode(
            &[BassAttack {
                index: 100,
                strength: f32::NAN,
            }],
            0.01,
            10.,
        );
        assert!(decode(&invalid, 0.01, 10.).is_none());
        assert_eq!(decode(&encode(&[], 0.01, 10.), 0.005, 10.), Some(vec![]));
    }
}
