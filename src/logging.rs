//! Access logging per [[RFC-0007:C-LOGGING]]

use crate::config::ensure_dir;
use chrono::Utc;
use rand::prelude::*;
use rusqlite::{Connection, ErrorCode};
use std::env;
use std::path::{Path, PathBuf};

/// Log entry for gateway command access
pub struct LogEntry {
    pub run_id: String,
    pub command: String,
    pub skill: String,
    pub skill_path: String,
    pub cwd: String,
    pub args: String,
    pub error: Option<String>,
}

/// Initialize the log database and return a connection.
///
/// Per [[RFC-0007:C-LOGGING]]:
/// - Creates runtime directory, .skillc-meta/ subdirectory, database, and schema if they don't exist
/// - Returns None if initialization fails (command should continue without logging)
///
/// Note: This does not test writability. Use `log_access_with_fallback` which handles
/// readonly errors at write time (EAFP pattern).
pub fn init_log_db(runtime_dir: &Path) -> Option<Connection> {
    try_init_db_at(runtime_dir)
}

/// Try to initialize log database at a specific directory.
/// Returns None on failure (does not print warnings).
fn try_init_db_at(dir: &Path) -> Option<Connection> {
    // Try to create directories (may fail silently if they exist but aren't writable)
    let _ = ensure_dir(dir);

    let meta_dir = dir.join(".skillc-meta");
    let _ = ensure_dir(&meta_dir);

    let db_path = meta_dir.join("logs.db");

    // Try to open/create database
    let conn = Connection::open(&db_path).ok()?;

    // Create schema if not exists (may fail on readonly, that's ok - we'll catch it on INSERT)
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS access_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            run_id TEXT NOT NULL,
            command TEXT NOT NULL,
            skill TEXT NOT NULL,
            skill_path TEXT NOT NULL,
            cwd TEXT NOT NULL,
            args TEXT NOT NULL,
            error TEXT
        )",
        [],
    );

    // Migration: add cwd column if missing (for existing databases)
    let _ = conn.execute(
        "ALTER TABLE access_log ADD COLUMN cwd TEXT NOT NULL DEFAULT ''",
        [],
    );

    Some(conn)
}

/// Get the fallback log directory for a skill in the current working directory.
pub fn get_fallback_log_dir(skill_name: &str) -> Option<PathBuf> {
    env::current_dir()
        .ok()
        .map(|cwd| crate::util::project_skill_logs_dir(&cwd, skill_name))
}

/// List all skills with fallback logs in the current working directory.
pub fn list_fallback_skills() -> Vec<String> {
    let cwd = match env::current_dir() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let logs_dir = crate::util::project_logs_dir(&cwd);
    if !logs_dir.exists() {
        return Vec::new();
    }

    let mut skills = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&logs_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let db_path = entry.path().join(".skillc-meta").join("logs.db");
                if db_path.exists()
                    && let Some(name) = entry.file_name().to_str()
                {
                    skills.push(name.to_string());
                }
            }
        }
    }

    skills.sort();
    skills
}

/// Log an access event to the database with automatic fallback.
///
/// Per [[RFC-0007:C-LOGGING]] (EAFP pattern):
/// 1. Try to write to the provided connection
/// 2. If it fails with SQLITE_READONLY, open fallback at `<cwd>/.skillc/logs/<skill>/`
/// 3. Retry write to fallback
/// 4. If both fail, warn but continue
///
/// Also checks for stale fallback logs and warns if found (per [[RFC-0007:C-LOGGING]]).
pub fn log_access_with_fallback(conn: Option<&Connection>, entry: &LogEntry) {
    // Check for stale fallback logs first (for all commands)
    check_stale_fallback_logs(&entry.skill);

    // Try primary connection first
    if let Some(c) = conn {
        match try_log_access(c, entry) {
            Ok(()) => return,
            Err(e) if is_readonly_error(&e) => {
                // Fall through to fallback
            }
            Err(e) => {
                eprintln!("warning: failed to log access: {}", e);
                return;
            }
        }
    }

    // Try fallback: <cwd>/.skillc/logs/<skill>/
    if let Ok(cwd) = env::current_dir() {
        let fallback_dir = crate::util::project_skill_logs_dir(&cwd, &entry.skill);
        if let Some(fallback_conn) = try_init_db_at(&fallback_dir) {
            if let Err(e) = try_log_access(&fallback_conn, entry) {
                eprintln!("warning: failed to log access to fallback: {}", e);
            }
            return;
        }
    }

    // Both failed (W002 per [[RFC-0005:C-CODES]])
    crate::error::SkillcWarning::LoggingDisabled.emit();
}

/// Check for stale fallback logs and emit warning if found.
///
/// Per [[RFC-0007:C-LOGGING]]: If fallback logs exist for the skill and the
/// database file's mtime is older than 1 hour, emit a warning.
/// The warning SHOULD be emitted at most once per command invocation.
fn check_stale_fallback_logs(skill: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, SystemTime};

    // Only warn once per process (command invocation)
    static WARNED: AtomicBool = AtomicBool::new(false);
    if WARNED.swap(true, Ordering::Relaxed) {
        return;
    }

    let Ok(cwd) = env::current_dir() else {
        return;
    };

    let fallback_db = cwd
        .join(".skillc")
        .join("logs")
        .join(skill)
        .join(".skillc-meta")
        .join("logs.db");

    if !fallback_db.exists() {
        return;
    }

    // Check mtime
    let Ok(metadata) = fallback_db.metadata() else {
        return;
    };

    let Ok(mtime) = metadata.modified() else {
        return;
    };

    let Ok(age) = SystemTime::now().duration_since(mtime) else {
        return;
    };

    // Warn if older than 1 hour (W003 per [[RFC-0005:C-CODES]])
    if age > Duration::from_secs(3600) {
        crate::error::SkillcWarning::StaleLogs(skill.to_string()).emit();
    }
}

/// Try to insert a log entry. Returns error on failure.
fn try_log_access(conn: &Connection, entry: &LogEntry) -> Result<(), rusqlite::Error> {
    let timestamp = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO access_log (timestamp, run_id, command, skill, skill_path, cwd, args, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            timestamp,
            entry.run_id,
            entry.command,
            entry.skill,
            entry.skill_path,
            entry.cwd,
            entry.args,
            entry.error.as_deref(),
        ],
    )?;
    Ok(())
}

/// Check if an error is a readonly database error.
fn is_readonly_error(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: ErrorCode::ReadOnly,
                ..
            },
            _
        )
    )
}

/// Legacy function for compatibility. Prefer `log_access_with_fallback`.
pub fn log_access(conn: &Connection, entry: &LogEntry) {
    if let Err(e) = try_log_access(conn, entry) {
        eprintln!("warning: failed to log access: {}", e);
    }
}

/// Generate or retrieve run ID.
///
/// Per [[RFC-0007:C-LOGGING]]:
/// - If SKC_RUN_ID env var is set, use its value
/// - Otherwise, generate in format `YYYYMMDDTHHMMSSZ-{rand4}`
pub fn get_run_id() -> String {
    if let Ok(run_id) = env::var("SKC_RUN_ID") {
        return run_id;
    }

    let now = Utc::now();
    let mut rng = rand::rng();
    let rand_hex: String = (0..4)
        .map(|_| format!("{:x}", rng.random::<u8>() % 16))
        .collect();

    format!("{}Z-{}", now.format("%Y%m%dT%H%M%S"), rand_hex)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_init_log_db() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let runtime_dir = temp.path().join("runtime");

        let conn = init_log_db(&runtime_dir).expect("should create db");

        // Verify schema exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='access_log'",
                [],
                |row| row.get(0),
            )
            .expect("failed to query schema");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_get_run_id_format() {
        let run_id = get_run_id();
        // Should be in format YYYYMMDDTHHMMSSZ-XXXX
        assert!(run_id.contains('T'));
        assert!(run_id.contains('Z'));
        assert!(run_id.contains('-'));
    }

    #[test]
    fn test_try_log_access() {
        let temp = TempDir::new().expect("create temp dir");
        let runtime_dir = temp.path().join("runtime");

        let conn = init_log_db(&runtime_dir).expect("create db");

        let entry = LogEntry {
            run_id: "test-run-123".to_string(),
            command: "outline".to_string(),
            skill: "test-skill".to_string(),
            skill_path: "/path/to/skill".to_string(),
            cwd: "/current/dir".to_string(),
            args: r#"{"section": "API"}"#.to_string(),
            error: None,
        };

        try_log_access(&conn, &entry).expect("log access should succeed");

        // Verify entry was inserted
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM access_log", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_try_log_access_with_error() {
        let temp = TempDir::new().expect("create temp dir");
        let runtime_dir = temp.path().join("runtime");

        let conn = init_log_db(&runtime_dir).expect("create db");

        let entry = LogEntry {
            run_id: "error-run".to_string(),
            command: "show".to_string(),
            skill: "error-skill".to_string(),
            skill_path: "/path".to_string(),
            cwd: "/cwd".to_string(),
            args: "{}".to_string(),
            error: Some("E001: skill not found".to_string()),
        };

        try_log_access(&conn, &entry).expect("log access should succeed");

        // Verify error was stored
        let error: Option<String> = conn
            .query_row(
                "SELECT error FROM access_log WHERE run_id = 'error-run'",
                [],
                |row| row.get(0),
            )
            .expect("query error");
        assert_eq!(error, Some("E001: skill not found".to_string()));
    }

    #[test]
    fn test_log_access_legacy() {
        let temp = TempDir::new().expect("create temp dir");
        let runtime_dir = temp.path().join("runtime");

        let conn = init_log_db(&runtime_dir).expect("create db");

        let entry = LogEntry {
            run_id: "legacy-run".to_string(),
            command: "search".to_string(),
            skill: "legacy-skill".to_string(),
            skill_path: "/path".to_string(),
            cwd: "/cwd".to_string(),
            args: r#"{"query": "test"}"#.to_string(),
            error: None,
        };

        // Legacy function should not panic
        log_access(&conn, &entry);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM access_log", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_is_readonly_error() {
        // Create a non-readonly error
        let other_error = rusqlite::Error::InvalidQuery;
        assert!(!is_readonly_error(&other_error));
    }

    #[test]
    fn test_init_log_db_creates_directories() {
        let temp = TempDir::new().expect("create temp dir");
        let runtime_dir = temp.path().join("deep").join("nested").join("runtime");

        // Directory doesn't exist yet
        assert!(!runtime_dir.exists());

        let conn = init_log_db(&runtime_dir);
        assert!(conn.is_some(), "should create db even with nested dirs");

        // Verify meta dir was created
        let meta_dir = runtime_dir.join(".skillc-meta");
        assert!(meta_dir.exists(), "meta dir should exist");
        assert!(meta_dir.join("logs.db").exists(), "db file should exist");
    }

    // Note: We don't test get_run_id with env var override here because
    // tests run in parallel and modifying env vars causes race conditions.
    // The env var logic is trivial and tested implicitly via integration tests.
}
