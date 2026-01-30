//! skillc CLI - skc command

use clap::{Parser, Subcommand};
use skillc::config::{
    TargetSpec, find_project_root, find_project_skill, global_runtime_store, global_source_store,
    resolve_source_store,
};
use skillc::deploy::{self, DeployMethod};
use skillc::{InitOptions, LintOptions, OutputFormat, QueryType, StatsOptions, SyncOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "skc")]
#[command(about = "skillc - A development kit for Agent Skills")]
#[command(version)]
#[command(help_template = "\
{before-help}{name} {version}
{about}

USAGE:
    {usage}

COMMANDS:
{subcommands}

OPTIONS:
{options}

For more information, see https://github.com/govctl-org/skillc")]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize skillc project or create a new skill
    Init {
        /// Skill name to create (omit to initialize project only)
        name: Option<String>,

        /// Create skill in global source store (~/.skillc/skills/)
        #[arg(short, long)]
        global: bool,
    },

    /// List all skillc-managed skills
    List {
        /// Filter by scope: project, global, or all
        #[arg(long, value_enum, default_value = "all")]
        scope: ScopeFilter,

        /// Filter by status: normal, not-built, obsolete, or all
        #[arg(long, value_enum, default_value = "all")]
        status: StatusFilter,

        /// Maximum skills to return
        #[arg(short, long)]
        limit: Option<usize>,

        /// Filter by skill name (glob pattern)
        #[arg(short, long)]
        pattern: Option<String>,

        /// Enable obsolete runtime detection
        #[arg(long)]
        check_obsolete: bool,

        /// Output format
        #[arg(short = 'o', long, value_enum, default_value = "text")]
        format: OutputFormat,
    },

    /// Compile a skill to runtime format
    Build {
        /// Skill name (looks in source store) or path to skill directory
        skill: String,

        /// Force SSOT to global (~/.skillc/runtime/) regardless of source location
        #[arg(short, long)]
        global: bool,

        /// Target agents to deploy to (comma-separated, or custom path)
        #[arg(short, long, value_delimiter = ',', default_value = "claude")]
        target: Vec<TargetSpec>,

        /// Force copy instead of symlink/junction for deployment
        #[arg(long)]
        copy: bool,

        /// Force overwrite when importing from a path
        #[arg(short, long)]
        force: bool,
    },

    /// List all sections in a skill
    Outline {
        /// Skill name or path to skill directory
        skill: String,

        /// Maximum heading level to include (1-6)
        #[arg(long)]
        level: Option<usize>,
    },

    /// Show content of a specific section
    Show {
        /// Skill name or path to skill directory
        skill: String,

        /// Section heading to show (case-insensitive)
        #[arg(long)]
        section: String,

        /// Limit search to a specific file
        #[arg(long)]
        file: Option<String>,

        /// Maximum lines to return
        #[arg(long)]
        max_lines: Option<usize>,
    },

    /// Open a file from a skill
    Open {
        /// Skill name or path to skill directory
        skill: String,

        /// Relative path to file within the skill
        path: String,

        /// Maximum lines to return
        #[arg(long)]
        max_lines: Option<usize>,
    },

    /// Show usage analytics for a skill
    Stats {
        /// Skill name or path to skill directory
        skill: String,

        /// Group by dimension
        #[arg(long, value_enum, default_value = "summary")]
        group_by: QueryType,

        /// Output format
        #[arg(short = 'o', long, value_enum, default_value = "text")]
        format: OutputFormat,

        /// Include accesses on or after this time
        #[arg(long)]
        since: Option<String>,

        /// Include accesses on or before this time
        #[arg(long)]
        until: Option<String>,

        /// Include accesses from this project directory (may be repeated)
        #[arg(long)]
        project: Vec<String>,
    },

    /// Search skill content
    Search {
        /// Skill name or path to skill directory
        skill: String,

        /// Search query (bag-of-words, implicit AND)
        query: String,

        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Output format
        #[arg(short = 'o', long, value_enum, default_value = "text")]
        format: SearchFormat,
    },

    /// List source files in a skill
    Sources {
        /// Skill name or path to skill directory
        skill: String,

        /// Maximum tree depth to display (default: unlimited)
        #[arg(long)]
        depth: Option<usize>,

        /// Scope listing to a subdirectory
        #[arg(long)]
        dir: Option<String>,

        /// Maximum entries to display (default: 100)
        #[arg(short, long, default_value = "100")]
        limit: usize,

        /// Filter files by glob pattern (e.g., "*.md")
        #[arg(short, long)]
        pattern: Option<String>,
    },

    /// Sync local logs to global runtime
    Sync {
        /// Specific skill to sync (syncs all if omitted)
        skill: Option<String>,

        /// Project directory to sync from (default: current directory)
        #[arg(long)]
        project: Option<PathBuf>,

        /// Show what would be synced without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Start MCP server for agent integration
    Mcp,

    /// Lint a skill for authoring quality
    Lint {
        /// Skill name or path to skill directory
        skill: String,

        /// Force linting even on compiled skills
        #[arg(short, long)]
        force: bool,
    },
}

/// Search output format per [[RFC-0004:C-SEARCH]]
#[derive(Clone, Debug, clap::ValueEnum)]
enum SearchFormat {
    Text,
    Json,
}

/// Scope filter for list command per [[RFC-0007:C-LIST]]
#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum ScopeFilter {
    Project,
    Global,
    #[default]
    All,
}

/// Status filter for list command per [[RFC-0007:C-LIST]]
#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum StatusFilter {
    Normal,
    NotBuilt,
    Obsolete,
    #[default]
    All,
}

/// Extract skill name from SKILL.md frontmatter.
fn extract_skill_name(skill_dir: &Path) -> skillc::Result<String> {
    let skill_md = skill_dir.join("SKILL.md");
    let content = fs::read_to_string(&skill_md).map_err(|_| {
        skillc::SkillcError::Internal(format!("Cannot read SKILL.md at {}", skill_md.display()))
    })?;

    // Parse YAML frontmatter
    if !content.starts_with("---") {
        return Err(skillc::SkillcError::Internal(
            "SKILL.md missing YAML frontmatter".to_string(),
        ));
    }

    let end = content[3..].find("---").ok_or_else(|| {
        skillc::SkillcError::Internal("SKILL.md frontmatter not closed".to_string())
    })?;

    let frontmatter = &content[3..3 + end];

    // Simple YAML parsing for `name:` field
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("name:") {
            let name = value.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }
    }

    Err(skillc::SkillcError::Internal(
        "SKILL.md missing 'name' in frontmatter".to_string(),
    ))
}

/// Check if a path is inside a .skillc directory.
fn is_inside_skillc(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == ".skillc")
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Per [[RFC-0005:C-CODES]], error messages are printed verbatim to stderr
            // and already include the "error:" prefix.
            eprintln!("{}", e);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> skillc::Result<()> {
    skillc::set_verbose(cli.verbose);

    match cli.command {
        Commands::Init { name, global } => {
            let output = skillc::init(InitOptions { name, global })?;
            println!("{}", output);
        }

        Commands::List {
            scope,
            status,
            limit,
            pattern,
            check_obsolete,
            format,
        } => {
            // Convert CLI filters to library types per [[RFC-0007:C-LIST]]
            let scope_filter = match scope {
                ScopeFilter::Project => Some(skillc::SkillScope::Project),
                ScopeFilter::Global => Some(skillc::SkillScope::Global),
                ScopeFilter::All => None,
            };
            let status_filter = match status {
                StatusFilter::Normal => Some(skillc::SkillStatus::Normal),
                StatusFilter::NotBuilt => Some(skillc::SkillStatus::NotBuilt),
                StatusFilter::Obsolete => Some(skillc::SkillStatus::Obsolete),
                StatusFilter::All => None,
            };

            let options = skillc::ListOptions {
                scope: scope_filter,
                status: status_filter,
                limit,
                pattern,
                check_obsolete,
            };

            let result = skillc::list(&options)?;
            let output = skillc::format_list(&result, format, cli.verbose)?;
            println!("{}", output);
        }

        Commands::Build {
            skill,
            global,
            target,
            copy,
            force,
        } => {
            // Per [[RFC-0001:C-DEPLOYMENT]] - simplified build with import flow

            let path = PathBuf::from(&skill);
            let is_direct_path = path.exists() && path.is_dir() && !is_inside_skillc(&path);

            // Resolve source and project context
            let (source, skill_name, project_root, is_local) = if is_direct_path {
                // === IMPORT FLOW ===
                // Direct path outside .skillc/ → import to source store

                // 1. Extract skill name from frontmatter
                let name = extract_skill_name(&path)?;

                // 2. Determine destination (respect --global flag, else use CWD project or global)
                let (dest_store, is_local) = if global {
                    (global_source_store()?, false)
                } else {
                    resolve_source_store()?
                };
                let dest = dest_store.join(&name);

                // 3. Check for conflicts
                if dest.exists() && !force {
                    return Err(skillc::SkillcError::Internal(format!(
                        "Skill '{}' already exists at {}. Use --force to overwrite.",
                        name,
                        dest.display()
                    )));
                }

                // 4. Copy skill to source store
                if dest.exists() {
                    fs::remove_dir_all(&dest).map_err(|e| {
                        skillc::SkillcError::Io(std::io::Error::new(
                            e.kind(),
                            format!("Failed to remove existing skill: {}", e),
                        ))
                    })?;
                }
                skillc::util::copy_dir_recursive(&path, &dest).map_err(|e| {
                    skillc::SkillcError::Io(std::io::Error::new(
                        e.kind(),
                        format!("Failed to copy skill: {}", e),
                    ))
                })?;

                let scope = if is_local { "project" } else { "global" };
                println!(
                    "Imported {} → {} ({})",
                    path.display(),
                    dest.display(),
                    scope
                );

                let project_root = if is_local { find_project_root() } else { None };
                (dest, name, project_root, is_local)
            } else {
                // === LOOKUP FLOW ===
                // Skill name → find in project source store or global

                // 1. Try project source store (walk up from CWD)
                if let Some((skill_path, project_root)) = find_project_skill(&skill) {
                    let name = skill_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&skill)
                        .to_string();
                    (skill_path, name, Some(project_root), true)
                } else {
                    // 2. Try global source store
                    let global_path = global_source_store()?.join(&skill);
                    if global_path.exists() && global_path.is_dir() {
                        (global_path, skill.clone(), None, false)
                    } else {
                        return Err(skillc::SkillcError::SkillNotFound(skill));
                    }
                }
            };

            // Determine SSOT location per [[RFC-0001:C-DEPLOYMENT]]
            let ssot = if global || !is_local {
                // Global SSOT: ~/.skillc/runtime/<skill>/
                global_runtime_store()?.join(&skill_name)
            } else {
                // Local SSOT: {project}/.skillc/runtime/{skill}/
                match project_root.as_ref() {
                    Some(r) => skillc::util::project_skill_runtime_dir(r, &skill_name),
                    None => global_runtime_store()?.join(&skill_name),
                }
            };

            // Compile to SSOT
            skillc::compile(&source, &ssot)?;

            // Build output summary
            let scope = if is_local && project_root.is_some() {
                "project"
            } else {
                "global"
            };
            println!("Built {} ({})", skill_name, scope);
            println!("  Source:  {}", source.display());
            println!("  Runtime: {}", ssot.display());

            // Deploy to agent directories (project-local if applicable)
            let deploy_root = if is_local {
                project_root.as_deref()
            } else {
                None
            };
            for t in &target {
                let result = deploy::deploy_to_agent(&ssot, t, &skill_name, copy, deploy_root)?;
                let method_str = match result.method {
                    DeployMethod::Symlink => "symlink",
                    DeployMethod::Junction => "junction",
                    DeployMethod::Copy => "copy",
                };
                println!("  Deploy:  {} ({})", result.target.display(), method_str);
            }
        }

        Commands::Outline { skill, level } => {
            let output = skillc::outline(&skill, level, OutputFormat::Text)?;
            println!("{}", output);
        }

        Commands::Show {
            skill,
            section,
            file,
            max_lines,
        } => {
            let output = skillc::show(
                &skill,
                &section,
                file.as_deref(),
                max_lines,
                OutputFormat::Text,
            )?;
            println!("{}", output);
        }

        Commands::Open {
            skill,
            path,
            max_lines,
        } => {
            let output = skillc::open(&skill, &path, max_lines, OutputFormat::Text)?;
            print!("{}", output);
        }

        Commands::Stats {
            skill,
            group_by,
            format,
            since,
            until,
            project,
        } => {
            let output = skillc::stats(
                &skill,
                StatsOptions {
                    query: group_by,
                    format,
                    since,
                    until,
                    projects: project,
                },
            )?;
            println!("{}", output);
        }

        Commands::Search {
            skill,
            query,
            limit,
            format,
        } => {
            let output_format = match format {
                SearchFormat::Json => OutputFormat::Json,
                SearchFormat::Text => OutputFormat::Text,
            };
            let output = skillc::search(&skill, &query, limit, output_format)?;
            println!("{}", output);
        }

        Commands::Sources {
            skill,
            depth,
            dir,
            limit,
            pattern,
        } => {
            let output = skillc::sources(
                &skill,
                depth,
                dir.as_deref(),
                limit,
                pattern.as_deref(),
                OutputFormat::Text,
            )?;
            println!("{}", output);
        }

        Commands::Sync {
            skill,
            project,
            dry_run,
        } => {
            skillc::sync(SyncOptions {
                skill,
                project,
                dry_run,
            })?;
        }

        Commands::Mcp => {
            // MCP server requires async runtime
            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                skillc::SkillcError::Internal(format!("Failed to create tokio runtime: {}", e))
            })?;
            rt.block_on(skillc::mcp::run_server())?;
        }

        Commands::Lint { skill, force } => {
            // Resolve skill path per [[RFC-0007:C-RESOLUTION]]
            let skill_path = {
                let path = PathBuf::from(&skill);
                if path.exists() && path.is_dir() {
                    // Direct path
                    path
                } else if let Some((project_skill_path, _)) = find_project_skill(&skill) {
                    // Found in project source store
                    project_skill_path
                } else {
                    // Fall back to global source store
                    global_source_store()?.join(&skill)
                }
            };

            let result = skillc::lint(&skill_path, LintOptions { force })?;

            // Print diagnostics to stderr per [[RFC-0005:C-CODES]]
            for diag in &result.diagnostics {
                eprintln!("{}", diag);
            }

            // Print summary
            if result.error_count == 0 && result.warning_count == 0 {
                println!("Lint passed: no issues found");
            } else {
                println!(
                    "Lint complete: {} error(s), {} warning(s)",
                    result.error_count, result.warning_count
                );
            }

            // Exit with error if any errors per [[RFC-0008:C-DIAGNOSTICS]]
            if result.error_count > 0 {
                return Err(skillc::SkillcError::Internal(format!(
                    "{} lint error(s) found",
                    result.error_count
                )));
            }
        }
    }

    Ok(())
}
