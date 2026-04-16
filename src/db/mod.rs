//! Vector Database module using SQLite and sqlite-vec
use rusqlite::{Connection, Result};
use sqlite_vec::sqlite3_vec_init;
use std::path::Path;
use std::sync::Once;
use tracing::info;

pub mod documents;
pub mod models;
pub mod relations;
pub mod search;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    filename TEXT NOT NULL UNIQUE,
    indexed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    modified_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_filename ON documents(filename);

CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    content TEXT NOT NULL,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_document_id ON chunks(document_id);

CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
    embedding INT8[384]
);

CREATE TABLE IF NOT EXISTS code_metadata (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chunk_id INTEGER NOT NULL UNIQUE,
    symbol_name TEXT,
    symbol_type TEXT NOT NULL,
    language TEXT NOT NULL,
    start_line INTEGER,
    end_line INTEGER,
    parent_symbol TEXT,
    signature TEXT,
    FOREIGN KEY (chunk_id) REFERENCES chunks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_code_symbol ON code_metadata(symbol_name);
CREATE INDEX IF NOT EXISTS idx_code_language ON code_metadata(language);
CREATE INDEX IF NOT EXISTS idx_code_type ON code_metadata(symbol_type);

CREATE TABLE IF NOT EXISTS code_relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_chunk_id INTEGER NOT NULL,
    target_chunk_id INTEGER,
    relation_type TEXT NOT NULL,
    target_name TEXT NOT NULL,
    target_file TEXT,
    confidence REAL DEFAULT 1.0,
    FOREIGN KEY (source_chunk_id) REFERENCES chunks(id) ON DELETE CASCADE,
    FOREIGN KEY (target_chunk_id) REFERENCES chunks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_rel_source ON code_relations(source_chunk_id);
CREATE INDEX IF NOT EXISTS idx_rel_target ON code_relations(target_chunk_id);
CREATE INDEX IF NOT EXISTS idx_rel_type ON code_relations(relation_type);
CREATE INDEX IF NOT EXISTS idx_rel_name ON code_relations(target_name);

CREATE TABLE IF NOT EXISTS system_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS word_mapping (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_word TEXT NOT NULL,
    target_word TEXT NOT NULL,
    source_lang TEXT DEFAULT 'ja',
    confidence REAL DEFAULT 1.0,
    source_document TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(source_word, target_word, source_lang)
);

CREATE INDEX IF NOT EXISTS idx_word_source ON word_mapping(source_word);
CREATE INDEX IF NOT EXISTS idx_word_target ON word_mapping(target_word);
CREATE INDEX IF NOT EXISTS idx_word_lang ON word_mapping(source_lang);
"#;

static INIT_VEC: Once = Once::new();

/// Initialize the sqlite-vec extension. Safe to call multiple times.
fn init_sqlite_vec() {
    INIT_VEC.call_once(|| unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        let func = std::mem::transmute(sqlite3_vec_init as *const ());
        rusqlite::ffi::sqlite3_auto_extension(Some(func));
    });
}

use r2d2::{ManageConnection, Pool};

#[derive(Clone)]
pub struct SqliteManager {
    path: Option<std::path::PathBuf>,
}

impl ManageConnection for SqliteManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    fn connect(&self) -> std::result::Result<Self::Connection, Self::Error> {
        let conn = if let Some(p) = &self.path {
            Connection::open(p)?
        } else {
            Connection::open_in_memory()?
        };
        init_sqlite_vec();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        // Verification
        let vec_version: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
        info!("sqlite-vec version: {}", vec_version);
        Ok(conn)
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> std::result::Result<(), Self::Error> {
        conn.execute_batch("SELECT 1")
    }

    fn has_broken(&self, _: &mut Self::Connection) -> bool {
        false
    }
}

/// A wrapper around a SQLite connection pool initialized with sqlite-vec and the application schema.
#[derive(Clone)]
pub struct Db {
    pub pool: Pool<SqliteManager>,
}

impl Db {
    pub fn get_conn(&self) -> Result<r2d2::PooledConnection<SqliteManager>> {
        self.pool.get().map_err(|e| {
            rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), Some(e.to_string()))
        })
    }

    /// Open a database connection pool at the given path and initialize the schema.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        info!("Initializing database: {}", path.display());

        let manager = SqliteManager {
            path: Some(path.to_path_buf()),
        };
        let pool = r2d2::Pool::builder()
            .max_size(15) // sufficient for concurrent read/write and search
            .build(manager)
            .map_err(|e| {
                rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), Some(e.to_string()))
            })?;

        // Initialize schema using the first connection
        let conn = pool.get().map_err(|e| {
            rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), Some(e.to_string()))
        })?;
        conn.execute_batch(SCHEMA_SQL)?;

        info!("Database initialized successfully");

        Ok(Self { pool })
    }

    pub fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        let conn = self.get_conn()?;
        let res = conn.query_row(
            "SELECT value FROM system_metadata WHERE key = ?",
            [key],
            |row| row.get(0),
        );
        match res {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO system_metadata (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
            [key, value],
        )?;
        Ok(())
    }

    pub fn open_in_memory() -> Result<Self> {
        let manager = SqliteManager { path: None };
        let pool = r2d2::Pool::builder()
            .max_size(1) // Single connection so all queries hit the initialized schema
            .build(manager)
            .map_err(|e| {
                rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), Some(e.to_string()))
            })?;

        let conn = pool.get().map_err(|e| {
            rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), Some(e.to_string()))
        })?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self { pool })
    }
}

/// Helper to serialize a float32 vector into a scalar-quantized int8 vector blob
pub fn serialize_vector_int8(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len());
    for &v in vec {
        // Clamp to [-1.0, 1.0] and scale to [-127, 127]
        let clamped = v.clamp(-1.0, 1.0);
        let q = (clamped * 127.0).round() as i8;
        bytes.push(q as u8); // Store purely as 1-byte
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_init() {
        let db = Db::open_in_memory().expect("Failed to open in-memory DB");

        // Verify tables exist
        let conn = db.get_conn().unwrap();
        let tables: usize = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('documents', 'chunks', 'vec_chunks', 'code_metadata', 'code_relations', 'word_mapping');",
            [],
            |row| row.get(0),
        ).unwrap();

        // sqlite_master virtual tables might create multiple internal tables, but the main count should be 6
        assert_eq!(tables, 6);
    }

    #[test]
    fn test_serialize_vector_int8() {
        // Some boundary float variables
        let vec = vec![1.0, 0.0, -1.0, 0.5, -0.5, 2.0, -2.0];
        let bytes = serialize_vector_int8(&vec);
        assert_eq!(bytes.len(), 7); // Should be exactly 1 byte per dimension

        // 1.0 -> 127
        assert_eq!(bytes[0], 127);
        // 0.0 -> 0
        assert_eq!(bytes[1], 0);
        // -1.0 -> -127 (which is 129 in 2's complement u8 representation, technically 256 - 127 = 129, actually -127 as i8 is 129 in u8)
        assert_eq!(bytes[2], 129);
        // 0.5 -> 64
        assert_eq!(bytes[3], 64);
        // -0.5 -> -64 (which is 192)
        assert_eq!(bytes[4], 192);
        // Out of bounds checking (clamped to 1.0 and -1.0)
        assert_eq!(bytes[5], 127);
        assert_eq!(bytes[6], 129);
    }
}
