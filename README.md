# skillc

[![CI](https://github.com/govctl-org/skillc/actions/workflows/ci.yml/badge.svg)](https://github.com/govctl-org/skillc/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/govctl-org/skillc/graph/badge.svg)](https://codecov.io/gh/govctl-org/skillc)
[![Crates.io](https://img.shields.io/crates/v/skillc.svg)](https://crates.io/crates/skillc)
[![governed by govctl](https://img.shields.io/badge/governed%20by-govctl-6366F1)](https://github.com/govctl-org/govctl)

**The development kit for [Agent Skills](https://agentskills.io/)** — create, validate, and optimize skills that extend AI agent capabilities.

```text
                        SKILL.md (source)
                              │
              ┌───────────────┴───────────────┐
              │                               │
              ▼                               ▼
   ┌─────────────────────┐         ┌─────────────────────┐
   │    Local Testing    │         │      Publishing     │
   │                     │         │                     │
   │  skc lint           │         │  git push           │
   │  skc build          │         │         ↓           │
   │  skc stats          │         │  npx skills add     │
   │         ↓           │         │         ↓           │
   │  test with agent    │         │  users get source   │
   └─────────────────────┘         └─────────────────────┘
```

> **Note:** `skc build` is for local testing only. Published skills are source files — no compilation needed by consumers.

## Installation

```bash
cargo install skillc
```

## Quick Start

```bash
skc init my-skill           # Create a new skill
# ... edit SKILL.md ...
skc lint my-skill           # Validate quality
skc build my-skill          # Build for local testing
git push origin main        # Publish to GitHub
```

Users install your skill with: `npx skills add username/my-skill`

## Commands

| Category      | Command       | Description                    |
| ------------- | ------------- | ------------------------------ |
| **Author**    | `skc init`    | Create new skill or project    |
|               | `skc lint`    | Validate structure and quality |
|               | `skc build`   | Compile and deploy locally     |
|               | `skc list`    | List all managed skills        |
| **Read**      | `skc outline` | List all sections              |
|               | `skc show`    | Show section content           |
|               | `skc open`    | Read file contents             |
|               | `skc search`  | Full-text search               |
|               | `skc sources` | List source files              |
| **Analytics** | `skc stats`   | Usage analytics                |
|               | `skc sync`    | Merge local logs               |
| **Agent**     | `skc mcp`     | Start MCP server               |

<details>
<summary><strong>Command examples</strong></summary>

```bash
# Authoring
skc init my-skill --global          # Create in ~/.skillc/skills/
skc lint my-skill                   # Check for issues
skc build my-skill --target cursor  # Deploy to Cursor

# Reading (also available as MCP tools)
skc outline my-skill                # List all headings
skc outline my-skill --level 2      # Only # and ## headings
skc show my-skill --section "API Reference"
skc show my-skill --section "API" --max-lines 50  # Truncate output
skc open my-skill SKILL.md --max-lines 100        # Read first 100 lines
skc search my-skill "borrow checker"
skc sources my-skill --pattern "*.md"

# Analytics
skc stats my-skill --group-by sections
skc stats my-skill --group-by search  # Most frequent search terms
skc sync --dry-run
```

</details>

## What is an Agent Skill?

A directory with a `SKILL.md` file:

```
my-skill/
├── SKILL.md          # Required: metadata + instructions
└── docs/             # Optional: additional content
    ├── reference.md
    └── examples.md
```

The `SKILL.md` has YAML frontmatter:

```markdown
---
name: my-skill
description: A skill that does something useful
---

# My Skill

Instructions for the agent...
```

See [agentskills.io](https://agentskills.io/) for the full specification.

## MCP Integration

skillc exposes all read commands as MCP tools for direct agent integration:

```bash
skc mcp  # Start the MCP server
```

Add to your agent's MCP configuration to enable tools like `skc_outline`, `skc_show`, `skc_search`.

## Storage

| Path                | Purpose                   |
| ------------------- | ------------------------- |
| `.skillc/skills/`   | Project-local skills      |
| `~/.skillc/skills/` | Global source store       |
| `~/.claude/skills/` | Claude runtime (deployed) |
| `~/.cursor/skills/` | Cursor runtime (deployed) |

## Development

```bash
just test    # Run tests
just cov     # Coverage report
just lint    # Clippy
just fmt     # Format
```

## License

MIT OR Apache-2.0
