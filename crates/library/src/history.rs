use crate::{Library, Result};
use rusqlite::params;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct PlayHistoryEntry {
    pub path: PathBuf,
    pub title: String,
    pub played_at: i64,
}

impl Library {
    pub fn current_set(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT MAX(id) FROM play_sets", [], |r| r.get(0))?)
    }

    /// Starts a fresh set without deleting earlier history.
    pub fn new_set(&self) -> Result<i64> {
        self.conn
            .execute("INSERT INTO play_sets DEFAULT VALUES", [])?;
        self.current_set()
    }

    /// One entry per file per set; loading or rescanning never changes it.
    pub fn record_play(&self, path: &Path, title: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO play_history(set_id, path, title) VALUES ((SELECT MAX(id) FROM play_sets), ?1, ?2)",
            params![path.to_string_lossy(), title],
        )?;
        Ok(())
    }

    pub fn play_history(&self, set: i64) -> Result<Vec<PlayHistoryEntry>> {
        Ok(self
            .conn
            .prepare(
                "SELECT path, title, played_at FROM play_history WHERE set_id = ?1 ORDER BY id",
            )?
            .query_map([set], |r| {
                Ok(PlayHistoryEntry {
                    path: PathBuf::from(r.get::<_, String>(0)?),
                    title: r.get(1)?,
                    played_at: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sets_preserve_history_and_deduplicate_tracks() {
        let library = Library::open_in_memory().unwrap();
        let first = library.current_set().unwrap();
        let path = Path::new("music/song.mp3");
        library.record_play(path, "Song").unwrap();
        library.record_play(path, "Song").unwrap();
        let history = library.play_history(first).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].path, path);
        assert!(history[0].played_at > 0);
        let second = library.new_set().unwrap();
        assert!(second > first);
        assert!(library.play_history(second).unwrap().is_empty());
        library.record_play(path, "Song").unwrap();
        assert_eq!(library.play_history(second).unwrap().len(), 1);
        assert_eq!(library.play_history(first).unwrap().len(), 1);
    }
    #[test]
    fn history_survives_reopening_the_database() {
        let database = std::env::temp_dir().join(format!(
            "djcraft-history-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let library = Library::open(&database).unwrap();
        let set = library.current_set().unwrap();
        library.record_play(Path::new("song.flac"), "Song").unwrap();
        drop(library);
        let reopened = Library::open(&database).unwrap();
        assert_eq!(reopened.current_set().unwrap(), set);
        assert_eq!(reopened.play_history(set).unwrap()[0].title, "Song");
        drop(reopened);
        std::fs::remove_file(database).unwrap();
    }
}
