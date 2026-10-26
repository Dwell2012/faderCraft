//! Exercise the same local stem pipeline as Prepare Stems on a short excerpt.
#[path = "../src/stems.rs"]
mod stems;

use djcraft_engine::Track;
use std::path::Path;
use std::sync::atomic::AtomicU8;

fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: check_stems <audio file>")?;
    let decoded = Track::decode(Path::new(&path), 44_100).map_err(|e| e.to_string())?;
    let audio = decoded.export_stereo();
    let excerpt = Track::from_samples(
        "stem check",
        44_100,
        audio[..audio.len().min(44_100 * 2 * 8)].to_vec(),
    );
    let root = std::env::var_os("DJCRAFT_DATA_DIR").ok_or("Set DJCRAFT_DATA_DIR for this check")?;
    let root = Path::new(&root).join("stem-check");
    println!("Runtime: {}", stems::runtime_dir().display());
    let progress = AtomicU8::new(0);
    let model = std::env::args().nth(2).unwrap_or_else(|| "htdemucs".into());
    stems::warm_up(true, &model)?;
    let result = stems::load_or_prepare(&excerpt, &root, true, &progress, true, &model);
    stems::shutdown();
    result?;
    if !excerpt.stems_ready() {
        return Err("stem validation did not complete".into());
    }
    println!(
        "Validated four stems for an 8-second excerpt; cache: {}",
        root.display()
    );
    Ok(())
}
