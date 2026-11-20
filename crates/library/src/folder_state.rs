//! Portable, track-specific records owned by one explicitly added music folder.
//! Paths in this database are relative to that folder; the central library is
//! still the fast searchable index and owns playlists spanning sources.
use crate::{Error, Library, Result, SourceKind};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

pub const DIRECTORY: &str = "faderCraft";
pub const DATABASE: &str = "tracks.sqlite";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TrackJsonField {
    HotCues,
    CueAppearance,
    SavedLoops,
    Preparation,
}
impl TrackJsonField {
    fn column(self) -> &'static str {
        match self {
            Self::HotCues => "hot_cues",
            Self::CueAppearance => "cue_appearance",
            Self::SavedLoops => "saved_loops",
            Self::Preparation => "preparation",
        }
    }
}

pub struct FolderState {
    conn: Connection,
}
impl FolderState {
    pub fn open(folder: &Path) -> Result<Self> {
        let dir = folder.join(DIRECTORY);
        std::fs::create_dir_all(&dir)?;
        let conn = Connection::open(dir.join(DATABASE))?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 3 {
            return Err(Error::Library(format!(
                "Folder metadata needs a newer faderCraft version ({version})"
            )));
        }
        if version == 0 {
            conn.execute_batch(
                "BEGIN;
            CREATE TABLE track_state (
                locator TEXT PRIMARY KEY,
                token TEXT NOT NULL UNIQUE,
                hot_cues TEXT,
                cue_appearance TEXT,
                saved_loops TEXT,
                preparation TEXT,
                manual_cues TEXT,
                notes TEXT NOT NULL DEFAULT '',
                beat_grid_first REAL,
                beat_grid_downbeat INTEGER,
                play_count INTEGER NOT NULL DEFAULT 0,
                played_in_set INTEGER NOT NULL DEFAULT 0,
                last_played INTEGER
            );
            PRAGMA user_version = 3;
            COMMIT;",
            )?;
        }
        if version == 1 {
            conn.execute_batch(
                "BEGIN; ALTER TABLE track_state ADD COLUMN manual_cues TEXT;
                PRAGMA user_version = 2; COMMIT;",
            )?;
        }
        if version == 1 || version == 2 {
            conn.execute_batch("BEGIN; ALTER TABLE track_state ADD COLUMN played_in_set INTEGER NOT NULL DEFAULT 0;
                PRAGMA user_version = 3; COMMIT;")?;
        }
        Ok(Self { conn })
    }

    fn upsert_identity(&self, locator: &str, token: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO track_state(locator,token) VALUES (?1,?2)
             ON CONFLICT(locator) DO UPDATE SET token=excluded.token",
            params![locator, token],
        )?;
        Ok(())
    }

    fn set_json(
        &self,
        locator: &str,
        token: &str,
        field: TrackJsonField,
        json: &str,
    ) -> Result<()> {
        self.upsert_identity(locator, token)?;
        self.conn.execute(
            &format!(
                "UPDATE track_state SET {}=?2 WHERE locator=?1",
                field.column()
            ),
            params![locator, json],
        )?;
        Ok(())
    }

    fn json_entries(&self, field: TrackJsonField) -> Result<Vec<(String, String)>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT token, {} FROM track_state WHERE {} IS NOT NULL",
            field.column(),
            field.column()
        ))?;
        let rows = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

impl Library {
    pub fn seed_unmigrated_folder_states(&self) -> Result<usize> {
        let mut created = 0;
        for source in self.sources()? {
            if source.kind != SourceKind::Folder {
                continue;
            }
            let root = Path::new(&source.location);
            if !root.is_dir() {
                continue;
            }
            if root.join(DIRECTORY).join(DATABASE).exists() {
                FolderState::open(root)?;
                continue;
            }
            self.sync_folder_state(source.id)?;
            created += 1;
        }
        Ok(created)
    }

    /// Resolve a stable track key to the source that owns it. Loose imported
    /// files have no folder-local database and remain in the legacy store.
    fn folder_location_for_key(&self, key: &str) -> Result<Option<(PathBuf, String, String)>> {
        let token = key.strip_prefix("library-track:");
        let row = if let Some(token) = token {
            self.conn
                .query_row(
                    "SELECT s.location,t.locator,i.token FROM track_identities i
                 JOIN tracks t ON t.id=i.track_id JOIN sources s ON s.id=t.source_id
                 WHERE i.token=?1 AND s.kind='folder'",
                    [token],
                    |r| Ok((PathBuf::from(r.get::<_, String>(0)?), r.get(1)?, r.get(2)?)),
                )
                .optional()?
        } else {
            let path = Path::new(key);
            let Some(id) = self.track_at(path)? else {
                return Ok(None);
            };
            self.conn.query_row(
                "SELECT s.location,t.locator,i.token FROM tracks t JOIN sources s ON s.id=t.source_id
                 JOIN track_identities i ON i.track_id=t.id WHERE t.id=?1 AND s.kind='folder'",
                [id],
                |r| Ok((PathBuf::from(r.get::<_, String>(0)?), r.get(1)?, r.get(2)?)),
            ).optional()?
        };
        Ok(row)
    }

    /// Returns true if the record was written to its owning folder.
    pub fn save_folder_json(&self, key: &str, field: TrackJsonField, json: &str) -> Result<bool> {
        let Some((root, locator, token)) = self.folder_location_for_key(key)? else {
            return Ok(false);
        };
        FolderState::open(&root)?.set_json(&locator, &token, field, json)?;
        Ok(true)
    }

    pub fn folder_json_entries(&self, field: TrackJsonField) -> Result<Vec<(String, String)>> {
        let mut output = Vec::new();
        for source in self.sources()? {
            if source.kind != SourceKind::Folder {
                continue;
            }
            let root = Path::new(&source.location);
            if !root.join(DIRECTORY).join(DATABASE).exists() {
                continue;
            }
            output.extend(
                FolderState::open(root)?
                    .json_entries(field)?
                    .into_iter()
                    .map(|(token, value)| (format!("library-track:{token}"), value)),
            );
        }
        Ok(output)
    }

    pub fn folder_json_keys(&self, field: TrackJsonField) -> Result<Vec<String>> {
        Ok(self
            .folder_json_entries(field)?
            .into_iter()
            .map(|(key, _)| key)
            .collect())
    }

    pub fn save_folder_beat_grid(&self, id: i64, first: Option<(f64, u8)>) -> Result<()> {
        let Some(token) = self.track_token(id)? else {
            return Ok(());
        };
        let key = format!("library-track:{token}");
        let Some((root, locator, token)) = self.folder_location_for_key(&key)? else {
            return Ok(());
        };
        let state = FolderState::open(&root)?;
        state.upsert_identity(&locator, &token)?;
        state.conn.execute(
            "UPDATE track_state SET beat_grid_first=?2,beat_grid_downbeat=?3 WHERE locator=?1",
            params![locator, first.map(|v| v.0), first.map(|v| i64::from(v.1))],
        )?;
        Ok(())
    }

    pub fn save_folder_manual_cues(&self, id: i64) -> Result<()> {
        let Some(token) = self.track_token(id)? else {
            return Ok(());
        };
        let Some((root, locator, token)) =
            self.folder_location_for_key(&format!("library-track:{token}"))?
        else {
            return Ok(());
        };
        let json = self.manual_cues_json(id)?;
        let state = FolderState::open(&root)?;
        state.upsert_identity(&locator, &token)?;
        state.conn.execute(
            "UPDATE track_state SET manual_cues=?2 WHERE locator=?1",
            params![locator, json],
        )?;
        Ok(())
    }

    fn manual_cues_json(&self, id: i64) -> Result<String> {
        let cues: Vec<_> = self
            .cues(id)?
            .into_iter()
            .map(|cue| (cue.seconds, cue.energy))
            .collect();
        serde_json::to_string(&cues).map_err(|e| Error::Library(e.to_string()))
    }

    pub fn record_folder_play(&self, path: &Path) -> Result<()> {
        let Some((root, locator, token)) = self.folder_location_for_key(&path.to_string_lossy())?
        else {
            return Ok(());
        };
        let state = FolderState::open(&root)?;
        state.upsert_identity(&locator, &token)?;
        state.conn.execute(
            "UPDATE track_state SET play_count=play_count+CASE WHEN played_in_set=0 THEN 1 ELSE 0 END,
                played_in_set=1,last_played=unixepoch() WHERE locator=?1",
            [locator],
        )?;
        Ok(())
    }

    pub fn set_folder_note(&self, id: i64, note: &str) -> Result<()> {
        let Some(token) = self.track_token(id)? else {
            return Ok(());
        };
        let Some((root, locator, token)) =
            self.folder_location_for_key(&format!("library-track:{token}"))?
        else {
            return Err(Error::Library("Track is not in an added folder".into()));
        };
        let state = FolderState::open(&root)?;
        state.upsert_identity(&locator, &token)?;
        state.conn.execute(
            "UPDATE track_state SET notes=?2 WHERE locator=?1",
            params![locator, note],
        )?;
        Ok(())
    }

    pub fn folder_note(&self, id: i64) -> Result<Option<String>> {
        let Some(token) = self.track_token(id)? else {
            return Ok(None);
        };
        let Some((root, locator, _)) =
            self.folder_location_for_key(&format!("library-track:{token}"))?
        else {
            return Ok(None);
        };
        let state = FolderState::open(&root)?;
        Ok(state
            .conn
            .query_row(
                "SELECT notes FROM track_state WHERE locator=?1",
                [locator],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn folder_play_stats(&self, id: i64) -> Result<Option<(i64, Option<i64>)>> {
        let Some(token) = self.track_token(id)? else {
            return Ok(None);
        };
        let Some((root, locator, _)) =
            self.folder_location_for_key(&format!("library-track:{token}"))?
        else {
            return Ok(None);
        };
        let state = FolderState::open(&root)?;
        Ok(state
            .conn
            .query_row(
                "SELECT play_count,last_played FROM track_state WHERE locator=?1",
                [locator],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn folder_played_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for source in self.sources()? {
            if source.kind != SourceKind::Folder {
                continue;
            }
            let root = Path::new(&source.location);
            if !root.join(DIRECTORY).join(DATABASE).is_file() {
                continue;
            }
            let state = FolderState::open(root)?;
            let mut query = state
                .conn
                .prepare("SELECT locator FROM track_state WHERE played_in_set=1")?;
            let rows = query.query_map([], |r| r.get::<_, String>(0))?;
            for row in rows {
                paths.push(root.join(row?));
            }
        }
        Ok(paths)
    }

    pub fn clear_folder_played_flags(&self) -> Result<()> {
        for source in self.sources()? {
            if source.kind != SourceKind::Folder {
                continue;
            }
            let root = Path::new(&source.location);
            if !root.join(DIRECTORY).join(DATABASE).is_file() {
                continue;
            }
            FolderState::open(root)?.conn.execute(
                "UPDATE track_state SET played_in_set=0 WHERE played_in_set=1",
                [],
            )?;
        }
        Ok(())
    }

    /// On scan, seed newly added folders from existing library records, or
    /// restore portable identity and manual grid edits from an existing folder.
    pub(crate) fn sync_folder_state(&self, source_id: i64) -> Result<()> {
        let Some(source) = self.source(source_id)? else {
            return Ok(());
        };
        if source.kind != SourceKind::Folder {
            return Ok(());
        }
        let state = FolderState::open(Path::new(&source.location))?;
        state.conn.execute_batch("BEGIN IMMEDIATE")?;
        let mut query = self.conn.prepare(
            "SELECT t.id,t.locator,i.token,t.tempo_first_beat_manual,t.tempo_downbeat_manual,t.cues_manual
             FROM tracks t JOIN track_identities i ON i.track_id=t.id WHERE t.source_id=?1",
        )?;
        let tracks = query.query_map([source_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<f64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, bool>(5)?,
            ))
        })?;
        for row in tracks {
            let (id, locator, token, first, downbeat, cues_manual) = row?;
            let saved: Option<(String, Option<f64>, Option<i64>, Option<String>)> = state.conn.query_row(
                "SELECT token,beat_grid_first,beat_grid_downbeat,manual_cues FROM track_state WHERE locator=?1",
                [&locator], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
            ).optional()?;
            if let Some((saved_token, saved_first, saved_downbeat, saved_cues)) = saved {
                if saved_token != token {
                    self.conn.execute(
                        "UPDATE track_identities SET token=?2 WHERE track_id=?1",
                        params![id, saved_token],
                    )?;
                }
                if let Some(saved_first) = saved_first {
                    self.conn.execute(
                        "UPDATE tracks SET tempo_first_beat_manual=?2,
                        tempo_downbeat_manual=?3 WHERE id=?1",
                        params![id, saved_first, saved_downbeat],
                    )?;
                } else if let Some(first) = first {
                    state.conn.execute(
                        "UPDATE track_state SET beat_grid_first=?2,
                        beat_grid_downbeat=?3 WHERE locator=?1",
                        params![locator, first, downbeat],
                    )?;
                }
                if let Some(json) = saved_cues {
                    let cues: Vec<(f64, u8)> =
                        serde_json::from_str(&json).map_err(|e| Error::Library(e.to_string()))?;
                    self.conn
                        .execute("DELETE FROM cues WHERE track_id=?1", [id])?;
                    for (position, (seconds, energy)) in cues.into_iter().enumerate() {
                        self.conn.execute("INSERT INTO cues(track_id,position,seconds,energy) VALUES (?1,?2,?3,?4)",
                            params![id,position as i64,seconds,energy])?;
                    }
                    self.conn
                        .execute("UPDATE tracks SET cues_manual=1 WHERE id=?1", [id])?;
                } else if cues_manual {
                    state.conn.execute(
                        "UPDATE track_state SET manual_cues=?2 WHERE locator=?1",
                        params![locator, self.manual_cues_json(id)?],
                    )?;
                }
            } else {
                // A scan may have followed a renamed file. Keep its portable
                // record (and all edits) with the stable token.
                let moved = state.conn.execute(
                    "UPDATE track_state SET locator=?1 WHERE token=?2",
                    params![locator, token],
                )?;
                if moved == 0 {
                    state.upsert_identity(&locator, &token)?;
                }
                if let Some(first) = first {
                    state.conn.execute(
                        "UPDATE track_state SET beat_grid_first=?2,
                        beat_grid_downbeat=?3 WHERE locator=?1",
                        params![locator, first, downbeat],
                    )?;
                }
                if cues_manual {
                    state.conn.execute(
                        "UPDATE track_state SET manual_cues=?2 WHERE locator=?1",
                        params![locator, self.manual_cues_json(id)?],
                    )?;
                }
            }
        }
        state.conn.execute_batch("COMMIT")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn added_folder_carries_track_edits_to_another_library() {
        let base = std::env::temp_dir().join(format!(
            "fadercraft-folder-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let music = base.join("Music");
        std::fs::create_dir_all(music.join("Artist")).unwrap();
        let track = music.join("Artist").join("song.mp3");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/silence.mp3"),
            &track,
        )
        .unwrap();
        let mut first = Library::open(&base.join("first.db")).unwrap();
        let source = first.add_folder(&music).unwrap();
        std::fs::copy(
            &track,
            music.join(DIRECTORY).join("not-a-library-track.mp3"),
        )
        .unwrap();
        first.scan(source.id, |_| {}).unwrap();
        assert!(
            first
                .track_at(&music.join(DIRECTORY).join("not-a-library-track.mp3"))
                .unwrap()
                .is_none()
        );
        let id = first.track_at(&track).unwrap().unwrap();
        let token = first.track_token(id).unwrap().unwrap();
        first.set_beat_grid_anchor(id, 0.1, 0).unwrap();
        first
            .set_cues(
                id,
                &[crate::CuePoint {
                    seconds: 0.2,
                    energy: 7,
                }],
            )
            .unwrap();
        first
            .save_folder_json(
                &format!("library-track:{token}"),
                TrackJsonField::HotCues,
                r#"[{"seconds":0.1,"loop_end":null}]"#,
            )
            .unwrap();
        first.record_play(&track, "song").unwrap();
        first.record_play(&track, "song").unwrap();
        first
            .set_folder_note(id, "Mix after the breakdown")
            .unwrap();
        assert!(music.join(DIRECTORY).join(DATABASE).is_file());
        assert!(!music.join("Artist").join(DIRECTORY).exists());
        drop(first);

        let mut second = Library::open(&base.join("second.db")).unwrap();
        let source = second.add_folder(&music).unwrap();
        second.scan(source.id, |_| {}).unwrap();
        let new_id = second.track_at(&track).unwrap().unwrap();
        assert_eq!(second.track_token(new_id).unwrap(), Some(token.clone()));
        assert!(second.beat_grid_anchor_is_manual(new_id).unwrap());
        assert_eq!(
            second.cues(new_id).unwrap(),
            vec![crate::CuePoint {
                seconds: 0.2,
                energy: 7
            }]
        );
        assert_eq!(
            second.folder_note(new_id).unwrap().as_deref(),
            Some("Mix after the breakdown")
        );
        assert_eq!(second.folder_play_stats(new_id).unwrap().unwrap().0, 1);
        assert_eq!(
            second.folder_played_paths().unwrap()[0]
                .canonicalize()
                .unwrap(),
            track.canonicalize().unwrap()
        );
        second.new_set().unwrap();
        assert!(second.folder_played_paths().unwrap().is_empty());
        assert_eq!(second.folder_play_stats(new_id).unwrap().unwrap().0, 1);
        assert_eq!(
            second.folder_json_entries(TrackJsonField::HotCues).unwrap(),
            vec![(
                format!("library-track:{token}"),
                r#"[{"seconds":0.1,"loop_end":null}]"#.into()
            )]
        );
        let state = FolderState::open(&music).unwrap();
        let plays: i64 = state
            .conn
            .query_row("SELECT play_count FROM track_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(plays, 1);
        drop(state);
        drop(second);
        std::fs::remove_dir_all(base).unwrap();
    }
}
