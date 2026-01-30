# Developing Guide for `skillc`

This file provides guidance for AI agents working on the skillc codebase.

## Project Overview

**skillc** is a development kit for [Agent Skills](https://agentskills.io/) — the open format for extending AI agent capabilities. It provides tooling to author, validate, build, and analyze skills throughout their lifecycle.

- **Binary name**: `skc`
- **Language**: Rust (Edition 2024)
- **License**: MIT OR Apache-2.0

## Quick Reference

```bash
# Build & test
just build          # Build release binary
just test           # Run all tests
just lint           # Run clippy (warnings are errors)
just fmt            # Format code

# Coverage
just cov            # Summary
just cov-html       # HTML report

# Governance
just check          # Validate governed documents
just render         # Render RFCs to markdown
just status         # Show governance status

# Snapshots
just update-snapshots  # Update insta snapshots
```

## Architecture

### Directory Structure

```
src/
├── main.rs          # CLI entry point (clap)
├── lib.rs           # Public API exports
├── compiler.rs      # Skill compilation (RFC-0001)
├── gateway.rs       # Read commands: outline, show, open, sources (RFC-0002)
├── search.rs        # Full-text search (RFC-0004)
├── analytics.rs     # Usage stats (RFC-0003)
├── lint/            # Skill validation (RFC-0008)
├── mcp.rs           # MCP server implementation
├── error.rs         # Error types with codes (RFC-0005)
└── ...

tests/
├── integration_*.rs # Integration tests per command
├── common/mod.rs    # Test utilities
└── snapshots/       # Insta snapshot files

gov/
├── rfc/             # RFC specifications (source of truth)
├── adr/             # Architecture Decision Records
├── work/            # Work items (tasks)
└── config.toml      # Governance configuration

docs/
├── rfc/             # Rendered RFC markdown (generated)
└── workflows/       # User documentation
```

### Key Abstractions

- **Skill**: A directory with `SKILL.md` containing YAML frontmatter + markdown
- **Source store**: Where skill sources live (`.skillc/skills/` or `~/.skillc/skills/`)
- **Runtime store**: Compiled output (`.skillc/runtime/` or `~/.skillc/runtime/`)
- **Deploy target**: Agent-specific locations (`~/.claude/skills/`, `~/.cursor/skills/`)

## Code Conventions

### Error Handling

All errors use the canonical code registry from RFC-0005:

```rust
// Errors have codes: E001, E010, E999, etc.
return Err(SkillcError::SkillNotFound(name.to_string()));

// Warnings don't affect exit code
SkillcWarning::MultipleMatches(query).emit();
```

Error format: `error[EXXX]: message`
Warning format: `warning[WXXX]: message`

### RFC References

Code references RFCs using double-bracket notation in doc comments:

```rust
//! Integration tests for `skc build` command per [[RFC-0001:C-DEPLOYMENT]]
```

This links to specific clauses in the specification.

### Module Organization

- Each command has a corresponding module (e.g., `gateway.rs` for read commands)
- Lint rules are in `lint/` submodule with separate files per check type
- Public types are re-exported from `lib.rs`

### Testing

- Integration tests use `assert_cmd` for CLI testing
- Snapshot tests use `insta` (files in `tests/snapshots/`)
- Test helpers in `tests/common/mod.rs`

```rust
// Integration test pattern
#[test]
fn test_command_behavior() {
    let temp = TempDir::new().expect("create temp dir");
    let output = run_skc(&["command", "arg"], temp.path());
    assert!(output.status.success());
}
```

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(compiler): add progressive disclosure support
fix(resolver): handle symlinks in skill paths
docs(readme): update installation instructions
test(build): add integration test for --force flag
refactor(lint): extract markdown parsing
chore(deps): update clap to v4.5
```

## Governance

This project uses a governed workflow:

1. **RFCs** (`gov/rfc/`): Specifications for features. Source files are JSON; rendered markdown goes to `docs/rfc/`.
2. **ADRs** (`gov/adr/`): Architecture decisions (TOML format).
3. **Work items** (`gov/work/`): Task tracking (TOML format).

Use `govctl` commands to manage:

```bash
govctl work list pending  # List pending work
govctl rfc list           # List RFCs
govctl check              # Validate documents
```

## Key Files

| File                      | Purpose                           |
| ------------------------- | --------------------------------- |
| `Cargo.toml`              | Dependencies and package metadata |
| `Justfile`                | Development commands              |
| `.pre-commit-config.yaml` | Pre-commit hooks                  |
| `deny.toml`               | cargo-deny security audit config  |
| `gov/config.toml`         | Governance settings               |

## Common Tasks

### Adding a New Command

1. Add subcommand to `main.rs` CLI parser
2. Implement logic in appropriate module
3. Add MCP tool wrapper in `mcp.rs` if it's a read command
4. Write integration tests in `tests/integration_*.rs`
5. Update RFC if protocol changes

### Adding an Error Code

1. Add variant to `ErrorCode` enum in `error.rs`
2. Add corresponding `SkillcError` variant
3. Implement `code()` and `message()` methods
4. Update RFC-0005 documentation

### Running a Single Test

```bash
cargo test test_name
cargo test integration_build  # Run all build integration tests
```

## Dependencies

Key dependencies (see `Cargo.toml` for full list):

- `clap` - CLI parsing
- `serde` / `serde_yaml` / `serde_json` - Serialization
- `pulldown-cmark` - Markdown parsing
- `rusqlite` - SQLite for analytics/search index
- `rmcp` - MCP server SDK
- `insta` - Snapshot testing (dev)
- `assert_cmd` - CLI testing (dev)
