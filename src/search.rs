//! Search protocol per RFC-0004
//!
//! Provides full-text search over skill content using SQLite FTS5.

use crate::config::{ensure_dir, get_cwd};
use crate::error::{Result, SkillcError};
use crate::logging::{LogEntry, get_run_id, init_log_db, log_access_with_fallback};
use crate::resolver::{ResolvedSkill, resolve_skill};
use crate::{OutputFormat, verbose};
use chrono::Utc;
use crossterm::style::Stylize;
use lazy_regex::{Lazy, Regex, lazy_regex};
use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

/// Regex for parsing markdown headings (validated at compile time).
static HEADING_RE: Lazy<Regex> = lazy_regex!(r"^(#{1,6})\s+(.+)$");

/// Current schema version for the search index.
const SCHEMA_VERSION: i32 = 1;

/// Search result entry.
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub file: String,
    pub section: String,
    pub snippet: String,
    pub score: f64,
}

/// Search response for JSON output.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResult>,
}

/// Compute hash16 for index filename per [[RFC-0004:C-INDEX]].
fn compute_hash16(source_path: &Path) -> String {
    let canonical = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    hash[..16].to_string()
}

/// Get the index database path for a skill.
fn get_index_path(runtime_dir: &Path, source_dir: &Path) -> PathBuf {
    let hash16 = compute_hash16(source_dir);
    runtime_dir
        .join(".skillc-meta")
        .join(format!("search-{}.db", hash16))
}

/// Determine the tokenizer preference per [[RFC-0009:C-TOKENIZER]] and [[RFC-0004:C-INDEX]].
///
/// Uses config-based tokenizer setting:
/// - "ascii": porter unicode61 (English word-level with stemming)
/// - "cjk": unicode61 (character-level for CJK content)
fn get_tokenizer_preference(conn: &Connection) -> String {
    use crate::config::{Tokenizer, get_tokenizer};

    let config_tokenizer = get_tokenizer();

    match config_tokenizer {
        Tokenizer::Cjk => {
            // CJK mode: use unicode61 without porter (character-level)
            "unicode61".to_string()
        }
        Tokenizer::Ascii => {
            // ASCII mode: try porter unicode61 first (word-level with stemming)
            let result = conn.execute_batch(
                "CREATE VIRTUAL TABLE IF NOT EXISTS _tokenizer_test USING fts5(x, tokenize='porter unicode61');
                 DROP TABLE IF EXISTS _tokenizer_test;",
            );

            if result.is_ok() {
                "porter unicode61".to_string()
            } else {
                "unicode61".to_string()
            }
        }
    }
}

/// Get short tokenizer name for metadata storage.
fn tokenizer_short_name(tokenizer: &str) -> &str {
    if tokenizer.contains("porter") {
        "porter"
    } else {
        "unicode61"
    }
}

/// Read required metadata key from index.
fn read_meta(conn: &Connection, key: &str) -> Result<String> {
    conn.query_row(
        "SELECT value FROM index_meta WHERE key = ?1",
        [key],
        |row| row.get(0),
    )
    .map_err(|_| {
        SkillcError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Missing metadata key: {}", key),
        ))
    })
}

/// Index state for decision making.
#[derive(Debug)]
enum IndexState {
    /// File does not exist
    Missing,
    /// Cannot read or parse
    Corrupt,
    /// skill_path mismatch (hash collision)
    Collision,
    /// Metadata differs (needs rebuild)
    Stale,
    /// All metadata matches
    UpToDate,
}

/// Check index state per [[RFC-0004:C-INDEX]].
fn check_index_state(
    index_path: &Path,
    source_dir: &Path,
    source_hash: &str,
    tokenizer_pref: &str,
) -> IndexState {
    if !index_path.exists() {
        return IndexState::Missing;
    }

    let conn = match Connection::open(index_path) {
        Ok(c) => c,
        Err(_) => return IndexState::Corrupt,
    };

    // Check if index_meta table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='index_meta'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    if !table_exists {
        return IndexState::Corrupt;
    }

    // Read required keys
    let stored_skill_path = match read_meta(&conn, "skill_path") {
        Ok(v) => v,
        Err(_) => return IndexState::Corrupt,
    };
    let stored_hash = match read_meta(&conn, "source_hash") {
        Ok(v) => v,
        Err(_) => return IndexState::Corrupt,
    };
    let stored_schema: i32 = match read_meta(&conn, "schema_version") {
        Ok(v) => match v.parse() {
            Ok(n) => n,
            Err(_) => return IndexState::Corrupt,
        },
        Err(_) => return IndexState::Corrupt,
    };
    let stored_tokenizer = match read_meta(&conn, "tokenizer") {
        Ok(v) => v,
        Err(_) => return IndexState::Corrupt,
    };

    // Check collision
    let canonical_source = source_dir
        .canonicalize()
        .unwrap_or_else(|_| source_dir.to_path_buf())
        .to_string_lossy()
        .to_string();

    if stored_skill_path != canonical_source {
        return IndexState::Collision;
    }

    // Check staleness
    let current_tokenizer_short = tokenizer_short_name(tokenizer_pref);
    if stored_hash != source_hash
        || stored_schema < SCHEMA_VERSION
        || stored_tokenizer != current_tokenizer_short
    {
        return IndexState::Stale;
    }

    IndexState::UpToDate
}

/// Build the search index for a skill per [[RFC-0004:C-INDEX]].
pub fn build_index(source_dir: &Path, runtime_dir: &Path, source_hash: &str) -> Result<()> {
    let start = Instant::now();
    let index_path = get_index_path(runtime_dir, source_dir);

    verbose!("build_index: index_path={}", index_path.display());

    // Ensure directory exists
    if let Some(parent) = index_path.parent() {
        ensure_dir(parent)?;
    }

    // Create a temporary connection to determine tokenizer preference
    let temp_conn = Connection::open_in_memory()?;
    let tokenizer_pref = get_tokenizer_preference(&temp_conn);
    drop(temp_conn);

    verbose!("build_index: tokenizer={}", tokenizer_pref);

    // Check current state
    let state = check_index_state(&index_path, source_dir, source_hash, &tokenizer_pref);

    verbose!("build_index: state={:?}", state);

    match state {
        IndexState::UpToDate => {
            // Skip rebuild
            verbose!("build_index: skipping rebuild (up to date)");
            return Ok(());
        }
        IndexState::Collision => {
            // E003: Cannot proceed
            let hash16 = compute_hash16(source_dir);
            return Err(SkillcError::IndexHashCollision(hash16));
        }
        IndexState::Missing => {
            // Will create new
            verbose!("build_index: creating new index");
        }
        IndexState::Corrupt | IndexState::Stale => {
            // Delete and rebuild
            verbose!("build_index: deleting stale/corrupt index");
            let _ = fs::remove_file(&index_path);
        }
    }

    // Create new index
    create_index(&index_path, source_dir, source_hash, &tokenizer_pref)?;

    verbose!("build_index: completed in {:?}", start.elapsed());

    Ok(())
}

/// Create a new search index.
fn create_index(
    index_path: &Path,
    source_dir: &Path,
    source_hash: &str,
    tokenizer: &str,
) -> Result<()> {
    let conn = Connection::open(index_path)?;

    // Create FTS5 table
    let create_fts = format!(
        "CREATE VIRTUAL TABLE sections USING fts5(file, section, content, tokenize='{}')",
        tokenizer
    );
    conn.execute(&create_fts, [])?;

    // Create metadata table
    conn.execute(
        "CREATE TABLE index_meta (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )?;

    // Index files
    index_files(&conn, source_dir)?;

    // Write metadata
    let canonical_path = source_dir
        .canonicalize()
        .unwrap_or_else(|_| source_dir.to_path_buf())
        .to_string_lossy()
        .to_string();
    let tokenizer_short = tokenizer_short_name(tokenizer);
    let indexed_at = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO index_meta (key, value) VALUES (?1, ?2)",
        params!["skill_path", canonical_path],
    )?;
    conn.execute(
        "INSERT INTO index_meta (key, value) VALUES (?1, ?2)",
        params!["source_hash", source_hash],
    )?;
    conn.execute(
        "INSERT INTO index_meta (key, value) VALUES (?1, ?2)",
        params!["schema_version", SCHEMA_VERSION.to_string()],
    )?;
    conn.execute(
        "INSERT INTO index_meta (key, value) VALUES (?1, ?2)",
        params!["tokenizer", tokenizer_short],
    )?;
    conn.execute(
        "INSERT INTO index_meta (key, value) VALUES (?1, ?2)",
        params!["indexed_at", indexed_at],
    )?;

    Ok(())
}

/// Index all supported files in source directory.
fn index_files(conn: &Connection, source_dir: &Path) -> Result<()> {
    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());

        match ext {
            Some("md") => index_markdown(conn, source_dir, path)?,
            Some("txt") => index_text(conn, source_dir, path)?,
            _ => {
                // Silently skip unsupported formats per [[RFC-0004:C-FORMATS]]
            }
        }
    }

    Ok(())
}

/// Index a markdown file by sections per [[RFC-0004:C-FORMATS]].
fn index_markdown(conn: &Connection, source_dir: &Path, file_path: &Path) -> Result<()> {
    let content = fs::read_to_string(file_path)?;
    let relative_path = file_path
        .strip_prefix(source_dir)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();

    let lines: Vec<&str> = content.lines().collect();

    // Find all headings with their positions
    let mut headings: Vec<(usize, usize, String)> = Vec::new(); // (line_num, level, text)
    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = HEADING_RE.captures(line) {
            let level = caps
                .get(1)
                .ok_or_else(|| SkillcError::Internal("regex group 1 missing".into()))?
                .as_str()
                .len();
            let text = caps
                .get(2)
                .ok_or_else(|| SkillcError::Internal("regex group 2 missing".into()))?
                .as_str()
                .to_string();
            headings.push((i, level, text));
        }
    }

    if headings.is_empty() {
        // No headings, index entire file as one section
        conn.execute(
            "INSERT INTO sections (file, section, content) VALUES (?1, ?2, ?3)",
            params![relative_path, "", content],
        )?;
        return Ok(());
    }

    // Extract each section
    for (idx, (start_line, level, heading_text)) in headings.iter().enumerate() {
        // Find end of section (next heading of equal or higher level)
        let end_line = headings
            .iter()
            .skip(idx + 1)
            .find(|(_, l, _)| *l <= *level)
            .map(|(line, _, _)| *line)
            .unwrap_or(lines.len());

        // Extract content
        let section_content = lines[*start_line..end_line].join("\n");

        conn.execute(
            "INSERT INTO sections (file, section, content) VALUES (?1, ?2, ?3)",
            params![relative_path, heading_text, section_content],
        )?;
    }

    Ok(())
}

/// Index a plain text file as single document per [[RFC-0004:C-FORMATS]].
fn index_text(conn: &Connection, source_dir: &Path, file_path: &Path) -> Result<()> {
    let content = fs::read_to_string(file_path)?;
    let relative_path = file_path
        .strip_prefix(source_dir)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();

    // Section field MUST be empty string for .txt files
    conn.execute(
        "INSERT INTO sections (file, section, content) VALUES (?1, ?2, ?3)",
        params![relative_path, "", content],
    )?;

    Ok(())
}

/// Execute search command per [[RFC-0004:C-SEARCH]].
///
/// Returns formatted output as a string.
pub fn search(skill: &str, query: &str, limit: usize, format: OutputFormat) -> Result<String> {
    let start = Instant::now();

    // Validate query per [[RFC-0004:C-QUERY-SYNTAX]]
    if query.trim().is_empty() {
        return Err(SkillcError::EmptyQuery);
    }

    let resolved = resolve_skill(skill)?;
    let run_id = get_run_id();

    verbose!("search: query=\"{}\" limit={}", query, limit);
    verbose!("search: source_dir={}", resolved.source_dir.display());

    // Initialize logging
    let log_conn = init_log_db(&resolved.runtime_dir);

    let result = do_search(&resolved, query, limit, &format);

    verbose!("search: completed in {:?}", start.elapsed());

    // Log access - extract result count from successful result
    let result_count = match &result {
        Ok((_, count)) => *count,
        Err(_) => 0,
    };

    let args = serde_json::json!({
        "query": query,
        "result_count": result_count,
    });

    // Log access (with automatic fallback for sandboxed environments)
    log_access_with_fallback(
        log_conn.as_ref(),
        &LogEntry {
            run_id,
            command: "search".to_string(),
            skill: resolved.name.clone(),
            skill_path: resolved.source_dir.to_string_lossy().to_string(),
            cwd: get_cwd(),
            args: args.to_string(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result.map(|(output, _)| output)
}

/// Perform the actual search.
/// Returns (output_string, result_count).
fn do_search(
    resolved: &ResolvedSkill,
    query: &str,
    limit: usize,
    format: &OutputFormat,
) -> Result<(String, usize)> {
    let index_path = get_index_path(&resolved.runtime_dir, &resolved.source_dir);

    // Check if index exists
    if !index_path.exists() {
        return Err(SkillcError::IndexUnusable(resolved.name.clone()));
    }

    // Open and validate index
    let conn = Connection::open(&index_path)
        .map_err(|_| SkillcError::IndexUnusable(resolved.name.clone()))?;

    // Check index state (simplified - just check skill_path for collision)
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='index_meta'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .map_err(|_| SkillcError::IndexUnusable(resolved.name.clone()))?;

    if !table_exists {
        return Err(SkillcError::IndexUnusable(resolved.name.clone()));
    }

    // Read skill_path and check for collision
    let stored_skill_path: String = conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'skill_path'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| SkillcError::IndexUnusable(resolved.name.clone()))?;

    let canonical_source = resolved
        .source_dir
        .canonicalize()
        .unwrap_or_else(|_| resolved.source_dir.clone())
        .to_string_lossy()
        .to_string();

    if stored_skill_path != canonical_source {
        let hash16 = compute_hash16(&resolved.source_dir);
        return Err(SkillcError::IndexHashCollision(hash16));
    }

    // Check staleness per [[RFC-0004:C-INDEX]]: source_hash, schema_version, tokenizer
    let stored_hash: String = conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'source_hash'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| SkillcError::IndexUnusable(resolved.name.clone()))?;

    let stored_schema: i32 = conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| SkillcError::IndexUnusable(resolved.name.clone()))?
        .parse()
        .unwrap_or(0);

    let stored_tokenizer: String = conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'tokenizer'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| SkillcError::IndexUnusable(resolved.name.clone()))?;

    // Check schema version
    if stored_schema < SCHEMA_VERSION {
        return Err(SkillcError::IndexUnusable(resolved.name.clone()));
    }

    // Check tokenizer preference (use temp connection to test availability)
    let temp_conn = Connection::open_in_memory()
        .map_err(|_| SkillcError::IndexUnusable(resolved.name.clone()))?;
    let tokenizer_pref = get_tokenizer_preference(&temp_conn);
    drop(temp_conn);
    let current_tokenizer = tokenizer_short_name(&tokenizer_pref);
    if stored_tokenizer != current_tokenizer {
        return Err(SkillcError::IndexUnusable(resolved.name.clone()));
    }

    // Read manifest to get current source_hash
    let manifest_path = resolved
        .runtime_dir
        .join(".skillc-meta")
        .join("manifest.json");
    if manifest_path.exists() {
        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_str(&manifest_content)?;
        if let Some(current_hash) = manifest.get("source_hash").and_then(|v| v.as_str())
            && stored_hash != current_hash
        {
            return Err(SkillcError::IndexUnusable(resolved.name.clone()));
        }
    }

    // Build FTS5 query per [[RFC-0004:C-QUERY-SYNTAX]]
    let fts_query = build_fts_query(query);

    // Execute search
    let mut stmt = conn.prepare(
        "SELECT file, section, snippet(sections, 2, '[MATCH]', '[/MATCH]', '...', 32), bm25(sections)
         FROM sections
         WHERE sections MATCH ?1
         ORDER BY bm25(sections)
         LIMIT ?2",
    )?;

    let results: Vec<SearchResult> = stmt
        .query_map(params![fts_query, limit as i64], |row| {
            Ok(SearchResult {
                file: row.get(0)?,
                section: row.get(1)?,
                snippet: row.get(2)?,
                score: -row.get::<_, f64>(3)?, // Negate BM25 score
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let result_count = results.len();

    // Format output
    let output = match format {
        OutputFormat::Json => {
            let response = SearchResponse {
                query: query.to_string(),
                results,
            };
            serde_json::to_string_pretty(&response)?
        }
        OutputFormat::Text => {
            let is_tty = std::io::stdout().is_terminal();
            let mut lines = Vec::new();
            for result in &results {
                if result.section.is_empty() {
                    lines.push(format!("{} (score: {:.4})", result.file, result.score));
                } else {
                    lines.push(format!(
                        "{}#{} (score: {:.4})",
                        result.file, result.section, result.score
                    ));
                }
                // Render [MATCH]...[/MATCH] as colored text when outputting to TTY
                let snippet = if is_tty {
                    render_match_highlights(&result.snippet)
                } else {
                    // Strip markers for non-TTY output
                    result
                        .snippet
                        .replace("[MATCH]", "")
                        .replace("[/MATCH]", "")
                };
                lines.push(format!("  {}", snippet));
            }
            lines.join("\n")
        }
    };

    Ok((output, result_count))
}

/// Render [MATCH]...[/MATCH] markers as colored text.
fn render_match_highlights(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;

    while let Some(start_idx) = remaining.find("[MATCH]") {
        // Add text before the match
        result.push_str(&remaining[..start_idx]);

        // Find the end marker
        let after_start = &remaining[start_idx + 7..]; // Skip "[MATCH]"
        if let Some(end_idx) = after_start.find("[/MATCH]") {
            // Extract matched text and apply yellow bold styling
            let matched = &after_start[..end_idx];
            result.push_str(&format!("{}", matched.yellow().bold()));
            remaining = &after_start[end_idx + 8..]; // Skip "[/MATCH]"
        } else {
            // No closing marker, just add the rest as-is
            result.push_str(&remaining[start_idx..]);
            break;
        }
    }

    // Add any remaining text
    result.push_str(remaining);
    result
}

/// Build FTS5 query from user input per [[RFC-0004:C-QUERY-SYNTAX]].
fn build_fts_query(query: &str) -> String {
    // Split on ASCII whitespace only
    let tokens: Vec<&str> = query
        .split([' ', '\t', '\n', '\r'])
        .filter(|s| !s.is_empty())
        .collect();

    // Quote each token (escape internal quotes)
    let quoted: Vec<String> = tokens
        .iter()
        .map(|t| {
            let escaped = t.replace('"', "\"\"");
            format!("\"{}\"", escaped)
        })
        .collect();

    quoted.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_fts_query_simple() {
        assert_eq!(
            build_fts_query("configure authentication"),
            "\"configure\" \"authentication\""
        );
    }

    #[test]
    fn test_build_fts_query_with_quotes() {
        assert_eq!(
            build_fts_query("my \"special\" app"),
            "\"my\" \"\"\"special\"\"\" \"app\""
        );
    }

    #[test]
    fn test_build_fts_query_extra_whitespace() {
        assert_eq!(build_fts_query("  hello   world  "), "\"hello\" \"world\"");
    }

    #[test]
    fn test_compute_hash16() {
        let path = PathBuf::from("/tmp/test-skill");
        let hash = compute_hash16(&path);
        assert_eq!(hash.len(), 16);
    }
}
