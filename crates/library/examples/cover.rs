//! Download/cache a MusicBrainz release front cover without changing music files.
//! cargo run -p djcraft-library --example cover -- <release-id> <cache-root>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, release, root] = args.as_slice() else {
        return Err("usage: cover <release-id> <cache-root>".into());
    };
    let url = djcraft_library::artwork::source_url(release).ok_or("Invalid release ID")?;
    match djcraft_library::artwork::download(std::path::Path::new(root), &url)? {
        Some(relative) => println!(
            "Cached cover: {}",
            std::path::Path::new(root).join(relative).display()
        ),
        None => println!("No front cover available for this release."),
    }
    Ok(())
}
