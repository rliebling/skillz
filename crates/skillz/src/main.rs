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
    /// Show Managed Project / binding status (stub)
    Status,
    /// Manage Libraries in the user registry
    Library {
        #[command(subcommand)]
        command: LibraryCommands,
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
                    let vcs = e
                        .vcs
                        .map(|v| v.as_str())
                        .unwrap_or("-");
                    let ro = if e.read_only { "ro" } else { "rw" };
                    println!("{}\t{ro}\t{vcs}\t{}", e.name, e.path.display());
                }
                Ok(())
            }
        },
    }
}
