# Introduction

**skillc** is the development kit for [Agent Skills](https://agentskills.io/) — the open format for extending AI agent capabilities with specialized knowledge and workflows.

## At a Glance

```text
                        SKILL.md (source)
                              │
              ┌───────────────┴───────────────┐
              │                               │
              ▼                               ▼
   ┌─────────────────────┐         ┌─────────────────────┐
   │  skillc (optional)  │         │      git push       │
   │                     │         │         ↓           │
   │  lint · build       │         │  npx skills add     │
   │  stats · search     │         │         ↓           │
   │         ↓           │         │  Claude · Cursor    │
   │  local testing      │         │  Codex · Copilot    │
   └─────────────────────┘         └─────────────────────┘
```

**skillc helps authors** validate and test skills locally. Consumers just run `npx skills add` — no compilation needed.

## Who is this for?

| Audience          | What skillc provides                                |
| ----------------- | --------------------------------------------------- |
| **Skill authors** | Create, validate, and test skills before publishing |
| **Power users**   | Track how agents use skills locally                 |

## Core Commands

| Command   | What it does                                   |
| --------- | ---------------------------------------------- |
| `init`    | Create a new skill or project structure        |
| `lint`    | Validate structure, frontmatter, and links     |
| `build`   | Compile for local testing and deploy to agents |
| `list`    | Show all managed skills                        |
| `outline` | List sections across all files                 |
| `show`    | Retrieve specific section content              |
| `open`    | Read a file                                    |
| `search`  | Full-text search with FTS5                     |
| `sources` | Tree view of source files                      |
| `stats`   | Usage analytics                                |
| `sync`    | Merge local logs to global store               |
| `mcp`     | Start MCP server for agent integration         |

## Two Interfaces

| Interface       | For                 | Example             |
| --------------- | ------------------- | ------------------- |
| **CLI** (`skc`) | Humans, scripts, CI | `skc lint my-skill` |
| **MCP**         | AI agents directly  | `skc_lint` tool     |

Both expose the same functionality. MCP provides structured output for agent integration.

## Key Concepts

### Source vs. Compiled

|              | Source                            | Compiled                                  |
| ------------ | --------------------------------- | ----------------------------------------- |
| **What**     | Your `SKILL.md` with full content | Stub directing agents to gateway commands |
| **For**      | Publishing to GitHub              | Local development/testing                 |
| **Publish?** | Yes                               | No                                        |

**Always push source, never compiled output.**

### Skill Resolution

When you run `skc <command> my-skill`, skillc searches:

1. **Project**: `.skillc/skills/my-skill/` (from current directory upward)
2. **Global**: `~/.skillc/skills/my-skill/`

Project-local skills take precedence.

### Project Detection

skillc walks upward from your current directory looking for `.skillc/`:

```
~/projects/myapp/src/          ← you are here
~/projects/myapp/.skillc/      ← project root found
```

Run commands from anywhere within a project.

## Workflow Guides

- **[Authoring](./workflows/authoring.md)** — create and structure skills
- **[Validating](./workflows/validating.md)** — lint and check quality
- **[Testing](./workflows/testing.md)** — local testing with gateway commands
- **[Publishing](./workflows/publishing.md)** — git-based distribution
- **[Analytics](./workflows/analytics.md)** — track skill usage

## Supported Agents

skillc deploys to these agent directories:

| Target     | Directory             |
| ---------- | --------------------- |
| `claude`   | `~/.claude/skills/`   |
| `codex`    | `~/.codex/skills/`    |
| `copilot`  | `~/.github/skills/`   |
| `cursor`   | `~/.cursor/skills/`   |
| `gemini`   | `~/.gemini/skills/`   |
| `kiro`     | `~/.kiro/skills/`     |
| `opencode` | `~/.opencode/skills/` |
| `trae`     | `~/.trae/skills/`     |

Use `skc build my-skill --target cursor` to deploy to a specific agent.
