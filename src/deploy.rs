//! Cross-platform deployment for compiled skills per [[RFC-0001:C-DEPLOYMENT]]
//!
//! This module handles deploying compiled skills from SSOT locations to agent directories
//! using symlinks (Unix), junctions (Windows), or copies (fallback).

use crate::Result;
use crate::config::{TargetSpec, ensure_dir};
use crate::error::SkillcError;
use std::path::{Path, PathBuf};

/// Deployment method used to create agent directory entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployMethod {
    /// Unix symlink
    Symlink,
    /// Windows directory junction
    Junction,
    /// Full directory copy (fallback or forced)
    Copy,
}

impl std::fmt::Display for DeployMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployMethod::Symlink => write!(f, "symlink"),
            DeployMethod::Junction => write!(f, "junction"),
            DeployMethod::Copy => write!(f, "copy"),
        }
    }
}

/// Result of a deployment operation
#[derive(Debug)]
pub struct DeployResult {
    /// Path to the deployed skill in agent directory
    pub target: PathBuf,
    /// Method used for deployment
    pub method: DeployMethod,
}

/// Deploy a compiled skill to an agent directory.
///
/// # Arguments
///
/// * `ssot_path` - Path to the compiled skill in SSOT location
/// * `target` - Target agent (known target or custom path)
/// * `skill_name` - Name of the skill
/// * `force_copy` - If true, always copy instead of linking
/// * `project_root` - If Some and target is known, deploy to project-local agent dir
///
/// # Returns
///
/// `DeployResult` containing the target path and method used.
///
/// # Errors
///
/// Returns error if:
/// - SSOT path doesn't exist
/// - Target is an existing non-symlink directory (use --force to overwrite)
/// - Filesystem operations fail
pub fn deploy_to_agent(
    ssot_path: &Path,
    target: &TargetSpec,
    skill_name: &str,
    force_copy: bool,
    project_root: Option<&Path>,
) -> Result<DeployResult> {
    // Validate SSOT path exists
    if !ssot_path.exists() {
        return Err(SkillcError::DirectoryNotFound(format!(
            "SSOT path does not exist: {}",
            ssot_path.display()
        )));
    }

    // Resolve agent directory
    // Only known targets get project-local treatment; custom paths are used as-is
    let agent_dir = if target.is_known() {
        target.skills_path(project_root)?
    } else {
        target.skills_path(None)?
    };
    let dest = agent_dir.join(skill_name);

    // Ensure parent directory exists
    ensure_dir(&agent_dir)?;

    // Handle existing destination
    if dest.exists() {
        if is_link(&dest) {
            // Always remove existing symlinks/junctions
            remove_link(&dest)?;
        } else if force_copy {
            // With --copy, remove existing directory to replace it
            std::fs::remove_dir_all(&dest).map_err(|e| {
                SkillcError::Internal(format!(
                    "Failed to remove existing directory {}: {}",
                    dest.display(),
                    e
                ))
            })?;
        } else {
            // Non-symlink directory without --copy: error
            return Err(SkillcError::Internal(format!(
                "Destination exists and is not a symlink: {}. Use --copy to overwrite.",
                dest.display()
            )));
        }
    }

    // Deploy using appropriate method
    let method = if force_copy {
        crate::util::copy_dir_recursive(ssot_path, &dest)?;
        DeployMethod::Copy
    } else {
        create_link(ssot_path, &dest)?
    };

    Ok(DeployResult {
        target: dest,
        method,
    })
}

/// Check if a path is a symlink or junction
fn is_link(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Remove a symlink or junction
fn remove_link(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::fs::remove_file(path)?;
    }

    #[cfg(windows)]
    {
        // On Windows, junctions are directories
        std::fs::remove_dir(path)?;
    }

    Ok(())
}

/// Create a symlink or junction from source to target
#[cfg(unix)]
fn create_link(source: &Path, target: &Path) -> Result<DeployMethod> {
    // Use absolute path for the symlink source
    let abs_source = source.canonicalize().map_err(|e| {
        SkillcError::Internal(format!(
            "Failed to canonicalize source path {}: {}",
            source.display(),
            e
        ))
    })?;

    std::os::unix::fs::symlink(&abs_source, target).map_err(|e| {
        SkillcError::Internal(format!(
            "Failed to create symlink {} -> {}: {}",
            target.display(),
            abs_source.display(),
            e
        ))
    })?;

    Ok(DeployMethod::Symlink)
}

/// Create a symlink or junction from source to target
#[cfg(windows)]
fn create_link(source: &Path, target: &Path) -> Result<DeployMethod> {
    // Junction requires absolute paths
    let abs_source = source.canonicalize().map_err(|e| {
        SkillcError::Internal(format!(
            "Failed to canonicalize source path {}: {}",
            source.display(),
            e
        ))
    })?;

    // Try symlink first (requires Developer Mode or admin)
    match std::os::windows::fs::symlink_dir(&abs_source, target) {
        Ok(()) => return Ok(DeployMethod::Symlink),
        Err(_) => {
            // Fall through to junction
        }
    }

    // Try junction (no admin required on NTFS)
    match junction::create(&abs_source, target) {
        Ok(()) => return Ok(DeployMethod::Junction),
        Err(_) => {
            // Fall through to copy
        }
    }

    // Last resort: copy
    eprintln!("warning: Could not create symlink or junction. Using copy instead.");
    crate::util::copy_dir_recursive(source, target)?;
    Ok(DeployMethod::Copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_deploy_creates_symlink() {
        let temp = TempDir::new().expect("create temp dir");
        let ssot = temp.path().join("ssot").join("test-skill");
        let agent_dir = temp.path().join("agent");

        // Create SSOT with content
        std::fs::create_dir_all(&ssot).expect("create test dir");
        std::fs::write(ssot.join("SKILL.md"), "# Test").expect("test operation");

        // Deploy
        let result =
            deploy_to_agent_internal(&ssot, &agent_dir, "test-skill", false).expect("deploy");

        assert!(result.target.exists());
        #[cfg(unix)]
        assert_eq!(result.method, DeployMethod::Symlink);
        #[cfg(windows)]
        assert!(matches!(
            result.method,
            DeployMethod::Symlink | DeployMethod::Copy
        ));

        // Verify content is accessible
        let content =
            std::fs::read_to_string(result.target.join("SKILL.md")).expect("test operation");
        assert_eq!(content, "# Test");
    }

    #[test]
    fn test_deploy_removes_existing_symlink() {
        let temp = TempDir::new().expect("create temp dir");
        let ssot1 = temp.path().join("ssot1").join("test-skill");
        let ssot2 = temp.path().join("ssot2").join("test-skill");
        let agent_dir = temp.path().join("agent");

        // Create two SSOTs
        std::fs::create_dir_all(&ssot1).expect("create test dir");
        std::fs::write(ssot1.join("SKILL.md"), "# Version 1").expect("test operation");
        std::fs::create_dir_all(&ssot2).expect("create test dir");
        std::fs::write(ssot2.join("SKILL.md"), "# Version 2").expect("test operation");

        // Deploy first version
        deploy_to_agent_internal(&ssot1, &agent_dir, "test-skill", false).expect("deploy");

        // Deploy second version (should replace)
        let result =
            deploy_to_agent_internal(&ssot2, &agent_dir, "test-skill", false).expect("deploy");

        // Verify new content
        let content =
            std::fs::read_to_string(result.target.join("SKILL.md")).expect("test operation");
        assert_eq!(content, "# Version 2");
    }

    #[test]
    fn test_deploy_force_copy() {
        let temp = TempDir::new().expect("create temp dir");
        let ssot = temp.path().join("ssot").join("test-skill");
        let agent_dir = temp.path().join("agent");

        // Create SSOT
        std::fs::create_dir_all(&ssot).expect("create test dir");
        std::fs::write(ssot.join("SKILL.md"), "# Test").expect("test operation");

        // Deploy with force_copy
        let result =
            deploy_to_agent_internal(&ssot, &agent_dir, "test-skill", true).expect("deploy");

        assert_eq!(result.method, DeployMethod::Copy);
        assert!(!is_link(&result.target));

        // Verify content
        let content =
            std::fs::read_to_string(result.target.join("SKILL.md")).expect("test operation");
        assert_eq!(content, "# Test");
    }

    #[test]
    fn test_deploy_fails_on_existing_directory() {
        let temp = TempDir::new().expect("create temp dir");
        let ssot = temp.path().join("ssot").join("test-skill");
        let agent_dir = temp.path().join("agent");
        let target = agent_dir.join("test-skill");

        // Create SSOT
        std::fs::create_dir_all(&ssot).expect("create test dir");
        std::fs::write(ssot.join("SKILL.md"), "# Test").expect("test operation");

        // Create existing directory (not a symlink)
        std::fs::create_dir_all(&target).expect("create test dir");
        std::fs::write(target.join("existing.txt"), "existing").expect("test operation");

        // Deploy should fail
        let result = deploy_to_agent_internal(&ssot, &agent_dir, "test-skill", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a symlink"));
    }

    /// Internal test helper that takes agent_dir directly instead of agent name
    fn deploy_to_agent_internal(
        ssot_path: &Path,
        agent_dir: &Path,
        skill_name: &str,
        force_copy: bool,
    ) -> Result<DeployResult> {
        // Validate SSOT path exists
        if !ssot_path.exists() {
            return Err(SkillcError::DirectoryNotFound(format!(
                "SSOT path does not exist: {}",
                ssot_path.display()
            )));
        }

        let target = agent_dir.join(skill_name);

        // Ensure parent directory exists
        ensure_dir(agent_dir)?;

        // Check for existing non-symlink directory
        if target.exists() && !is_link(&target) {
            return Err(SkillcError::Internal(format!(
                "Target exists and is not a symlink: {}. Use --force to overwrite.",
                target.display()
            )));
        }

        // Remove existing symlink/junction if present
        if is_link(&target) {
            remove_link(&target)?;
        }

        // Deploy using appropriate method
        let method = if force_copy {
            crate::util::copy_dir_recursive(ssot_path, &target)?;
            DeployMethod::Copy
        } else {
            create_link(ssot_path, &target)?
        };

        Ok(DeployResult { target, method })
    }
}
