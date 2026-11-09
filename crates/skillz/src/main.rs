use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use skillz_core::{default_config_dir, AddOutcome, App, DriftReason, SkillzPaths};

#[derive(Debug, Parser)]
#[command(
    name = "skillz",
    version,
    about = "Manage agent skill Libraries and Managed Projects",
    long_about = None
)]
struct Cli {
    /// Override skillz config directory (default: ~/.config/skillz)
    #[arg(long, global = true, value_name = "DIR", env = "SKILLZ_CONFIG_DIR")]
    config_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Show Managed Project bindings and installations
    Status,
    /// Scaffold a new Authored skill in the Project skills dir
    New {
        /// Skill name (directory under skills_dir)
        name: String,
        /// Optional tags written into SKILL.md frontmatter
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Fetch an Upstream skill into the Project skills dir (Drift until libcommit)
    Fetch {
        /// Filesystem path or `github:owner/repo[/path][@rev]`
        source: String,
        /// Skill name for nested lookup (`github:owner/repo NAME`)
        skill_name: Option<String>,
    },
    /// Formalize a tested Installation into a Library Catalog Entry + Lock
    Libcommit {
        /// Skill name under the Project skills dir
        name: String,
        /// Target Library Binding name
        #[arg(long)]
        library: String,
        /// How to record the Catalog Entry
        #[arg(long = "as", value_name = "KIND")]
        as_kind: LibcommitKind,
        /// Upstream source string (defaults to fetch sidecar)
        #[arg(long)]
        source: Option<String>,
    },
    /// Report Installation Drift against Locks
    Drift,
    /// List Catalog Entries from bound Libraries
    List {
        /// Restrict to one Library Binding
        #[arg(long)]
        library: Option<String>,
        /// Filter by tag (repeatable; comma-separated allowed; AND)
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Keep only the first Binding match per skill name
        #[arg(long)]
        dedupe: bool,
    },
    /// Install Catalog Entries into this Managed Project
    Add {
        /// Skill name to install (omit when using --tag)
        name: Option<String>,
        /// Restrict / disambiguate Library Binding
        #[arg(long)]
        library: Option<String>,
        /// Add all Catalog Entries matching tags (AND)
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Manage Libraries in the user registry
    Library {
        #[command(subcommand)]
        command: LibraryCommands,
    },
    /// Manage the current Project as a Managed Project
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum LibcommitKind {
    Authored,
    Upstream,
}

#[derive(Debug, Subcommand)]
enum LibraryCommands {
    /// Create a Library skeleton and register it
    Init {
        /// Directory for the new Library
        path: PathBuf,
        /// Registry name for this Library
        #[arg(long)]
        name: String,
    },
    /// Register an existing Library checkout
    Add {
        /// Path to an existing Library repo
        path: PathBuf,
        /// Registry name for this Library
        #[arg(long)]
        name: String,
        /// Mark the Library as read-only in this registry
        #[arg(long)]
        read_only: bool,
    },
    /// List registered Libraries
    List,
}

#[derive(Debug, Subcommand)]
enum ProjectCommands {
    /// Initialize a Managed Project with Library Bindings
    Init {
        /// Library binding (`name` or `name:ro`); repeatable, order = precedence
        #[arg(long = "bind", required = true)]
        bind: Vec<String>,
        /// Skills directory relative to the Project root
        #[arg(long, default_value = ".agents/skills")]
        skills_dir: String,
    },
}

fn format_add_outcome(outcome: &AddOutcome) -> String {
    match outcome {
        AddOutcome::Installed(l) => format!("installed ({})", l.content_hash),
        AddOutcome::Refreshed(l) => format!("refreshed ({})", l.content_hash),
        AddOutcome::AlreadyPresent(l) => format!("already present ({})", l.content_hash),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> skillz_core::Result<()> {
    let cli = Cli::parse();
    let config_dir = cli.config_dir.unwrap_or_else(default_config_dir);
    let cwd = std::env::current_dir().map_err(|e| {
        skillz_core::SkillzError::Message(format!("failed to read current directory: {e}"))
    })?;
    let app = App::new(SkillzPaths::new(config_dir, cwd));

    match cli.command {
        None => {
            println!(
                "skillz {}\n\nUse `skillz --help` for commands.",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
        Some(Commands::Status) => {
            let report = app.status()?;
            println!("cwd: {}", report.cwd.display());
            println!("config: {}", report.config_dir.display());
            if report.managed {
                if let Some(skills_dir) = &report.skills_dir {
                    println!("skills_dir: {skills_dir}");
                }
                println!("bindings:");
                if report.bindings.is_empty() {
                    println!("  (none)");
                } else {
                    for (i, b) in report.bindings.iter().enumerate() {
                        let ro = if b.read_only { "ro" } else { "rw" };
                        println!("  {}. {} [{ro}] {}", i + 1, b.name, b.url);
                    }
                }
                println!("installations:");
                if report.installations.is_empty() {
                    println!("  (none)");
                } else {
                    for name in &report.installations {
                        println!("  - {name}");
                    }
                }
            }
            println!("{}", report.message);
            Ok(())
        }
        Some(Commands::New { name, tags }) => {
            let path = app.skill_new(&name, &tags)?;
            println!("scaffolded skill at {}", path.display());
            println!("(Drift until `skillz libcommit`)");
            Ok(())
        }
        Some(Commands::Fetch { source, skill_name }) => {
            let (dest, pinned) = app.fetch_skill(&source, skill_name.as_deref())?;
            println!("fetched skill into {}", dest.display());
            if let Some(src) = pinned {
                println!("source: {src}");
            }
            println!("(Drift until `skillz libcommit`)");
            Ok(())
        }
        Some(Commands::Libcommit {
            name,
            library,
            as_kind,
            source,
        }) => match as_kind {
            LibcommitKind::Authored => {
                let lock = app.libcommit_authored(&name, &library)?;
                println!(
                    "libcommit `{name}` → library `{library}` as owned ({})",
                    lock.content_hash
                );
                Ok(())
            }
            LibcommitKind::Upstream => {
                let lock = app.libcommit_upstream(&name, &library, source.as_deref())?;
                println!(
                    "libcommit `{name}` → library `{library}` as upstream ({})",
                    lock.content_hash
                );
                if let Some(src) = &lock.source {
                    println!("source: {src}");
                }
                Ok(())
            }
        },
        Some(Commands::Drift) => {
            let items = app.detect_drift()?;
            if items.is_empty() {
                println!("no drift");
                return Ok(());
            }
            for item in items {
                let reason = match &item.reason {
                    DriftReason::Unlocked => "unlocked (not in skillz.lock)".to_string(),
                    DriftReason::MissingFiles => "missing installation files".to_string(),
                    DriftReason::HashMismatch { .. } => {
                        "content hash differs from Lock".to_string()
                    }
                };
                println!("{}\t{reason}", item.name);
            }
            Ok(())
        }
        Some(Commands::List {
            library,
            tags,
            dedupe,
        }) => {
            let rows = app.list_skills(library.as_deref(), &tags, dedupe)?;
            if rows.is_empty() {
                println!("(no matching skills)");
                return Ok(());
            }
            for row in rows {
                let tags = if row.entry.tags.is_empty() {
                    "-".to_string()
                } else {
                    row.entry.tags.join(",")
                };
                println!(
                    "{}\t{}\t{}\t{}",
                    row.entry.name,
                    row.entry.library,
                    tags,
                    row.availability.summary()
                );
            }
            Ok(())
        }
        Some(Commands::Add {
            name,
            library,
            tags,
        }) => {
            if !tags.is_empty() {
                let results = app.add_skills_by_tag(&tags, library.as_deref())?;
                if results.is_empty() {
                    println!("(no matching skills to add)");
                    return Ok(());
                }
                for (skill_name, outcome, warn) in results {
                    if let Some(w) = warn {
                        eprintln!("warning: {w}");
                    }
                    println!("{}: {}", skill_name, format_add_outcome(&outcome));
                }
                return Ok(());
            }
            let Some(name) = name else {
                return Err(skillz_core::SkillzError::Message(
                    "provide a skill name or --tag".into(),
                ));
            };
            let (outcome, warn) = app.add_skill(&name, library.as_deref())?;
            if let Some(w) = warn {
                eprintln!("warning: {w}");
            }
            println!("{name}: {}", format_add_outcome(&outcome));
            Ok(())
        }
        Some(Commands::Library { command }) => match command {
            LibraryCommands::Init { path, name } => {
                let entry = app.library_init(&path, &name)?;
                println!(
                    "initialized library `{}` at {}",
                    entry.name,
                    entry.path.display()
                );
                Ok(())
            }
            LibraryCommands::Add {
                path,
                name,
                read_only,
            } => {
                let entry = app.library_add(&path, &name, read_only)?;
                let ro = if entry.read_only { " (read-only)" } else { "" };
                println!(
                    "registered library `{}` at {}{ro}",
                    entry.name,
                    entry.path.display()
                );
                Ok(())
            }
            LibraryCommands::List => {
                let entries = app.library_list()?;
                if entries.is_empty() {
                    println!("(no libraries registered)");
                    return Ok(());
                }
                for e in entries {
                    let vcs = e.vcs.map(|v| v.as_str()).unwrap_or("-");
                    let ro = if e.read_only { "ro" } else { "rw" };
                    println!("{}\t{ro}\t{vcs}\t{}", e.name, e.path.display());
                }
                Ok(())
            }
        },
        Some(Commands::Project { command }) => match command {
            ProjectCommands::Init { bind, skills_dir } => {
                let skills = if skills_dir == ".agents/skills" {
                    None
                } else {
                    Some(skills_dir.as_str())
                };
                let config = app.project_init(&bind, skills)?;
                println!(
                    "initialized Managed Project with {} binding(s); skills_dir={}",
                    config.bindings.len(),
                    config.skills_dir
                );
                for b in &config.bindings {
                    let ro = if b.read_only { "ro" } else { "rw" };
                    println!("  - {} [{ro}] {}", b.name, b.url);
                }
                Ok(())
            }
        },
    }
}
