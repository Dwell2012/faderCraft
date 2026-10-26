//! A portable asset folder with a machine-local working database.
//! Cloud storage exchanges consistent closed snapshots, never a live WAL database.
use djcraft_library::Library;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Clone, Serialize, Deserialize)]
struct Location {
    folder: PathBuf,
    seed_folder: PathBuf,
    seed_database: PathBuf,
}

struct Session {
    root: PathBuf,
    database: PathBuf,
    workspace: Option<PathBuf>,
    baseline: Option<u64>,
    _lock: Option<std::fs::File>,
}
static SESSION: OnceLock<Session> = OnceLock::new();

/// Per-computer configuration; development overrides remain isolated.
pub fn machine_dir() -> PathBuf {
    std::env::var_os("DJCRAFT_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|p| p.join("djcraft")))
        .unwrap_or_else(std::env::temp_dir)
}

pub fn library_dir() -> PathBuf {
    SESSION
        .get()
        .map(|s| s.root.clone())
        .unwrap_or_else(machine_dir)
}
pub fn database_path() -> PathBuf {
    SESSION
        .get()
        .map(|s| s.database.clone())
        .unwrap_or_else(|| machine_dir().join("library.db"))
}

pub fn configured_folder() -> PathBuf {
    std::fs::read(machine_dir().join("library-location.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Location>(&b).ok())
        .map(|l| l.folder)
        .unwrap_or_else(library_dir)
}

/// Choosing a location takes effect after restart, so open connections never switch.
pub fn choose_folder(folder: &Path) -> io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let folder = folder.canonicalize()?;
    let location = Location {
        folder,
        seed_folder: library_dir(),
        seed_database: database_path(),
    };
    let local = machine_dir();
    std::fs::create_dir_all(&local)?;
    let temporary = local.join("library-location.json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&location).map_err(io::Error::other)?,
    )?;
    std::fs::rename(temporary, local.join("library-location.json"))
}

fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}
fn signature(path: &Path) -> io::Result<Option<u64>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(hash(&bytes))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn snapshot(database: &Path, destination: &Path) -> io::Result<()> {
    Library::open(database)
        .and_then(|l| l.snapshot(destination))
        .map_err(io::Error::other)
}
fn copy_missing(source: &Path, destination: &Path) -> io::Result<()> {
    if !source.exists() {
        return Ok(());
    }
    if source.is_dir() {
        let source_real = source.canonicalize()?;
        let destination_real = if destination.exists() {
            destination.canonicalize()?
        } else {
            destination
                .parent()
                .ok_or_else(|| io::Error::other("Library destination has no parent"))?
                .canonicalize()?
                .join(
                    destination
                        .file_name()
                        .ok_or_else(|| io::Error::other("Library destination has no name"))?,
                )
        };
        if destination_real == source_real {
            return Ok(());
        }
        if destination_real.starts_with(&source_real) {
            return Err(io::Error::other(
                "Choose a library folder outside the existing asset folders.",
            ));
        }
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            // Never follow linked folders into an unrelated tree.
            if entry.file_type()?.is_symlink() {
                continue;
            }
            copy_missing(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if !destination.exists() {
        std::fs::copy(source, destination)?;
    }
    Ok(())
}

fn open_portable(local: &Path, location: &Location) -> io::Result<Session> {
    let root = &location.folder;
    std::fs::create_dir_all(root)?;
    let published = root.join("library.snapshot.db");
    // An existing portable library always wins; opening one must not merge another library.
    if !published.exists() {
        let source = if root.join("library.db").exists() {
            root.join("library.db")
        } else {
            location.seed_database.clone()
        };
        let temporary = root.join(format!("library.seed-{}.db", std::process::id()));
        if temporary.exists() {
            return Err(io::Error::other(
                "An unfinished library seed exists; preserve it before retrying.",
            ));
        }
        for item in [
            "hot-cues.json",
            "cue-appearance.json",
            "saved-loops.json",
            "track-preparation.json",
            "automix-queue.json",
            "stems",
            "waveforms",
            "artwork",
            "Sampler",
            "Sampler Recordings",
        ] {
            copy_missing(&location.seed_folder.join(item), &root.join(item))?;
        }
        snapshot(&source, &temporary)?;
        std::fs::rename(temporary, &published)?;
    }
    let workspace = local
        .join("library-workspaces")
        .join(format!("{:016x}", hash(root.to_string_lossy().as_bytes())));
    std::fs::create_dir_all(&workspace)?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(workspace.join("session.lock"))?;
    lock.try_lock().map_err(|e| {
        io::Error::other(format!(
            "This library is already open on this computer: {e}"
        ))
    })?;
    let database = workspace.join("library.db");
    let baseline = signature(&published)?;
    let marker = workspace.join("session.json");
    if marker.exists() && database.exists() {
        let previous: Option<u64> =
            serde_json::from_slice(&std::fs::read(&marker)?).map_err(io::Error::other)?;
        if previous != baseline {
            return Err(io::Error::other(format!(
                "Unpublished local library changes conflict with a newer shared snapshot. Local recovery database: {}",
                database.display()
            )));
        }
        // Recover local work after an interrupted session, rather than overwrite it.
    } else {
        // Only this session owns the local workspace; discard no unpublished work.
        for suffix in ["-wal", "-shm"] {
            let sidecar = workspace.join(format!("library.db{suffix}"));
            if sidecar.exists() {
                std::fs::remove_file(sidecar)?;
            }
        }
        std::fs::copy(&published, &database)?;
    }
    std::fs::write(
        marker,
        serde_json::to_vec(&baseline).map_err(io::Error::other)?,
    )?;
    Ok(Session {
        root: root.clone(),
        database,
        workspace: Some(workspace),
        baseline,
        _lock: Some(lock),
    })
}

pub fn initialize() -> io::Result<()> {
    let local = machine_dir();
    let session = match std::fs::read(local.join("library-location.json")) {
        Ok(bytes) => open_portable(
            &local,
            &serde_json::from_slice(&bytes).map_err(io::Error::other)?,
        )?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Session {
            root: local.clone(),
            database: local.join("library.db"),
            workspace: None,
            baseline: None,
            _lock: None,
        },
        Err(e) => return Err(e),
    };
    SESSION
        .set(session)
        .map_err(|_| io::Error::other("Storage already initialized"))
}

fn publish(session: &Session) -> io::Result<()> {
    let Some(workspace) = &session.workspace else {
        return Ok(());
    };
    let published = session.root.join("library.snapshot.db");
    if signature(&published)? != session.baseline {
        return Err(io::Error::other(format!(
            "Shared library changed elsewhere. Local changes remain at {}",
            session.database.display()
        )));
    }
    let temporary = session
        .root
        .join(format!("library.publish-{}.db", std::process::id()));
    snapshot(&session.database, &temporary)?;
    if signature(&published)? != session.baseline {
        return Err(io::Error::other(format!(
            "Shared library changed during publication. Recovery snapshot: {}",
            temporary.display()
        )));
    }
    std::fs::rename(temporary, published)?;
    std::fs::remove_file(workspace.join("session.json"))?;
    Ok(())
}
pub fn finish() -> io::Result<()> {
    SESSION.get().map(publish).unwrap_or(Ok(()))
}

/// Storage failures must be visible even when launched without a terminal.
pub fn show_problem(message: String) {
    use gpui_kit::*;
    struct Problem(String);
    impl Render for Problem {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().flex().flex_col().gap_4().p_6().child("faderCraft could not open the shared library")
                .child(self.0.clone())
                .child("Existing data has been retained. Let cloud sync finish before retrying. Resolve conflicting local/shared copies before continuing.")
        }
    }
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let bounds = Bounds::centered(None, size(px(720.), px(360.)), cx);
        let _ = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("faderCraft — library storage".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            move |_, cx| cx.new(|_| Problem(message)),
        );
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_copy_rejects_a_destination_inside_itself() {
        let root = std::env::temp_dir().join(format!("fadercraft-copy-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(copy_missing(&root, &root.join("nested")).is_err());
        assert!(!root.join("nested").exists());
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn portable_handoff_uses_consistent_snapshots_and_detects_conflicts() {
        let root = std::env::temp_dir().join(format!("fadercraft-storage-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(root.join("source")).unwrap();
        let source = root.join("source/library.db");
        let source_library = Library::open(&source).unwrap();
        source_library.create_playlist("First machine").unwrap();
        std::fs::write(root.join("source/hot-cues.json"), b"{}").unwrap();
        std::fs::write(root.join("source/settings.json"), b"private credentials").unwrap();
        let location = Location {
            folder: root.join("cloud"),
            seed_folder: root.join("source"),
            seed_database: source,
        };
        let a = open_portable(&root.join("machine-a"), &location).unwrap();
        Library::open(&a.database)
            .unwrap()
            .create_playlist("Saved in A")
            .unwrap();
        publish(&a).unwrap();
        drop(a);
        assert!(!location.folder.join("settings.json").exists());
        assert!(location.folder.join("hot-cues.json").exists());
        let b = open_portable(&root.join("machine-b"), &location).unwrap();
        assert_eq!(
            Library::open(&b.database)
                .unwrap()
                .playlists()
                .unwrap()
                .len(),
            2
        );
        let a = open_portable(&root.join("machine-a"), &location).unwrap();
        Library::open(&b.database)
            .unwrap()
            .create_playlist("B changed")
            .unwrap();
        publish(&b).unwrap();
        drop(b);
        assert!(
            publish(&a).is_err(),
            "must not overwrite another computer's work"
        );
        assert!(a.database.exists(), "local work retained for recovery");
        drop(a);
        assert!(open_portable(&root.join("machine-a"), &location).is_err());
        drop(source_library);
        std::fs::remove_dir_all(root).unwrap();
    }
}
