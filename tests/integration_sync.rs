//! Integration tests for sync command per [[RFC-0007]].
//!
//! These tests verify the sync behavior for merging fallback logs to primary runtime.

mod common;

use common::{
    create_minimal_skill, create_mock_home, fallback_db_path, run_skc_isolated, runtime_db_path,
};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// Threshold for "stale" logs in tests (2 days in seconds).
const STALE_LOG_AGE_SECS: u64 = 2 * 24 * 3600;

/// Create fallback logs at project-local location.
/// Use `offset` to generate unique timestamps (default 0).
fn create_fallback_logs(project_dir: &Path, skill_name: &str, count: usize) {
    create_fallback_logs_impl(project_dir, skill_name, count, 0);
}

/// Create fallback logs with an offset to generate unique timestamps.
fn create_fallback_logs_with_offset(
    project_dir: &Path,
    skill_name: &str,
    count: usize,
    offset: usize,
) {
    create_fallback_logs_impl(project_dir, skill_name, count, offset);
}

fn create_fallback_logs_impl(project_dir: &Path, skill_name: &str, count: usize, offset: usize) {
    let fallback_dir = project_dir
        .join(".skillc")
        .join("logs")
        .join(skill_name)
        .join(".skillc-meta");
    fs::create_dir_all(&fallback_dir).expect("failed to create fallback dir");

    let db_path = fallback_dir.join("logs.db");
    let conn = rusqlite::Connection::open(&db_path).expect("failed to create db");

    // Disable WAL mode for test reliability
    conn.pragma_update(None, "journal_mode", "DELETE")
        .expect("failed to set journal mode");

    // Create schema
    conn.execute(
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
    .expect("failed to create table");

    // Insert entries
    for i in 0..count {
        conn.execute(
            "INSERT INTO access_log (timestamp, run_id, command, skill, skill_path, cwd, args) VALUES (?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                format!("2026-01-30T10:00:{:02}Z", offset + i),
                "test-run-id",
                "outline",
                skill_name,
                format!("/path/to/{}", skill_name),
                "/test/cwd",
                "{}"
            ],
        )
        .expect("failed to insert entry");
    }

    // Ensure all data is flushed
    drop(conn);
}

/// Set file modification time to specified age in the past.
fn age_file(path: &Path, age: Duration) {
    use filetime::FileTime;
    let old_time = SystemTime::now() - age;
    let file_time = FileTime::from_system_time(old_time);
    filetime::set_file_mtime(path, file_time).expect("failed to set file mtime");
}

/// Helper to create a mock home with runtime structure and run skc.
///
/// Returns (stdout, stderr, success, mock_home_path)
#[cfg(unix)]
fn run_with_mock_home(
    project_dir: &Path,
    args: &[&str],
    skills: &[&str],
) -> (String, String, bool, std::path::PathBuf) {
    // Create mock home structure INSIDE the project_dir to avoid sharing between tests
    let mock_home = project_dir.join("mock_home");
    let mock_runtime = mock_home.join(".claude").join("skills");
    fs::create_dir_all(&mock_runtime).expect("failed to create mock runtime");

    // Create skills in mock runtime
    for skill in skills {
        create_minimal_skill(&mock_runtime, skill);
    }

    let (stdout, stderr, success) = run_skc_isolated(
        project_dir,
        args,
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    (stdout, stderr, success, mock_home)
}

#[cfg(unix)]
#[test]
fn test_sync_deletes_on_success() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path().to_path_buf();

    // Create fallback logs
    create_fallback_logs(&project_dir, "test-skill", 5);

    let fallback_db = fallback_db_path(&project_dir, "test-skill");
    assert!(fallback_db.exists(), "fallback db should exist before sync");

    // Verify entries were actually created
    {
        let conn = rusqlite::Connection::open(&fallback_db).expect("open fallback db");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM access_log", [], |row| row.get(0))
            .expect("count entries");
        assert_eq!(count, 5, "fallback db should have 5 entries before sync");
    }

    // Run sync with mock home (skill created there)
    let (stdout, _stderr, success, mock_home) =
        run_with_mock_home(&project_dir, &["sync"], &["test-skill"]);

    assert!(success, "sync should succeed");
    assert!(
        stdout.contains("Synced 5 entries for 'test-skill'"),
        "should report synced entries: {}",
        stdout
    );
    assert!(
        stdout.contains("(local logs removed)"),
        "should report local logs removed: {}",
        stdout
    );

    // Verify fallback logs are deleted
    assert!(
        !fallback_db.exists(),
        "fallback db should be deleted after sync"
    );

    // Verify entries are in the runtime store (mock home's .claude/skills/)
    let runtime_db = runtime_db_path(&mock_home, "test-skill");
    assert!(
        runtime_db.exists(),
        "runtime db should exist after sync at {}",
        runtime_db.display()
    );

    let conn = rusqlite::Connection::open(&runtime_db).expect("failed to open db");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM access_log", [], |row| row.get(0))
        .expect("failed to query count");
    assert_eq!(count, 5, "runtime db should have 5 entries");
}

#[cfg(unix)]
#[test]
fn test_sync_dry_run_does_not_delete() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path().to_path_buf();

    create_fallback_logs(&project_dir, "test-skill", 3);

    let fallback_db = fallback_db_path(&project_dir, "test-skill");
    assert!(fallback_db.exists(), "fallback db should exist before sync");

    // Run sync with --dry-run
    let (stdout, _stderr, success, _) =
        run_with_mock_home(&project_dir, &["sync", "--dry-run"], &["test-skill"]);

    assert!(success, "sync --dry-run should succeed");
    assert!(
        stdout.contains("Would sync") || stdout.contains("(dry run)"),
        "should indicate dry run: {}",
        stdout
    );

    // Verify fallback logs are NOT deleted
    assert!(
        fallback_db.exists(),
        "fallback db should still exist after dry run"
    );
}

#[test]
fn test_sync_no_local_logs() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    // No fallback logs exist
    let (stdout, _stderr, success) = run_skc_isolated(project_dir, &["sync"], &[]);

    assert!(success, "sync with no logs should succeed");
    assert!(
        stdout.contains("No local logs to sync"),
        "should report no logs: {}",
        stdout
    );
}

#[cfg(unix)]
#[test]
fn test_sync_specific_skill_only() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path().to_path_buf();

    // Create logs for multiple skills
    create_fallback_logs(&project_dir, "skill-a", 3);
    create_fallback_logs(&project_dir, "skill-b", 2);

    // Sync only skill-a
    let (stdout, _stderr, success, _) =
        run_with_mock_home(&project_dir, &["sync", "skill-a"], &["skill-a", "skill-b"]);

    assert!(success, "sync skill-a should succeed");
    assert!(
        stdout.contains("skill-a"),
        "should mention skill-a: {}",
        stdout
    );

    // skill-b logs should still exist
    let skill_b_db = fallback_db_path(&project_dir, "skill-b");
    assert!(
        skill_b_db.exists(),
        "skill-b logs should still exist after syncing only skill-a"
    );
}

#[test]
fn test_sync_specific_skill_no_logs_returns_error() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    // No logs for specified skill
    let (_stdout, stderr, success) =
        run_skc_isolated(project_dir, &["sync", "nonexistent-skill"], &[]);

    assert!(!success, "sync nonexistent skill should fail");
    assert!(
        stderr.contains("E040") || stderr.contains("No local logs found"),
        "should report error: {}",
        stderr
    );
}

#[cfg(unix)]
#[test]
fn test_sync_multiple_skills() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path().to_path_buf();

    create_fallback_logs(&project_dir, "skill-a", 2);
    create_fallback_logs(&project_dir, "skill-b", 3);

    let (stdout, _stderr, success, _) =
        run_with_mock_home(&project_dir, &["sync"], &["skill-a", "skill-b"]);

    assert!(success, "sync should succeed");
    assert!(
        stdout.contains("skill-a") && stdout.contains("skill-b"),
        "should sync both skills: {}",
        stdout
    );
}

#[cfg(unix)]
#[test]
fn test_sync_deduplication() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path().to_path_buf();

    create_fallback_logs(&project_dir, "test-skill", 3);

    // First sync
    let (stdout1, _stderr, success, mock_home) =
        run_with_mock_home(&project_dir, &["sync"], &["test-skill"]);
    assert!(success, "first sync should succeed");
    assert!(
        stdout1.contains("Synced 3 entries"),
        "should sync 3 entries: {}",
        stdout1
    );

    // Create more fallback logs with DIFFERENT timestamps (to avoid dedup)
    create_fallback_logs_with_offset(&project_dir, "test-skill", 2, 10);

    // Second sync (should only sync new entries)
    let (stdout2, _stderr, success) = run_skc_isolated(
        &project_dir,
        &["sync"],
        &[("HOME", mock_home.to_str().expect("path to str"))],
    );
    assert!(success, "second sync should succeed");
    assert!(
        stdout2.contains("Synced 2 entries"),
        "should sync only 2 new entries: {}",
        stdout2
    );

    // Verify total count
    let runtime_db = runtime_db_path(&mock_home, "test-skill");
    let conn = rusqlite::Connection::open(&runtime_db).expect("failed to open db");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM access_log", [], |row| row.get(0))
        .expect("failed to query count");
    assert_eq!(count, 5, "should have 5 total entries");
}

#[cfg(unix)]
#[test]
fn test_sync_nonexistent_skill_in_runtime() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path().to_path_buf();

    // Create logs for a skill that doesn't exist in runtime
    create_fallback_logs(&project_dir, "new-skill", 2);

    // Sync should create the destination
    let (_stdout, _stderr, success, mock_home) = run_with_mock_home(&project_dir, &["sync"], &[]); // No skills pre-created

    assert!(success, "sync should succeed even for new skill");

    // Verify destination was created
    let runtime_db = runtime_db_path(&mock_home, "new-skill");
    assert!(
        runtime_db.exists(),
        "destination should be created at {}",
        runtime_db.display()
    );
}

#[cfg(unix)]
#[test]
fn test_stale_warning_emitted_for_old_logs() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path().to_path_buf();

    // Create skill in project source store
    let skills_dir = project_dir.join(".skillc").join("skills");
    fs::create_dir_all(&skills_dir).expect("create skills dir");
    create_minimal_skill(&skills_dir, "test-skill");

    create_fallback_logs(&project_dir, "test-skill", 1);

    // Age the logs to make them stale
    let fallback_db = fallback_db_path(&project_dir, "test-skill");
    age_file(&fallback_db, Duration::from_secs(STALE_LOG_AGE_SECS));

    // Run a command that triggers logging - should warn about stale logs
    let mock_home = create_mock_home(&project_dir);
    // Use 'outline' which triggers log_access_with_fallback
    let (_stdout, stderr, _success) = run_skc_isolated(
        &project_dir,
        &["outline", "test-skill"],
        &[("HOME", mock_home.to_str().expect("path to str"))],
    );

    assert!(
        stderr.contains("stale") || stderr.contains("Local logs"),
        "should warn about stale logs: {}",
        stderr
    );
}

#[test]
fn test_stale_warning_not_emitted_for_fresh_logs() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    create_fallback_logs(project_dir, "test-skill", 1);
    // Logs are fresh (just created)

    let (_stdout, stderr, _success) = run_skc_isolated(project_dir, &["stats", "test-skill"], &[]);

    assert!(
        !stderr.contains("stale") && !stderr.contains("Local logs"),
        "should not warn about fresh logs: {}",
        stderr
    );
}

#[test]
fn test_stale_warning_once_per_invocation() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    create_fallback_logs(project_dir, "skill-a", 1);
    create_fallback_logs(project_dir, "skill-b", 1);

    // Age both to make them stale
    age_file(
        &fallback_db_path(project_dir, "skill-a"),
        Duration::from_secs(STALE_LOG_AGE_SECS),
    );
    age_file(
        &fallback_db_path(project_dir, "skill-b"),
        Duration::from_secs(STALE_LOG_AGE_SECS),
    );

    let (_stdout, stderr, _success) = run_skc_isolated(project_dir, &["stats", "skill-a"], &[]);

    // Should only warn once, not per-skill
    let warning_count = stderr.matches("stale").count() + stderr.matches("Local logs").count();
    assert!(
        warning_count <= 1,
        "should warn at most once: {} (count: {})",
        stderr,
        warning_count
    );
}
