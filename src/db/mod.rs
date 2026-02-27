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
    embedding FLOAT[384]
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
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const (),
        )));
    });
}

/// A wrapper around a SQLite connection initialized with sqlite-vec and the application schema.
pub struct Db {
    pub(crate) conn: Connection,
}

impl Db {
    /// Open a database connection at the given path and initialize the schema.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        info!("Initializing database: {}", path.display());

        // Register sqlite-vec extension globally
        init_sqlite_vec();

        let conn = Connection::open(path)?;

        // Verify sqlite-vec is loaded
        let vec_version: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
        info!("sqlite-vec version: {}", vec_version);

        // Configure connection
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        // Initialize schema
        conn.execute_batch(SCHEMA_SQL)?;

        info!("Database initialized successfully");

        Ok(Self { conn })
    }

    /// Open an in-memory database connection (useful for testing).
    pub fn open_in_memory() -> Result<Self> {
        init_sqlite_vec();
        let conn = Connection::open_in_memory()?;
        let vec_version: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
        info!("sqlite-vec version: {}", vec_version);
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self { conn })
    }
}

/// Helper to serialize a float32 vector into bytes for vec0 virtual table
pub fn serialize_vector(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for v in vec {
        bytes.extend_from_slice(&v.to_le_bytes());
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
        let tables: usize = db.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('documents', 'chunks', 'vec_chunks', 'code_metadata', 'code_relations', 'word_mapping');",
            [],
            |row| row.get(0),
        ).unwrap();

        // sqlite_master virtual tables might create multiple internal tables, but the main count should be 6
        assert_eq!(tables, 6);
    }

    #[test]
    fn test_serialize_vector() {
        let vec = vec![1.0, 2.0, -3.5];
        let bytes = serialize_vector(&vec);
        assert_eq!(bytes.len(), 12);

        // 1.0f32 in hex: 0x3f800000 -> little endian: 00 00 80 3f
        assert_eq!(&bytes[0..4], &[0x00, 0x00, 0x80, 0x3f]);
        // 2.0f32 in hex: 0x40000000 -> little endian: 00 00 00 40
        assert_eq!(&bytes[4..8], &[0x00, 0x00, 0x00, 0x40]);
        // -3.5f32 in hex: 0xc0600000 -> little endian: 00 00 60 c0
        assert_eq!(&bytes[8..12], &[0x00, 0x00, 0x60, 0xc0]);
    }
}
