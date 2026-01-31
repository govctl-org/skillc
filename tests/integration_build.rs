//! Integration tests for `skc build` command per [[RFC-0001:C-DEPLOYMENT]]

mod common;

use common::TestContext;
use std::fs;

/// Test: Build a project-local skill by name
#[test]
fn test_build_project_local_skill() {
    let ctx = TestContext::new().with_project().with_mock_agent();
    ctx.create_skill("test-skill");

    let result = ctx.run_skc(&["build", "test-skill", "--target", ctx.mock_agent_str()]);
    result.assert_success("Build");

    // SSOT should exist in project runtime
    let ssot_dir = ctx
        .project_dir()
        .join(".skillc")
        .join("runtime")
        .join("test-skill");
    assert!(
        ssot_dir.exists(),
        "SSOT should exist at {}",
        ssot_dir.display()
    );

    // Skill should be deployed to mock agent
    let deployed = ctx.mock_agent().join("test-skill");
    assert!(deployed.exists(), "Skill should be deployed to mock agent");

    // Output should show correct format
    assert!(
        result.stdout.contains("Built test-skill"),
        "Should show 'Built' message, got: {}",
        result.stdout
    );
    assert!(result.stdout.contains("Source:"), "Should show source");
    assert!(result.stdout.contains("Runtime:"), "Should show runtime");
    assert!(result.stdout.contains("Deploy:"), "Should show deploy");
}

/// Test: Import a skill from direct path
#[test]
fn test_build_import_from_path() {
    let ctx = TestContext::new().with_project().with_mock_agent();
    let external_skill = ctx.create_external_skill("external-skill");

    let result = ctx.run_skc(&[
        "build",
        external_skill.to_str().expect("path to str"),
        "--target",
        ctx.mock_agent_str(),
    ]);
    result.assert_success("Build import");

    // Skill should be imported to project source store
    let imported = ctx
        .project_dir()
        .join(".skillc")
        .join("skills")
        .join("external-skill");
    assert!(
        imported.exists(),
        "Skill should be imported to {}",
        imported.display()
    );

    // SSOT should exist
    let ssot_dir = ctx
        .project_dir()
        .join(".skillc")
        .join("runtime")
        .join("external-skill");
    assert!(ssot_dir.exists(), "SSOT should exist");

    // Output should mention import
    assert!(
        result.stdout.contains("Imported"),
        "Should show 'Imported' message, got: {}",
        result.stdout
    );
}

/// Test: Import with --force overwrites existing skill
#[test]
fn test_build_import_force_overwrite() {
    let ctx = TestContext::new().with_project().with_mock_agent();

    // Create existing skill in project
    let existing_skill = ctx.create_skill("overwrite-skill");
    fs::write(existing_skill.join("marker.txt"), "original").expect("test operation");

    // Create new version outside
    let external_skill = ctx.create_external_skill("overwrite-skill");
    fs::write(external_skill.join("marker.txt"), "updated").expect("test operation");

    // First: try without --force (should fail)
    let result_no_force = ctx.run_skc(&[
        "build",
        external_skill.to_str().expect("path to str"),
        "--target",
        ctx.mock_agent_str(),
    ]);
    result_no_force.assert_failure("Build without --force");

    // Second: with --force (should succeed)
    let result_force = ctx.run_skc(&[
        "build",
        external_skill.to_str().expect("path to str"),
        "--force",
        "--target",
        ctx.mock_agent_str(),
    ]);
    result_force.assert_success("Build with --force");

    // Check marker file was updated
    let marker = existing_skill.join("marker.txt");
    let content = fs::read_to_string(marker).expect("test operation");
    assert_eq!(content, "updated", "Skill should be overwritten");
}

/// Test: Build with --global flag uses global SSOT
#[test]
fn test_build_global_flag() {
    let ctx = TestContext::new().with_project().with_mock_agent();
    ctx.create_skill("global-test");

    let result = ctx.run_skc(&[
        "build",
        "global-test",
        "--global",
        "--target",
        ctx.mock_agent_str(),
    ]);
    result.assert_success("Build with --global");

    // Output should mention ~/.skillc/runtime/ (global SSOT)
    // Use platform-agnostic check (Windows uses backslashes)
    assert!(
        (result.stdout.contains(".skillc/runtime") || result.stdout.contains(".skillc\\runtime"))
            && result.stdout.contains("Runtime:"),
        "Should output to global SSOT, got: {}",
        result.stdout
    );

    // Verify SSOT was created in mock home, not real home
    let mock_ssot = ctx
        .mock_home()
        .join(".skillc")
        .join("runtime")
        .join("global-test");
    assert!(
        mock_ssot.exists(),
        "SSOT should exist in mock home at {}",
        mock_ssot.display()
    );
}

/// Test: Build with --copy forces directory copy
#[test]
fn test_build_copy_flag() {
    let ctx = TestContext::new().with_project().with_mock_agent();
    ctx.create_skill("copy-skill");

    let result = ctx.run_skc(&[
        "build",
        "copy-skill",
        "--copy",
        "--target",
        ctx.mock_agent_str(),
    ]);
    result.assert_success("Build with --copy");

    // Output should mention "copy"
    assert!(
        result.stdout.contains("(copy)"),
        "Should use copy method, got: {}",
        result.stdout
    );
}

/// Test: Build with multiple targets
#[test]
fn test_build_multiple_targets() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("multi-skill");

    // Create multiple mock agent directories
    let mock_claude = ctx.temp_path().join("mock-claude");
    let mock_cursor = ctx.temp_path().join("mock-cursor");
    fs::create_dir_all(&mock_claude).expect("test operation");
    fs::create_dir_all(&mock_cursor).expect("test operation");

    let targets = format!(
        "{},{}",
        mock_claude.to_str().expect("path to str"),
        mock_cursor.to_str().expect("path to str")
    );
    let result = ctx.run_skc(&["build", "multi-skill", "--target", &targets]);
    result.assert_success("Build with multiple targets");

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
    let ctx = TestContext::new().with_project().with_mock_agent();
    ctx.create_skill("nested-skill");

    // Create a nested subdirectory
    let nested_dir = ctx.project_dir().join("src").join("components");
    fs::create_dir_all(&nested_dir).expect("test operation");

    // Run build from nested directory
    let result = ctx.run_skc_in(
        &nested_dir,
        &["build", "nested-skill", "--target", ctx.mock_agent_str()],
    );
    result.assert_success("Build from nested dir");

    // SSOT should exist in project (not nested dir)
    let ssot_dir = ctx
        .project_dir()
        .join(".skillc")
        .join("runtime")
        .join("nested-skill");
    assert!(ssot_dir.exists(), "SSOT should exist in project root");
}

/// Test: Skill not found error
#[test]
fn test_build_skill_not_found() {
    let ctx = TestContext::new().with_project().with_mock_agent();

    let result = ctx.run_skc(&[
        "build",
        "nonexistent-skill",
        "--target",
        ctx.mock_agent_str(),
    ]);
    result.assert_failure("Build for non-existent skill");

    assert!(
        result.stderr.contains("not found") || result.stderr.contains("E001"),
        "Error should mention skill not found, got: {}",
        result.stderr
    );
}

/// Test: SSOT structure is valid after build (verifies compiled artifacts exist)
#[test]
fn test_ssot_structure() {
    let ctx = TestContext::new().with_project().with_mock_agent();
    ctx.create_skill("ssot-skill");

    let result = ctx.run_skc(&["build", "ssot-skill", "--target", ctx.mock_agent_str()]);
    result.assert_success("Build");

    // Verify SSOT structure contains compiled artifacts
    let ssot_dir = ctx
        .project_dir()
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
    let deployed = ctx.mock_agent().join("ssot-skill");
    assert!(deployed.exists(), "deployed skill should exist");
    assert!(
        deployed.join("SKILL.md").exists(),
        "deployed skill should have stub"
    );
}

/// Test: --global flag during import respects global source store
#[test]
fn test_build_import_global_flag() {
    let ctx = TestContext::new().with_project().with_mock_agent();

    // Pre-create global source store structure
    let global_source = ctx.mock_home().join(".skillc").join("skills");
    fs::create_dir_all(&global_source).expect("test operation");

    // Create an external skill to import (without triggers, simpler format)
    let external_skill = ctx.temp_path().join("external");
    fs::create_dir_all(&external_skill).expect("test operation");
    fs::write(
        external_skill.join("SKILL.md"),
        "---\nname: global-import-test\ndescription: test\n---\n# Test\n",
    )
    .expect("test operation");

    let result = ctx.run_skc(&[
        "build",
        external_skill.to_str().expect("path to str"),
        "--global",
        "--target",
        ctx.mock_agent_str(),
    ]);
    result.assert_success("Build with --global import");

    // With --global, skill should be imported to global source store, not project
    let global_skill = global_source.join("global-import-test");
    assert!(
        global_skill.exists(),
        "Skill should be imported to global source store at {}",
        global_skill.display()
    );

    // Should NOT be in project source store
    let project_skill = ctx
        .project_dir()
        .join(".skillc")
        .join("skills")
        .join("global-import-test");
    assert!(
        !project_skill.exists(),
        "Skill should NOT be in project source store"
    );

    // Output should say "global"
    assert!(
        result.stdout.contains("(global)"),
        "Output should indicate global scope"
    );
}

/// Test: --copy overwrites existing non-symlink directory
#[test]
fn test_build_copy_overwrites_existing_directory() {
    let ctx = TestContext::new().with_project().with_mock_agent();
    ctx.create_skill("overwrite-test");

    // Create existing non-symlink directory in mock agent
    let existing_dir = ctx.mock_agent().join("overwrite-test");
    fs::create_dir_all(&existing_dir).expect("test operation");
    fs::write(existing_dir.join("old-file.txt"), "old content").expect("test operation");

    // First build without --copy should fail
    let result = ctx.run_skc(&["build", "overwrite-test", "--target", ctx.mock_agent_str()]);
    result.assert_failure("Build without --copy on existing dir");
    assert!(
        result.stderr.contains("not a symlink") || result.stderr.contains("Use --copy"),
        "Error should mention --copy"
    );

    // Build with --copy should succeed and overwrite
    let result = ctx.run_skc(&[
        "build",
        "overwrite-test",
        "--copy",
        "--target",
        ctx.mock_agent_str(),
    ]);
    result.assert_success("Build with --copy");

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
#[test]
fn test_build_project_local_deploys_to_project_agent_dir() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("local-deploy-test");

    // Pre-create global agent dir that should NOT be used
    fs::create_dir_all(ctx.mock_home().join(".claude").join("skills")).expect("test operation");

    // Build targeting "claude" (a known target)
    let result = ctx.run_skc(&["build", "local-deploy-test", "--target", "claude"]);
    result.assert_success("Build to project agent dir");

    // Should deploy to project's .claude/skills/, not global
    let project_deploy = ctx
        .project_dir()
        .join(".claude")
        .join("skills")
        .join("local-deploy-test");
    assert!(
        project_deploy.exists(),
        "Should deploy to project agent dir at {}",
        project_deploy.display()
    );

    // Should NOT deploy to global agent dir
    let global_deploy = ctx
        .mock_home()
        .join(".claude")
        .join("skills")
        .join("local-deploy-test");
    assert!(
        !global_deploy.exists(),
        "Should NOT deploy to global agent dir"
    );

    // Output should show project path (handle both Unix and Windows separators)
    assert!(
        result.stdout.contains(".claude/skills/local-deploy-test")
            || result.stdout.contains(".claude\\skills\\local-deploy-test"),
        "Output should show project agent path"
    );
}
