//! Run the app's pending analysis without restarting an active DJ session.
#[path = "../src/analysis.rs"]
mod analysis;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("DJCRAFT_DATA_DIR").ok_or("Set DJCRAFT_DATA_DIR")?;
    let root = std::path::Path::new(&root);
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("settings.json"))?)?;
    let range = djcraft_analysis::TempoRange {
        min_bpm: settings["tempo_min_bpm"].as_f64().unwrap_or(70.),
        max_bpm: settings["tempo_max_bpm"].as_f64().unwrap_or(180.),
    };
    let progress = analysis::Progress::default();
    println!("Analysing pending tracks in {}", root.display());
    analysis::analyse_pending(
        &root.join("library.db"),
        &root.join("waveforms"),
        range,
        &progress,
    )?;
    println!(
        "Analysis completed: {} tracks",
        progress.done.load(std::sync::atomic::Ordering::Relaxed)
    );
    Ok(())
}
