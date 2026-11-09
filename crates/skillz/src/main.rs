use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use skillz_core::{default_config_dir, App, SkillzPaths};

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
