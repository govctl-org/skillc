# Publishing Skills

This guide covers publishing skills for distribution via the [skills](https://skills.sh/) ecosystem.

## What you publish

You publish **source files**, not compiled output.

```
Published (source):           NOT published (compiled):
├── skills/                   ├── SKILL.md (stub)
│   └── my-skill/             └── .skillc-meta/
│       ├── SKILL.md              ├── manifest.json
│       └── examples/             └── index.tantivy/
```

The compilation step (`skc build`) is for local development and testing. Consumers download source directly and don't need skillc.

## Repository structure

The [skills](https://skills.sh/) tool expects this structure:

```
your-repo/
└── skills/
    └── my-skill/
        ├── SKILL.md
        └── ...
```

## Publishing workflow

### Option 1: Dedicated skills repository

Make `.skillc/` its own git repository:

```bash
cd .skillc
git init
git add skills/
git commit -m "Initial skill"
git remote add origin git@github.com:you/my-skills.git
git push -u origin main
```

The repo root becomes the published structure:

```
my-skills/           # repo root
└── skills/
    └── my-skill/
```

### Option 2: Monorepo with skills

If your project already has a `skills/` directory, you can author directly there:

```bash
# Initialize skillc to use skills/ instead of .skillc/skills/
# (configure via .skillc/config.toml if needed)
```

Or maintain both and sync:

```bash
# Your project structure
project/
├── .skillc/
│   └── skills/my-skill/    # author here
├── skills/
│   └── my-skill/           # publish here (copy or symlink)
└── src/
```

## Before publishing

Run through this checklist:

1. **Lint passes**: `skc lint my-skill` reports no errors
2. **Content complete**: All sections filled in
3. **Examples work**: Code examples are tested
4. **Links valid**: No broken internal links

## Consumer experience

After you publish, consumers install with:

```bash
npx skills add github:you/my-skills
```

This downloads `skills/my-skill/` to their agent directory. They don't need skillc — the agent reads source files directly.

## Versioning

Use git tags or branches for versioning:

```bash
git tag v1.0.0
git push --tags
```

Consumers can install specific versions:

```bash
npx skills add github:you/my-skills@v1.0.0
```

## Multiple skills

One repository can contain multiple skills:

```
my-skills/
└── skills/
    ├── rust/
    │   └── SKILL.md
    ├── cuda/
    │   └── SKILL.md
    └── typst/
        └── SKILL.md
```

Consumers choose which to install:

```bash
npx skills add github:you/my-skills --skill rust
```

## Next steps

- [Track usage](./analytics.md) after publishing
- Share your skill on [skills.sh](https://skills.sh/)
