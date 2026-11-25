//! Apply a reviewed JSON list of blank-tag fills through the library's undoable tag batches.
//! Usage: cargo run -p djcraft-library --example tag_from_review -- <database> <json> [--apply] [--limit N]

use std::path::PathBuf;

use djcraft_library::{Library, TrackQuery};
use serde::Deserialize;

#[derive(Deserialize)]
struct Candidate {
    id: i64,
    locator: String,
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    replace_artist: bool,
    #[serde(default)]
    replace_title: bool,
    title: Option<String>,
    artist: Option<String>,
    album_artist: Option<String>,
    contributors: Option<String>,
    album: Option<String>,
    year: Option<i32>,
    bpm: Option<f64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return Err("expected <database> <json> [--apply] [--limit N]".into());
    }
    let apply = args.iter().any(|arg| arg == "--apply");
    let label = args
        .iter()
        .position(|arg| arg == "--label")
        .and_then(|index| args.get(index + 1))
        .cloned()
        .unwrap_or_else(|| {
            PathBuf::from(&args[2])
                .file_stem()
                .map(|stem| stem.to_string_lossy().replace('-', " "))
                .unwrap_or_else(|| "Reviewed tag changes".into())
        });
    let report_path = args
        .iter()
        .position(|arg| arg == "--report")
        .and_then(|index| args.get(index + 1));
    let plan_path = args
        .iter()
        .position(|arg| arg == "--plan-report")
        .and_then(|index| args.get(index + 1));
    let limit = args
        .iter()
        .position(|arg| arg == "--limit")
        .and_then(|index| args.get(index + 1))
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(usize::MAX);
    let candidates: Vec<Candidate> = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let mut library = Library::open(&PathBuf::from(&args[1]))?;
    let mut changes = Vec::new();
    let mut skipped = 0;
    let mut skip_details = Vec::new();
    for candidate in candidates {
        if changes.len() >= limit {
            break;
        }
        let Some(track) = library
            .tracks(&TrackQuery {
                id: Some(candidate.id),
                ..Default::default()
            })?
            .pop()
        else {
            skipped += 1;
            continue;
        };
        if !track
            .path
            .to_string_lossy()
            .replace('/', "\\")
            .ends_with(&candidate.locator)
        {
            skipped += 1;
            continue;
        }
        let (_, before) = match library.file_tags(candidate.id) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Skipping {}: {error}", track.path.display());
                skip_details.push((track.path.display().to_string(), error.to_string()));
                skipped += 1;
                continue;
            }
        };
        let mut after = before.clone();
        if let Some(title) = candidate.title
            && (candidate.replace_title
                || after.title.trim().is_empty()
                || after.title.to_ascii_lowercase().starts_with("track "))
        {
            after.title = title;
        }
        if let Some(artist) = candidate.artist
            && (candidate.replace_artist || after.artist.trim().is_empty())
        {
            after.artist = artist;
        }
        if let Some(album_artist) = candidate.album_artist
            && (candidate.replace
                || after
                    .album_artist
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty())
        {
            after.album_artist = Some(album_artist);
        }
        if let Some(contributors) = candidate.contributors
            && after
                .contributors
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            after.contributors = Some(contributors);
        }
        if let Some(album) = candidate.album
            && (candidate.replace || after.album.trim().is_empty())
        {
            after.album = album;
        }
        if let Some(year) = candidate.year
            && (candidate.replace || after.date.as_deref().unwrap_or_default().trim().is_empty())
        {
            after.date = Some(year.to_string());
        }
        if let Some(bpm) = candidate.bpm
            && after.bpm.is_none()
            && (60.0..=220.0).contains(&bpm)
        {
            after.bpm = Some((bpm * 100.0).round() / 100.0);
        }
        let change = library.plan_edit(candidate.id, &after)?;
        if !change.is_empty() {
            changes.push(change);
        }
    }
    println!(
        "Planned {} changes; skipped {} stale or unreadable tracks",
        changes.len(),
        skipped
    );
    if let Some(path) = plan_path {
        let paths: Vec<String> = changes
            .iter()
            .map(|change| change.path.display().to_string())
            .collect();
        std::fs::write(path, serde_json::to_vec_pretty(&paths)?)?;
    }
    for change in changes.iter().take(10) {
        println!("{}: {:?}", change.path.display(), change.differences());
    }
    if apply {
        let summary = library.apply_changes(&label, &changes)?;
        println!(
            "Written {}; skipped {}; undo batch {:?}",
            summary.written,
            summary.skipped.len(),
            summary.batch
        );
        for (path, reason) in summary.skipped.iter().take(10) {
            eprintln!("Skipped {}: {reason}", path.display());
        }
        skip_details.extend(
            summary
                .skipped
                .iter()
                .map(|(path, reason)| (path.display().to_string(), reason.clone())),
        );
    }
    if let Some(path) = report_path {
        std::fs::write(path, serde_json::to_vec_pretty(&skip_details)?)?;
    }
    Ok(())
}
