# Folder-owned library records

Each folder explicitly added to the library owns one `faderCraft/tracks.sqlite`
database at its root. Subfolders do not get separate databases. Track locators
inside that database are relative to the added folder, and each track has a
stable token independent of its drive letter. The music scanner and folder
watcher skip `faderCraft` directories.

The folder database currently stores hot cues, cue labels/colours, saved loops,
phrase preparation, manual section cues, manual beat-grid anchors, notes,
current-set played marks, lifetime play counts and last-played times. The
database uses `PRAGMA user_version` for schema upgrades. Opening a newer
version fails instead of rewriting it.

The main library database remains the fast working index. It owns track search,
analysis caches, playlists and crates, which may reference music from several
added folders. Playlist names, membership and order are the playlist data;
there is no additional per-playlist metadata file. Machine settings and
controller mappings remain outside music folders.

When an existing folder is first used with this format, faderCraft seeds its
database from the current library. Existing hot-cue, appearance, saved-loop and
preparation JSON entries are copied into folder databases when loaded. The old
JSON files are retained as migration backups. New writes for registered-folder
tracks go to the folder database; loose tracks continue to use the shared JSON
store. Moving a whole added folder to another machine preserves relative
locators and lets a new library recover its track tokens and preparation.

Only one computer should write a shared/synced folder at a time. The database
is not a merge format for simultaneous edits. Saved online metadata overrides,
the full historical set log and a notes editor are still held in or depend on
the main library; those require a later storage/UI migration before this is a
complete replacement for the portable library snapshot.
