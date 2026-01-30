//! Integration tests for `skc build` command per [[RFC-0001:C-DEPLOYMENT]]

mod common;

use common::{create_project, create_test_skill};
use std::fs;
use tempfile::TempDir;

/// Run skc command with arguments
fn run_skc(args: &[&str], cwd: &std::path::Path) -> std::process::Output {
    use assert_cmd::Command;
    Command::new(assert_cmd::cargo::cargo_bin!("skc"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("Failed to execute skc")
}

/// Test: Build a project-local skill by name
#[test]
fn test_build_project_local_skill() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create skill in project source store
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "test-skill");

    // Mock agent directory (custom path)
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Run build by name with custom target path
    let output = run_skc(
        &[
            "build",
            "test-skill",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build failed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // SSOT should exist in project runtime
    let ssot_dir = project_dir
        .join(".skillc")
        .join("runtime")
        .join("test-skill");
    assert!(
        ssot_dir.exists(),
        "SSOT should exist at {}",
        ssot_dir.display()
    );

    // Skill should be deployed to mock agent
    let deployed = mock_agent.join("test-skill");
    assert!(deployed.exists(), "Skill should be deployed to mock agent");

    // Output should show correct format
    assert!(
        stdout.contains("Built test-skill"),
        "Should show 'Built' message, got: {}",
        stdout
    );
    assert!(stdout.contains("Source:"), "Should show source");
    assert!(stdout.contains("Runtime:"), "Should show runtime");
    assert!(stdout.contains("Deploy:"), "Should show deploy");
}

/// Test: Import a skill from direct path
#[test]
fn test_build_import_from_path() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create a skill outside .skillc/ (in temp/external/)
    let external_dir = temp.path().join("external");
    create_test_skill(&external_dir, "external-skill");

    // Mock agent directory
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Run build with direct path (triggers import)
    let skill_path = external_dir.join("external-skill");
    let output = run_skc(
        &[
            "build",
            skill_path.to_str().expect("path to str"),
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build failed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // Skill should be imported to project source store
    let imported = project_dir
        .join(".skillc")
        .join("skills")
        .join("external-skill");
    assert!(
        imported.exists(),
        "Skill should be imported to {}",
        imported.display()
    );

    // SSOT should exist
    let ssot_dir = project_dir
        .join(".skillc")
        .join("runtime")
        .join("external-skill");
    assert!(ssot_dir.exists(), "SSOT should exist");

    // Output should mention import
    assert!(
        stdout.contains("Imported"),
        "Should show 'Imported' message, got: {}",
        stdout
    );
}

/// Test: Import with --force overwrites existing skill
#[test]
fn test_build_import_force_overwrite() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create existing skill in project
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "overwrite-skill");
    fs::write(
        skills_dir.join("overwrite-skill").join("marker.txt"),
        "original",
    )
    .expect("test operation");

    // Create new version outside
    let external_dir = temp.path().join("external");
    create_test_skill(&external_dir, "overwrite-skill");
    fs::write(
        external_dir.join("overwrite-skill").join("marker.txt"),
        "updated",
    )
    .expect("test operation");

    // Mock agent directory
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // First: try without --force (should fail)
    let skill_path = external_dir.join("overwrite-skill");
    let output_no_force = run_skc(
        &[
            "build",
            skill_path.to_str().expect("path to str"),
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );
    assert!(
        !output_no_force.status.success(),
        "Build without --force should fail"
    );

    // Second: with --force (should succeed)
    let output_force = run_skc(
        &[
            "build",
            skill_path.to_str().expect("path to str"),
            "--force",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );

    let stdout = String::from_utf8_lossy(&output_force.stdout);
    let stderr = String::from_utf8_lossy(&output_force.stderr);

    assert!(
        output_force.status.success(),
        "Build with --force should succeed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // Check marker file was updated
    let marker = skills_dir.join("overwrite-skill").join("marker.txt");
    let content = fs::read_to_string(marker).expect("test operation");
    assert_eq!(content, "updated", "Skill should be overwritten");
}

/// Test: Build with --global flag uses global SSOT
#[test]
fn test_build_global_flag() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create skill in project source store
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "global-test");

    // Mock agent directory
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Run build with --global (should output to global SSOT, not project)
    let output = run_skc(
        &[
            "build",
            "global-test",
            "--global",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build failed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // Output should mention ~/.skillc/runtime/ (global SSOT)
    assert!(
        stdout.contains(".skillc/runtime") && stdout.contains("Runtime:"),
        "Should output to global SSOT, got: {}",
        stdout
    );
}

/// Test: Build with --copy forces directory copy
#[test]
fn test_build_copy_flag() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create skill in project source store
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "copy-skill");

    // Mock agent directory
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Run build with --copy
    let output = run_skc(
        &[
            "build",
            "copy-skill",
            "--copy",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build failed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // Output should mention "copy"
    assert!(
        stdout.contains("(copy)"),
        "Should use copy method, got: {}",
        stdout
    );
}

/// Test: Build with multiple targets
#[test]
fn test_build_multiple_targets() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create skill in project source store
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "multi-skill");

    // Mock agent directories
    let mock_claude = temp.path().join("mock-claude");
    let mock_cursor = temp.path().join("mock-cursor");
    fs::create_dir_all(&mock_claude).expect("test operation");
    fs::create_dir_all(&mock_cursor).expect("test operation");

    // Run build with multiple targets
    let targets = format!(
        "{},{}",
        mock_claude.to_str().expect("path to str"),
        mock_cursor.to_str().expect("path to str")
    );
    let output = run_skc(
        &["build", "multi-skill", "--target", &targets],
        &project_dir,
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build failed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // Skill should be deployed to both mock agents
    assert!(
        mock_claude.join("multi-skill").exists(),
        "Skill should be deployed to mock-claude"
    );
    assert!(
        mock_cursor.join("multi-skill").exists(),
        "Skill should be deployed to mock-cursor"
    );
}

/// Test: Skill lookup from nested subdirectory (walks up to find project)
#[test]
fn test_build_from_nested_subdir() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create skill in project source store
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "nested-skill");

    // Create a nested subdirectory
    let nested_dir = project_dir.join("src").join("components");
    fs::create_dir_all(&nested_dir).expect("test operation");

    // Mock agent directory
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Run build from nested directory
    let output = run_skc(
        &[
            "build",
            "nested-skill",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &nested_dir,
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build from nested dir should succeed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // SSOT should exist in project (not nested dir)
    let ssot_dir = project_dir
        .join(".skillc")
        .join("runtime")
        .join("nested-skill");
    assert!(ssot_dir.exists(), "SSOT should exist in project root");
}

/// Test: Skill not found error
#[test]
fn test_build_skill_not_found() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Mock agent directory
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Run build for non-existent skill
    let output = run_skc(
        &[
            "build",
            "nonexistent-skill",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );

    assert!(
        !output.status.success(),
        "Build should fail for non-existent skill"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("E001"),
        "Error should mention skill not found, got: {}",
        stderr
    );
}

/// Test: SSOT structure is valid after build (verifies compiled artifacts exist)
#[test]
fn test_ssot_structure() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create skill
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "ssot-skill");

    // Mock agent directory
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Build the skill
    let output = run_skc(
        &[
            "build",
            "ssot-skill",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );
    assert!(output.status.success(), "build should succeed");

    // Verify SSOT structure contains compiled artifacts
    let ssot_dir = project_dir
        .join(".skillc")
        .join("runtime")
        .join("ssot-skill");
    assert!(ssot_dir.exists(), "SSOT directory should exist");

    // SSOT contains stub (compiled SKILL.md)
    let stub_path = ssot_dir.join("SKILL.md");
    assert!(stub_path.exists(), "SSOT should have stub SKILL.md");
    let stub_content = fs::read_to_string(&stub_path).expect("test operation");
    assert!(
        stub_content.contains("(compiled)"),
        "stub should indicate it's compiled"
    );

    // SSOT contains manifest
    assert!(
        ssot_dir.join(".skillc-meta").join("manifest.json").exists(),
        "SSOT should have manifest"
    );

    // Verify deployed skill (symlink target) works
    let deployed = mock_agent.join("ssot-skill");
    assert!(deployed.exists(), "deployed skill should exist");
    assert!(
        deployed.join("SKILL.md").exists(),
        "deployed skill should have stub"
    );
}

/// Test: --global flag during import respects global source store
#[cfg(unix)]
#[test]
fn test_build_import_global_flag() {
    let temp = TempDir::new().expect("create temp dir");

    // Create a project directory (we're in a project context)
    let project_dir = create_project(temp.path());

    // Create a mock home directory with skillc structure
    let mock_home = temp.path().join("mock_home");
    fs::create_dir_all(&mock_home).expect("test operation");
    let global_source = mock_home.join(".skillc").join("skills");
    fs::create_dir_all(&global_source).expect("test operation");

    // Create an external skill to import
    let external_skill = temp.path().join("external");
    fs::create_dir_all(&external_skill).expect("test operation");
    fs::write(
        external_skill.join("SKILL.md"),
        "---\nname: global-import-test\ndescription: test\n---\n# Test\n",
    )
    .expect("test operation");

    // Mock agent for deployment
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Build with --global flag
    use assert_cmd::Command;
    let output = Command::new(assert_cmd::cargo::cargo_bin!("skc"))
        .args([
            "build",
            external_skill.to_str().expect("path to str"),
            "--global",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ])
        .current_dir(&project_dir)
        .env("HOME", &mock_home)
        .output()
        .expect("Failed to execute skc");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build should succeed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // With --global, skill should be imported to global source store, not project
    let global_skill = global_source.join("global-import-test");
    assert!(
        global_skill.exists(),
        "Skill should be imported to global source store at {}",
        global_skill.display()
    );

    // Should NOT be in project source store
    let project_skill = project_dir
        .join(".skillc")
        .join("skills")
        .join("global-import-test");
    assert!(
        !project_skill.exists(),
        "Skill should NOT be in project source store"
    );

    // Output should say "global"
    assert!(
        stdout.contains("(global)"),
        "Output should indicate global scope"
    );
}

/// Test: --copy overwrites existing non-symlink directory
#[test]
fn test_build_copy_overwrites_existing_directory() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create skill in project
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "overwrite-test");

    // Mock agent with existing non-symlink directory
    let mock_agent = temp.path().join("mock-agent");
    fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    let existing_dir = mock_agent.join("overwrite-test");
    fs::create_dir_all(&existing_dir).expect("test operation");
    fs::write(existing_dir.join("old-file.txt"), "old content").expect("test operation");

    // First build without --copy should fail
    let output = run_skc(
        &[
            "build",
            "overwrite-test",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "Build without --copy should fail on existing dir"
    );
    assert!(
        stderr.contains("not a symlink") || stderr.contains("Use --copy"),
        "Error should mention --copy"
    );

    // Build with --copy should succeed and overwrite
    let output = run_skc(
        &[
            "build",
            "overwrite-test",
            "--copy",
            "--target",
            mock_agent.to_str().expect("path to str"),
        ],
        &project_dir,
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build with --copy should succeed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // Old file should be gone
    assert!(
        !existing_dir.join("old-file.txt").exists(),
        "Old content should be replaced"
    );

    // New content should be there
    assert!(
        existing_dir.join("SKILL.md").exists(),
        "New skill should be deployed"
    );
}

/// Test: Project-local skills deploy to project agent directory
#[cfg(unix)]
#[test]
fn test_build_project_local_deploys_to_project_agent_dir() {
    let temp = TempDir::new().expect("create temp dir");
    let project_dir = create_project(temp.path());

    // Create skill in project
    let skills_dir = project_dir.join(".skillc").join("skills");
    create_test_skill(&skills_dir, "local-deploy-test");

    // Mock home (for global agent dir that should NOT be used)
    let mock_home = temp.path().join("mock_home");
    fs::create_dir_all(mock_home.join(".claude").join("skills")).expect("test operation");

    // Build targeting "claude" (a known target)
    use assert_cmd::Command;
    let output = Command::new(assert_cmd::cargo::cargo_bin!("skc"))
        .args(["build", "local-deploy-test", "--target", "claude"])
        .current_dir(&project_dir)
        .env("HOME", &mock_home)
        .output()
        .expect("Failed to execute skc");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Build should succeed: stdout={}, stderr={}",
        stdout,
        stderr
    );

    // Should deploy to project's .claude/skills/, not global
    let project_deploy = project_dir
        .join(".claude")
        .join("skills")
        .join("local-deploy-test");
    assert!(
        project_deploy.exists(),
        "Should deploy to project agent dir at {}",
        project_deploy.display()
    );

    // Should NOT deploy to global agent dir
    let global_deploy = mock_home
        .join(".claude")
        .join("skills")
        .join("local-deploy-test");
    assert!(
        !global_deploy.exists(),
        "Should NOT deploy to global agent dir"
    );

    // Output should show project path
    assert!(
        stdout.contains(".claude/skills/local-deploy-test"),
        "Output should show project agent path"
    );
}
