//! Portable cue/preparation keys use stable library track IDs, not machine paths.
use djcraft_library::{Library, TrackQuery};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{Mutex, OnceLock},
};

static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
fn key_in(library: &Library, path: &Path) -> Option<String> {
    let id = library.track_at(path).ok().flatten()?;
    library
        .track_token(id)
        .ok()
        .flatten()
        .map(|token| format!("library-track:{token}"))
}
fn resolve_in(library: &Library, key: &str) -> Option<String> {
    let token = key.strip_prefix("library-track:")?;
    let id = library.track_id_for_token(token).ok().flatten()?;
    library
        .tracks(&TrackQuery {
            id: Some(id),
            ..Default::default()
        })
        .ok()?
        .pop()
        .map(|t| t.path.to_string_lossy().into_owned())
}
pub fn clear_cache() {
    if let Some(cache) = CACHE.get() {
        cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}
pub fn key(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let text = path.to_string_lossy().into_owned();
    if text.starts_with("library-track:") {
        return text;
    }
    let cache = CACHE.get_or_init(Default::default);
    if let Some(key) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&text) {
        return key.clone();
    }
    let db = crate::storage::database_path();
    if db.exists() {
        if let Some(key) = Library::open(&db).ok().and_then(|l| key_in(&l, path)) {
            cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(text, key.clone());
            return key;
        }
    }
    text
}
pub fn resolve(key: &str) -> Option<String> {
    let Some(id) = key.strip_prefix("library-track:") else {
        return Some(key.into());
    };
    let _ = id;
    let db = crate::storage::database_path();
    if !db.exists() {
        return None;
    }
    resolve_in(&Library::open(&db).ok()?, key)
}
pub fn normalize<T: serde::Serialize>(
    root: &Path,
    filename: &str,
    entries: BTreeMap<String, T>,
) -> BTreeMap<String, T> {
    // Unit-test and export folders are independent of the running user library.
    if root != crate::storage::library_dir() {
        return entries;
    }
    let old_keys: Vec<_> = entries.keys().cloned().collect();
    let entries: BTreeMap<_, _> = entries
        .into_iter()
        .map(|(path, value)| (key(Path::new(&path)), value))
        .collect();
    // Persist the key conversion before a later relink changes the old machine paths.
    if entries.keys().cloned().collect::<Vec<_>>() != old_keys {
        let write = (|| -> std::io::Result<()> {
            let backup = root.join(format!("{filename}.before-portable-ids"));
            if !backup.exists() {
                std::fs::copy(root.join(filename), backup)?;
            }
            let temporary = root.join(format!("{filename}.identity.tmp"));
            std::fs::write(
                &temporary,
                serde_json::to_vec_pretty(&entries).map_err(std::io::Error::other)?,
            )?;
            std::fs::rename(temporary, root.join(filename))
        })();
        if let Err(error) = write {
            eprintln!("Portable track records: {error}");
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn track_identity_follows_relinked_music_without_changing_cue_keys() {
        let root =
            std::env::temp_dir().join(format!("fadercraft-track-identity-{}", std::process::id()));
        std::fs::create_dir_all(root.join("old")).unwrap();
        std::fs::create_dir_all(root.join("new")).unwrap();
        let old = root.join("old/song.mp3");
        let new = root.join("new/song.mp3");
        std::fs::write(
            &old,
            include_bytes!("../../library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        let mut library = Library::open_in_memory().unwrap();
        let source = library.add_folder(&root.join("old")).unwrap();
        library.scan(source.id, |_| {}).unwrap();
        let id = library.tracks(&TrackQuery::default()).unwrap()[0].id;
        let key = key_in(&library, &old).unwrap();
        let cues = BTreeMap::from([(key.clone(), 1.25)]);
        std::fs::rename(&old, &new).unwrap();
        library.relocate(id, &new).unwrap();
        assert_eq!(key_in(&library, &new).as_deref(), Some(key.as_str()));
        assert_eq!(cues[&key], 1.25);
        assert!(Path::new(&resolve_in(&library, &key).unwrap()).exists());
        assert_eq!(key_in(&library, &root.join("unknown.mp3")), None);
        library.remove_track(id).unwrap();
        let another = root.join("old/another.mp3");
        std::fs::write(
            &another,
            include_bytes!("../../library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        library.scan(source.id, |_| {}).unwrap();
        let new_id = library.track_at(&another).unwrap().unwrap();
        assert_eq!(new_id, id, "exercise SQLite row-ID reuse");
        assert_ne!(key_in(&library, &another).unwrap(), key);
        assert_eq!(
            resolve_in(&library, &key),
            None,
            "deleted track cues must not attach to new music"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
