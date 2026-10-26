//! Optional cover downloads stored beside the library, never embedded in music files.
use crate::{Error, Result};
use std::path::{Component, Path, PathBuf};

const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub fn source_url(id: &str) -> Option<String> {
    (id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .then(|| format!("https://coverartarchive.org/release/{id}/front-500"))
}

/// Only library-relative artwork paths are accepted from saved metadata.
pub fn cached_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    let mut components = path.components();
    if components.next() != Some(Component::Normal("artwork".as_ref()))
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return None;
    }
    Some(root.join(path))
}

fn extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else {
        None
    }
}

/// Download a 500px front cover via the Cover Art Archive. A missing cover is
/// normal and does not invalidate the selected metadata. No provider credentials
/// are sent to the image server or its redirect targets.
pub fn download(root: &Path, url: &str) -> Result<Option<String>> {
    let prefix = "https://coverartarchive.org/release/";
    let id = url
        .strip_prefix(prefix)
        .and_then(|s| s.strip_suffix("/front-500"))
        .filter(|id| id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .ok_or_else(|| Error::Lookup("Unsupported cover-art source.".into()))?;
    for ext in ["jpg", "png"] {
        let relative = format!("artwork/{id}-500.{ext}");
        if let Ok(bytes) = std::fs::read(root.join(&relative)) {
            if bytes.len() as u64 <= MAX_BYTES && extension(&bytes) == Some(ext) {
                return Ok(Some(relative));
            }
        }
    }
    let mut response = match ureq::get(url)
        .header(
            "User-Agent",
            concat!("faderCraft/", env!("CARGO_PKG_VERSION")),
        )
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(error) => return Err(Error::Lookup(format!("Cover download failed: {error}"))),
    };
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|e| Error::Lookup(format!("Cover download failed: {e}")))?;
    let ext = extension(&bytes)
        .ok_or_else(|| Error::Lookup("Cover service did not return a JPEG or PNG image.".into()))?;
    let relative = format!("artwork/{id}-500.{ext}");
    let destination = root.join(&relative);
    std::fs::create_dir_all(root.join("artwork"))?;
    let temporary = destination.with_extension(format!("{ext}.{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, destination)?;
    Ok(Some(relative))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_paths_cannot_escape_the_artwork_directory() {
        let root = Path::new("library");
        assert_eq!(
            cached_path(root, "artwork/cover.jpg"),
            Some(root.join("artwork/cover.jpg"))
        );
        for bad in [
            "../secret",
            "artwork/../secret",
            "C:/secret",
            "/artwork/cover.jpg",
            "cover.jpg",
        ] {
            assert_eq!(cached_path(root, bad), None, "{bad}");
        }
    }
    #[test]
    fn only_supported_images_and_sources_are_accepted() {
        assert_eq!(extension(b"<html>not a cover</html>"), None);
        assert_eq!(extension(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert!(download(Path::new("unused"), "https://example.com/image.jpg").is_err());
    }
}
