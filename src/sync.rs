//! Log synchronization per [[RFC-0007:C-SYNC]]

use crate::config::global_runtime_store;
use crate::error::{Result, SkillcError};
use crate::verbose;
use rusqlite::Connection;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Options for sync command
pub struct SyncOptions {
    /// Specific skill to sync (None = all)
    pub skill: Option<String>,
    /// Project directory to sync from (None = CWD)
    pub project: Option<PathBuf>,
    /// Dry run mode
    pub dry_run: bool,
}

/// Sync result for a single skill
struct SyncResult {
    skill: String,
    entries_synced: usize,
    entries_skipped: usize,
    /// Whether sync succeeded (for partial failure handling)
    success: bool,
}

/// Execute the sync command per [[RFC-0007:C-SYNC]].
///
/// Copies log entries from project-local fallback databases to primary runtime logs.
pub fn sync(options: SyncOptions) -> Result<()> {
    let project_dir = options
        .project
        .clone()
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| SkillcError::Internal("cannot determine project directory".to_string()))?;

    verbose!("sync: project_dir={}", project_dir.display());

    let logs_dir = crate::util::project_logs_dir(&project_dir);

    // Determine which skills to sync
    let skills: Vec<String> = if let Some(ref skill) = options.skill {
        // Single skill specified — error if no logs exist (E040)
        let skill_log_dir = logs_dir.join(skill);
        let skill_db = skill_log_dir.join(".skillc-meta").join("logs.db");
        if !skill_db.exists() {
            return Err(SkillcError::NoLocalLogs);
        }
        vec![skill.clone()]
    } else {
        // All skills with fallback logs — informational if none
        if !logs_dir.exists() {
            println!("No local logs to sync");
            return Ok(());
        }
        let found = list_skills_in_logs_dir(&logs_dir);
        if found.is_empty() {
            println!("No local logs to sync");
            return Ok(());
        }
        found
    };

    verbose!("sync: found {} skill(s) to sync", skills.len());

    let mut results = Vec::new();
    let mut had_errors = false;

    for skill in &skills {
        match sync_skill(&logs_dir, skill, options.dry_run) {
            Ok(result) => results.push(result),
            Err(e) => {
                // Partial failure: report error, continue with other skills
                eprintln!("error: failed to sync '{}': {}", skill, e);
                had_errors = true;
                results.push(SyncResult {
                    skill: skill.clone(),
                    entries_synced: 0,
                    entries_skipped: 0,
                    success: false,
                });
            }
        }
    }

    // Print results and purge successful syncs (SSOT: sync = move, not copy)
    for result in &results {
        if options.dry_run {
            println!(
                "Would sync {} entries for '{}'",
                result.entries_synced, result.skill
            );
        } else if result.success {
            println!(
                "Synced {} entries for '{}' (local logs removed)",
                result.entries_synced, result.skill
            );
            // Always delete local logs after successful sync
            if let Err(e) = purge_local_logs(&logs_dir, &result.skill) {
                eprintln!(
                    "warning: failed to remove local logs for '{}': {}",
                    result.skill, e
                );
            }
        }
        if result.entries_skipped > 0 {
            verbose!(
                "sync: skipped {} duplicate entries for '{}'",
                result.entries_skipped,
                result.skill
            );
        }
    }

    if had_errors {
        // Return error if any skill failed (but we still synced what we could)
        Err(SkillcError::Internal(
            "some skills failed to sync".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// List all skills with logs in the given logs directory.
fn list_skills_in_logs_dir(logs_dir: &Path) -> Vec<String> {
    let mut skills = Vec::new();

    if let Ok(entries) = fs::read_dir(logs_dir) {
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

/// Sync a single skill's logs from fallback to primary.
fn sync_skill(logs_dir: &Path, skill: &str, dry_run: bool) -> Result<SyncResult> {
    let fallback_dir = logs_dir.join(skill);
    let fallback_db = fallback_dir.join(".skillc-meta").join("logs.db");

    if !fallback_db.exists() {
        return Ok(SyncResult {
            skill: skill.to_string(),
            entries_synced: 0,
            entries_skipped: 0,
            success: true,
        });
    }

    // Open fallback (source) database
    let src_conn = Connection::open(&fallback_db).map_err(|e| {
        SkillcError::SyncSourceNotReadable(fallback_db.to_string_lossy().to_string(), e.to_string())
    })?;

    // Determine primary destination
    let primary_dir = global_runtime_store()?.join(skill);
    let primary_meta = primary_dir.join(".skillc-meta");
    let primary_db = primary_meta.join("logs.db");

    verbose!(
        "sync: {} -> {}",
        fallback_db.display(),
        primary_db.display()
    );

    // Read all entries from source
    let entries = read_log_entries(&src_conn)?;

    if entries.is_empty() {
        return Ok(SyncResult {
            skill: skill.to_string(),
            entries_synced: 0,
            entries_skipped: 0,
            success: true,
        });
    }

    if dry_run {
        // In dry run, just count entries (no dedup check)
        return Ok(SyncResult {
            skill: skill.to_string(),
            entries_synced: entries.len(),
            entries_skipped: 0,
            success: false, // Don't purge on dry run
        });
    }

    // Create destination directory if needed
    fs::create_dir_all(&primary_meta).map_err(|e| {
        SkillcError::SyncDestNotWritable(primary_db.to_string_lossy().to_string(), e.to_string())
    })?;

    // Open destination database
    let dst_conn = Connection::open(&primary_db).map_err(|e| {
        SkillcError::SyncDestNotWritable(primary_db.to_string_lossy().to_string(), e.to_string())
    })?;

    // Create schema if not exists
    dst_conn
        .execute(
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
        )
        .map_err(|e| {
            SkillcError::SyncDestNotWritable(
                primary_db.to_string_lossy().to_string(),
                e.to_string(),
            )
        })?;

    // Insert entries with deduplication
    let mut synced = 0;
    let mut skipped = 0;

    for entry in &entries {
        if entry_exists(&dst_conn, entry)? {
            skipped += 1;
            continue;
        }

        insert_entry(&dst_conn, entry).map_err(|e| {
            SkillcError::SyncDestNotWritable(
                primary_db.to_string_lossy().to_string(),
                e.to_string(),
            )
        })?;
        synced += 1;
    }

    Ok(SyncResult {
        skill: skill.to_string(),
        entries_synced: synced,
        entries_skipped: skipped,
        success: true,
    })
}

/// Log entry for sync operations
struct LogEntryRow {
    timestamp: String,
    run_id: String,
    command: String,
    skill: String,
    skill_path: String,
    cwd: String,
    args: String,
    error: Option<String>,
}

/// Read all log entries from a database.
fn read_log_entries(conn: &Connection) -> Result<Vec<LogEntryRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, run_id, command, skill, skill_path, cwd, args, error
         FROM access_log",
        )
        .map_err(|e| SkillcError::Internal(format!("failed to prepare query: {}", e)))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(LogEntryRow {
                timestamp: row.get(0)?,
                run_id: row.get(1)?,
                command: row.get(2)?,
                skill: row.get(3)?,
                skill_path: row.get(4)?,
                cwd: row.get(5)?,
                args: row.get(6)?,
                error: row.get(7)?,
            })
        })
        .map_err(|e| SkillcError::Internal(format!("failed to query logs: {}", e)))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| SkillcError::Internal(format!("failed to read row: {}", e)))?);
    }

    Ok(entries)
}

/// Check if an entry already exists in the destination database.
/// Deduplication key: (run_id, timestamp, command, args)
fn entry_exists(conn: &Connection, entry: &LogEntryRow) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM access_log
         WHERE run_id = ?1 AND timestamp = ?2 AND command = ?3 AND args = ?4",
            rusqlite::params![entry.run_id, entry.timestamp, entry.command, entry.args],
            |row| row.get(0),
        )
        .map_err(|e| SkillcError::Internal(format!("failed to check duplicate: {}", e)))?;

    Ok(count > 0)
}

/// Insert an entry into the destination database.
fn insert_entry(
    conn: &Connection,
    entry: &LogEntryRow,
) -> std::result::Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO access_log (timestamp, run_id, command, skill, skill_path, cwd, args, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            entry.timestamp,
            entry.run_id,
            entry.command,
            entry.skill,
            entry.skill_path,
            entry.cwd,
            entry.args,
            entry.error,
        ],
    )?;
    Ok(())
}

/// Delete local fallback logs for a skill.
fn purge_local_logs(logs_dir: &Path, skill: &str) -> Result<()> {
    let skill_dir = logs_dir.join(skill);
    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir).map_err(|e| {
            SkillcError::Internal(format!("failed to purge local logs for '{}': {}", skill, e))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_list_skills_in_logs_dir_empty() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let logs_dir = temp.path().join("logs");
        fs::create_dir_all(&logs_dir).expect("failed to create logs dir");

        let skills = list_skills_in_logs_dir(&logs_dir);
        assert!(skills.is_empty());
    }

    #[test]
    fn test_list_skills_in_logs_dir_with_skills() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let logs_dir = temp.path().join("logs");

        // Create skill directories with logs.db
        for skill in ["rust", "cuda", "go"] {
            let skill_dir = logs_dir.join(skill).join(".skillc-meta");
            fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
            fs::write(skill_dir.join("logs.db"), b"").expect("failed to write logs.db");
        }

        let skills = list_skills_in_logs_dir(&logs_dir);
        assert_eq!(skills, vec!["cuda", "go", "rust"]);
    }

    #[test]
    fn test_list_skills_in_logs_dir_nonexistent() {
        let skills = list_skills_in_logs_dir(Path::new("/nonexistent/path"));
        assert!(skills.is_empty());
    }

    #[test]
    fn test_list_skills_ignores_dirs_without_logs_db() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let logs_dir = temp.path().join("logs");

        // Create skill dir without logs.db
        let skill_dir = logs_dir.join("incomplete-skill").join(".skillc-meta");
        fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
        // No logs.db file

        // Create skill dir with logs.db
        let valid_skill_dir = logs_dir.join("valid-skill").join(".skillc-meta");
        fs::create_dir_all(&valid_skill_dir).expect("failed to create skill dir");
        fs::write(valid_skill_dir.join("logs.db"), b"").expect("failed to write logs.db");

        let skills = list_skills_in_logs_dir(&logs_dir);
        assert_eq!(skills, vec!["valid-skill"]);
    }

    fn create_test_logs_db(path: &Path) -> Connection {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = Connection::open(path).unwrap();
        conn.execute(
            "CREATE TABLE access_log (
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
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_read_log_entries_empty() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let db_path = temp.path().join("logs.db");
        let conn = create_test_logs_db(&db_path);

        let entries = read_log_entries(&conn).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_log_entries_with_data() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let db_path = temp.path().join("logs.db");
        let conn = create_test_logs_db(&db_path);

        conn.execute(
            "INSERT INTO access_log (timestamp, run_id, command, skill, skill_path, cwd, args, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "2025-01-01T00:00:00Z",
                "run-123",
                "show",
                "test-skill",
                "/path/to/skill",
                "/cwd",
                "--section Foo",
                None::<String>
            ],
        )
        .unwrap();

        let entries = read_log_entries(&conn).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].run_id, "run-123");
        assert_eq!(entries[0].command, "show");
    }

    #[test]
    fn test_entry_exists() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let db_path = temp.path().join("logs.db");
        let conn = create_test_logs_db(&db_path);

        let entry = LogEntryRow {
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            run_id: "run-123".to_string(),
            command: "show".to_string(),
            skill: "test-skill".to_string(),
            skill_path: "/path/to/skill".to_string(),
            cwd: "/cwd".to_string(),
            args: "--section Foo".to_string(),
            error: None,
        };

        // Entry doesn't exist yet
        assert!(!entry_exists(&conn, &entry).unwrap());

        // Insert entry
        insert_entry(&conn, &entry).unwrap();

        // Now it exists
        assert!(entry_exists(&conn, &entry).unwrap());
    }

    #[test]
    fn test_sync_skill_empty_db() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let logs_dir = temp.path().join("logs");

        // Create empty logs database
        let skill_dir = logs_dir.join("test-skill").join(".skillc-meta");
        create_test_logs_db(&skill_dir.join("logs.db"));

        let result = sync_skill(&logs_dir, "test-skill", true).unwrap();
        assert_eq!(result.skill, "test-skill");
        assert_eq!(result.entries_synced, 0);
        assert!(result.success); // Empty is success
    }

    #[test]
    fn test_sync_skill_nonexistent() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let logs_dir = temp.path().join("logs");
        fs::create_dir_all(&logs_dir).unwrap();

        let result = sync_skill(&logs_dir, "nonexistent", true).unwrap();
        assert_eq!(result.entries_synced, 0);
        assert!(result.success);
    }

    #[test]
    fn test_purge_local_logs() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let logs_dir = temp.path().join("logs");
        let skill_dir = logs_dir.join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("test.txt"), b"test").unwrap();

        assert!(skill_dir.exists());
        purge_local_logs(&logs_dir, "test-skill").unwrap();
        assert!(!skill_dir.exists());
    }

    #[test]
    fn test_purge_local_logs_nonexistent() {
        let temp = TempDir::new().expect("failed to create temp dir");
        // Should not error on nonexistent
        purge_local_logs(temp.path(), "nonexistent").unwrap();
    }
}
