//! General utilities for skillc.

use crate::error::{Result, SkillcError};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

// =============================================================================
// Skill validation
// =============================================================================

/// Validate that a path is a valid skill directory.
///
/// Returns `Ok(())` if the path is a directory containing SKILL.md.
/// Returns appropriate error codes per [[RFC-0005:C-CODES]]:
/// - E001: Directory does not exist
/// - E010: Directory exists but lacks SKILL.md
pub fn validate_skill_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(SkillcError::SkillNotFound(
            path.to_string_lossy().to_string(),
        ));
    }
    if !path.join("SKILL.md").exists() {
        return Err(SkillcError::NotAValidSkill(
            path.to_string_lossy().to_string(),
        ));
    }
    Ok(())
}

/// Check if a path is a valid skill directory (non-error version).
///
/// Returns `true` if the path is a directory containing SKILL.md.
pub fn is_valid_skill(path: &Path) -> bool {
    path.is_dir() && path.join("SKILL.md").exists()
}

/// Recursively copy a directory and its contents.
///
/// Copies all files and subdirectories from `src` to `dst`.
/// Symlinks and other special file types are skipped.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
        // Skip symlinks and other types
    }

    Ok(())
}

// =============================================================================
// Path helpers for skillc's directory structure
// =============================================================================

/// Get the `.skillc/logs/` directory for a project root.
pub fn project_logs_dir(root: &Path) -> PathBuf {
    root.join(".skillc").join("logs")
}

/// Get the `.skillc/logs/<skill>/` directory for a project root.
pub fn project_skill_logs_dir(root: &Path, skill: &str) -> PathBuf {
    project_logs_dir(root).join(skill)
}

/// Get the `.skillc/runtime/` directory for a project root.
pub fn project_runtime_dir(root: &Path) -> PathBuf {
    root.join(".skillc").join("runtime")
}

/// Get the `.skillc/runtime/<skill>/` directory for a project root.
pub fn project_skill_runtime_dir(root: &Path, skill: &str) -> PathBuf {
    project_runtime_dir(root).join(skill)
}

/// Get the `.skillc/skills/` directory for a project root.
pub fn project_skills_dir(root: &Path) -> PathBuf {
    root.join(".skillc").join("skills")
}

/// Get the `.skillc/skills/<skill>/` directory for a project root.
pub fn project_skill_dir(root: &Path, skill: &str) -> PathBuf {
    project_skills_dir(root).join(skill)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_copy_dir_recursive() {
        let temp = TempDir::new().expect("create temp dir");
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        // Create source structure
        fs::create_dir_all(src.join("subdir")).expect("test operation");
        fs::write(src.join("file.txt"), "content").expect("test operation");
        fs::write(src.join("subdir").join("nested.txt"), "nested").expect("test operation");

        // Copy
        copy_dir_recursive(&src, &dst).expect("copy dir");

        // Verify
        assert!(dst.join("file.txt").exists());
        assert!(dst.join("subdir").join("nested.txt").exists());
        assert_eq!(
            fs::read_to_string(dst.join("file.txt")).expect("read file"),
            "content"
        );
    }

    #[test]
    fn test_project_path_helpers() {
        let root = Path::new("/project");

        assert_eq!(
            project_logs_dir(root),
            PathBuf::from("/project/.skillc/logs")
        );
        assert_eq!(
            project_skill_logs_dir(root, "my-skill"),
            PathBuf::from("/project/.skillc/logs/my-skill")
        );
        assert_eq!(
            project_runtime_dir(root),
            PathBuf::from("/project/.skillc/runtime")
        );
        assert_eq!(
            project_skill_runtime_dir(root, "my-skill"),
            PathBuf::from("/project/.skillc/runtime/my-skill")
        );
        assert_eq!(
            project_skills_dir(root),
            PathBuf::from("/project/.skillc/skills")
        );
        assert_eq!(
            project_skill_dir(root, "my-skill"),
            PathBuf::from("/project/.skillc/skills/my-skill")
        );
    }
}
