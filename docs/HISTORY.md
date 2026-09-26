# Translation History

Successful translations are stored locally in SQLite under the Tauri application data directory (`lingvoloc.sqlite`). Existing installations using `lingoloc.sqlite` are opened automatically. The UI displays history in pages of 20, supports text search across source and translation, toggles favorites, provides a clear action, and exports all retained rows to `LingvoLoc-history.csv` in the system Downloads directory. Up to 1,000 non-favorite rows are retained; when the limit is exceeded, the oldest non-favorite rows are removed while favorites are preserved.

History is written only after a successful runtime response. Runtime failures are not persisted. The SQLite schema uses `PRAGMA user_version` and migrates older databases that do not yet have the `favorite` column. Search and export failures are shown in the history feedback area instead of being silently ignored; the UI also has local search/download fallbacks when native commands are unavailable.
