# Authoring Skills

This guide covers creating and structuring skills with skillc.

## Initialize a project

Before creating skills, initialize skillc in your project:

```bash
skc init
```

This creates `.skillc/skills/` where your skill sources live.

## Create a new skill

```bash
skc init my-skill
```

This creates `.skillc/skills/my-skill/SKILL.md` with minimal frontmatter:

```markdown
---
name: my-skill
description: A brief description of what this skill does
---

# my-skill

Your skill content here...
```

## Skill structure

A skill is a directory containing at least `SKILL.md`. You can add additional markdown files for organization:

```
.skillc/skills/my-skill/
├── SKILL.md              # Required: main entry point
├── examples/
│   └── usage.md          # Optional: examples
├── reference/
│   └── api.md            # Optional: reference docs
└── troubleshooting.md    # Optional: troubleshooting
```

## Frontmatter

Every `SKILL.md` must have YAML frontmatter with at least:

```yaml
---
name: my-skill
description: A brief description
---
```

Optional fields:

```yaml
---
name: my-skill
description: A brief description
version: 1.0.0
author: Your Name
tags: [rust, development]
---
```

## Writing effective content

### Keep it scannable

Agents read skills to find relevant information quickly. Use:

- Clear headings (H2 for major sections, H3 for subsections)
- Bullet points for lists
- Code blocks for examples

### Use progressive disclosure

Put the most important information first. Detailed reference material can go in separate files.

### Include activation triggers

Tell agents when to use this skill:

```markdown
## When to use this skill

Use this skill when:

- Working with Rust projects
- Debugging lifetime errors
- Optimizing Rust performance
```

## Global skills

To create a skill that's available across all projects:

```bash
skc init my-skill --global
```

This creates the skill in `~/.skillc/skills/my-skill/`.

## Next steps

- [Validate your skill](./validating.md) with `skc lint`
- [Test locally](./testing.md) with gateway commands
- [Publish](./publishing.md) when ready
