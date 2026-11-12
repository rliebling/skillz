use std::path::PathBuf;
use std::process::ExitCode;

mod provided_skill;
mod tui;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::{generate, Shell};
use skillz_core::{
    default_config_dir, AddOutcome, App, DriftReason, DriftScan, LibraryImportAction, ListRow,
    Scope, SkillzPaths,
};
use tui::RatatuiPrompter;

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

    /// Operate on the user-global skills scope (`~/.agents/skills`)
    #[arg(long, global = true)]
    user: bool,

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
        /// Also fetch detected sibling dependencies when resolvable locally (best-effort)
        #[arg(long)]
        with_deps: bool,
    },
    /// Formalize a tested Installation into a Library Catalog Entry + Lock
    #[command(after_help = "\
Upstream --source strings (required for `--as upstream` unless a fetch sidecar exists):

  github:owner/repo[/path/to/skill][@rev]
      Catalog may omit @rev; Locks require @rev (commit SHA or tag).
      Example: github:owner/repo/skills/foo@abc1234

  cmd:<install-template>[|version:<version-cmd>][@tool-version]
      Tool-Provided install command template. Placeholders:
        {dir}  Installation directory (required in practice)
        {name} Skill name
      Optional |version:<cmd> overrides the default version probe `{argv0} --version`
      (argv0 = first token of the install template). Catalog stores the template
      without @tool-version; Locks pin the probed tool version after validation.
      Examples:
        cmd:skillz skill install --dir {dir}
        cmd:rexpad skill install --dir {dir}|version:rexpad version
")]
    Libcommit {
        /// Skill name under the Project skills dir
        name: String,
        /// Target Library Binding name
        #[arg(long)]
        library: Option<String>,
        /// How to record the Catalog Entry (`authored` / `upstream`); omit for Branch Lock edit path
        #[arg(long = "as", value_name = "KIND")]
        as_kind: Option<LibcommitKind>,
        /// Upstream source: `github:owner/repo[/path][@rev]` or `cmd:<template>[|version:<cmd>]` (see after help)
        #[arg(long, value_name = "SOURCE")]
        source: Option<String>,
        /// Commit message for Branch Lock libcommit
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Merge a skill's Library Branch Lock into Library main (does not re-Lock the Project)
    Promote {
        /// Skill name whose Branch Lock should land on Library main
        name: String,
    },
    /// After promote: point the Project Lock at Library main and refresh the Installation
    Lock {
        /// Skill name
        name: String,
        /// Re-Lock to Library main and refresh Installation files from main
        #[arg(long)]
        main: bool,
    },
    /// Report Installation Drift against Locks
    #[command(after_help = "\
Scope (default matches interactive `-i` — current scope only):

  Inside a Managed Project:
    skillz drift              this Project only
    skillz drift -i           interactive board for this Project
    skillz drift -g           all Managed Projects + user-global
    skillz drift --project P  one explicit Managed Project

  User-global:
    skillz drift --user       ~/.agents/skills only
    skillz drift -g           all Managed Projects + user-global

  Outside a Managed Project (without --user/--project/-g): error — pick a scope.

Hooks already pass `--project` for the repo being gated.
")]
    Drift {
        /// Exit non-zero when Drift is present (for hooks)
        #[arg(long)]
        check: bool,
        /// Limit to one Managed Project path
        #[arg(long, value_name = "PATH")]
        project: Option<PathBuf>,
        /// Scan every Managed Project in the registry, plus user-global
        #[arg(short = 'g', long = "global", conflicts_with = "project")]
        global: bool,
        /// Allow Drift (also via SKILLZ_ALLOW_DRIFT=1)
        #[arg(long)]
        allow_drift: bool,
        /// Interactive next-actions for the current scope (Ratatui; not with -g)
        #[arg(short = 'i', long, conflicts_with = "global")]
        interactive: bool,
    },
    /// Show or install git/jj Drift hooks for this Managed Project
    Hooks {
        #[command(subcommand)]
        command: HooksCommands,
    },
    /// List skills in this Project (default) or the bound Library Catalog
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
        /// List full bound Library Catalog instead of Project Installations
        #[arg(long)]
        catalog: bool,
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
        /// Automatically add detected sibling dependencies (no prompt)
        #[arg(long)]
        with_deps: bool,
        /// Do not prompt for or add sibling dependencies
        #[arg(long)]
        no_deps: bool,
        /// Interactive multi-select (Ratatui); works without --tag (filter in UI with /)
        #[arg(short = 'i', long)]
        interactive: bool,
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
    /// Manage the user-global skills scope
    User {
        #[command(subcommand)]
        command: UserCommands,
    },
    /// Install or print the Tool-Provided skillz agent skill
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },
    /// Generate shell completions (print script to stdout; redirect to install)
    #[command(after_help = "\
Examples:
  # bash — write into bash-completion's completions dir (or source from ~/.bashrc)
  skillz completions bash > ~/.local/share/bash-completion/completions/skillz

  # zsh — (1) ensure ~/.zfunc is on fpath *before* compinit in ~/.zshrc:
  #         fpath=(~/.zfunc $fpath)
  #         autoload -Uz compinit && compinit
  #       (2) generate the script:
  mkdir -p ~/.zfunc && skillz completions zsh > ~/.zfunc/_skillz
  #       (3) reload the shell (new tab, or: exec zsh)

  # fish
  skillz completions fish > ~/.config/fish/completions/skillz.fish

  # elvish / powershell — print the script, then load it with your shell's usual mechanism
  skillz completions elvish
  skillz completions powershell
")]
    Completions {
        /// Target shell (script goes to stdout — redirect it; see Examples)
        shell: Shell,
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
    #[command(visible_alias = "add")]
    Register {
        /// Path to an existing Library repo
        path: PathBuf,
        /// Registry name for this Library
        #[arg(long)]
        name: String,
        /// Mark the Library as read-only in this registry
        #[arg(long)]
        read_only: bool,
    },
    /// List registered Libraries, or a Library Catalog with `--catalog` / `--library`
    List {
        /// Library Catalog to list (defaults to the registered Library containing cwd)
        #[arg(long)]
        library: Option<String>,
        /// List Catalog Entries instead of the user registry
        #[arg(long)]
        catalog: bool,
        /// Filter Catalog by tag (repeatable; comma-separated allowed; AND)
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Bulk-add Upstream Catalog Entries from a GitHub skills repo
    Import {
        /// `github:owner/repo[/path][@rev]` or `https://github.com/owner/repo…`
        source: String,
        /// Target registered Library name (defaults to the Library containing cwd)
        #[arg(long)]
        library: Option<String>,
        /// Limit discovery to this subtree (after any path in `source`)
        #[arg(long)]
        path: Option<String>,
        /// List what would be added/updated/skipped; write nothing
        #[arg(long)]
        dry_run: bool,
        /// Interactive multi-select of discovered skills (Ratatui)
        #[arg(short = 'i', long)]
        interactive: bool,
        /// Assign tags to imported Upstream entries (repeatable; comma-separated OK)
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum HooksCommands {
    /// Install the Drift gate for this repo's VCS (git pre-commit or jj alias)
    Install,
    /// Show the Drift gate install would write (dry-run; no file changes)
    Show,
}

#[derive(Debug, Subcommand)]
enum ProjectCommands {
    /// Initialize a Managed Project with Library Bindings
    Init {
        /// Library binding (`name` or `name:ro`); repeatable, order = precedence
        #[arg(long = "binding", visible_alias = "bind", required = true)]
        bind: Vec<String>,
        /// Skills directory relative to the Project root
        #[arg(long, default_value = ".agents/skills")]
        skills_dir: String,
    },
    /// Import skills from Skillfile / adopt local leftovers
    Import {
        /// Report detection only
        #[arg(long)]
        dry_run: bool,
        /// Import github Skillfile entries into this Library
        #[arg(long = "from", value_name = "SOURCE")]
        from: Option<String>,
        /// Target Library Binding for import/adopt
        #[arg(long)]
        library: Option<String>,
        /// Adopt leftover local skills: `name=upstream|authored|skip` (repeatable)
        #[arg(long = "adopt-local")]
        adopt_local: Vec<String>,
        /// Interactive per-skill adopt (Ratatui)
        #[arg(short = 'i', long)]
        interactive: bool,
    },
}

#[derive(Debug, Subcommand)]
enum UserCommands {
    /// Initialize user-global skills scope with Library Bindings
    Init {
        /// Library binding (`name` or `name:ro`); repeatable
        #[arg(long = "binding", visible_alias = "bind", required = true)]
        bind: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommands {
    /// Write the embedded skillz SKILL.md into an Installation directory
    Install {
        /// Installation directory (default: `~/.agents/skills/skillz`).
        /// For Tool-Provided `cmd:` sources, pass `{dir}` — SKILL.md is written
        /// directly under that path.
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
        /// Print the target path; write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the embedded skillz SKILL.md to stdout
    Show,
}

/// Build the clap `Command`, hiding the global `--user` flag when help is for
/// `skillz user` (that subcommand already selects user-global scope).
fn command() -> clap::Command {
    let mut cmd = Cli::command();
    if under_user_subcommand(std::env::args()) {
        cmd = cmd.mut_arg("user", |a| a.hide(true));
    }
    cmd
}

fn under_user_subcommand(args: impl IntoIterator<Item = String>) -> bool {
    let mut args = args.into_iter().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => break,
            "--config-dir" => {
                let _ = args.next();
            }
            a if a.starts_with("--config-dir=") || a.starts_with('-') => {}
            "user" => return true,
            _ => return false,
        }
    }
    false
}

fn format_add_outcome(outcome: &AddOutcome) -> String {
    match outcome {
        AddOutcome::Installed(l) => format!("installed ({})", l.content_hash),
        AddOutcome::Refreshed(l) => format!("refreshed ({})", l.content_hash),
        AddOutcome::AlreadyPresent(l) => format!("already present ({})", l.content_hash),
    }
}

fn print_skill_table(rows: &[ListRow]) {
    let mut name_w = "NAME".len();
    let mut lib_w = "LIBRARY".len();
    let mut tags_w = "TAGS".len();
    let mut avail_w = "AVAIL".len();
    let rendered: Vec<(String, String, String, String)> = rows
        .iter()
        .map(|row| {
            let tags = if row.entry.tags.is_empty() {
                "-".to_string()
            } else {
                row.entry.tags.join(",")
            };
            let avail = row.availability.summary();
            name_w = name_w.max(row.entry.name.len());
            lib_w = lib_w.max(row.entry.library.len());
            tags_w = tags_w.max(tags.len());
            avail_w = avail_w.max(avail.len());
            (
                row.entry.name.clone(),
                row.entry.library.clone(),
                tags,
                avail,
            )
        })
        .collect();
    println!(
        "{:<name_w$}  {:<lib_w$}  {:<tags_w$}  {:<avail_w$}",
        "NAME", "LIBRARY", "TAGS", "AVAIL"
    );
    for (name, library, tags, avail) in rendered {
        println!("{name:<name_w$}  {library:<lib_w$}  {tags:<tags_w$}  {avail:<avail_w$}");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepsPolicy {
    Always,
    Never,
    Prompt,
}

fn stdin_confirm(prompt: &str) -> bool {
    eprint!("{prompt} [y/N] ");
    let _ = std::io::Write::flush(&mut std::io::stderr());
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim(), "y" | "Y" | "yes" | "YES")
}

fn resolve_missing_deps(
    app: &App,
    skills: &[String],
    library: Option<&str>,
    policy: DepsPolicy,
    mut prompter: Option<&mut dyn skillz_core::Prompter>,
) -> skillz_core::Result<()> {
    let mut missing: Vec<String> = Vec::new();
    for name in skills {
        for dep in app.missing_sibling_deps(name, library)? {
            if !missing.iter().any(|m| m == &dep) && !skills.iter().any(|s| s == &dep) {
                missing.push(dep);
            }
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    let add_them = match policy {
        DepsPolicy::Always => true,
        DepsPolicy::Never => {
            eprintln!(
                "note: possible sibling deps not added: {} (--no-deps)",
                missing.join(", ")
            );
            false
        }
        DepsPolicy::Prompt => {
            let detail = format!(
                "Possible sibling dependencies not present: {}",
                missing.join(", ")
            );
            eprintln!("{detail}");
            if let Some(ui) = prompter.as_mut() {
                ui.confirm("Add missing sibling dependencies?", &detail)?
            } else if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                stdin_confirm("Add them now?")
            } else {
                eprintln!("(non-interactive; pass --with-deps to add, or --no-deps to silence)");
                false
            }
        }
    };
    if !add_them {
        return Ok(());
    }
    for dep in &missing {
        match app.add_skill_with_deps(dep, library, true) {
            Ok((outcome, warn, still_missing)) => {
                if let Some(w) = warn {
                    eprintln!("warning: {w}");
                }
                println!("{dep}: {}", format_add_outcome(&outcome));
                if !still_missing.is_empty() {
                    eprintln!(
                        "warning: still missing for `{dep}`: {}",
                        still_missing.join(", ")
                    );
                }
            }
            Err(e) => eprintln!("warning: could not add dependency `{dep}`: {e}"),
        }
    }
    Ok(())
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
    let cli = Cli::from_arg_matches(&command().get_matches()).unwrap_or_else(|e| e.exit());

    if let Some(Commands::Completions { shell }) = cli.command {
        let mut cmd = command();
        let name = cmd.get_name().to_string();
        generate(shell, &mut cmd, name, &mut std::io::stdout());
        return Ok(());
    }

    if let Some(Commands::Skill { command }) = cli.command {
        return match command {
            SkillCommands::Show => {
                print!("{}", provided_skill::SKILL_MD);
                Ok(())
            }
            SkillCommands::Install { dir, dry_run } => {
                let dir = dir.unwrap_or_else(provided_skill::default_install_dir);
                if dry_run {
                    let target = provided_skill::skill_md_path(&dir);
                    println!(
                        "dry-run: would install the skillz skill at {}",
                        target.display()
                    );
                    return Ok(());
                }
                let target = provided_skill::install(&dir)?;
                println!("installed the skillz skill at {}", target.display());
                Ok(())
            }
        };
    }

    let config_dir = cli.config_dir.unwrap_or_else(default_config_dir);
    let cwd = std::env::current_dir().map_err(|e| {
        skillz_core::SkillzError::Message(format!("failed to read current directory: {e}"))
    })?;
    let scope = if cli.user {
        Scope::User
    } else {
        Scope::Project
    };
    let app = App::new(SkillzPaths::new(config_dir, cwd)).with_scope(scope);

    match cli.command {
        None => {
            println!(
                "skillz {}\n\nUse `skillz --help` for commands.",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
        Some(Commands::Completions { .. }) => unreachable!("handled above"),
        Some(Commands::Skill { .. }) => unreachable!("handled above"),
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
        Some(Commands::Fetch {
            source,
            skill_name,
            with_deps,
        }) => {
            let (dest, pinned) = app.fetch_skill(&source, skill_name.as_deref())?;
            println!("fetched skill into {}", dest.display());
            if let Some(src) = pinned {
                println!("source: {src}");
            }
            // Best-effort sibling warnings / local with-deps from same tree root.
            if let Some(parent) = dest.parent() {
                let known = skillz_core::deps::list_skill_names(parent).unwrap_or_default();
                let present = known.clone();
                let missing = app
                    .sibling_dep_warnings(&dest, &known, &present)
                    .unwrap_or_default();
                if !missing.is_empty() {
                    eprintln!(
                        "warning: possible sibling deps not present: {} (best-effort)",
                        missing.join(", ")
                    );
                }
                if with_deps {
                    let root = if source.starts_with("github:") {
                        None
                    } else {
                        Some(std::path::PathBuf::from(&source))
                    };
                    if let Some(root) = root {
                        for dep in missing {
                            if app
                                .fetch_skill(root.to_str().unwrap_or(""), Some(&dep))
                                .is_ok()
                            {
                                println!("fetched dependency `{dep}`");
                            }
                        }
                    }
                }
            }
            println!("(Drift until `skillz libcommit`)");
            Ok(())
        }
        Some(Commands::Libcommit {
            name,
            library,
            as_kind,
            source,
            message,
        }) => match as_kind {
            Some(LibcommitKind::Authored) => {
                let library = library.ok_or_else(|| {
                    skillz_core::SkillzError::Message(
                        "`libcommit --as authored` requires --library".into(),
                    )
                })?;
                let lock = app.libcommit_authored(&name, &library)?;
                println!(
                    "libcommit `{name}` → library `{library}` as owned ({})",
                    lock.content_hash
                );
                Ok(())
            }
            Some(LibcommitKind::Upstream) => {
                let library = library.ok_or_else(|| {
                    skillz_core::SkillzError::Message(
                        "`libcommit --as upstream` requires --library".into(),
                    )
                })?;
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
            None => {
                let lock =
                    app.libcommit_branch(&name, library.as_deref(), message.as_deref())?;
                println!(
                    "libcommit `{name}` Branch Lock on {}@{}",
                    lock.branch.as_deref().unwrap_or("?"),
                    lock.rev.as_deref().unwrap_or("?")
                );
                Ok(())
            }
        },
        Some(Commands::Promote { name }) => {
            let rev = app.promote_skill(&name)?;
            println!("promoted `{name}` to library main ({rev})");
            Ok(())
        }
        Some(Commands::Lock { name, main }) => {
            if !main {
                return Err(skillz_core::SkillzError::Message(
                    "only `skillz lock --main` is supported".into(),
                ));
            }
            let lock = app.lock_main(&name)?;
            println!(
                "locked `{name}` to main ({})",
                lock.rev.as_deref().unwrap_or("?")
            );
            Ok(())
        },
        Some(Commands::Drift {
            check,
            project,
            global,
            allow_drift,
            interactive,
        }) => {
            if allow_drift || App::allow_drift_env() {
                if check {
                    println!("drift check skipped (--allow-drift / SKILLZ_ALLOW_DRIFT)");
                }
                return Ok(());
            }
            if interactive {
                // Current App scope only (Project cwd or --user) — same as default report.
                tui::run_drift_board(&app)?;
                return Ok(());
            }
            let scan = if global {
                DriftScan::Global
            } else if let Some(path) = project {
                DriftScan::Project(path)
            } else {
                DriftScan::Current
            };
            let report = app.drift_report(scan)?;
            if report.is_empty() {
                println!("no drift");
                return Ok(());
            }
            let user_skills = app.paths().user_skills_dir();
            for proj in &report {
                let label = if proj.project == user_skills {
                    format!("user-global: {}", proj.project.display())
                } else {
                    format!("project: {}", proj.project.display())
                };
                println!("{label}");
                let name_width = proj
                    .items
                    .iter()
                    .map(|item| item.name.len())
                    .max()
                    .unwrap_or(0);
                for item in &proj.items {
                    let reason = match &item.reason {
                        DriftReason::Unlocked => "unlocked (not in skillz.lock)",
                        DriftReason::MissingFiles => "missing installation files",
                        DriftReason::HashMismatch { .. } => "content hash differs from Lock",
                    };
                    println!("  {:<width$}  {reason}", item.name, width = name_width);
                }
            }
            if check {
                return Err(skillz_core::SkillzError::Message(
                    "drift detected; commit blocked (use --allow-drift or SKILLZ_ALLOW_DRIFT=1)"
                        .into(),
                ));
            }
            Ok(())
        }
        Some(Commands::Hooks { command }) => match command {
            HooksCommands::Install => {
                let paths = app.hooks_install()?;
                for path in paths {
                    println!("installed drift gate at {}", path.display());
                }
                Ok(())
            }
            HooksCommands::Show => {
                let plans = app.hooks_show()?;
                for (i, plan) in plans.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    println!("{}", plan.kind.as_str());
                    println!("  path:    {}", plan.path.display());
                    println!("  status:  {}", plan.status.as_str());
                    println!("  ---");
                    for line in plan.contents.lines() {
                        println!("  {line}");
                    }
                }
                Ok(())
            }
        },
        Some(Commands::List {
            library,
            tags,
            dedupe,
            catalog,
        }) => {
            let rows = app.list_skills(library.as_deref(), &tags, dedupe, !catalog)?;
            if rows.is_empty() {
                if catalog {
                    println!("(no matching Catalog Entries)");
                } else {
                    println!("(no skills in this Project; try `skillz list --catalog`)");
                }
                return Ok(());
            }
            print_skill_table(&rows);
            Ok(())
        }
        Some(Commands::Add {
            name,
            library,
            tags,
            with_deps,
            no_deps,
            interactive,
        }) => {
            if with_deps && no_deps {
                return Err(skillz_core::SkillzError::Message(
                    "pass only one of --with-deps or --no-deps".into(),
                ));
            }
            let deps_policy = if with_deps {
                DepsPolicy::Always
            } else if no_deps {
                DepsPolicy::Never
            } else {
                DepsPolicy::Prompt
            };

            if interactive {
                let mut ui = RatatuiPrompter;
                let added = app.interactive_add_by_tag(&tags, library.as_deref(), &mut ui)?;
                if added.is_empty() {
                    println!("(no skills added)");
                    return Ok(());
                }
                for skill_name in &added {
                    println!("added {skill_name}");
                }
                resolve_missing_deps(&app, &added, library.as_deref(), deps_policy, Some(&mut ui))?;
                return Ok(());
            }
            if !tags.is_empty() {
                let results = app.add_skills_by_tag(&tags, library.as_deref())?;
                if results.is_empty() {
                    println!("(no matching skills to add)");
                    return Ok(());
                }
                let mut names = Vec::new();
                for (skill_name, outcome, warn) in results {
                    if let Some(w) = warn {
                        eprintln!("warning: {w}");
                    }
                    println!("{}: {}", skill_name, format_add_outcome(&outcome));
                    names.push(skill_name);
                }
                resolve_missing_deps(&app, &names, library.as_deref(), deps_policy, None)?;
                return Ok(());
            }
            let Some(name) = name else {
                return Err(skillz_core::SkillzError::Message(
                    "provide a skill name, --tag, or -i".into(),
                ));
            };
            let (outcome, warn, _) =
                app.add_skill_with_deps(&name, library.as_deref(), false)?;
            if let Some(w) = warn {
                eprintln!("warning: {w}");
            }
            println!("{name}: {}", format_add_outcome(&outcome));
            resolve_missing_deps(&app, &[name], library.as_deref(), deps_policy, None)?;
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
            LibraryCommands::Register {
                path,
                name,
                read_only,
            } => {
                let entry = app.library_register(&path, &name, read_only)?;
                let ro = if entry.read_only { " (read-only)" } else { "" };
                println!(
                    "registered library `{}` at {}{ro}",
                    entry.name,
                    entry.path.display()
                );
                Ok(())
            }
            LibraryCommands::List {
                library,
                catalog,
                tags,
            } => {
                if catalog || library.is_some() {
                    let name = app.resolve_library_name(library.as_deref())?;
                    let rows = app.list_library_catalog(&name, &tags, false)?;
                    if rows.is_empty() {
                        println!("(no matching Catalog Entries in library `{name}`)");
                        return Ok(());
                    }
                    print_skill_table(&rows);
                    return Ok(());
                }
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
            LibraryCommands::Import {
                source,
                library,
                path,
                dry_run,
                interactive,
                tags,
            } => {
                if interactive && dry_run {
                    return Err(skillz_core::SkillzError::Message(
                        "`library import` refuses --dry-run with -i; omit one of them".into(),
                    ));
                }
                let library = app.resolve_library_name(library.as_deref())?;
                let report = if interactive {
                    let mut ui = RatatuiPrompter;
                    app.interactive_library_import(
                        &source,
                        &library,
                        path.as_deref(),
                        &tags,
                        &mut ui,
                    )?
                } else {
                    app.library_import(&source, &library, path.as_deref(), dry_run, &tags)?
                };
                let prefix = if report.dry_run { "dry-run " } else { "" };
                if report.items.is_empty() {
                    println!("{prefix}no skills discovered");
                    return Ok(());
                }
                for item in &report.items {
                    match &item.action {
                        LibraryImportAction::Add => {
                            println!("{prefix}add\t{}\t{}", item.name, item.source);
                        }
                        LibraryImportAction::Update => {
                            println!("{prefix}update\t{}\t{}", item.name, item.source);
                        }
                        LibraryImportAction::SkipIdentical => {
                            println!("{prefix}skip\t{}\t{} (identical)", item.name, item.source);
                        }
                        LibraryImportAction::SkipCollision { existing_source } => {
                            eprintln!(
                                "warning: skip `{}` — name collision with existing source `{existing_source}`",
                                item.name
                            );
                            println!(
                                "{prefix}skip\t{}\t{} (collision)",
                                item.name, item.source
                            );
                        }
                        LibraryImportAction::SkipOwned => {
                            eprintln!(
                                "warning: skip `{}` — Owned Entry already exists in library",
                                item.name
                            );
                            println!("{prefix}skip\t{}\t{} (owned)", item.name, item.source);
                        }
                    }
                }
                let added = report
                    .items
                    .iter()
                    .filter(|i| {
                        matches!(
                            i.action,
                            LibraryImportAction::Add | LibraryImportAction::Update
                        )
                    })
                    .count();
                println!(
                    "{prefix}{} skill(s) planned; {} add/update for library `{}`",
                    report.items.len(),
                    added,
                    report.library
                );
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
            ProjectCommands::Import {
                dry_run,
                from,
                library,
                adopt_local,
                interactive,
            } => {
                if interactive {
                    let library = library.as_deref().ok_or_else(|| {
                        skillz_core::SkillzError::Message(
                            "`project import -i` requires --library".into(),
                        )
                    })?;
                    let mut ui = RatatuiPrompter;
                    let results = app.interactive_import_adopt(library, &mut ui)?;
                    for (name, msg) in results {
                        println!("{name}: {msg}");
                    }
                    return Ok(());
                }
                if dry_run {
                    let plan = app.project_import_dry_run()?;
                    println!("skills_dir: {}", plan.skills_dir.display());
                    println!(
                        "skillfile: {}",
                        plan.skillfile
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "(none)".into())
                    );
                    println!("skillfile entries:");
                    for e in &plan.skillfile_entries {
                        println!("  - {} ({})", e.name, e.source);
                    }
                    println!("local skills: {}", plan.local_skills.join(", "));
                    println!("leftovers:");
                    for name in &plan.leftovers {
                        println!("  - {name}");
                    }
                    return Ok(());
                }
                let mut did_work = false;
                if let Some(ref from_src) = from {
                    if from_src != "skillfile" {
                        return Err(skillz_core::SkillzError::Message(format!(
                            "unknown --from `{from_src}` (supported: skillfile)"
                        )));
                    }
                    let library = library.as_deref().ok_or_else(|| {
                        skillz_core::SkillzError::Message(
                            "`project import --from skillfile` requires --library".into(),
                        )
                    })?;
                    let imported = app.project_import_from_skillfile(library)?;
                    println!("imported {} Skillfile entr(y/ies)", imported.len());
                    for name in imported {
                        println!("  - {name}");
                    }
                    did_work = true;
                }
                if !adopt_local.is_empty() {
                    let library = library.as_deref().ok_or_else(|| {
                        skillz_core::SkillzError::Message(
                            "`--adopt-local` requires --library".into(),
                        )
                    })?;
                    let results = app.project_adopt_local(library, &adopt_local)?;
                    for (name, msg) in results {
                        println!("{name}: {msg}");
                    }
                    did_work = true;
                }
                if !did_work {
                    return Err(skillz_core::SkillzError::Message(
                        "specify --dry-run, --from skillfile, and/or --adopt-local".into(),
                    ));
                }
                Ok(())
            }
        },
        Some(Commands::User { command }) => match command {
            UserCommands::Init { bind } => {
                let config = app.user_init(&bind)?;
                println!(
                    "initialized user scope with {} binding(s); skills_dir={}",
                    config.bindings.len(),
                    config.skills_dir
                );
                Ok(())
            }
        },
    }
}
