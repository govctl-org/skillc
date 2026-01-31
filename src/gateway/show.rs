//! Show command per [[RFC-0002:C-SHOW]]

use crate::config::get_cwd;
use crate::error::{Result, SkillcError};
use crate::index;
use crate::logging::{LogEntry, get_run_id, init_log_db, log_access_with_fallback};
use crate::resolver::{ResolvedSkill, resolve_skill};
use crate::{OutputFormat, verbose};
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use super::extract_headings;

/// Execute the show command per [[RFC-0002:C-SHOW]].
///
/// Locates the specified heading and returns its content.
///
/// The `max_lines` parameter truncates output to the first n lines if specified.
pub fn show(
    skill: &str,
    section: &str,
    file: Option<&str>,
    max_lines: Option<usize>,
    format: OutputFormat,
) -> Result<String> {
    let start = Instant::now();
    let resolved = resolve_skill(skill)?;
    let run_id = get_run_id();

    verbose!(
        "show: section=\"{}\" file={:?} max_lines={:?}",
        section,
        file,
        max_lines
    );
    verbose!("show: source_dir={}", resolved.source_dir.display());

    // Initialize logging
    let log_conn = init_log_db(&resolved.runtime_dir);

    let result = do_show(&resolved, section, file, max_lines, &format);

    verbose!("show: completed in {:?}", start.elapsed());

    // Log the matched file (from successful result) or input file (on error)
    let args = match &result {
        Ok((_, matched_file)) => serde_json::json!({
            "section": section,
            "file": matched_file.to_string_lossy(),
            "max_lines": max_lines,
        }),
        Err(_) => serde_json::json!({
            "section": section,
            "file": file,
            "max_lines": max_lines,
        }),
    };

    // Log access (with automatic fallback for sandboxed environments)
    log_access_with_fallback(
        log_conn.as_ref(),
        &LogEntry {
            run_id,
            command: "show".to_string(),
            skill: resolved.name.clone(),
            skill_path: resolved.source_dir.to_string_lossy().to_string(),
            cwd: get_cwd(),
            args: args.to_string(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result.map(|(content, _)| content)
}

/// Normalize query per [[RFC-0002:C-SHOW]].
///
/// Strips em-dash suffix (e.g., "Title — description" → "Title").
fn normalize_query(query: &str) -> String {
    let trimmed = query.trim();
    if let Some(idx) = trimmed.find(" — ") {
        trimmed[..idx].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Format suggestions for section not found error per [[RFC-0002:C-SHOW]].
fn format_suggestions(suggestions: &[index::HeadingEntry]) -> String {
    if suggestions.is_empty() {
        return String::new();
    }
    let mut result = String::from("\n\nDid you mean one of these?");
    for entry in suggestions.iter().take(5) {
        result.push_str(&format!("\n  - {} ({})", entry.text, entry.file));
    }
    result
}

/// Returns (content, matched_file_path)
///
/// Uses index-based lookup per [[RFC-0002:C-SHOW]] with fallback to runtime parsing.
fn do_show(
    resolved: &ResolvedSkill,
    section: &str,
    file: Option<&str>,
    max_lines: Option<usize>,
    _format: &OutputFormat,
) -> Result<(String, PathBuf)> {
    // Normalize query (strip em-dash suffix)
    let query = normalize_query(section);
    verbose!("show: normalized query=\"{}\"", query);

    // Try index-based lookup first
    match index::open_index(&resolved.runtime_dir, &resolved.source_dir, &resolved.name) {
        Ok(conn) => do_show_with_index(&conn, resolved, &query, section, file, max_lines),
        Err(_) => {
            // Fallback to runtime parsing for unbuilt skills
            verbose!("show: index unavailable, falling back to runtime parsing");
            do_show_fallback(resolved, &query, file, max_lines)
        }
    }
}

/// Index-based show implementation per [[RFC-0002:C-SHOW]].
fn do_show_with_index(
    conn: &Connection,
    resolved: &ResolvedSkill,
    query: &str,
    original_section: &str,
    file: Option<&str>,
    max_lines: Option<usize>,
) -> Result<(String, PathBuf)> {
    let matches = index::query_headings(conn, query, file)?;

    if matches.is_empty() {
        // Get suggestions for error message
        let suggestions = index::get_suggestions(conn, query, 5)?;
        let suggestion_text = format_suggestions(&suggestions);
        return Err(SkillcError::SectionNotFoundWithSuggestions(
            original_section.to_string(),
            suggestion_text,
        ));
    }

    // Warn if multiple matches (W001 per [[RFC-0005:C-CODES]])
    if matches.len() > 1 {
        crate::error::SkillcWarning::MultipleMatches(original_section.to_string()).emit();
    }

    let matched = &matches[0];
    let file_path = resolved.source_dir.join(&matched.file);
    let content = fs::read_to_string(&file_path)?;
    let lines: Vec<&str> = content.lines().collect();

    // Use pre-computed line range from index (1-based, end is exclusive)
    let start_idx = matched.start_line.saturating_sub(1);
    let end_idx = (matched.end_line.saturating_sub(1)).min(lines.len());
    let content_lines: Vec<&str> = lines[start_idx..end_idx].to_vec();

    extract_output(content_lines, max_lines, PathBuf::from(&matched.file))
}

/// Fallback show implementation using runtime parsing.
/// Used when index is not available (skill not built).
fn do_show_fallback(
    resolved: &ResolvedSkill,
    query: &str,
    file: Option<&str>,
    max_lines: Option<usize>,
) -> Result<(String, PathBuf)> {
    use lazy_regex::{Lazy, Regex, lazy_regex};

    /// Regex for detecting heading lines (level only).
    static HEADING_LEVEL_RE: Lazy<Regex> = lazy_regex!(r"^(#{1,6})\s+");

    let query_lower = query.to_lowercase();
    let headings = extract_headings(&resolved.source_dir)?;

    // Filter by file if specified
    let filtered: Vec<_> = if let Some(file_path) = file {
        let target = PathBuf::from(file_path);
        headings.into_iter().filter(|h| h.file == target).collect()
    } else {
        headings
    };

    // Find matching heading (case-insensitive, trimmed)
    let matches: Vec<_> = filtered
        .iter()
        .filter(|h| h.text.trim().to_lowercase() == query_lower)
        .collect();

    if matches.is_empty() {
        return Err(SkillcError::SectionNotFound(query.to_string()));
    }

    // Warn if multiple matches (W001 per [[RFC-0005:C-CODES]])
    if matches.len() > 1 {
        crate::error::SkillcWarning::MultipleMatches(query.to_string()).emit();
    }

    let matched = matches[0];
    let file_path = resolved.source_dir.join(&matched.file);
    let content = fs::read_to_string(&file_path)?;
    let lines: Vec<&str> = content.lines().collect();

    // Extract content from heading to next heading of equal or higher level
    let start_line = matched.line_number;
    let mut end_line = lines.len();

    for (i, line) in lines.iter().enumerate().skip(start_line) {
        if let Some(caps) = HEADING_LEVEL_RE.captures(line) {
            let level = caps
                .get(1)
                .ok_or_else(|| SkillcError::Internal("regex group 1 missing".into()))?
                .as_str()
                .len();
            if level <= matched.level {
                end_line = i;
                break;
            }
        }
    }

    let content_lines: Vec<&str> = lines[start_line - 1..end_line].to_vec();
    extract_output(content_lines, max_lines, matched.file.clone())
}

/// Format output with optional truncation.
fn extract_output(
    content_lines: Vec<&str>,
    max_lines: Option<usize>,
    matched_file: PathBuf,
) -> Result<(String, PathBuf)> {
    let output = if let Some(limit) = max_lines {
        if content_lines.len() > limit {
            let truncated: Vec<&str> = content_lines[..limit].to_vec();
            let remaining = content_lines.len() - limit;
            format!("{}\n... ({} more lines)", truncated.join("\n"), remaining)
        } else {
            content_lines.join("\n")
        }
    } else {
        content_lines.join("\n")
    };

    Ok((output, matched_file))
}
