//! Keeps legacy loose-file records in the asset folder while putting records
//! for added music folders in their own portable faderCraft SQLite database.
use djcraft_library::{Library, folder_state::TrackJsonField};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{Mutex, OnceLock},
};

type Cache = HashMap<(TrackJsonField, String), String>;
static SAVED: OnceLock<Mutex<Cache>> = OnceLock::new();
fn cache() -> &'static Mutex<Cache> {
    SAVED.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn load<T: DeserializeOwned + Serialize>(
    root: &Path,
    field: TrackJsonField,
    mut legacy: BTreeMap<String, T>,
) -> BTreeMap<String, T> {
    if root != crate::storage::library_dir() {
        return legacy;
    }
    let db = crate::storage::database_path();
    if !db.exists() {
        return legacy;
    }
    let Ok(library) = Library::open(&db) else {
        return legacy;
    };
    let Ok(portable) = library.folder_json_entries(field) else {
        return legacy;
    };
    cache()
        .lock()
        .unwrap()
        .retain(|(stored_field, _), _| *stored_field != field);
    let existing: std::collections::HashSet<_> =
        portable.iter().map(|(key, _)| key.as_str()).collect();
    // Seed from the old JSON once. It stays available as a migration backup.
    for (key, value) in &legacy {
        if existing.contains(key.as_str()) {
            continue;
        }
        if let Ok(json) = serde_json::to_string(value) {
            match library.save_folder_json(key, field, &json) {
                Ok(true) => {
                    cache().lock().unwrap().insert((field, key.clone()), json);
                }
                Ok(false) => {}
                Err(error) => eprintln!("Could not migrate {key} to folder metadata: {error}"),
            }
        }
    }
    for (key, json) in portable {
        cache()
            .lock()
            .unwrap()
            .insert((field, key.clone()), json.clone());
        if json == "null" {
            legacy.remove(&key);
            continue;
        }
        match serde_json::from_str(&json) {
            Ok(value) => {
                legacy.insert(key, value);
            }
            Err(error) => eprintln!("Invalid folder metadata for {key}: {error}"),
        }
    }
    legacy
}

pub fn save<T: Serialize>(
    root: &Path,
    filename: &str,
    field: TrackJsonField,
    entries: &BTreeMap<String, T>,
) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut fallback = BTreeMap::new();
    if root == crate::storage::library_dir() {
        let db = crate::storage::database_path();
        if db.exists() {
            let library = Library::open(&db).map_err(|e| e.to_string())?;
            for key in library.folder_json_keys(field).map_err(|e| e.to_string())? {
                if !entries.contains_key(&key) {
                    library
                        .save_folder_json(&key, field, "null")
                        .map_err(|e| e.to_string())?;
                    cache().lock().unwrap().insert((field, key), "null".into());
                }
            }
            for (key, value) in entries {
                let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
                if cache().lock().unwrap().get(&(field, key.clone())) == Some(&json) {
                    continue;
                }
                if !library
                    .save_folder_json(key, field, &json)
                    .map_err(|e| e.to_string())?
                {
                    fallback.insert(key, value);
                } else {
                    cache().lock().unwrap().insert((field, key.clone()), json);
                }
            }
        } else {
            fallback.extend(entries);
        }
    } else {
        fallback.extend(entries);
    }
    let original = root.join(filename);
    let backup = root.join(format!("{filename}.before-folder-state"));
    if root == crate::storage::library_dir() && original.exists() && !backup.exists() {
        std::fs::copy(&original, backup).map_err(|e| e.to_string())?;
    }
    let temporary = root.join(format!("{filename}.tmp"));
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&fallback).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, original).map_err(|e| e.to_string())
}
