use super::domain::{DocumentBlock, DocumentJob, JobState};
use crate::domain::RuntimeError;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::path::Path;

const SCHEMA_VERSION: i64 = 3;

pub struct DocumentJobStore {
    connection: Connection,
}

impl DocumentJobStore {
    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self, RuntimeError> {
        let store = Self {
            connection: Connection::open_in_memory().map_err(|error| {
                RuntimeError::Connection(format!("document job database: {error}"))
            })?,
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn open(path: &Path) -> Result<Self, RuntimeError> {
        let connection = Connection::open(path)
            .map_err(|error| RuntimeError::Connection(format!("document job database: {error}")))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| RuntimeError::Connection(format!("document job timeout: {error}")))?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> Result<(), RuntimeError> {
        self.connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS document_jobs (
                    id TEXT PRIMARY KEY,
                    source_path TEXT NOT NULL,
                    source_hash TEXT NOT NULL,
                    format TEXT NOT NULL,
                    parser_version TEXT NOT NULL,
                    source_language TEXT NOT NULL,
                    target_language TEXT NOT NULL,
                    runtime_snapshot TEXT NOT NULL,
                    configuration_version TEXT NOT NULL,
                    translation_style TEXT NOT NULL DEFAULT 'neutral',
                    state TEXT NOT NULL,
                    error TEXT
                );
                CREATE TABLE IF NOT EXISTS document_blocks (
                    job_id TEXT NOT NULL REFERENCES document_jobs(id) ON DELETE CASCADE,
                    block_id TEXT NOT NULL,
                    ordinal INTEGER NOT NULL,
                    block_type TEXT NOT NULL,
                    source_text TEXT NOT NULL,
                    translated_text TEXT,
                    PRIMARY KEY (job_id, block_id)
                );
                CREATE INDEX IF NOT EXISTS idx_document_blocks_order
                    ON document_blocks (job_id, ordinal);
                CREATE TABLE IF NOT EXISTS document_diagnostics (
                    job_id TEXT NOT NULL REFERENCES document_jobs(id) ON DELETE CASCADE,
                    ordinal INTEGER NOT NULL,
                    message TEXT NOT NULL,
                    PRIMARY KEY (job_id, ordinal)
                );
                PRAGMA foreign_keys = ON;",
            )
            .map_err(|error| RuntimeError::Connection(format!("document job schema: {error}")))?;
        let version: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| RuntimeError::Connection(format!("document job version: {error}")))?;
        if version > SCHEMA_VERSION {
            return Err(RuntimeError::Connection(format!(
                "document job schema {version} is newer than supported schema {SCHEMA_VERSION}"
            )));
        }
        let has_translation_style: bool = self
            .connection
            .prepare("SELECT translation_style FROM document_jobs LIMIT 0")
            .is_ok();
        if !has_translation_style {
            self.connection
                .execute(
                    "ALTER TABLE document_jobs ADD COLUMN translation_style TEXT NOT NULL DEFAULT 'neutral'",
                    [],
                )
                .map_err(|error| {
                    RuntimeError::Connection(format!("document job style migration: {error}"))
                })?;
        }
        self.connection
            .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))
            .map_err(|error| RuntimeError::Connection(format!("document job migration: {error}")))
    }

    pub fn create(&self, job: &DocumentJob) -> Result<(), RuntimeError> {
        self.connection
            .execute(
                "INSERT INTO document_jobs
                 (id, source_path, source_hash, format, parser_version, source_language,
                  target_language, runtime_snapshot, configuration_version, translation_style,
                  state, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    job.id,
                    job.source_path,
                    job.source_hash,
                    job.format,
                    job.parser_version,
                    job.source_language,
                    job.target_language,
                    job.runtime_snapshot,
                    job.configuration_version,
                    serde_json::to_string(&job.translation_style)
                        .unwrap()
                        .trim_matches('"'),
                    state_name(job.state),
                    job.error,
                ],
            )
            .map_err(|error| RuntimeError::Connection(format!("document job insert: {error}")))?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<DocumentJob>, RuntimeError> {
        self.connection
            .query_row(
                "SELECT id, source_path, source_hash, format, parser_version, source_language,
                        target_language, runtime_snapshot, configuration_version, translation_style,
                        state, error
                 FROM document_jobs WHERE id = ?1",
                [id],
                row_to_job,
            )
            .optional()
            .map_err(|error| RuntimeError::Connection(format!("document job read: {error}")))
    }

    /// Newest jobs first, with block counts computed in SQL so a list of large books
    /// does not load every block.
    pub fn summaries(
        &self,
        limit: usize,
    ) -> Result<Vec<(DocumentJob, usize, usize)>, RuntimeError> {
        self.query_summaries(
            "ORDER BY j.rowid DESC LIMIT ?1",
            i64::try_from(limit).unwrap_or(i64::MAX),
        )
    }

    /// One job with block counts and without its blocks; used for progress polling.
    pub fn summary(&self, id: &str) -> Result<Option<(DocumentJob, usize, usize)>, RuntimeError> {
        Ok(self.query_summaries_by_id(id)?.into_iter().next())
    }

    fn query_summaries_by_id(
        &self,
        id: &str,
    ) -> Result<Vec<(DocumentJob, usize, usize)>, RuntimeError> {
        let mut statement = self
            .connection
            .prepare(&format!("{SUMMARY_SELECT} WHERE j.id = ?1"))
            .map_err(summary_error)?;
        let rows = statement
            .query_map([id], summary_row)
            .map_err(summary_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(summary_error)
    }

    fn query_summaries(
        &self,
        tail: &str,
        limit: i64,
    ) -> Result<Vec<(DocumentJob, usize, usize)>, RuntimeError> {
        let mut statement = self
            .connection
            .prepare(&format!("{SUMMARY_SELECT} {tail}"))
            .map_err(summary_error)?;
        let rows = statement
            .query_map([limit], summary_row)
            .map_err(summary_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(summary_error)
    }

    pub fn delete(&self, id: &str) -> Result<(), RuntimeError> {
        self.connection
            .execute("DELETE FROM document_jobs WHERE id = ?1", [id])
            .map_err(|error| RuntimeError::Connection(format!("document job delete: {error}")))?;
        Ok(())
    }

    pub fn blocks(&self, job_id: &str) -> Result<Vec<DocumentBlock>, RuntimeError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT block_id, ordinal, block_type, source_text, translated_text
                 FROM document_blocks WHERE job_id = ?1 ORDER BY ordinal",
            )
            .map_err(|error| RuntimeError::Connection(format!("document blocks query: {error}")))?;
        let rows = statement
            .query_map([job_id], |row| {
                Ok(DocumentBlock {
                    id: row.get(0)?,
                    ordinal: row.get(1)?,
                    block_type: serde_json::from_str(&row.get::<_, String>(2)?).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    source_text: row.get(3)?,
                    translated_text: row.get(4)?,
                })
            })
            .map_err(|error| RuntimeError::Connection(format!("document blocks rows: {error}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| RuntimeError::Connection(format!("document blocks decode: {error}")))
    }

    pub fn transition(
        &self,
        id: &str,
        next: JobState,
        error: Option<&str>,
    ) -> Result<(), RuntimeError> {
        let job = self
            .get(id)?
            .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
        job.state
            .transition_to(next)
            .map_err(|error| RuntimeError::InvalidInput(error.to_string()))?;
        self.connection
            .execute(
                "UPDATE document_jobs SET state = ?1, error = ?2 WHERE id = ?3",
                params![state_name(next), error, id],
            )
            .map_err(|error| {
                RuntimeError::Connection(format!("document job transition: {error}"))
            })?;
        Ok(())
    }

    pub fn resume(
        &self,
        id: &str,
        runtime_snapshot: &str,
        configuration_version: &str,
    ) -> Result<(), RuntimeError> {
        let job = self
            .get(id)?
            .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
        if job.runtime_snapshot != runtime_snapshot
            || job.configuration_version != configuration_version
        {
            return Err(RuntimeError::InvalidInput(
                "document job settings changed; explicit restart is required".into(),
            ));
        }
        self.transition(id, JobState::Translating, None)
    }

    pub fn save_block(&mut self, job_id: &str, block: &DocumentBlock) -> Result<(), RuntimeError> {
        let transaction = self.connection.transaction().map_err(|error| {
            RuntimeError::Connection(format!("document block transaction: {error}"))
        })?;
        save_block_transaction(&transaction, job_id, block)?;
        transaction
            .commit()
            .map_err(|error| RuntimeError::Connection(format!("document block commit: {error}")))
    }

    pub fn replace_block(
        &mut self,
        job_id: &str,
        original_id: &str,
        replacements: &[DocumentBlock],
    ) -> Result<(), RuntimeError> {
        let transaction = self.connection.transaction().map_err(|error| {
            RuntimeError::Connection(format!("document block transaction: {error}"))
        })?;
        transaction
            .execute(
                "DELETE FROM document_blocks WHERE job_id = ?1 AND block_id = ?2",
                params![job_id, original_id],
            )
            .map_err(|error| RuntimeError::Connection(format!("document block split: {error}")))?;
        for block in replacements {
            save_block_transaction(&transaction, job_id, block)?;
        }
        transaction.commit().map_err(|error| {
            RuntimeError::Connection(format!("document block split commit: {error}"))
        })
    }

    pub fn replace_diagnostics(
        &mut self,
        job_id: &str,
        diagnostics: &[String],
    ) -> Result<(), RuntimeError> {
        let transaction = self.connection.transaction().map_err(|error| {
            RuntimeError::Connection(format!("document diagnostics transaction: {error}"))
        })?;
        transaction
            .execute(
                "DELETE FROM document_diagnostics WHERE job_id = ?1",
                [job_id],
            )
            .map_err(|error| {
                RuntimeError::Connection(format!("document diagnostics replace: {error}"))
            })?;
        for (ordinal, message) in diagnostics.iter().enumerate() {
            let ordinal = i64::try_from(ordinal).map_err(|error| {
                RuntimeError::Connection(format!("document diagnostic ordinal: {error}"))
            })?;
            transaction
                .execute(
                    "INSERT INTO document_diagnostics (job_id, ordinal, message)
                     VALUES (?1, ?2, ?3)",
                    params![job_id, ordinal, message],
                )
                .map_err(|error| {
                    RuntimeError::Connection(format!("document diagnostic insert: {error}"))
                })?;
        }
        transaction.commit().map_err(|error| {
            RuntimeError::Connection(format!("document diagnostics commit: {error}"))
        })
    }

    pub fn diagnostics(&self, job_id: &str) -> Result<Vec<String>, RuntimeError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT message FROM document_diagnostics
                 WHERE job_id = ?1 ORDER BY ordinal",
            )
            .map_err(|error| {
                RuntimeError::Connection(format!("document diagnostics query: {error}"))
            })?;
        let rows = statement
            .query_map([job_id], |row| row.get(0))
            .map_err(|error| {
                RuntimeError::Connection(format!("document diagnostics rows: {error}"))
            })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
            RuntimeError::Connection(format!("document diagnostics decode: {error}"))
        })
    }

    pub fn recover_interrupted(&self) -> Result<usize, RuntimeError> {
        let changed = self
            .connection
            .execute(
                "UPDATE document_jobs SET state = 'interrupted', error = 'application stopped before completion'
                 WHERE state IN ('analyzing', 'translating', 'pausing', 'exporting')",
                [],
            )
            .map_err(|error| RuntimeError::Connection(format!("document job recovery: {error}")))?;
        Ok(changed)
    }

    pub fn source_is_current(job: &DocumentJob, source: &[u8]) -> bool {
        content_hash(source) == job.source_hash
    }

    #[cfg(test)]
    pub(crate) fn set_translation_style_for_test(
        &self,
        job_id: &str,
        style: &str,
    ) -> Result<(), RuntimeError> {
        self.connection
            .execute(
                "UPDATE document_jobs SET translation_style = ?1 WHERE id = ?2",
                params![style, job_id],
            )
            .map(|_| ())
            .map_err(|error| RuntimeError::Connection(format!("document job style test: {error}")))
    }
}

fn save_block_transaction(
    transaction: &Transaction<'_>,
    job_id: &str,
    block: &DocumentBlock,
) -> Result<(), RuntimeError> {
    transaction
        .execute(
            "INSERT INTO document_blocks
             (job_id, block_id, ordinal, block_type, source_text, translated_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(job_id, block_id) DO UPDATE SET
                 ordinal = excluded.ordinal, block_type = excluded.block_type,
                 source_text = excluded.source_text, translated_text = excluded.translated_text",
            params![
                job_id,
                block.id,
                block.ordinal,
                serde_json::to_string(&block.block_type).unwrap(),
                block.source_text,
                block.translated_text
            ],
        )
        .map_err(|error| RuntimeError::Connection(format!("document block save: {error}")))?;
    Ok(())
}

const SUMMARY_SELECT: &str = "SELECT j.id, j.source_path, j.source_hash, j.format,
        j.parser_version, j.source_language, j.target_language, j.runtime_snapshot,
        j.configuration_version, j.translation_style, j.state, j.error,
        (SELECT COUNT(*) FROM document_blocks b WHERE b.job_id = j.id),
        (SELECT COUNT(*) FROM document_blocks b
          WHERE b.job_id = j.id AND b.translated_text IS NOT NULL)
        FROM document_jobs j";

fn summary_error(error: rusqlite::Error) -> RuntimeError {
    RuntimeError::Connection(format!("document job summary: {error}"))
}

fn summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(DocumentJob, usize, usize)> {
    let total: i64 = row.get(12)?;
    let translated: i64 = row.get(13)?;
    Ok((
        row_to_job(row)?,
        usize::try_from(total).unwrap_or_default(),
        usize::try_from(translated).unwrap_or_default(),
    ))
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentJob> {
    let style: String = row.get(9)?;
    let state: String = row.get(10)?;
    Ok(DocumentJob {
        id: row.get(0)?,
        source_path: row.get(1)?,
        source_hash: row.get(2)?,
        format: row.get(3)?,
        parser_version: row.get(4)?,
        source_language: row.get(5)?,
        target_language: row.get(6)?,
        runtime_snapshot: row.get(7)?,
        configuration_version: row.get(8)?,
        translation_style: serde_json::from_str(&format!("\"{style}\"")).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                9,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        state: serde_json::from_str(&format!("\"{state}\"")).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        error: row.get(11)?,
    })
}

fn state_name(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Analyzing => "analyzing",
        JobState::Ready => "ready",
        JobState::Translating => "translating",
        JobState::Pausing => "pausing",
        JobState::Paused => "paused",
        JobState::Exporting => "exporting",
        JobState::Completed => "completed",
        JobState::CompletedWithWarnings => "completed_with_warnings",
        JobState::Interrupted => "interrupted",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    }
}

pub fn content_hash(content: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(content);
    format!("{:x}", hash.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::{BlockType, DocumentBlock, DocumentJob, JobState};
    use crate::domain::TranslationStyle;

    fn store() -> DocumentJobStore {
        DocumentJobStore {
            connection: Connection::open_in_memory().unwrap(),
        }
    }
    fn job() -> DocumentJob {
        DocumentJob {
            id: "job-1".into(),
            source_path: "book.txt".into(),
            source_hash: content_hash(b"source"),
            format: "txt".into(),
            parser_version: "1".into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            runtime_snapshot: "model-a".into(),
            configuration_version: "1".into(),
            translation_style: TranslationStyle::Neutral,
            state: JobState::Queued,
            error: None,
        }
    }
    fn block(id: &str, ordinal: i64, text: &str) -> DocumentBlock {
        DocumentBlock {
            id: id.into(),
            ordinal,
            block_type: BlockType::Paragraph,
            source_text: text.into(),
            translated_text: Some("translated".into()),
        }
    }

    fn initialized_store_with_job() -> DocumentJobStore {
        let store = store();
        store.initialize().unwrap();
        store.create(&job()).unwrap();
        store
    }

    #[test]
    fn summaries_list_newest_first_with_counts() {
        let mut store = initialized_store_with_job();
        let mut second = job();
        second.id = "job-2".into();
        store.create(&second).unwrap();
        store.save_block("job-1", &block("b-1", 0, "one")).unwrap();

        let list = store.summaries(10).unwrap();

        assert_eq!(list[0].0.id, "job-2");
        let one = store.summary("job-1").unwrap().unwrap();
        assert_eq!((one.1, one.2), (1, 1));
        assert!(store.summary("missing").unwrap().is_none());
        assert_eq!((list[0].1, list[0].2), (0, 0));
        assert_eq!(
            (list[1].0.id.as_str(), list[1].1, list[1].2),
            ("job-1", 1, 1)
        );
    }

    #[test]
    fn diagnostics_preserve_slice_order() {
        let mut store = initialized_store_with_job();

        store
            .replace_diagnostics("job-1", &["field".into(), "header".into()])
            .unwrap();

        assert_eq!(store.diagnostics("job-1").unwrap(), vec!["field", "header"]);
    }

    #[test]
    fn diagnostics_read_in_ordinal_order_not_insertion_order() {
        let store = initialized_store_with_job();

        // `PRAGMA reverse_unordered_selects` makes a query that lacks ORDER BY return rows in
        // reverse of its natural order. Without it this test is vacuous: the
        // `PRIMARY KEY (job_id, ordinal)` index lets SQLite satisfy the WHERE clause by scanning
        // that index, which already yields ordinal order and silently masks a dropped ORDER BY.
        // With the pragma, dropping `ORDER BY ordinal` returns ["third", "second", "first"] and
        // this test fails, so it genuinely pins the ordering contract.
        store
            .connection
            .execute_batch("PRAGMA reverse_unordered_selects = ON;")
            .unwrap();

        // Insert in deliberately non-ordinal order so the read path has to sort.
        for (ordinal, message) in [(2_i64, "third"), (0_i64, "first"), (1_i64, "second")] {
            store
                .connection
                .execute(
                    "INSERT INTO document_diagnostics (job_id, ordinal, message)
                     VALUES ('job-1', ?1, ?2)",
                    params![ordinal, message],
                )
                .unwrap();
        }

        assert_eq!(
            store.diagnostics("job-1").unwrap(),
            vec!["first", "second", "third"]
        );
    }

    #[test]
    fn replace_diagnostics_rolls_back_when_a_later_insert_fails() {
        let mut store = initialized_store_with_job();
        store
            .replace_diagnostics("job-1", &["original".to_string()])
            .unwrap();

        // Force the second row of the replacement batch to abort, after the DELETE and the first
        // INSERT have already run. Without a transaction each statement would auto-commit, leaving
        // ["first"] behind; the transaction must roll the whole batch back to ["original"].
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_on_boom BEFORE INSERT ON document_diagnostics
                 WHEN NEW.message = 'boom'
                 BEGIN
                     SELECT RAISE(ABORT, 'forced insert failure');
                 END;",
            )
            .unwrap();

        let error = store
            .replace_diagnostics("job-1", &["first".to_string(), "boom".to_string()])
            .unwrap_err();

        assert!(matches!(error, RuntimeError::Connection(_)));
        assert_eq!(store.diagnostics("job-1").unwrap(), vec!["original"]);
    }

    #[test]
    fn diagnostics_replace_without_duplication() {
        let mut store = initialized_store_with_job();
        store
            .replace_diagnostics("job-1", &["header".into(), "field".into()])
            .unwrap();

        store
            .replace_diagnostics("job-1", &["field".into()])
            .unwrap();

        assert_eq!(store.diagnostics("job-1").unwrap(), vec!["field"]);
    }

    #[test]
    fn deleting_a_job_cascades_to_blocks_and_diagnostics() {
        let mut store = initialized_store_with_job();
        store
            .save_block("job-1", &block("b-1", 0, "source"))
            .unwrap();
        store
            .replace_diagnostics("job-1", &["warning".into()])
            .unwrap();

        store.delete("job-1").unwrap();

        assert!(store.get("job-1").unwrap().is_none());
        assert!(store.blocks("job-1").unwrap().is_empty());
        assert!(store.diagnostics("job-1").unwrap().is_empty());
    }

    #[test]
    fn schema_version_two_upgrades_without_losing_jobs_or_blocks() {
        let mut store = store();
        store
            .connection
            .execute_batch(
                "CREATE TABLE document_jobs (
                    id TEXT PRIMARY KEY,
                    source_path TEXT NOT NULL,
                    source_hash TEXT NOT NULL,
                    format TEXT NOT NULL,
                    parser_version TEXT NOT NULL,
                    source_language TEXT NOT NULL,
                    target_language TEXT NOT NULL,
                    runtime_snapshot TEXT NOT NULL,
                    configuration_version TEXT NOT NULL,
                    state TEXT NOT NULL,
                    error TEXT
                );
                CREATE TABLE document_blocks (
                    job_id TEXT NOT NULL REFERENCES document_jobs(id) ON DELETE CASCADE,
                    block_id TEXT NOT NULL,
                    ordinal INTEGER NOT NULL,
                    block_type TEXT NOT NULL,
                    source_text TEXT NOT NULL,
                    translated_text TEXT,
                    PRIMARY KEY (job_id, block_id)
                );
                    CREATE TABLE document_diagnostics (
                        job_id TEXT NOT NULL REFERENCES document_jobs(id) ON DELETE CASCADE,
                        ordinal INTEGER NOT NULL,
                        message TEXT NOT NULL,
                        PRIMARY KEY (job_id, ordinal)
                    );
                    PRAGMA user_version = 2;",
            )
            .unwrap();
        let old_job = job();
        store
            .connection
            .execute(
                "INSERT INTO document_jobs
                 (id, source_path, source_hash, format, parser_version, source_language,
                  target_language, runtime_snapshot, configuration_version, state, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    old_job.id,
                    old_job.source_path,
                    old_job.source_hash,
                    old_job.format,
                    old_job.parser_version,
                    old_job.source_language,
                    old_job.target_language,
                    old_job.runtime_snapshot,
                    old_job.configuration_version,
                    state_name(old_job.state),
                    old_job.error,
                ],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO document_diagnostics (job_id, ordinal, message)
                 VALUES ('job-1', 0, 'legacy warning')",
                [],
            )
            .unwrap();
        store
            .save_block("job-1", &block("b-1", 0, "kept text"))
            .unwrap();

        store.initialize().unwrap();

        let version: i64 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 3);
        assert_eq!(store.get("job-1").unwrap(), Some(job()));
        assert_eq!(
            store.get("job-1").unwrap().unwrap().translation_style,
            TranslationStyle::Neutral
        );
        assert_eq!(store.diagnostics("job-1").unwrap(), vec!["legacy warning"]);

        let blocks = store.blocks("job-1").unwrap();
        assert_eq!(blocks.len(), 1, "migration dropped a stored block");
        assert_eq!(blocks[0].id, "b-1");
        assert_eq!(blocks[0].source_text, "kept text");
        assert_eq!(blocks[0].translated_text.as_deref(), Some("translated"));
    }

    #[test]
    fn rejects_unknown_persisted_translation_style() {
        let store = initialized_store_with_job();
        store
            .set_translation_style_for_test("job-1", "unknown")
            .unwrap();

        assert!(matches!(
            store.get("job-1"),
            Err(RuntimeError::Connection(message)) if message.contains("document job read")
        ));
    }

    #[test]
    fn saves_blocks_idempotently_and_recovers_after_restart() {
        let path = std::env::temp_dir().join(format!(
            "lingvoloc-document-test-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let mut store = DocumentJobStore::open(&path).unwrap();
            store.create(&job()).unwrap();
            store
                .transition("job-1", JobState::Analyzing, None)
                .unwrap();
            store.transition("job-1", JobState::Ready, None).unwrap();
            store
                .transition("job-1", JobState::Translating, None)
                .unwrap();
            store.save_block("job-1", &block("b-1", 0, "one")).unwrap();
            store.save_block("job-1", &block("b-2", 1, "two")).unwrap();
            store.save_block("job-1", &block("b-1", 0, "one")).unwrap();
        }
        let store = DocumentJobStore::open(&path).unwrap();
        assert_eq!(store.blocks("job-1").unwrap().len(), 2);
        assert_eq!(store.recover_interrupted().unwrap(), 1);
        assert_eq!(
            store.get("job-1").unwrap().unwrap().state,
            JobState::Interrupted
        );
        store.resume("job-1", "model-a", "1").unwrap();
        assert_eq!(
            store.get("job-1").unwrap().unwrap().state,
            JobState::Translating
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_invalid_transitions_and_detects_changed_sources() {
        let store = store();
        store.initialize().unwrap();
        store.create(&job()).unwrap();
        assert!(store
            .transition("job-1", JobState::Completed, None)
            .is_err());
        assert!(DocumentJobStore::source_is_current(&job(), b"source"));
        assert!(!DocumentJobStore::source_is_current(&job(), b"changed"));
    }

    #[test]
    fn resume_requires_the_original_runtime_snapshot() {
        let store = store();
        store.initialize().unwrap();
        store.create(&job()).unwrap();
        store
            .transition("job-1", JobState::Analyzing, None)
            .unwrap();
        store.transition("job-1", JobState::Ready, None).unwrap();
        store
            .transition("job-1", JobState::Translating, None)
            .unwrap();
        store.recover_interrupted().unwrap();
        assert!(store.resume("job-1", "model-b", "1").is_err());
        store.resume("job-1", "model-a", "1").unwrap();
        assert_eq!(
            store.get("job-1").unwrap().unwrap().state,
            JobState::Translating
        );
    }

    #[test]
    fn rejects_newer_schema_and_corrupt_state() {
        let corrupt_store = store();
        corrupt_store
            .connection
            .execute_batch("PRAGMA user_version = 99")
            .unwrap();
        assert!(corrupt_store.initialize().is_err());
        let store = store();
        store.connection.execute_batch("CREATE TABLE document_jobs (id TEXT PRIMARY KEY, source_path TEXT, source_hash TEXT, format TEXT, parser_version TEXT, source_language TEXT, target_language TEXT, runtime_snapshot TEXT, configuration_version TEXT, state TEXT, error TEXT); INSERT INTO document_jobs VALUES ('x','','','','','','','','','broken',NULL);").unwrap();
        assert!(store.get("x").is_err());
    }
}
