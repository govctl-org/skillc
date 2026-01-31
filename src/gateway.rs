//! Gateway commands per RFC-0002

use crate::config::get_cwd;
use crate::error::{Result, SkillcError};
use crate::index;
use crate::logging::{LogEntry, get_run_id, init_log_db, log_access_with_fallback};
use crate::resolver::{ResolvedSkill, resolve_skill};
use crate::{Heading, OutputFormat, markdown, verbose};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

/// Execute the outline command per [[RFC-0002:C-OUTLINE]].
///
/// Scans all .md files and extracts headings, sorted lexicographically by path.
/// Returns formatted output as a string.
///
/// The `max_level` parameter filters headings to include only those with level ≤ max_level.
pub fn outline(skill: &str, max_level: Option<usize>, format: OutputFormat) -> Result<String> {
    let start = Instant::now();
    let resolved = resolve_skill(skill)?;
    let run_id = get_run_id();

    verbose!(
        "outline: source_dir={} max_level={:?}",
        resolved.source_dir.display(),
        max_level
    );

    // Initialize logging (continue even if it fails)
    let log_conn = init_log_db(&resolved.runtime_dir);

    let args = serde_json::json!({ "level": max_level });
    let result = do_outline(&resolved, max_level, &format);

    verbose!("outline: completed in {:?}", start.elapsed());

    // Log access (with automatic fallback for sandboxed environments)
    log_access_with_fallback(
        log_conn.as_ref(),
        &LogEntry {
            run_id,
            command: "outline".to_string(),
            skill: resolved.name.clone(),
            skill_path: resolved.source_dir.to_string_lossy().to_string(),
            cwd: get_cwd(),
            args: args.to_string(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result
}

fn do_outline(
    resolved: &ResolvedSkill,
    max_level: Option<usize>,
    format: &OutputFormat,
) -> Result<String> {
    let mut headings = extract_headings(&resolved.source_dir)?;

    // Filter by max level if specified
    if let Some(level) = max_level {
        headings.retain(|h| h.level <= level);
    }

    match format {
        OutputFormat::Json => {
            let json_headings: Vec<_> = headings
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "level": h.level,
                        "heading": h.text,
                        "file": h.file.to_string_lossy()
                    })
                })
                .collect();
            Ok(serde_json::to_string_pretty(&json_headings)?)
        }
        OutputFormat::Text => {
            let mut output = String::new();
            let mut current_file: Option<&PathBuf> = None;

            for heading in &headings {
                if current_file != Some(&heading.file) {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&heading.file.display().to_string());
                    output.push('\n');
                    current_file = Some(&heading.file);
                }

                // Indent based on level
                let indent = "  ".repeat(heading.level);
                let hashes = "#".repeat(heading.level);
                output.push_str(&format!("{}{} {}\n", indent, hashes, heading.text));
            }

            // Remove trailing newline if present
            if output.ends_with('\n') {
                output.pop();
            }

            Ok(output)
        }
    }
}

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

/// Execute the open command per [[RFC-0002:C-OPEN]].
///
/// Returns the contents of the specified file.
///
/// The `max_lines` parameter truncates output to the first n lines if specified.
pub fn open(
    skill: &str,
    path: &str,
    max_lines: Option<usize>,
    format: OutputFormat,
) -> Result<String> {
    let start = Instant::now();
    let resolved = resolve_skill(skill)?;
    let run_id = get_run_id();

    verbose!("open: path=\"{}\" max_lines={:?}", path, max_lines);
    verbose!("open: source_dir={}", resolved.source_dir.display());

    // Initialize logging
    let log_conn = init_log_db(&resolved.runtime_dir);

    let args = serde_json::json!({ "path": path, "max_lines": max_lines });

    let result = do_open(&resolved, path, max_lines, &format);

    verbose!("open: completed in {:?}", start.elapsed());

    // Log access (with automatic fallback for sandboxed environments)
    log_access_with_fallback(
        log_conn.as_ref(),
        &LogEntry {
            run_id,
            command: "open".to_string(),
            skill: resolved.name.clone(),
            skill_path: resolved.source_dir.to_string_lossy().to_string(),
            cwd: get_cwd(),
            args: args.to_string(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result
}

fn do_open(
    resolved: &ResolvedSkill,
    path: &str,
    max_lines: Option<usize>,
    _format: &OutputFormat,
) -> Result<String> {
    // Validate path doesn't escape skill root
    if path.contains("..") {
        // Check if it actually escapes after canonicalization
        let full_path = resolved.source_dir.join(path);
        if let Ok(canonical) = full_path.canonicalize() {
            if !canonical.starts_with(&resolved.source_dir) {
                return Err(SkillcError::PathEscapesRoot(path.to_string()));
            }
        } else {
            return Err(SkillcError::PathEscapesRoot(path.to_string()));
        }
    }

    let file_path = resolved.source_dir.join(path);

    // Validate path after canonicalization
    if file_path.exists() {
        let canonical = file_path.canonicalize()?;
        if !canonical.starts_with(&resolved.source_dir) {
            return Err(SkillcError::PathEscapesRoot(path.to_string()));
        }
    }

    if !file_path.exists() {
        return Err(SkillcError::FileNotFound(path.to_string()));
    }

    if file_path.is_dir() {
        return Err(SkillcError::InvalidPath(
            "Path must be a file, not a directory".to_string(),
        ));
    }

    let content = fs::read_to_string(&file_path)?;

    // Apply max_lines truncation if specified
    if let Some(limit) = max_lines {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() > limit {
            let truncated: Vec<&str> = lines[..limit].to_vec();
            let remaining = lines.len() - limit;
            return Ok(format!(
                "{}\n... ({} more lines)",
                truncated.join("\n"),
                remaining
            ));
        }
    }

    Ok(content)
}

/// Execute the sources command per [[RFC-0002:C-SOURCES]].
///
/// Lists source files in a tree-style format or JSON.
pub fn sources(
    skill: &str,
    depth: Option<usize>,
    dir: Option<&str>,
    limit: usize,
    pattern: Option<&str>,
    format: OutputFormat,
) -> Result<String> {
    let start = Instant::now();
    let resolved = resolve_skill(skill)?;
    let run_id = get_run_id();

    verbose!(
        "sources: skill={} depth={:?} dir={:?} limit={} pattern={:?}",
        skill,
        depth,
        dir,
        limit,
        pattern
    );

    // Initialize logging
    let log_conn = init_log_db(&resolved.runtime_dir);

    let args = serde_json::json!({
        "depth": depth,
        "dir": dir,
        "limit": limit,
        "pattern": pattern,
    });

    let result = do_sources(&resolved, depth, dir, limit, pattern, &format);

    verbose!("sources: completed in {:?}", start.elapsed());

    // Log access (with automatic fallback for sandboxed environments)
    log_access_with_fallback(
        log_conn.as_ref(),
        &LogEntry {
            run_id,
            command: "sources".to_string(),
            skill: resolved.name.clone(),
            skill_path: resolved.source_dir.to_string_lossy().to_string(),
            cwd: get_cwd(),
            args: args.to_string(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result
}

fn do_sources(
    resolved: &ResolvedSkill,
    max_depth: Option<usize>,
    subdir: Option<&str>,
    limit: usize,
    pattern: Option<&str>,
    format: &OutputFormat,
) -> Result<String> {
    // Determine root directory (skill root or subdirectory)
    let root = if let Some(dir_path) = subdir {
        // Path safety check
        if dir_path.contains("..") {
            let full_path = resolved.source_dir.join(dir_path);
            if let Ok(canonical) = full_path.canonicalize() {
                if !canonical.starts_with(&resolved.source_dir) {
                    return Err(SkillcError::PathEscapesRoot(dir_path.to_string()));
                }
            } else {
                return Err(SkillcError::DirectoryNotFound(dir_path.to_string()));
            }
        }

        let dir_full = resolved.source_dir.join(dir_path);
        if !dir_full.exists() {
            return Err(SkillcError::DirectoryNotFound(dir_path.to_string()));
        }
        if !dir_full.is_dir() {
            return Err(SkillcError::InvalidPath(format!(
                "{} is not a directory",
                dir_path
            )));
        }

        // Validate after canonicalization
        let canonical = dir_full.canonicalize()?;
        if !canonical.starts_with(&resolved.source_dir) {
            return Err(SkillcError::PathEscapesRoot(dir_path.to_string()));
        }

        dir_full
    } else {
        resolved.source_dir.clone()
    };

    // Compile glob pattern if provided
    let glob_pattern = pattern
        .map(glob::Pattern::new)
        .transpose()
        .map_err(|e| SkillcError::InvalidPath(format!("invalid glob pattern: {}", e)))?;

    match format {
        OutputFormat::Json => {
            // JSON format: flat list of entries
            let mut entries = Vec::new();
            let mut count = 0;

            for entry in WalkDir::new(&root)
                .min_depth(1)
                .max_depth(max_depth.unwrap_or(usize::MAX))
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if count >= limit {
                    break;
                }

                let rel_path = entry
                    .path()
                    .strip_prefix(&resolved.source_dir)
                    .unwrap_or(entry.path());

                // Skip hidden files
                if rel_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.starts_with('.'))
                    .unwrap_or(false)
                {
                    continue;
                }

                // Apply glob filter if specified
                if let Some(ref pat) = glob_pattern
                    && let Some(name) = rel_path.file_name().and_then(|n| n.to_str())
                    && !pat.matches(name)
                {
                    continue;
                }

                let entry_type = if entry.file_type().is_dir() {
                    "dir"
                } else {
                    "file"
                };

                entries.push(serde_json::json!({
                    "path": rel_path.to_string_lossy(),
                    "type": entry_type
                }));
                count += 1;
            }

            Ok(serde_json::to_string_pretty(&entries)?)
        }
        OutputFormat::Text => {
            // Text format: tree display
            let entries =
                collect_tree_entries(&root, &resolved.source_dir, max_depth, &glob_pattern)?;
            Ok(format_tree(&resolved.name, &entries, limit))
        }
    }
}

/// A tree entry for display
#[derive(Debug)]
struct TreeEntry {
    /// Relative path from skill root
    path: PathBuf,
    /// Depth in tree (0 = root level)
    depth: usize,
    /// Is this a directory?
    is_dir: bool,
    /// For unexpanded directories, count of files inside
    file_count: Option<usize>,
    /// Is this the last entry at its level?
    is_last: bool,
}

/// Collect tree entries, respecting depth limit and glob pattern
fn collect_tree_entries(
    root: &Path,
    skill_root: &Path,
    max_depth: Option<usize>,
    pattern: &Option<glob::Pattern>,
) -> Result<Vec<TreeEntry>> {
    let mut entries = Vec::new();
    collect_entries_recursive(root, skill_root, 0, max_depth, pattern, &mut entries)?;

    // Mark last entries at each depth level
    mark_last_entries(&mut entries);

    Ok(entries)
}

fn collect_entries_recursive(
    dir: &Path,
    skill_root: &Path,
    current_depth: usize,
    max_depth: Option<usize>,
    pattern: &Option<glob::Pattern>,
    entries: &mut Vec<TreeEntry>,
) -> Result<()> {
    // Read directory entries
    let mut dir_entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            // Skip hidden files/directories
            !e.file_name().to_string_lossy().starts_with('.')
        })
        .collect();

    // Sort: directories first, then lexicographically by name
    dir_entries.sort_by(|a, b| {
        let a_is_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_is_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });

    for entry in dir_entries {
        let path = entry.path();
        let relative = path.strip_prefix(skill_root).unwrap_or(&path).to_path_buf();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        // Apply glob filter (only to files, or to dir names)
        if let Some(pat) = pattern
            && !is_dir
        {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !pat.matches(file_name) {
                continue;
            }
        }

        if is_dir {
            // Check if we should expand this directory
            let should_expand = max_depth.map(|d| current_depth < d).unwrap_or(true);

            if should_expand {
                // Add directory entry
                entries.push(TreeEntry {
                    path: relative.clone(),
                    depth: current_depth,
                    is_dir: true,
                    file_count: None,
                    is_last: false,
                });
                // Recurse
                collect_entries_recursive(
                    &path,
                    skill_root,
                    current_depth + 1,
                    max_depth,
                    pattern,
                    entries,
                )?;
            } else {
                // Count files in unexpanded directory
                let count = count_files_in_dir(&path, pattern)?;
                entries.push(TreeEntry {
                    path: relative,
                    depth: current_depth,
                    is_dir: true,
                    file_count: Some(count),
                    is_last: false,
                });
            }
        } else {
            entries.push(TreeEntry {
                path: relative,
                depth: current_depth,
                is_dir: false,
                file_count: None,
                is_last: false,
            });
        }
    }

    Ok(())
}

/// Count files in a directory (recursively), respecting glob pattern
fn count_files_in_dir(dir: &Path, pattern: &Option<glob::Pattern>) -> Result<usize> {
    let mut count = 0;
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            // Skip hidden files
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // Apply pattern filter
            if let Some(pat) = pattern {
                let file_name = entry.file_name().to_string_lossy();
                if !pat.matches(&file_name) {
                    continue;
                }
            }
            count += 1;
        }
    }
    Ok(count)
}

/// Mark the last entry at each depth level
fn mark_last_entries(entries: &mut [TreeEntry]) {
    if entries.is_empty() {
        return;
    }

    // For each depth, find the actual last entry considering hierarchy
    for i in (0..entries.len()).rev() {
        let depth = entries[i].depth;
        // Check if there are any more entries at this depth after this one
        // that are at the same parent level
        let mut is_last = true;
        for subsequent in entries.iter().skip(i + 1) {
            if subsequent.depth == depth {
                is_last = false;
                break;
            }
            if subsequent.depth < depth {
                // We've exited this subtree
                break;
            }
        }
        entries[i].is_last = is_last;
    }
}

/// Format tree with box-drawing characters, returning a string
fn format_tree(skill_name: &str, entries: &[TreeEntry], limit: usize) -> String {
    let mut output = format!("{}/\n", skill_name);
    let mut ancestors_last: Vec<bool> = Vec::new();

    for (printed, entry) in entries.iter().enumerate() {
        if printed >= limit {
            let remaining = entries.len() - printed;
            if remaining > 0 {
                output.push_str(&format!("... ({} more)\n", remaining));
            }
            break;
        }

        // Adjust ancestors_last to current depth
        while ancestors_last.len() > entry.depth {
            ancestors_last.pop();
        }

        // Build prefix
        let mut prefix = String::new();
        for &ancestor_is_last in &ancestors_last {
            if ancestor_is_last {
                prefix.push_str("    ");
            } else {
                prefix.push_str("│   ");
            }
        }

        // Add branch character
        if entry.is_last {
            prefix.push_str("└── ");
        } else {
            prefix.push_str("├── ");
        }

        // Format entry
        let name = entry
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");

        if entry.is_dir {
            if let Some(count) = entry.file_count {
                output.push_str(&format!("{}{name}/ ({count} files)\n", prefix));
            } else {
                output.push_str(&format!("{}{name}/\n", prefix));
            }
        } else {
            output.push_str(&format!("{}{name}\n", prefix));
        }

        // Update ancestors for next iteration
        if entry.is_dir && entry.file_count.is_none() {
            // This directory is expanded, add to ancestors
            while ancestors_last.len() < entry.depth {
                ancestors_last.push(false);
            }
            ancestors_last.push(entry.is_last);
        }
    }

    // Remove trailing newline
    if output.ends_with('\n') {
        output.pop();
    }

    output
}

/// Extract headings from all .md files, sorted lexicographically by path.
///
/// Uses AST-based parsing to correctly skip headings inside code blocks.
fn extract_headings(source_dir: &Path) -> Result<Vec<Heading>> {
    let mut headings = Vec::new();

    // Collect all .md files
    let mut md_files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let relative = entry
            .path()
            .strip_prefix(source_dir)
            .map_err(|_| SkillcError::Internal("path does not start with source_dir".into()))?
            .to_path_buf();
        md_files.push(relative);
    }

    // Sort lexicographically by path (bytewise ASCII order)
    md_files.sort();

    for file in md_files {
        let full_path = source_dir.join(&file);
        let content = fs::read_to_string(&full_path)?;

        // Use AST-based extraction to skip code blocks
        for extracted in markdown::extract_headings(&content) {
            headings.push(Heading {
                level: extracted.level,
                text: extracted.text,
                file: file.clone(),
                line_number: extracted.line,
            });
        }
    }

    Ok(headings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_skill() -> TempDir {
        let temp = TempDir::new().expect("failed to create temp dir");
        let skill_dir = temp.path();

        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: test-skill
description: A test skill
---

# Test Skill

## Getting Started

This is the getting started section.

### Prerequisites

You need these things.

## API Reference

API docs here.
"#,
        )
        .expect("failed to write SKILL.md");

        fs::create_dir_all(skill_dir.join("docs")).expect("failed to create docs dir");
        fs::write(
            skill_dir.join("docs").join("advanced.md"),
            r#"# Advanced Topics

## Performance

Performance tips here.
"#,
        )
        .expect("failed to write advanced.md");

        temp
    }

    #[test]
    fn test_extract_headings_sorted() {
        let temp = setup_test_skill();
        let headings = extract_headings(temp.path()).expect("failed to extract headings");

        // Should be sorted: SKILL.md before docs/advanced.md
        assert!(!headings.is_empty());
        assert_eq!(headings[0].file, PathBuf::from("SKILL.md"));
    }

    #[test]
    fn test_extract_headings_levels() {
        let temp = setup_test_skill();
        let headings = extract_headings(temp.path()).expect("failed to extract headings");

        // Check first few headings
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[0].text, "Test Skill");
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[1].text, "Getting Started");
    }

    #[test]
    fn test_format_tree_output() {
        let entries = vec![
            TreeEntry {
                path: PathBuf::from("docs"),
                depth: 0,
                is_dir: true,
                file_count: None,
                is_last: false,
            },
            TreeEntry {
                path: PathBuf::from("docs/guide.md"),
                depth: 1,
                is_dir: false,
                file_count: None,
                is_last: true,
            },
            TreeEntry {
                path: PathBuf::from("README.md"),
                depth: 0,
                is_dir: false,
                file_count: None,
                is_last: true,
            },
        ];

        let output = format_tree("my-skill", &entries, 100);
        assert!(output.contains("my-skill/"));
        assert!(output.contains("docs"));
        assert!(output.contains("guide.md"));
        assert!(output.contains("README.md"));
    }

    #[test]
    fn test_format_tree_with_limit() {
        let entries: Vec<TreeEntry> = (0..10)
            .map(|i| TreeEntry {
                path: PathBuf::from(format!("file{}.md", i)),
                depth: 0,
                is_dir: false,
                file_count: None,
                is_last: i == 9,
            })
            .collect();

        let output = format_tree("skill", &entries, 3);
        assert!(output.contains("file0.md"));
        assert!(output.contains("file1.md"));
        assert!(output.contains("file2.md"));
        assert!(output.contains("... (7 more)"));
        assert!(!output.contains("file9.md"));
    }

    #[test]
    fn test_mark_last_entries() {
        let mut entries = vec![
            TreeEntry {
                path: PathBuf::from("a"),
                depth: 0,
                is_dir: false,
                file_count: None,
                is_last: false,
            },
            TreeEntry {
                path: PathBuf::from("b"),
                depth: 0,
                is_dir: false,
                file_count: None,
                is_last: false,
            },
            TreeEntry {
                path: PathBuf::from("c"),
                depth: 0,
                is_dir: false,
                file_count: None,
                is_last: false,
            },
        ];

        mark_last_entries(&mut entries);
        assert!(!entries[0].is_last);
        assert!(!entries[1].is_last);
        assert!(entries[2].is_last);
    }

    #[test]
    fn test_count_files_in_dir() {
        let temp = setup_test_skill();

        let count = count_files_in_dir(temp.path(), &None).expect("failed to count files");
        assert!(count >= 2); // At least SKILL.md and docs/advanced.md
    }

    #[test]
    fn test_count_files_with_pattern() {
        let temp = setup_test_skill();
        let pattern = glob::Pattern::new("*.md").expect("invalid pattern");

        let count = count_files_in_dir(temp.path(), &Some(pattern)).expect("failed to count files");
        assert!(count >= 2); // .md files only
    }
}
