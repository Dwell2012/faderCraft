//! Accepted online metadata is local library data. No audio-file writes or network I/O.
use crate::lookup::{TrackDetails, TrackMatch};
use crate::{Error, Library, Result, TrackQuery};
use rusqlite::{OptionalExtension, params};

#[derive(Clone, Debug, PartialEq)]
pub struct SavedLookup {
    pub candidate: TrackMatch,
    pub details: TrackDetails,
}

impl Library {
    /// A permanent portable identity, independent of path and SQLite row-ID reuse.
    pub fn track_token(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT token FROM track_identities WHERE track_id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?)
    }
    pub fn track_id_for_token(&self, token: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT track_id FROM track_identities WHERE token=?1",
                [token],
                |row| row.get(0),
            )
            .optional()?)
    }
    /// Save one accepted match and its provider snapshot for offline inspection/search.
    /// Empty provider fields preserve existing library values. Analysis and file tags
    /// are untouched; rescanning a file preserves these accepted library overrides.
    pub fn save_lookup(
        &mut self,
        id: i64,
        candidate: &TrackMatch,
        details: &TrackDetails,
    ) -> Result<()> {
        let current = self
            .tracks(&TrackQuery {
                id: Some(id),
                ..Default::default()
            })?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Library("Track is no longer in the library.".into()))?;
        let mut candidate = candidate.clone();
        for (value, fallback) in [
            (&mut candidate.title, current.title),
            (&mut candidate.artist, current.artist),
            (&mut candidate.album, current.album),
        ] {
            *value = if value.trim().is_empty() {
                fallback
            } else {
                value.trim().to_string()
            };
        }
        candidate.genre = candidate
            .genre
            .filter(|s| !s.trim().is_empty())
            .or(Some(current.genre));
        let year = candidate
            .year
            .as_deref()
            .and_then(|s| s.get(..4)?.parse::<i32>().ok())
            .filter(|year| (1000..=9999).contains(year))
            .or(current.year);
        let genre = candidate.genre.clone().unwrap_or_default();
        let search_text = details
            .fields
            .iter()
            .map(|(_, value)| value.as_str())
            .chain([
                candidate.title.as_str(),
                candidate.artist.as_str(),
                candidate.album.as_str(),
                genre.as_str(),
            ])
            .collect::<Vec<_>>()
            .join("\n");
        let snapshot =
            serde_json::to_string(&candidate).map_err(|e| Error::Library(e.to_string()))?;
        let detail_json =
            serde_json::to_string(details).map_err(|e| Error::Library(e.to_string()))?;
        let transaction = self.conn.transaction()?;
        transaction.execute("INSERT INTO track_metadata (track_id,title,artist,album,genre,year,candidate,details,search_text)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
            ON CONFLICT(track_id) DO UPDATE SET title=excluded.title,artist=excluded.artist,
                album=excluded.album,genre=excluded.genre,year=excluded.year,candidate=excluded.candidate,
                details=excluded.details,search_text=excluded.search_text,saved_at=unixepoch()",
            params![id,candidate.title,candidate.artist,candidate.album,genre,year,snapshot,detail_json,search_text])?;
        transaction.execute(
            "UPDATE tracks SET title=?2,artist=?3,album=?4,genre=?5,year=?6 WHERE id=?1",
            params![
                id,
                candidate.title,
                candidate.artist,
                candidate.album,
                genre,
                year
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Read the accepted snapshot offline. Opening it never repeats a lookup.
    pub fn saved_lookup(&self, id: i64) -> Result<Option<SavedLookup>> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT candidate,details FROM track_metadata WHERE track_id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(candidate, details)| {
            Ok(SavedLookup {
                candidate: serde_json::from_str(&candidate)
                    .map_err(|e| Error::Library(e.to_string()))?,
                details: serde_json::from_str(&details)
                    .map_err(|e| Error::Library(e.to_string()))?,
            })
        })
        .transpose()
    }
}
