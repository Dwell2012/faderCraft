//! Read-only audit using the same tag reader as the application.
use djcraft_library::{Library, TrackQuery};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let folder = std::env::args()
        .nth(1)
        .ok_or("usage: audit_tags <folder>")?;
    let mut library = Library::open_in_memory()?;
    let source = library.add_folder(Path::new(&folder))?;
    library.scan(source.id, |_| {})?;
    let mut rows = Vec::new();
    for track in library.tracks(&TrackQuery::default())? {
        let (path, tags) = library.file_tags(track.id)?;
        rows.push(serde_json::json!({"path": path, "tags": tags}));
    }
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}
