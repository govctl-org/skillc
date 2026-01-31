//! Index utilities shared by search and gateway per [[RFC-0004:C-INDEX]].

use crate::error::{Result, SkillcError};
use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Current schema version for the search index per [[RFC-0004:C-INDEX]].
/// v2: Added headings table for index-based section lookup.
pub const SCHEMA_VERSION: i32 = 2;

/// A heading entry from the index per [[RFC-0004:C-INDEX]].
#[derive(Debug, Clone, Serialize)]
pub struct HeadingEntry {
    pub file: String,
    pub text: String,
    pub level: usize,
    pub start_line: usize,
    pub end_line: usize,
}

/// Compute hash16 for index filename per [[RFC-0004:C-INDEX]].
pub fn compute_hash16(source_path: &Path) -> String {
    let canonical = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    hash[..16].to_string()
}

/// Get the index database path for a skill per [[RFC-0004:C-INDEX]].
pub fn get_index_path(runtime_dir: &Path, source_dir: &Path) -> PathBuf {
    let hash16 = compute_hash16(source_dir);
    runtime_dir
        .join(".skillc-meta")
        .join(format!("search-{}.db", hash16))
}

/// Open the index database, returning an error if missing or corrupt.
///
/// The `skill_name` is used for error messages.
pub fn open_index(runtime_dir: &Path, source_dir: &Path, skill_name: &str) -> Result<Connection> {
    let index_path = get_index_path(runtime_dir, source_dir);

    if !index_path.exists() {
        return Err(SkillcError::IndexUnusable(skill_name.to_string()));
    }

    Connection::open(&index_path).map_err(|_| SkillcError::IndexUnusable(skill_name.to_string()))
}

/// Query headings from the index per [[RFC-0002:C-SHOW]].
///
/// Returns all headings matching the query (case-insensitive exact match).
/// If `file_filter` is provided, only headings from that file are returned.
pub fn query_headings(
    conn: &Connection,
    query: &str,
    file_filter: Option<&str>,
) -> Result<Vec<HeadingEntry>> {
    let query_lower = query.trim().to_lowercase();

    let sql = if file_filter.is_some() {
        "SELECT file, text, level, start_line, end_line FROM headings
         WHERE LOWER(text) = ?1 AND file = ?2"
    } else {
        "SELECT file, text, level, start_line, end_line FROM headings
         WHERE LOWER(text) = ?1"
    };

    let mut stmt = conn.prepare(sql)?;

    let rows = if let Some(file) = file_filter {
        stmt.query_map(params![query_lower, file], row_to_heading)?
    } else {
        stmt.query_map(params![query_lower], row_to_heading)?
    };

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }

    Ok(results)
}

/// Get suggestions for a query that didn't match per [[RFC-0002:C-SHOW]].
///
/// Returns headings where the text starts with the query or contains it as substring.
pub fn get_suggestions(conn: &Connection, query: &str, limit: usize) -> Result<Vec<HeadingEntry>> {
    let query_lower = query.trim().to_lowercase();
    let query_pattern = format!("%{}%", query_lower);

    let sql = "SELECT file, text, level, start_line, end_line FROM headings
               WHERE LOWER(text) LIKE ?1
               ORDER BY
                 CASE WHEN LOWER(text) LIKE ?2 THEN 0 ELSE 1 END,
                 text
               LIMIT ?3";

    let prefix_pattern = format!("{}%", query_lower);
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        params![query_pattern, prefix_pattern, limit as i64],
        row_to_heading,
    )?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }

    Ok(results)
}

/// Get all headings from the index (for outline command).
pub fn get_all_headings(conn: &Connection) -> Result<Vec<HeadingEntry>> {
    let sql =
        "SELECT file, text, level, start_line, end_line FROM headings ORDER BY file, start_line";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], row_to_heading)?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }

    Ok(results)
}

fn row_to_heading(row: &rusqlite::Row) -> rusqlite::Result<HeadingEntry> {
    Ok(HeadingEntry {
        file: row.get(0)?,
        text: row.get(1)?,
        level: row.get::<_, i64>(2)? as usize,
        start_line: row.get::<_, i64>(3)? as usize,
        end_line: row.get::<_, i64>(4)? as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_compute_hash16() {
        let temp = TempDir::new().unwrap();
        let hash = compute_hash16(temp.path());
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_get_index_path() {
        let temp = TempDir::new().unwrap();
        let runtime = temp.path().join("runtime");
        let source = temp.path().join("source");
        std::fs::create_dir_all(&source).unwrap();

        let path = get_index_path(&runtime, &source);
        assert!(path.to_string_lossy().contains(".skillc-meta"));
        assert!(path.to_string_lossy().contains("search-"));
        assert!(path.to_string_lossy().ends_with(".db"));
    }
}
