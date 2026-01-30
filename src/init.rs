//! Scaffolding commands per RFC-0006

use crate::config::{ensure_dir, global_source_store};
use crate::error::{Result, SkillcError};
use crate::verbose;
use std::fs;

/// Options for the init command
pub struct InitOptions {
    /// Skill name to create (None for project initialization only)
    pub name: Option<String>,
    /// Create in global source store instead of project-local
    pub global: bool,
}

/// Initialize a skillc project or create a new skill.
///
/// Per [[RFC-0006:C-INIT]]:
/// - Without name: creates `.skillc/` project structure
/// - With name: creates skill in `.skillc/skills/<name>/` (project-local)
/// - With name + global: creates skill in `~/.skillc/skills/<name>/` (global)
pub fn init(options: InitOptions) -> Result<String> {
    match options.name {
        None => init_project(),
        Some(name) => init_skill(&name, options.global),
    }
}

/// Initialize project structure.
///
/// Creates `.skillc/` and `.skillc/skills/` in current directory.
/// Idempotent: succeeds if already exists.
fn init_project() -> Result<String> {
    let cwd = std::env::current_dir()
        .map_err(|e| SkillcError::Internal(format!("Failed to get current directory: {}", e)))?;

    let skillc_dir = cwd.join(".skillc");
    let skills_dir = skillc_dir.join("skills");

    verbose!("Creating project structure in {:?}", cwd);

    // Create .skillc/ directory
    ensure_dir(&skillc_dir)
        .map_err(|e| SkillcError::Internal(format!("Failed to create .skillc/: {}", e)))?;

    // Create .skillc/skills/ subdirectory
    ensure_dir(&skills_dir)
        .map_err(|e| SkillcError::Internal(format!("Failed to create .skillc/skills/: {}", e)))?;

    verbose!("Created .skillc/ and .skillc/skills/");

    Ok(format!("Initialized skillc project in {}", cwd.display()))
}

/// Initialize a new skill.
///
/// Creates skill directory and SKILL.md with minimal frontmatter.
fn init_skill(name: &str, global: bool) -> Result<String> {
    // Determine target directory
    let target_dir = if global {
        global_source_store()?.join(name)
    } else {
        let cwd = std::env::current_dir().map_err(|e| {
            SkillcError::Internal(format!("Failed to get current directory: {}", e))
        })?;

        // Ensure project structure exists for local skills
        let skillc_dir = cwd.join(".skillc");
        let skills_dir = skillc_dir.join("skills");

        if !skillc_dir.exists() {
            verbose!("Creating .skillc/ for project-local skill");
            ensure_dir(&skillc_dir)
                .map_err(|e| SkillcError::Internal(format!("Failed to create .skillc/: {}", e)))?;
        }

        if !skills_dir.exists() {
            ensure_dir(&skills_dir).map_err(|e| {
                SkillcError::Internal(format!("Failed to create .skillc/skills/: {}", e))
            })?;
        }

        skills_dir.join(name)
    };

    let skill_md = target_dir.join("SKILL.md");

    verbose!("Creating skill '{}' at {:?}", name, target_dir);

    // Check if SKILL.md already exists (prevents accidental overwrite)
    if skill_md.exists() {
        return Err(SkillcError::SkillAlreadyExists(name.to_string()));
    }

    // Create target directory
    ensure_dir(&target_dir)
        .map_err(|e| SkillcError::Internal(format!("Failed to create skill directory: {}", e)))?;

    // Generate SKILL.md content with minimal frontmatter
    // Description must be non-empty per [[RFC-0001:C-INPUT]]
    let title_cased = title_case(name);
    let content = format!(
        r#"---
name: {}
description: "TODO: Add skill description"
---

# {}
"#,
        name, title_cased
    );

    // Write SKILL.md
    fs::write(&skill_md, content)
        .map_err(|e| SkillcError::Internal(format!("Failed to write SKILL.md: {}", e)))?;

    verbose!("Created SKILL.md");

    let location = if global { "global" } else { "project" };
    Ok(format!(
        "Created {} skill '{}' at {}",
        location,
        name,
        target_dir.display()
    ))
}

/// Convert a skill name to title case.
///
/// Examples:
/// - "my-skill" -> "My Skill"
/// - "cuda" -> "Cuda"
/// - "my_skill" -> "My Skill"
fn title_case(s: &str) -> String {
    s.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_title_case() {
        assert_eq!(title_case("cuda"), "Cuda");
        assert_eq!(title_case("my-skill"), "My Skill");
        assert_eq!(title_case("my_skill"), "My Skill");
        assert_eq!(title_case("my-cool_skill"), "My Cool Skill");
        assert_eq!(title_case("CAPS"), "CAPS");
    }
}
