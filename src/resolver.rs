//! Skill resolution per [[RFC-0007:C-RESOLUTION]]

use crate::config::{find_project_skill, global_runtime_store, global_source_store};
use crate::error::{Result, SkillcError};
use crate::verbose;
use std::env;
use std::path::PathBuf;

/// Resolved skill paths
#[derive(Debug)]
pub struct ResolvedSkill {
    /// Skill name (directory basename)
    pub name: String,
    /// Absolute path to source directory
    pub source_dir: PathBuf,
    /// Absolute path to runtime directory (for logging)
    pub runtime_dir: PathBuf,
}

/// Resolve a skill argument to source and runtime directories.
///
/// Query commands (show, open, search, outline, stats) use name-based resolution:
/// 1. Check project source store (`.skillc/skills/<skill>/`)
/// 2. Check global source store (`~/.skillc/skills/<skill>/`)
/// 3. Check global runtime store (`~/.claude/skills/<skill>/`) as fallback
/// 4. If not found, exit with error
///
/// Note: Direct paths are NOT supported for query commands. Use `skc build <path>`
/// to import a skill first, then query by name.
///
/// Per [[RFC-0005:C-CODES]] resolution error hierarchy:
/// - E010: A directory was found but it lacks SKILL.md
/// - E001: No directory was found at all
pub fn resolve_skill(skill: &str) -> Result<ResolvedSkill> {
    verbose!("resolving skill: {}", skill);

    // Skill must be a name, not a path
    if skill.contains('/') || skill.contains('\\') {
        return Err(SkillcError::SkillNotFound(format!(
            "{} (use skill name, not path; run 'skc build <path>' to import first)",
            skill
        )));
    }

    // Try project source store first
    if let Some(project_source) = try_project_source_store(skill)? {
        verbose!("  resolved via project store: {}", project_source.display());
        return finish_resolve(skill, project_source);
    }

    // Try global source store
    let global_path = global_source_store()?.join(skill);
    verbose!("  checking global source store: {}", global_path.display());
    if crate::util::is_valid_skill(&global_path) {
        let source_dir = global_path.canonicalize()?;
        verbose!(
            "  resolved via global source store: {}",
            source_dir.display()
        );
        return finish_resolve(skill, source_dir);
    } else if global_path.exists() {
        // E010: Directory exists but lacks SKILL.md
        return Err(SkillcError::NotAValidSkill(
            global_path.to_string_lossy().to_string(),
        ));
    }

    // Try global runtime store as fallback
    // This supports skills that live directly in ~/.claude/skills/ without
    // a separate source store, which is common for user-authored skills.
    let runtime_path = global_runtime_store()?.join(skill);
    verbose!(
        "  checking global runtime store: {}",
        runtime_path.display()
    );
    if crate::util::is_valid_skill(&runtime_path) {
        let source_dir = runtime_path.canonicalize()?;
        verbose!(
            "  resolved via global runtime store: {}",
            source_dir.display()
        );
        return finish_resolve(skill, source_dir);
    } else if runtime_path.exists() {
        // E010: Directory exists but lacks SKILL.md
        return Err(SkillcError::NotAValidSkill(
            runtime_path.to_string_lossy().to_string(),
        ));
    }

    // E001: No directory found at all
    Err(SkillcError::SkillNotFound(skill.to_string()))
}

/// Complete resolution with source_dir already determined.
fn finish_resolve(_skill: &str, source_dir: PathBuf) -> Result<ResolvedSkill> {
    // Extract skill name from source directory basename
    let name = source_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| SkillcError::InvalidPath("Cannot extract skill name".to_string()))?
        .to_string();

    // Resolve runtime directory based on where the source was found
    let runtime_dir = resolve_runtime_dir_for_source(&name, &source_dir)?;

    Ok(ResolvedSkill {
        name,
        source_dir,
        runtime_dir,
    })
}

/// Try to find skill in project source store using recursive-up search.
/// Returns None if no project exists or skill not found there.
///
/// Per ADR-0002, project source store is `{project}/.skillc/skills/{skill}/`
/// where `{project}` is found by walking up from CWD.
fn try_project_source_store(skill: &str) -> Result<Option<PathBuf>> {
    // Skill must be a name, not a path (no slashes allowed)
    if skill.contains('/') || skill.contains('\\') {
        return Ok(None);
    }

    // Use recursive-up search from config module
    if let Some((skill_path, _project_root)) = find_project_skill(skill) {
        return Ok(Some(skill_path.canonicalize()?));
    }

    Ok(None)
}

/// Resolve runtime directory based on where the source was found.
///
/// This ensures the runtime dir context matches the source dir context:
/// - If source is in a project source store, use that project's runtime store
/// - If source is in global source store, use global runtime store
/// - Otherwise, fall back to CWD-based resolution
fn resolve_runtime_dir_for_source(
    skill_name: &str,
    source_dir: &std::path::Path,
) -> Result<PathBuf> {
    // Check if source is in global source store (~/.skillc/skills/)
    let global_source = global_source_store()?;
    if source_dir.starts_with(&global_source) {
        // Source is global → use global runtime store
        return Ok(global_runtime_store()?.join(skill_name));
    }

    // Check if source is in a project source store ({project}/.skillc/skills/)
    // Walk up from source to find the project root
    let mut current = source_dir.parent();
    while let Some(dir) = current {
        // Check for .skillc/skills pattern
        if dir.file_name().is_some_and(|n| n == "skills")
            && let Some(skillc_dir) = dir.parent()
            && skillc_dir.file_name().is_some_and(|n| n == ".skillc")
            && let Some(project_root) = skillc_dir.parent()
        {
            // Found project root → use project runtime store
            return Ok(crate::util::project_skill_runtime_dir(
                project_root,
                skill_name,
            ));
        }
        current = dir.parent();
    }

    // Fallback: use CWD-based resolution (for direct paths)
    resolve_runtime_dir_from_cwd(skill_name)
}

/// Fallback: Resolve runtime directory based on CWD context.
fn resolve_runtime_dir_from_cwd(skill_name: &str) -> Result<PathBuf> {
    let cwd = env::current_dir()?;
    let project_config = cwd.join(".skillc").join("config.toml");

    if project_config.exists() {
        // Use project runtime store
        Ok(crate::util::project_skill_runtime_dir(&cwd, skill_name))
    } else {
        // Use global runtime store
        Ok(global_runtime_store()?.join(skill_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_skill_rejects_paths() {
        // Direct paths should be rejected - use 'skc build <path>' to import first
        let temp = TempDir::new().expect("failed to create temp dir");
        let skill_dir = temp.path().join("my-skill");
        fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: test\n---\n",
        )
        .expect("failed to write SKILL.md");

        let result = resolve_skill(skill_dir.to_str().expect("path should be valid UTF-8"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), crate::error::ErrorCode::E001);
        assert!(err.to_string().contains("use skill name, not path"));
    }

    #[test]
    fn test_resolve_skill_not_found() {
        let temp = TempDir::new().expect("failed to create temp dir");
        temp_env::with_var("SKILLC_HOME", Some(temp.path()), || {
            let result = resolve_skill("nonexistent-skill-12345");
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert_eq!(err.code(), crate::error::ErrorCode::E001);
        });
    }

    #[test]
    fn test_resolve_skill_rejects_relative_paths() {
        let temp = TempDir::new().expect("failed to create temp dir");
        temp_env::with_var("SKILLC_HOME", Some(temp.path()), || {
            // Even relative paths with slashes should be rejected
            let result = resolve_skill("./my-skill");
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert_eq!(err.code(), crate::error::ErrorCode::E001);
            assert!(err.to_string().contains("use skill name, not path"));
        });
    }

    #[test]
    fn test_try_project_source_store_no_project() {
        let temp = TempDir::new().expect("failed to create temp dir");
        temp_env::with_var("SKILLC_HOME", Some(temp.path()), || {
            // When no .skillc/skills/ exists, should return None
            let result = try_project_source_store("nonexistent").expect("test operation");
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_try_project_source_store_rejects_paths() {
        let temp = TempDir::new().expect("failed to create temp dir");
        temp_env::with_var("SKILLC_HOME", Some(temp.path()), || {
            // Paths should return None (not be interpreted as skill names)
            let result = try_project_source_store("/some/path").expect("test operation");
            assert!(result.is_none());

            let result = try_project_source_store("./relative").expect("test operation");
            assert!(result.is_none());
        });
    }
}
