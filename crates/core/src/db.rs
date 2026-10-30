use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::types::{DownloadRecord, DownloadState, Settings};
use crate::util::now_ms;

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS downloads (
    id            TEXT PRIMARY KEY,
    url           TEXT NOT NULL,
    final_url     TEXT,
    filename      TEXT NOT NULL,
    dir           TEXT NOT NULL,
    total_size    INTEGER NOT NULL DEFAULT 0,
    received      INTEGER NOT NULL DEFAULT 0,
    state         TEXT NOT NULL,
    etag          TEXT,
    last_modified TEXT,
    content_type  TEXT,
    accept_ranges INTEGER NOT NULL DEFAULT 0,
    connections   INTEGER NOT NULL DEFAULT 8,
    speed_limit   INTEGER NOT NULL DEFAULT 0,
    category      TEXT,
    priority      INTEGER NOT NULL DEFAULT 0,
    error         TEXT,
    sha256        TEXT,
    headers       TEXT NOT NULL DEFAULT '{}',
    user_agent    TEXT,
    cookie        TEXT,
    proxy         TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    finished_at   INTEGER
);

CREATE INDEX IF NOT EXISTS idx_downloads_state ON downloads(state);
CREATE INDEX IF NOT EXISTS idx_downloads_created ON downloads(created_at);
CREATE INDEX IF NOT EXISTS idx_downloads_url ON downloads(url);

CREATE TABLE IF NOT EXISTS ranges (
    download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    start       INTEGER NOT NULL,
    end         INTEGER NOT NULL,
    pos         INTEGER NOT NULL,
    PRIMARY KEY (download_id, start)
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn insert_download(&self, record: &DownloadRecord) -> Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO downloads (
                id, url, final_url, filename, dir, total_size, received, state,
                etag, last_modified, content_type, accept_ranges, connections,
                speed_limit, category, priority, error, sha256, headers,
                user_agent, cookie, proxy, created_at, updated_at, finished_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                      ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
            "#,
            params![
                record.id.to_string(),
                record.url,
                record.final_url,
                record.filename,
                record.dir,
                record.total_size as i64,
                record.received as i64,
                record.state.as_str(),
                record.etag,
                record.last_modified,
                record.content_type,
                record.accept_ranges as i64,
                record.connections as i64,
                record.speed_limit as i64,
                record.category,
                record.priority as i64,
                record.error,
                record.sha256,
                serde_json::to_string(&record.headers).unwrap_or_else(|_| "{}".into()),
                record.user_agent,
                record.cookie,
                record.proxy,
                record.created_at,
                record.updated_at,
                record.finished_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_download(&self, id: Uuid) -> Result<Option<DownloadRecord>> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let record = conn
            .query_row(
                "SELECT * FROM downloads WHERE id = ?1",
                params![id.to_string()],
                row_to_record,
            )
            .optional()?;
        Ok(record)
    }

    pub fn url_exists(&self, url: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let exists: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM downloads WHERE url = ?1 OR final_url = ?1)",
            params![url],
            |row| row.get(0),
        )?;
        Ok(exists != 0)
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadRecord>> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut statement =
            conn.prepare("SELECT * FROM downloads ORDER BY priority DESC, created_at DESC")?;
        let rows = statement.query_map([], row_to_record)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn list_unfinished(&self) -> Result<Vec<DownloadRecord>> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut statement = conn.prepare(
            r#"SELECT * FROM downloads
               WHERE state IN ('queued', 'probing', 'running', 'paused')
               ORDER BY priority DESC, created_at ASC"#,
        )?;
        let rows = statement.query_map([], row_to_record)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn set_state(&self, id: Uuid, state: DownloadState, error: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "UPDATE downloads SET state = ?2, error = ?3, updated_at = ?4 WHERE id = ?1",
            params![id.to_string(), state.as_str(), error, now_ms()],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_meta(
        &self,
        id: Uuid,
        final_url: Option<&str>,
        total_size: u64,
        accept_ranges: bool,
        etag: Option<&str>,
        last_modified: Option<&str>,
        content_type: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            r#"
            UPDATE downloads SET
                final_url = COALESCE(?2, final_url),
                total_size = ?3,
                accept_ranges = ?4,
                etag = COALESCE(?5, etag),
                last_modified = COALESCE(?6, last_modified),
                content_type = COALESCE(?7, content_type),
                updated_at = ?8
            WHERE id = ?1
            "#,
            params![
                id.to_string(),
                final_url,
                total_size as i64,
                accept_ranges as i64,
                etag,
                last_modified,
                content_type,
                now_ms(),
            ],
        )?;
        Ok(())
    }

    pub fn update_received(&self, id: Uuid, received: u64) -> Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "UPDATE downloads SET received = ?2, updated_at = ?3 WHERE id = ?1",
            params![id.to_string(), received as i64, now_ms()],
        )?;
        Ok(())
    }

    pub async fn update_received_async(&self, id: Uuid, received: u64) -> Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("database mutex poisoned");
            conn.execute(
                "UPDATE downloads SET received = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.to_string(), received as i64, now_ms()],
            )?;
            Ok(())
        })
        .await
        .map_err(|error| crate::error::Error::failed(format!("database task aborted: {error}")))?
    }

    pub fn finish(
        &self,
        id: Uuid,
        state: DownloadState,
        received: u64,
        sha256: Option<&str>,
        filename: &str,
        dir: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE downloads SET
                state = ?2, received = ?3, sha256 = ?4, filename = ?5, dir = ?6,
                error = NULL, updated_at = ?7, finished_at = ?7
            WHERE id = ?1
            "#,
            params![
                id.to_string(),
                state.as_str(),
                received as i64,
                sha256,
                filename,
                dir,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn delete_download(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "DELETE FROM ranges WHERE download_id = ?1",
            params![id.to_string()],
        )?;
        conn.execute(
            "DELETE FROM downloads WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    pub fn save_ranges(&self, id: Uuid, ranges: &[(u64, u64, u64)]) -> Result<()> {
        Self::save_ranges_blocking(&self.conn, id, ranges)
    }

    pub async fn save_ranges_async(&self, id: Uuid, ranges: Vec<(u64, u64, u64)>) -> Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || Self::save_ranges_blocking(&conn, id, &ranges))
            .await
            .map_err(|error| {
                crate::error::Error::failed(format!("database task aborted: {error}"))
            })?
    }

    fn save_ranges_blocking(
        conn: &Mutex<Connection>,
        id: Uuid,
        ranges: &[(u64, u64, u64)],
    ) -> Result<()> {
        let id_text = id.to_string();
        let mut conn = conn.lock().expect("database mutex poisoned");
        let transaction = conn.transaction()?;
        transaction.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS ranges_snapshot (
                 start INTEGER NOT NULL,
                 end   INTEGER NOT NULL,
                 pos   INTEGER NOT NULL
             );
             DELETE FROM ranges_snapshot;",
        )?;
        {
            let mut stage = transaction.prepare_cached(
                "INSERT INTO ranges_snapshot (start, end, pos) VALUES (?1, ?2, ?3)",
            )?;
            for (start, end, pos) in ranges {
                stage.execute(params![*start as i64, *end as i64, *pos as i64])?;
            }
        }
        transaction.execute(
            "DELETE FROM ranges
             WHERE download_id = ?1
               AND NOT EXISTS (
                   SELECT 1 FROM ranges_snapshot s WHERE s.start = ranges.start
               )",
            params![id_text],
        )?;
        transaction.execute(
            r#"
            INSERT INTO ranges (download_id, start, end, pos)
            SELECT ?1, s.start, s.end, s.pos FROM ranges_snapshot s
            WHERE true
            ON CONFLICT(download_id, start) DO UPDATE SET
                end = excluded.end,
                pos = excluded.pos
            "#,
            params![id_text],
        )?;
        transaction.execute_batch("DELETE FROM ranges_snapshot;")?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_ranges(&self, id: Uuid) -> Result<Vec<(u64, u64, u64)>> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut statement = conn
            .prepare("SELECT start, end, pos FROM ranges WHERE download_id = ?1 ORDER BY start")?;
        let rows = statement.query_map(params![id.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, i64>(2)? as u64,
            ))
        })?;
        let mut ranges = Vec::new();
        for row in rows {
            ranges.push(row?);
        }
        Ok(ranges)
    }

    pub fn clear_ranges(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "DELETE FROM ranges WHERE download_id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<Option<Settings>> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'settings'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.and_then(|value| serde_json::from_str(&value).ok()))
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![serde_json::to_string(settings).unwrap_or_else(|_| "{}".into())],
        )?;
        Ok(())
    }
}

fn row_to_record(row: &Row<'_>) -> rusqlite::Result<DownloadRecord> {
    let id: String = row.get("id")?;
    let state: String = row.get("state")?;
    let headers: String = row.get("headers")?;

    Ok(DownloadRecord {
        id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
        url: row.get("url")?,
        final_url: row.get("final_url")?,
        filename: row.get("filename")?,
        dir: row.get("dir")?,
        total_size: row.get::<_, i64>("total_size")?.max(0) as u64,
        received: row.get::<_, i64>("received")?.max(0) as u64,
        state: DownloadState::parse(&state).unwrap_or(DownloadState::Error),
        etag: row.get("etag")?,
        last_modified: row.get("last_modified")?,
        content_type: row.get("content_type")?,
        accept_ranges: row.get::<_, i64>("accept_ranges")? != 0,
        connections: row.get::<_, i64>("connections")?.clamp(1, 32) as u32,
        speed_limit: row.get::<_, i64>("speed_limit")?.max(0) as u64,
        category: row.get("category")?,
        priority: row.get("priority")?,
        error: row.get("error")?,
        sha256: row.get("sha256")?,
        headers: serde_json::from_str(&headers).unwrap_or_else(|_| HashMap::new()),
        user_agent: row.get("user_agent")?,
        cookie: row.get("cookie")?,
        proxy: row.get("proxy")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        finished_at: row.get("finished_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Category;

    fn sample_record() -> DownloadRecord {
        DownloadRecord {
            id: Uuid::new_v4(),
            url: "https://example.com/file.zip".into(),
            final_url: Some("https://cdn.example.com/file.zip".into()),
            filename: "file.zip".into(),
            dir: "C:\\Downloads".into(),
            total_size: 1024,
            received: 0,
            state: DownloadState::Queued,
            etag: Some("\"abc\"".into()),
            last_modified: None,
            content_type: Some("application/zip".into()),
            accept_ranges: true,
            connections: 8,
            speed_limit: 0,
            category: Some(Category::Archives.label().into()),
            priority: 1,
            error: None,
            sha256: None,
            headers: HashMap::new(),
            user_agent: None,
            cookie: None,
            proxy: None,
            created_at: now_ms(),
            updated_at: now_ms(),
            finished_at: None,
        }
    }

    #[test]
    fn roundtrips_downloads_and_ranges() {
        let db = Db::open_in_memory().unwrap();
        let record = sample_record();
        db.insert_download(&record).unwrap();

        let loaded = db.get_download(record.id).unwrap().unwrap();
        assert_eq!(loaded.filename, "file.zip");
        assert_eq!(loaded.total_size, 1024);
        assert_eq!(loaded.state, DownloadState::Queued);
        assert!(db.url_exists("https://example.com/file.zip").unwrap());
        assert!(db.url_exists("https://cdn.example.com/file.zip").unwrap());
        assert!(!db.url_exists("https://example.com/other.zip").unwrap());

        db.save_ranges(record.id, &[(0, 512, 128), (512, 1024, 512)])
            .unwrap();
        let ranges = db.load_ranges(record.id).unwrap();
        assert_eq!(ranges, vec![(0, 512, 128), (512, 1024, 512)]);

        db.set_state(record.id, DownloadState::Running, None)
            .unwrap();
        db.update_received(record.id, 256).unwrap();
        let loaded = db.get_download(record.id).unwrap().unwrap();
        assert_eq!(loaded.state, DownloadState::Running);
        assert_eq!(loaded.received, 256);

        db.delete_download(record.id).unwrap();
        assert!(db.get_download(record.id).unwrap().is_none());
        assert!(db.load_ranges(record.id).unwrap().is_empty());
    }

    #[test]
    fn save_ranges_upserts_and_prunes_stale_rows() {
        let db = Db::open_in_memory().unwrap();
        let record = sample_record();
        db.insert_download(&record).unwrap();

        db.save_ranges(record.id, &[(0, 512, 128), (512, 1024, 512)])
            .unwrap();
        let ranges = db.load_ranges(record.id).unwrap();
        assert_eq!(ranges, vec![(0, 512, 128), (512, 1024, 512)]);

        db.save_ranges(record.id, &[(0, 512, 300), (512, 1024, 600)])
            .unwrap();
        let ranges = db.load_ranges(record.id).unwrap();
        assert_eq!(ranges, vec![(0, 512, 300), (512, 1024, 600)]);

        db.save_ranges(
            record.id,
            &[(0, 341, 100), (341, 682, 341), (682, 1024, 682)],
        )
        .unwrap();
        let ranges = db.load_ranges(record.id).unwrap();
        assert_eq!(
            ranges,
            vec![(0, 341, 100), (341, 682, 341), (682, 1024, 682)]
        );

        db.save_ranges(record.id, &[]).unwrap();
        assert!(db.load_ranges(record.id).unwrap().is_empty());
    }

    #[test]
    fn roundtrips_settings() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.load_settings().unwrap().is_none());

        let settings = Settings {
            max_concurrent: 7,
            categorize: false,
            ..Default::default()
        };
        db.save_settings(&settings).unwrap();

        let loaded = db.load_settings().unwrap().unwrap();
        assert_eq!(loaded.max_concurrent, 7);
        assert!(!loaded.categorize);
    }
}
