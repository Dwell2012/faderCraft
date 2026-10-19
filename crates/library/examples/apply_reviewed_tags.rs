//! Apply an explicitly reviewed tag manifest with the library's undo history.
use djcraft_library::{Library, PlannedChange, TrackEdit, TrackQuery};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Review {
    folder: PathBuf,
    changes: Vec<Change>,
}

#[derive(Deserialize)]
struct Change {
    path: PathBuf,
    before: TrackEdit,
    after: TrackEdit,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, db, manifest, confirmation] = args.as_slice() else {
        return Err(
            "usage: apply_reviewed_tags <library.db> <manifest.json> --apply-reviewed".into(),
        );
    };
    if confirmation != "--apply-reviewed" {
        return Err("review confirmation required".into());
    }
    let review: Review = serde_json::from_slice(&std::fs::read(manifest)?)?;
    let root = review.folder.canonicalize()?;
    let mut library = Library::open(std::path::Path::new(db))?;
    let tracks = library.tracks(&TrackQuery::default())?;
    let mut changes = Vec::new();
    for change in review.changes {
        let path = change.path.canonicalize()?;
        if !path.starts_with(&root) {
            return Err(format!("file outside reviewed folder: {}", path.display()).into());
        }
        let track = tracks
            .iter()
            .find(|t| t.path.canonicalize().ok().as_ref() == Some(&path))
            .ok_or_else(|| format!("file missing from library: {}", path.display()))?;
        changes.push(PlannedChange {
            track_id: track.id,
            path: change.path,
            before: change.before,
            after: change.after,
            rename_to: None,
        });
    }
    let label = format!(
        "Reviewed {} metadata cleanup",
        review
            .folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );
    let summary = library.apply_changes(&label, &changes)?;
    println!("{summary:#?}");
    if !summary.skipped.is_empty() {
        return Err("some files were skipped; inspect the summary".into());
    }
    for change in changes {
        let (_, actual) = library.file_tags(change.track_id)?;
        if actual != change.after {
            return Err(format!("tag verification failed: {}", change.path.display()).into());
        }
    }
    println!("All written tags verified against the reviewed manifest.");
    Ok(())
}
