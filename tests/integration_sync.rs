//! Integration tests for sync command per [[RFC-0007]].
//!
//! These tests verify the sync behavior for merging fallback logs to primary runtime.

mod common;

use common::{TestContext, create_minimal_skill, fallback_db_path, runtime_db_path};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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

/// Helper to create a mock home with runtime structure using TestContext.
fn setup_mock_runtime(ctx: &TestContext, skills: &[&str]) {
    let mock_runtime = ctx.mock_home().join(".claude").join("skills");
    fs::create_dir_all(&mock_runtime).expect("failed to create mock runtime");

    for skill in skills {
        create_minimal_skill(&mock_runtime, skill);
    }
}

#[test]
fn test_sync_deletes_on_success() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

    // Create fallback logs
    create_fallback_logs(project_dir, "test-skill", 5);

    let fallback_db = fallback_db_path(project_dir, "test-skill");
    assert!(fallback_db.exists(), "fallback db should exist before sync");

    // Verify entries were actually created
    {
        let conn = rusqlite::Connection::open(&fallback_db).expect("open fallback db");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM access_log", [], |row| row.get(0))
            .expect("count entries");
        assert_eq!(count, 5, "fallback db should have 5 entries before sync");
    }

    // Setup mock runtime with skill
    setup_mock_runtime(&ctx, &["test-skill"]);

    // Run sync with mock home
    let result = ctx.run_skc(&["sync"]);

    assert!(result.success, "sync should succeed");
    assert!(
        result.stdout.contains("Synced 5 entries for 'test-skill'"),
        "should report synced entries: {}",
        result.stdout
    );
    assert!(
        result.stdout.contains("(local logs removed)"),
        "should report local logs removed: {}",
        result.stdout
    );

    // Verify fallback logs are deleted
    assert!(
        !fallback_db.exists(),
        "fallback db should be deleted after sync"
    );

    // Verify entries are in the runtime store (mock home's .claude/skills/)
    let runtime_db = runtime_db_path(ctx.mock_home(), "test-skill");
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

#[test]
fn test_sync_dry_run_does_not_delete() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

    create_fallback_logs(project_dir, "test-skill", 3);

    let fallback_db = fallback_db_path(project_dir, "test-skill");
    assert!(fallback_db.exists(), "fallback db should exist before sync");

    // Setup mock runtime
    setup_mock_runtime(&ctx, &["test-skill"]);

    // Run sync with --dry-run
    let result = ctx.run_skc(&["sync", "--dry-run"]);

    assert!(result.success, "sync --dry-run should succeed");
    assert!(
        result.stdout.contains("Would sync") || result.stdout.contains("(dry run)"),
        "should indicate dry run: {}",
        result.stdout
    );

    // Verify fallback logs are NOT deleted
    assert!(
        fallback_db.exists(),
        "fallback db should still exist after dry run"
    );
}

#[test]
fn test_sync_no_local_logs() {
    let ctx = TestContext::new().with_project();

    // No fallback logs exist
    let result = ctx.run_skc(&["sync"]);

    assert!(result.success, "sync with no logs should succeed");
    assert!(
        result.stdout.contains("No local logs to sync"),
        "should report no logs: {}",
        result.stdout
    );
}

#[test]
fn test_sync_specific_skill_only() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

    // Create logs for multiple skills
    create_fallback_logs(project_dir, "skill-a", 3);
    create_fallback_logs(project_dir, "skill-b", 2);

    // Setup mock runtime
    setup_mock_runtime(&ctx, &["skill-a", "skill-b"]);

    // Sync only skill-a
    let result = ctx.run_skc(&["sync", "skill-a"]);

    assert!(result.success, "sync skill-a should succeed");
    assert!(
        result.stdout.contains("skill-a"),
        "should mention skill-a: {}",
        result.stdout
    );

    // skill-b logs should still exist
    let skill_b_db = fallback_db_path(project_dir, "skill-b");
    assert!(
        skill_b_db.exists(),
        "skill-b logs should still exist after syncing only skill-a"
    );
}

#[test]
fn test_sync_specific_skill_no_logs_returns_error() {
    let ctx = TestContext::new().with_project();

    // No logs for specified skill
    let result = ctx.run_skc(&["sync", "nonexistent-skill"]);

    assert!(!result.success, "sync nonexistent skill should fail");
    assert!(
        result.stderr.contains("E040") || result.stderr.contains("No local logs found"),
        "should report error: {}",
        result.stderr
    );
}

#[test]
fn test_sync_multiple_skills() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

    create_fallback_logs(project_dir, "skill-a", 2);
    create_fallback_logs(project_dir, "skill-b", 3);

    setup_mock_runtime(&ctx, &["skill-a", "skill-b"]);

    let result = ctx.run_skc(&["sync"]);

    assert!(result.success, "sync should succeed");
    assert!(
        result.stdout.contains("skill-a") && result.stdout.contains("skill-b"),
        "should sync both skills: {}",
        result.stdout
    );
}

#[test]
fn test_sync_deduplication() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

    create_fallback_logs(project_dir, "test-skill", 3);

    setup_mock_runtime(&ctx, &["test-skill"]);

    // First sync
    let result1 = ctx.run_skc(&["sync"]);
    assert!(result1.success, "first sync should succeed");
    assert!(
        result1.stdout.contains("Synced 3 entries"),
        "should sync 3 entries: {}",
        result1.stdout
    );

    // Create more fallback logs with DIFFERENT timestamps (to avoid dedup)
    create_fallback_logs_with_offset(project_dir, "test-skill", 2, 10);

    // Second sync (should only sync new entries)
    let result2 = ctx.run_skc(&["sync"]);
    assert!(result2.success, "second sync should succeed");
    assert!(
        result2.stdout.contains("Synced 2 entries"),
        "should sync only 2 new entries: {}",
        result2.stdout
    );

    // Verify total count
    let runtime_db = runtime_db_path(ctx.mock_home(), "test-skill");
    let conn = rusqlite::Connection::open(&runtime_db).expect("failed to open db");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM access_log", [], |row| row.get(0))
        .expect("failed to query count");
    assert_eq!(count, 5, "should have 5 total entries");
}

#[test]
fn test_sync_nonexistent_skill_in_runtime() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

    // Create logs for a skill that doesn't exist in runtime
    create_fallback_logs(project_dir, "new-skill", 2);

    // Setup mock home but NO skills pre-created
    let mock_runtime = ctx.mock_home().join(".claude").join("skills");
    fs::create_dir_all(&mock_runtime).expect("failed to create mock runtime");

    // Sync should create the destination
    let result = ctx.run_skc(&["sync"]);

    assert!(result.success, "sync should succeed even for new skill");

    // Verify destination was created
    let runtime_db = runtime_db_path(ctx.mock_home(), "new-skill");
    assert!(
        runtime_db.exists(),
        "destination should be created at {}",
        runtime_db.display()
    );
}

#[test]
fn test_stale_warning_emitted_for_old_logs() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

    // Create skill in project source store
    ctx.create_skill("test-skill");

    create_fallback_logs(project_dir, "test-skill", 1);

    // Age the logs to make them stale
    let fallback_db = fallback_db_path(project_dir, "test-skill");
    age_file(&fallback_db, Duration::from_secs(STALE_LOG_AGE_SECS));

    // Run a command that triggers logging - should warn about stale logs
    let result = ctx.run_skc(&["outline", "test-skill"]);

    assert!(
        result.stderr.contains("stale") || result.stderr.contains("Local logs"),
        "should warn about stale logs: {}",
        result.stderr
    );
}

#[test]
fn test_stale_warning_not_emitted_for_fresh_logs() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

    create_fallback_logs(project_dir, "test-skill", 1);
    // Logs are fresh (just created)

    let result = ctx.run_skc(&["stats", "test-skill"]);

    assert!(
        !result.stderr.contains("stale") && !result.stderr.contains("Local logs"),
        "should not warn about fresh logs: {}",
        result.stderr
    );
}

#[test]
fn test_stale_warning_once_per_invocation() {
    let ctx = TestContext::new().with_project();
    let project_dir = ctx.project_dir();

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

    let result = ctx.run_skc(&["stats", "skill-a"]);

    // Should only warn once, not per-skill
    let warning_count =
        result.stderr.matches("stale").count() + result.stderr.matches("Local logs").count();
    assert!(
        warning_count <= 1,
        "should warn at most once: {} (count: {})",
        result.stderr,
        warning_count
    );
}

// ============================================================================
// Logging integration tests per [[RFC-0007:C-LOGGING]]
// ============================================================================

/// Get path to project-local runtime logs database.
fn project_runtime_db(project_dir: &Path, skill_name: &str) -> PathBuf {
    project_dir
        .join(".skillc")
        .join("runtime")
        .join(skill_name)
        .join(".skillc-meta")
        .join("logs.db")
}

/// Test that gateway commands create logs in runtime store.
#[test]
fn test_logging_creates_runtime_logs() {
    let ctx = TestContext::new().with_rich_skill("log-test");

    // Run a few gateway commands
    let _ = ctx.run_skc(&["outline", "log-test"]);
    let _ = ctx.run_skc(&["show", "log-test", "--section", "Getting Started"]);

    // Verify logs were created in project runtime store
    let runtime_db = project_runtime_db(ctx.project_dir(), "log-test");
    assert!(
        runtime_db.exists(),
        "runtime logs should exist at {}",
        runtime_db.display()
    );

    let conn = rusqlite::Connection::open(&runtime_db).expect("open runtime db");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM access_log", [], |row| row.get(0))
        .expect("count entries");
    assert!(
        count >= 2,
        "should have at least 2 log entries, got {}",
        count
    );
}

/// Test that logs contain correct command names.
#[test]
fn test_logging_records_command_names() {
    let ctx = TestContext::new().with_rich_skill("cmd-test");

    let _ = ctx.run_skc(&["outline", "cmd-test"]);
    let _ = ctx.run_skc(&["show", "cmd-test", "--section", "API Reference"]);

    let runtime_db = project_runtime_db(ctx.project_dir(), "cmd-test");
    let conn = rusqlite::Connection::open(&runtime_db).expect("open db");

    let commands: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT command FROM access_log ORDER BY command")
            .expect("prepare");
        stmt.query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect()
    };

    assert!(
        commands.contains(&"outline".to_string()),
        "should log outline command: {:?}",
        commands
    );
    assert!(
        commands.contains(&"show".to_string()),
        "should log show command: {:?}",
        commands
    );
}

/// Test that logs contain skill path.
#[test]
fn test_logging_records_skill_path() {
    let ctx = TestContext::new().with_rich_skill("path-test");

    let _ = ctx.run_skc(&["outline", "path-test"]);

    let runtime_db = project_runtime_db(ctx.project_dir(), "path-test");
    let conn = rusqlite::Connection::open(&runtime_db).expect("open db");

    let skill_path: String = conn
        .query_row(
            "SELECT skill_path FROM access_log WHERE command = 'outline' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("query skill_path");

    assert!(
        skill_path.contains("path-test"),
        "skill_path should contain skill name: {}",
        skill_path
    );
}

/// Test that errors are logged.
#[test]
fn test_logging_records_errors() {
    let ctx = TestContext::new().with_rich_skill("err-test");

    // Run a command that will fail (invalid section)
    let _ = ctx.run_skc(&["show", "err-test", "--section", "NonexistentSection12345"]);

    let runtime_db = project_runtime_db(ctx.project_dir(), "err-test");
    let conn = rusqlite::Connection::open(&runtime_db).expect("open db");

    let error: Option<String> = conn
        .query_row(
            "SELECT error FROM access_log WHERE command = 'show' ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("query error");

    assert!(
        error.is_some(),
        "error field should be populated for failed command"
    );
}

/// Test that run_id is consistent (from SKC_RUN_ID env var).
#[test]
fn test_logging_uses_run_id_env() {
    let ctx = TestContext::new().with_rich_skill("runid-test");

    // TestContext sets SKC_RUN_ID to "TEST-RUN-ID"
    let _ = ctx.run_skc(&["outline", "runid-test"]);

    let runtime_db = project_runtime_db(ctx.project_dir(), "runid-test");
    let conn = rusqlite::Connection::open(&runtime_db).expect("open db");

    let run_id: String = conn
        .query_row("SELECT run_id FROM access_log LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("query run_id");

    assert_eq!(
        run_id, "TEST-RUN-ID",
        "should use SKC_RUN_ID from environment"
    );
}

/// Test that multiple commands in same session share run_id.
#[test]
fn test_logging_same_run_id_per_session() {
    let ctx = TestContext::new().with_rich_skill("session-test");

    // Run multiple commands - each invocation is separate, but has same env
    let _ = ctx.run_skc(&["outline", "session-test"]);
    let _ = ctx.run_skc(&["show", "session-test", "--section", "Getting Started"]);

    let runtime_db = project_runtime_db(ctx.project_dir(), "session-test");
    let conn = rusqlite::Connection::open(&runtime_db).expect("open db");

    let run_ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT run_id FROM access_log")
            .expect("prepare");
        stmt.query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect()
    };

    // All should have the same run_id from TestContext
    assert_eq!(
        run_ids.len(),
        1,
        "all entries should have same run_id: {:?}",
        run_ids
    );
}
