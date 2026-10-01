use crate::domain::{RuntimeError, TranslationResult};
use rusqlite::{params, Connection};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY_RETENTION_LIMIT: i64 = 1_000;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: i64,
    pub source_text: String,
    pub translated_text: String,
    pub source_language: String,
    pub target_language: String,
    pub model_id: String,
    pub created_at: i64,
    pub favorite: bool,
}

pub struct HistoryStore {
    connection: Connection,
}

impl HistoryStore {
    pub fn open(path: &Path) -> Result<Self, RuntimeError> {
        let connection = Connection::open(path)
            .map_err(|error| RuntimeError::Connection(format!("history database: {error}")))?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    #[cfg(test)]
    fn in_memory() -> Self {
        Self {
            connection: Connection::open_in_memory().expect("in-memory database should open"),
        }
    }

    fn initialize(&self) -> Result<(), RuntimeError> {
        self.connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS translation_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_text TEXT NOT NULL,
                    translated_text TEXT NOT NULL,
                    source_language TEXT NOT NULL,
                    target_language TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    favorite INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX IF NOT EXISTS idx_translation_history_created
                    ON translation_history (created_at DESC, id DESC);",
            )
            .map_err(|error| RuntimeError::Connection(format!("history schema: {error}")))?;
        let has_favorite = self
            .connection
            .prepare("SELECT favorite FROM translation_history LIMIT 0")
            .is_ok();
        if !has_favorite {
            self.connection
                .execute(
                    "ALTER TABLE translation_history ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(|error| RuntimeError::Connection(format!("history migration: {error}")))?;
        }
        self.connection
            .execute_batch("PRAGMA user_version = 2;")
            .map_err(|error| RuntimeError::Connection(format!("history version: {error}")))?;
        Ok(())
    }

    pub fn add(
        &self,
        source_text: &str,
        request: &crate::domain::TranslationRequest,
        result: &TranslationResult,
    ) -> Result<(), RuntimeError> {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| RuntimeError::Connection(error.to_string()))?
            .as_secs() as i64;
        self.connection
            .execute(
                "INSERT INTO translation_history
                 (source_text, translated_text, source_language, target_language, model_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    source_text,
                    result.text,
                    request.source_language,
                    request.target_language,
                    request.model_id,
                    created_at
                ],
            )
            .map_err(|error| RuntimeError::Connection(format!("history insert: {error}")))?;
        self.prune_old_entries()?;
        Ok(())
    }

    fn prune_old_entries(&self) -> Result<(), RuntimeError> {
        let total = self
            .connection
            .query_row("SELECT COUNT(*) FROM translation_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| RuntimeError::Connection(format!("history count: {error}")))?;
        let excess = total - HISTORY_RETENTION_LIMIT;
        if excess <= 0 {
            return Ok(());
        }
        self.connection
            .execute(
                "DELETE FROM translation_history
                 WHERE id IN (
                     SELECT id FROM translation_history
                     WHERE favorite = 0
                     ORDER BY id ASC
                     LIMIT ?1
                 )",
                params![excess],
            )
            .map_err(|error| RuntimeError::Connection(format!("history retention: {error}")))?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<HistoryEntry>, RuntimeError> {
        self.list_page(None, HISTORY_RETENTION_LIMIT, 0)
    }

    pub fn list_page(
        &self,
        query: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryEntry>, RuntimeError> {
        let limit = limit.clamp(1, HISTORY_RETENTION_LIMIT);
        let offset = offset.max(0);
        if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
            return self.search_page(query, limit, offset);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, source_text, translated_text, source_language,
                        target_language, model_id, created_at, favorite
                 FROM translation_history ORDER BY id DESC LIMIT ?1 OFFSET ?2",
            )
            .map_err(|error| RuntimeError::Connection(format!("history query: {error}")))?;
        let entries = statement
            .query_map(params![limit, offset], row_to_entry)
            .map_err(|error| RuntimeError::Connection(format!("history rows: {error}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| RuntimeError::Connection(format!("history row decode: {error}")))?;
        Ok(entries)
    }

    fn search_page(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryEntry>, RuntimeError> {
        let pattern = format!("%{}%", query.trim());
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, source_text, translated_text, source_language,
                        target_language, model_id, created_at, favorite
                 FROM translation_history
                 WHERE source_text LIKE ?1 OR translated_text LIKE ?1
                 ORDER BY id DESC LIMIT ?2 OFFSET ?3",
            )
            .map_err(|error| RuntimeError::Connection(format!("history search: {error}")))?;
        let entries = statement
            .query_map(params![pattern, limit, offset], row_to_entry)
            .map_err(|error| RuntimeError::Connection(format!("history search rows: {error}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| RuntimeError::Connection(format!("history search decode: {error}")))?;
        Ok(entries)
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> Result<(), RuntimeError> {
        self.connection
            .execute(
                "UPDATE translation_history SET favorite = ?1 WHERE id = ?2",
                params![favorite as i64, id],
            )
            .map_err(|error| RuntimeError::Connection(format!("history favorite: {error}")))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<(), RuntimeError> {
        self.connection
            .execute("DELETE FROM translation_history", [])
            .map_err(|error| RuntimeError::Connection(format!("history clear: {error}")))?;
        Ok(())
    }

    pub fn export_csv(&self) -> Result<String, RuntimeError> {
        let mut csv = String::from(
            "id,source_text,translated_text,source_language,target_language,model_id,created_at,favorite\n",
        );
        for entry in self.list()? {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                entry.id,
                csv_field(&entry.source_text),
                csv_field(&entry.translated_text),
                csv_field(&entry.source_language),
                csv_field(&entry.target_language),
                csv_field(&entry.model_id),
                entry.created_at,
                entry.favorite
            ));
        }
        Ok(csv)
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        source_text: row.get(1)?,
        translated_text: row.get(2)?,
        source_language: row.get(3)?,
        target_language: row.get(4)?,
        model_id: row.get(5)?,
        created_at: row.get(6)?,
        favorite: row.get::<_, i64>(7)? != 0,
    })
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> crate::domain::TranslationRequest {
        crate::domain::TranslationRequest {
            model_id: "model".into(),
            adapter_id: "translategemma".into(),
            source_language: "en".into(),
            target_language: "de".into(),
            text: "Hello".into(),
            translation_style: crate::domain::TranslationStyle::Neutral,
        }
    }

    #[test]
    fn stores_lists_and_clears_entries() {
        let store = HistoryStore::in_memory();
        store.initialize().unwrap();
        let result = TranslationResult {
            text: "Hallo".into(),
            model_id: "model".into(),
            adapter_id: "translategemma".into(),
            latency_ms: 10,
            completion_tokens: None,
        };
        store.add("Hello", &request(), &result).unwrap();
        assert_eq!(store.list().unwrap()[0].translated_text, "Hallo");
        let id = store.list().unwrap()[0].id;
        assert_eq!(store.list_page(Some("Hallo"), 100, 0).unwrap().len(), 1);
        store.set_favorite(id, true).unwrap();
        assert!(store.list().unwrap()[0].favorite);
        store.clear().unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn exports_quoted_csv_fields() {
        let store = HistoryStore::in_memory();
        store.initialize().unwrap();
        let mut request = request();
        request.text = "Hello, \"world\"".into();
        let result = TranslationResult {
            text: "Hallo".into(),
            model_id: "model".into(),
            adapter_id: "translategemma".into(),
            latency_ms: 10,
            completion_tokens: None,
        };
        store.add(&request.text, &request, &result).unwrap();
        assert!(store
            .export_csv()
            .unwrap()
            .contains("\"Hello, \"\"world\"\"\""));
    }

    #[test]
    fn migrates_history_without_favorite_column() {
        let store = HistoryStore::in_memory();
        store
            .connection
            .execute_batch(
                "CREATE TABLE translation_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_text TEXT NOT NULL,
                    translated_text TEXT NOT NULL,
                    source_language TEXT NOT NULL,
                    target_language TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );",
            )
            .unwrap();
        store.initialize().unwrap();
        let favorite: i64 = store
            .connection
            .query_row(
                "SELECT favorite FROM translation_history LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        assert_eq!(favorite, 0);
        let version: i64 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn retains_favorites_when_pruning_old_entries() {
        let store = HistoryStore::in_memory();
        store.initialize().unwrap();
        let result = TranslationResult {
            text: "Hallo".into(),
            model_id: "model".into(),
            adapter_id: "translategemma".into(),
            latency_ms: 10,
            completion_tokens: None,
        };
        for index in 0..=HISTORY_RETENTION_LIMIT {
            store
                .add(&format!("Hello {index}"), &request(), &result)
                .unwrap();
        }
        let oldest_id = store
            .connection
            .query_row(
                "SELECT id FROM translation_history ORDER BY id ASC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        store.set_favorite(oldest_id, true).unwrap();
        store.add("Final", &request(), &result).unwrap();
        assert!(store
            .list()
            .unwrap()
            .iter()
            .any(|entry| entry.id == oldest_id && entry.favorite));
        assert!(store.list().unwrap().len() <= HISTORY_RETENTION_LIMIT as usize + 1);
    }
}
