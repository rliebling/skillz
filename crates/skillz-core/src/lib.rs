//! Application command API for skillz.
//!
//! CLI and future TUI share this seam.

use std::path::{Path, PathBuf};

use thiserror::Error;

pub mod catalog;
pub mod deps;
pub mod fetch;
pub mod hooks;
pub mod import;
pub mod interactive;
pub mod library;
pub mod migrate;
pub mod project;
pub mod registry;
pub mod skill;
pub mod source;
pub mod ui;
pub mod vcs;
pub mod vcs_ops;

pub use import::{AdoptAs, ImportPlan, SkillfileEntry};
pub use migrate::{
    CatalogMatch, MgmtState, MigrateAction, MigrateOptions, MigratePlan, MigratePromptAnswer,
    MigrateSituationClass, MigrateSkillPlan,
};
pub use ui::{
    CancelPrompter, Prompter, SelectAllPrompter, SelectFirstNPrompter, SelectItem,
};

pub use catalog::{Availability, CatalogEntry};
pub use library::{
    ensure_library_meta, init_library_skeleton, Contribution, LibraryMeta,
};

/// Batch mode for [`App::publish_skills`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PublishMode {
    /// Create from Local when Catalog-missing; update when Library-backed.
    #[default]
    Auto,
    /// Fail if any name would create a Catalog Entry.
    UpdateOnly,
    /// Fail if any name already exists in the Catalog.
    CreateOnly,
}

/// One skill result from [`App::publish_skills`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishSkillResult {
    pub name: String,
    pub kind: EntryKind,
    pub created: bool,
    pub lock: InstallationLock,
}

/// Options for [`App::publish_skills`] / [`App::publish_with_options`].
#[derive(Debug, Clone, Default)]
pub struct PublishOptions {
    pub mode: PublishMode,
    pub message: Option<String>,
    pub dry_run: bool,
    /// Review Libraries: skip `git push` / `jj git push` (default: push).
    pub no_push: bool,
    /// Review Libraries: skip forge PR/MR create after push (default: open).
    pub no_pr: bool,
    /// Test seam: replace real push (`Ok` = pretend pushed, `Err` = soft-fail warning).
    pub mock_push: Option<std::result::Result<(), String>>,
    /// Test seam: replace real `gh`/`glab` (`Ok(Some(url))`, `Ok(None)` skipped, `Err` soft-fail).
    pub mock_pr: Option<std::result::Result<Option<String>, String>>,
}

/// Report from [`App::publish_skills`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReport {
    pub library: String,
    pub contribution: Contribution,
    pub rev: String,
    /// Review Branch Lock branch (None for direct main publish).
    pub branch: Option<String>,
    pub skills: Vec<PublishSkillResult>,
    /// Sibling deps referenced by the batch but not included in the publish names.
    pub missing_deps: Vec<String>,
    pub dry_run: bool,
    pub pushed: bool,
    pub pr_url: Option<String>,
    pub warnings: Vec<String>,
    pub next_steps: Vec<String>,
}
pub use project::{Binding, ProjectConfig, ProjectLock, DEFAULT_SKILLS_DIR};
pub use registry::{LibraryRecord, Registry};
pub use skill::{
    CatalogSuggestion, DriftAction, DriftItem, DriftReason, EntryKind, InstallationLock,
    LOCAL_LIBRARY,
};
pub use vcs::{infer_vcs, Vcs};

/// Filesystem roots skillz resolves against for a given invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillzPaths {
    /// Overridden config root (tests inject a temp dir).
    pub config_dir: PathBuf,
    /// Current working directory used for Project discovery.
    pub cwd: PathBuf,
    /// User home (for `~/.agents/skills` Availability).
    pub home_dir: PathBuf,
}

impl SkillzPaths {
    pub fn new(config_dir: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        let home_dir = dirs_home().unwrap_or_else(|| PathBuf::from("/"));
        Self::with_home(config_dir, cwd, home_dir)
    }

    pub fn with_home(
        config_dir: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        home_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            config_dir: config_dir.into(),
            cwd: cwd.into(),
            home_dir: home_dir.into(),
        }
    }

    pub fn registry_path(&self) -> PathBuf {
        self.config_dir.join("registry.toml")
    }

    pub fn user_skills_dir(&self) -> PathBuf {
        self.home_dir.join(".agents").join("skills")
    }

    pub fn user_config_path(&self) -> PathBuf {
        self.config_dir.join("user.toml")
    }

    pub fn user_lock_path(&self) -> PathBuf {
        self.config_dir.join("user.lock")
    }
}

/// Whether commands operate on the cwd Managed Project or user-global skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    Project,
    User,
}

/// Errors returned by the application command API.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SkillzError {
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, SkillzError>;

/// Snapshot returned by [`App::status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    pub cwd: PathBuf,
    pub config_dir: PathBuf,
    pub managed: bool,
    pub skills_dir: Option<String>,
    pub bindings: Vec<Binding>,
    pub installations: Vec<String>,
    /// Branch Locks waiting for merge onto Library main (not hard Drift).
    pub pending_review: Vec<String>,
    pub message: String,
}

/// A Library as shown by [`App::library_list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryListEntry {
    pub name: String,
    pub path: PathBuf,
    pub read_only: bool,
    pub vcs: Option<Vcs>,
}

/// Outcome for one discovered skill during [`App::library_import`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryImportAction {
    Add,
    Update,
    SkipIdentical,
    SkipCollision { existing_source: String },
    SkipOwned,
}

impl LibraryImportAction {
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            Self::SkipCollision { .. } | Self::SkipOwned
        )
    }
}

/// How [`App::library_import`] handles name conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryImportOnConflict {
    /// Abort the entire import when any conflict exists (default).
    Fail,
    /// Import non-conflicting rows; soft-skip conflicts.
    Skip,
    /// Resolve each conflict interactively, then apply atomically.
    Prompt,
}

/// Resolution chosen for one import conflict under [`LibraryImportOnConflict::Prompt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryImportConflictResolution {
    Skip,
    /// Import under a different Catalog name.
    Rename(String),
    /// Abort the whole import with no writes.
    Abort,
}

/// One planned/applied row from [`App::library_import`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryImportItem {
    pub name: String,
    pub source: String,
    pub action: LibraryImportAction,
}

/// Report from [`App::library_import`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryImportReport {
    pub library: String,
    pub dry_run: bool,
    pub items: Vec<LibraryImportItem>,
}

/// A discovered skill candidate before catalog upsert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryImportCandidate {
    pub name: String,
    pub rel_path: String,
    pub source: String,
}

/// Apply `--name` override: requires exactly one candidate; renames catalog entry only.
fn apply_library_import_name_override(
    candidates: &mut [LibraryImportCandidate],
    name_override: Option<&str>,
) -> Result<()> {
    let Some(name) = name_override else {
        return Ok(());
    };
    skill::validate_skill_name(name)?;
    match candidates.len() {
        1 => {
            candidates[0].name = name.to_string();
            Ok(())
        }
        n => Err(SkillzError::Message(format!(
            "`--name` requires exactly one skill to import (found {n}); narrow with a skill path or -i"
        ))),
    }
}

/// A list row including Availability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    pub entry: CatalogEntry,
    pub availability: Availability,
}

/// Drift for one Managed Project (or the user-global skills scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDrift {
    pub project: PathBuf,
    pub items: Vec<DriftItem>,
}

/// Which Installations [`App::drift_report`] should scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftScan {
    /// Current Managed Project (`cwd`) or user-global scope when `App` is [`Scope::User`].
    /// Matches interactive `drift -i` (current scope only).
    Current,
    /// Every Managed Project in the user registry, plus user-global when configured.
    Global,
    /// One explicit Managed Project path.
    Project(PathBuf),
}

/// Result of [`App::add_skill`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddOutcome {
    Installed(InstallationLock),
    Refreshed(InstallationLock),
    AlreadyPresent(InstallationLock),
}

impl AddOutcome {
    pub fn lock(&self) -> &InstallationLock {
        match self {
            AddOutcome::Installed(l) | AddOutcome::Refreshed(l) | AddOutcome::AlreadyPresent(l) => l,
        }
    }
}

/// Application command API — the test seam for CLI and TUI.
#[derive(Debug, Clone)]
pub struct App {
    paths: SkillzPaths,
    scope: Scope,
}

impl App {
    pub fn new(paths: SkillzPaths) -> Self {
        Self {
            paths,
            scope: Scope::Project,
        }
    }

    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }

    pub fn paths(&self) -> &SkillzPaths {
        &self.paths
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn load_registry(&self) -> Result<Registry> {
        Registry::load(&self.paths.registry_path())
    }

    pub fn save_registry(&self, registry: &Registry) -> Result<()> {
        self.ensure_config_dir()?;
        registry.save(&self.paths.registry_path())
    }

    /// Status for the cwd Managed Project (bindings + installations).
    pub fn status(&self) -> Result<StatusReport> {
        if self.scope == Scope::User {
            if !self.paths.user_config_path().is_file() {
                return Ok(StatusReport {
                    cwd: self.paths.user_skills_dir(),
                    config_dir: self.paths.config_dir.clone(),
                    managed: false,
                    skills_dir: None,
                    bindings: Vec::new(),
                    installations: Vec::new(),
                    pending_review: Vec::new(),
                    message: "User scope not initialized (no user.toml)".to_string(),
                });
            }
            let config = self.load_scope_config()?;
            let lock = self.load_scope_lock()?;
            let installations = lock.installation_names();
            let pending_review = self.pending_review_names(&lock)?;
            let message = format!(
                "User scope; {} binding(s); {} installation(s); {} pending review",
                config.bindings.len(),
                installations.len(),
                pending_review.len()
            );
            return Ok(StatusReport {
                cwd: self.paths.user_skills_dir(),
                config_dir: self.paths.config_dir.clone(),
                managed: true,
                skills_dir: Some(config.skills_dir),
                bindings: config.bindings,
                installations,
                pending_review,
                message,
            });
        }
        let cfg_path = ProjectConfig::path(&self.paths.cwd);
        if !cfg_path.is_file() {
            return Ok(StatusReport {
                cwd: self.paths.cwd.clone(),
                config_dir: self.paths.config_dir.clone(),
                managed: false,
                skills_dir: None,
                bindings: Vec::new(),
                installations: Vec::new(),
                pending_review: Vec::new(),
                message: "Not a Managed Project (no .skillz/skillz.toml)".to_string(),
            });
        }
        let config = ProjectConfig::load(&self.paths.cwd)?;
        let lock = self.load_scope_lock()?;
        let installations = lock.installation_names();
        let pending_review = self.pending_review_names(&lock)?;
        let message = format!(
            "Managed Project; {} binding(s); {} installation(s); {} pending review",
            config.bindings.len(),
            installations.len(),
            pending_review.len()
        );
        Ok(StatusReport {
            cwd: self.paths.cwd.clone(),
            config_dir: self.paths.config_dir.clone(),
            managed: true,
            skills_dir: Some(config.skills_dir),
            bindings: config.bindings,
            installations,
            pending_review,
            message,
        })
    }

    fn pending_review_names(&self, lock: &ProjectLock) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for (name, rec) in &lock.installations {
            if rec.branch.is_none() {
                continue;
            }
            if let Some(reason) = self.classify_branch_lock(name, rec)? {
                if matches!(
                    reason,
                    DriftReason::PendingReview { .. } | DriftReason::SquashFallout { .. }
                ) {
                    out.push(name.clone());
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// Classify a Branch Lock against Library main after consulting the remote.
    fn classify_branch_lock(
        &self,
        name: &str,
        rec: &InstallationLock,
    ) -> Result<Option<DriftReason>> {
        let branch = match &rec.branch {
            Some(b) => b.clone(),
            None => return Ok(None),
        };
        let rev = match &rec.rev {
            Some(r) => r.clone(),
            None => {
                return Ok(Some(DriftReason::PendingReview {
                    library: rec.library.clone(),
                    branch,
                    rev: "(missing rev)".into(),
                }));
            }
        };
        let registry = self.load_registry()?;
        let record = registry.libraries.get(&rec.library).ok_or_else(|| {
            SkillzError::Message(format!(
                "library `{}` is not in the local registry (needed to check pending review for `{name}`)",
                rec.library
            ))
        })?;
        vcs_ops::fetch_remote(&record.path, "origin")?;
        vcs_ops::ensure_rev(&record.path, &rev)?;
        if vcs_ops::is_ancestor_of_main(&record.path, &rev)? {
            return Ok(None);
        }
        let remote_ok = vcs_ops::remote_branch_exists(&record.path, &branch);
        let local_ok = vcs_ops::local_branch_exists(&record.path, &branch);
        if !remote_ok && !local_ok {
            return Ok(Some(DriftReason::SquashFallout {
                library: rec.library.clone(),
                rev: rev.clone(),
                recovery: format!(
                    "Lock rev `{rev}` is not an ancestor of main (squash/rebase-merge?). Run `skillz update {name}` to take main tip, or re-`publish` onto a new review branch"
                ),
            }));
        }
        Ok(Some(DriftReason::PendingReview {
            library: rec.library.clone(),
            branch,
            rev,
        }))
    }

    /// Initialize user-global skills scope (`user.toml` / `user.lock`).
    pub fn user_init(&self, bind_specs: &[String]) -> Result<ProjectConfig> {
        if self.paths.user_config_path().is_file() {
            return Err(SkillzError::Message(format!(
                "user scope already initialized ({})",
                self.paths.user_config_path().display()
            )));
        }
        let registry = self.load_registry()?;
        let bindings = project::bindings_from_specs(bind_specs, &registry.libraries)?;
        let skills_dir = self.paths.user_skills_dir().display().to_string();
        std::fs::create_dir_all(self.paths.user_skills_dir()).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create {}: {e}",
                self.paths.user_skills_dir().display()
            ))
        })?;
        let config = ProjectConfig {
            skills_dir,
            bindings,
        };
        self.ensure_config_dir()?;
        let text = toml::to_string_pretty(&config).map_err(|e| {
            SkillzError::Message(format!("failed to serialize user.toml: {e}"))
        })?;
        std::fs::write(self.paths.user_config_path(), text).map_err(|e| {
            SkillzError::Message(format!(
                "failed to write {}: {e}",
                self.paths.user_config_path().display()
            ))
        })?;
        let lock = ProjectLock::default();
        let lock_text = toml::to_string_pretty(&lock).map_err(|e| {
            SkillzError::Message(format!("failed to serialize user.lock: {e}"))
        })?;
        std::fs::write(self.paths.user_lock_path(), lock_text).map_err(|e| {
            SkillzError::Message(format!(
                "failed to write {}: {e}",
                self.paths.user_lock_path().display()
            ))
        })?;
        Ok(config)
    }

    fn load_scope_config(&self) -> Result<ProjectConfig> {
        match self.scope {
            Scope::Project => self.require_managed(),
            Scope::User => {
                let path = self.paths.user_config_path();
                if !path.is_file() {
                    return Err(SkillzError::Message(
                        "user scope not initialized; run `skillz user init`".into(),
                    ));
                }
                let text = std::fs::read_to_string(&path).map_err(|e| {
                    SkillzError::Message(format!("failed to read {}: {e}", path.display()))
                })?;
                toml::from_str(&text).map_err(|e| {
                    SkillzError::Message(format!("invalid {}: {e}", path.display()))
                })
            }
        }
    }

    fn load_scope_lock(&self) -> Result<ProjectLock> {
        match self.scope {
            Scope::Project => ProjectLock::load(&self.paths.cwd),
            Scope::User => {
                let path = self.paths.user_lock_path();
                if !path.is_file() {
                    return Ok(ProjectLock::default());
                }
                let text = std::fs::read_to_string(&path).map_err(|e| {
                    SkillzError::Message(format!("failed to read {}: {e}", path.display()))
                })?;
                toml::from_str(&text).map_err(|e| {
                    SkillzError::Message(format!("invalid {}: {e}", path.display()))
                })
            }
        }
    }

    fn save_scope_lock(&self, lock: &ProjectLock) -> Result<()> {
        match self.scope {
            Scope::Project => lock.save(&self.paths.cwd),
            Scope::User => {
                self.ensure_config_dir()?;
                let text = toml::to_string_pretty(lock).map_err(|e| {
                    SkillzError::Message(format!("failed to serialize user.lock: {e}"))
                })?;
                std::fs::write(self.paths.user_lock_path(), text).map_err(|e| {
                    SkillzError::Message(format!(
                        "failed to write {}: {e}",
                        self.paths.user_lock_path().display()
                    ))
                })?;
                Ok(())
            }
        }
    }

    fn scope_skills_root(&self, config: &ProjectConfig) -> PathBuf {
        match self.scope {
            Scope::Project => self.paths.cwd.join(&config.skills_dir),
            Scope::User => {
                if Path::new(&config.skills_dir).is_absolute() {
                    PathBuf::from(&config.skills_dir)
                } else {
                    self.paths.user_skills_dir()
                }
            }
        }
    }

    /// Dry-run import detection for the cwd Project.
    pub fn project_import_dry_run(&self) -> Result<ImportPlan> {
        import::build_plan(&self.paths.cwd)
    }

    /// Import Skillfile github entries as Upstream Catalog Entries + Locks.
    /// Keeps matching Installation files; requires an existing Managed Project.
    pub fn project_import_from_skillfile(&self, library_name: &str) -> Result<Vec<String>> {
        let config = self.require_managed()?;
        let plan = import::build_plan(&self.paths.cwd)?;
        let mut imported = Vec::new();
        for entry in &plan.skillfile_entries {
            let dest = skill::skill_dir(&self.scope_skills_root(&config), &entry.name);
            if !dest.join("SKILL.md").is_file() {
                // Keep files when present; otherwise leave as missing (user may fetch).
                continue;
            }
            // Ensure source has @rev for Lock; synthesize placeholder from content hash if needed.
            let mut source = entry.source.clone();
            if !source.contains('@') {
                let hash = skill::content_hash(&dest)?;
                let short = hash.trim_start_matches("sha256:");
                let short = &short[..short.len().min(12)];
                source = format!("{source}@{short}");
            }
            self.libcommit_upstream(&entry.name, library_name, Some(&source), &[])?;
            imported.push(entry.name.clone());
        }
        Ok(imported)
    }

    /// Adopt leftover local skills per scripted `name=upstream|authored|skip` specs.
    pub fn project_adopt_local(
        &self,
        library_name: &str,
        specs: &[String],
    ) -> Result<Vec<(String, String)>> {
        let _config = self.require_managed()?;
        let plan = import::build_plan(&self.paths.cwd)?;
        let leftover_set: std::collections::BTreeSet<_> =
            plan.leftovers.iter().cloned().collect();
        let mut results = Vec::new();
        for spec in specs {
            let (name, adopt) = import::parse_adopt_spec(spec)?;
            if !leftover_set.contains(&name) && !plan.local_skills.iter().any(|s| s == &name) {
                return Err(SkillzError::Message(format!(
                    "skill `{name}` not found as a local leftover"
                )));
            }
            match adopt {
                AdoptAs::Skip => {
                    results.push((name, "skipped (remains Drift/import candidate)".into()));
                }
                AdoptAs::Authored => {
                    self.libcommit_authored(&name, library_name, &[])?;
                    results.push((name, "adopted as authored".into()));
                }
                AdoptAs::Upstream => {
                    let source = format!("github:local/import/{name}@adopted");
                    self.libcommit_upstream(&name, library_name, Some(&source), &[])?;
                    results.push((name, "adopted as upstream".into()));
                }
            }
        }
        Ok(results)
    }

    /// Initialize cwd as a Managed Project with ordered Library Bindings.
    pub fn project_init(
        &self,
        bind_specs: &[String],
        skills_dir: Option<&str>,
    ) -> Result<ProjectConfig> {
        let cfg_path = ProjectConfig::path(&self.paths.cwd);
        if cfg_path.is_file() {
            return Err(SkillzError::Message(format!(
                "already a Managed Project ({})",
                cfg_path.display()
            )));
        }
        let registry = self.load_registry()?;
        let bindings = project::bindings_from_specs(bind_specs, &registry.libraries)?;
        let config = ProjectConfig {
            skills_dir: skills_dir
                .unwrap_or(DEFAULT_SKILLS_DIR)
                .to_string(),
            bindings,
        };
        config.save(&self.paths.cwd)?;
        ProjectLock::default().save(&self.paths.cwd)?;

        let project_abs = std::fs::canonicalize(&self.paths.cwd).unwrap_or_else(|_| {
            self.paths.cwd.clone()
        });
        let mut registry = registry;
        if !registry.projects.iter().any(|p| p == &project_abs) {
            registry.projects.push(project_abs);
            self.save_registry(&registry)?;
        }
        Ok(config)
    }

    fn require_managed(&self) -> Result<ProjectConfig> {
        let cfg_path = ProjectConfig::path(&self.paths.cwd);
        if !cfg_path.is_file() {
            return Err(SkillzError::Message(
                "not a Managed Project (no .skillz/skillz.toml); run `skillz project init`".into(),
            ));
        }
        ProjectConfig::load(&self.paths.cwd)
    }

    /// Scaffold a new Authored skill in the Project skills dir (Unlocked until managed).
    pub fn skill_new(&self, name: &str, tags: &[String]) -> Result<PathBuf> {
        self.skill_new_with_local(name, tags, false)
    }

    /// Scaffold a skill; with `local`, mark Local authored membership immediately.
    pub fn skill_new_with_local(
        &self,
        name: &str,
        tags: &[String],
        local: bool,
    ) -> Result<PathBuf> {
        let config = self.load_scope_config()?;
        let root = self.scope_skills_root(&config);
        let path = skill::scaffold_skill(&root, name, tags)?;
        if local {
            self.set_local_authored(name)?;
        }
        Ok(path)
    }

    /// Remove a skill Installation + Lock from the current scope (Library untouched).
    ///
    /// Soft-warns on stderr when retained Installations appear to depend on `name`.
    /// Pass other names also being removed in this batch as `also_removing` so
    /// mutual pairs do not warn.
    pub fn remove_skill(&self, name: &str) -> Result<()> {
        self.remove_skill_with_context(name, &[])
    }

    pub fn remove_skill_with_context(&self, name: &str, also_removing: &[String]) -> Result<()> {
        skill::validate_skill_name(name)?;
        let dependants = self.reverse_deps_for_remove(name, also_removing)?;
        if !dependants.is_empty() {
            eprintln!(
                "warning: `{}` is referenced by retained skill(s): {}",
                name,
                dependants.join(", ")
            );
        }
        self.remove_skill_files(name)
    }

    fn remove_skill_files(&self, name: &str) -> Result<()> {
        let config = self.load_scope_config()?;
        let root = self.scope_skills_root(&config);
        let path = skill::skill_dir(&root, name);
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| {
                SkillzError::Message(format!(
                    "failed to remove {}: {e}",
                    path.display()
                ))
            })?;
        }
        let mut lock = self.load_scope_lock()?;
        lock.installations.remove(name);
        self.save_scope_lock(&lock)?;
        Ok(())
    }

    /// Retained Installations that appear to depend on `name` (best-effort).
    ///
    /// Names in `also_removing` (and `name` itself) are excluded from the
    /// retained set so bulk removes of a dependant+dependency pair stay quiet.
    pub fn reverse_deps_for_remove(
        &self,
        name: &str,
        also_removing: &[String],
    ) -> Result<Vec<String>> {
        let config = self.load_scope_config()?;
        let root = self.scope_skills_root(&config);
        let all = deps::list_skill_names(&root)?;
        let retained: Vec<String> = all
            .into_iter()
            .filter(|n| n != name && !also_removing.iter().any(|r| r == n))
            .collect();
        deps::find_dependants(&root, name, &retained)
    }

    /// Apply one Drift review action to a skill in the current scope.
    ///
    /// Remove does not re-print reverse-dep warnings (shown at confirm /
    /// [`Self::remove_skill`]); it only deletes Installation + Lock.
    pub fn apply_drift_action(&self, name: &str, action: DriftAction) -> Result<&'static str> {
        match action {
            DriftAction::Skip => Ok("skipped"),
            DriftAction::Attach => {
                let sug = self.suggest_catalog_match(name)?.ok_or_else(|| {
                    SkillzError::Message(format!(
                        "no Catalog match for `{name}`; cannot `add --attach`"
                    ))
                })?;
                self.attach_to_catalog(name, &sug.library, sug.kind)?;
                Ok("attached")
            }
            DriftAction::Publish => {
                let lock = self.load_scope_lock()?;
                if let Some(rec) = lock.installations.get(name) {
                    if rec.entry_kind == EntryKind::Local
                        || rec.entry_kind == EntryKind::Owned
                        || rec.entry_kind == EntryKind::Upstream
                    {
                        let library = if rec.entry_kind == EntryKind::Local {
                            self.first_writable_binding(&self.load_scope_config()?)?
                                .to_string()
                        } else {
                            rec.library.clone()
                        };
                        self.publish_skills(
                            &[name.to_string()],
                            &library,
                            PublishMode::Auto,
                            Some("interactive publish"),
                        )?;
                        return Ok("published");
                    }
                }
                // Unlocked / no Local: Branch Lock edit path via first writable Binding.
                if let Some(sug) = self.suggest_catalog_match(name)? {
                    self.libcommit_branch(
                        name,
                        Some(&sug.library),
                        Some("interactive publish"),
                        &[],
                    )?;
                    return Ok("published");
                }
                self.libcommit_branch(name, None, Some("interactive publish"), &[])?;
                Ok("published")
            }
            DriftAction::Update => {
                self.update_skill(name)?;
                Ok("updated")
            }
            DriftAction::Remove => {
                self.remove_skill_files(name)?;
                Ok("removed")
            }
        }
    }

    /// Suggest a bound Catalog Entry for an unlocked Installation by name.
    pub fn suggest_catalog_match(&self, name: &str) -> Result<Option<skill::CatalogSuggestion>> {
        let rows = self.list_skills(None, &[], true, false)?;
        let Some(row) = rows.into_iter().find(|r| r.entry.name == name) else {
            return Ok(None);
        };
        let config = self.load_scope_config()?;
        let project_skill = skill::skill_dir(&self.scope_skills_root(&config), name);
        let exact_match = match row.entry.kind {
            EntryKind::Owned if row.entry.skill_path.join("SKILL.md").is_file() => {
                match (
                    skill::content_hash(&project_skill),
                    skill::content_hash(&row.entry.skill_path),
                ) {
                    (Ok(a), Ok(b)) => Some(a == b),
                    _ => None,
                }
            }
            EntryKind::Owned => None,
            EntryKind::Upstream => None,
            EntryKind::Local => None,
        };
        Ok(Some(skill::CatalogSuggestion {
            library: row.entry.library,
            kind: row.entry.kind,
            exact_match,
        }))
    }

    /// Mark an on-disk skill as Local authored (membership only; no content hash).
    pub fn set_local_authored(&self, name: &str) -> Result<InstallationLock> {
        skill::validate_skill_name(name)?;
        let config = self.load_scope_config()?;
        let dest = skill::skill_dir(&self.scope_skills_root(&config), name);
        if !dest.join("SKILL.md").is_file() {
            return Err(SkillzError::Message(format!(
                "skill `{name}` not found under {}",
                dest.display()
            )));
        }
        let install = InstallationLock {
            library: LOCAL_LIBRARY.into(),
            entry_kind: EntryKind::Local,
            content_hash: String::new(),
            source: None,
            rev: None,
            branch: None,
            overlay_from_library: None,
            overlay_from_entry: None,
        };
        let mut lock = self.load_scope_lock()?;
        lock.installations.insert(name.to_string(), install.clone());
        self.save_scope_lock(&lock)?;
        Ok(install)
    }

    /// Mark an on-disk skill as Local + real `github:` / `cmd:` provenance (no Library Entry).
    pub fn set_local_source(&self, name: &str, source: &str) -> Result<InstallationLock> {
        skill::validate_skill_name(name)?;
        let config = self.load_scope_config()?;
        let dest = skill::skill_dir(&self.scope_skills_root(&config), name);
        if !dest.join("SKILL.md").is_file() {
            return Err(SkillzError::Message(format!(
                "skill `{name}` not found under {}",
                dest.display()
            )));
        }
        let source = source.trim();
        let (canonical, rev) = if source::GithubSource::looks_like_remote(source) {
            let parsed = source::GithubSource::parse_input(source)?;
            if parsed.rev.as_deref().unwrap_or("").is_empty() {
                return Err(SkillzError::Message(
                    "Local + github: source requires @rev (commit SHA or tag) for provenance".into(),
                ));
            }
            (parsed.render(), parsed.rev.clone())
        } else if source.starts_with("cmd:") {
            let parsed = source::CmdSource::parse(source)?;
            if parsed.rev.as_deref().unwrap_or("").is_empty() {
                return Err(SkillzError::Message(
                    "Local + cmd: source requires @tool-version for provenance".into(),
                ));
            }
            (parsed.render(), parsed.rev.clone())
        } else {
            return Err(SkillzError::Message(format!(
                "Local source must be `github:…` (or https:// / git@ GitHub URL) or `cmd:…`, got `{source}`"
            )));
        };
        let install = InstallationLock {
            library: LOCAL_LIBRARY.into(),
            entry_kind: EntryKind::Local,
            content_hash: String::new(),
            source: Some(canonical),
            rev,
            branch: None,
            overlay_from_library: None,
            overlay_from_entry: None,
        };
        let mut lock = self.load_scope_lock()?;
        lock.installations.insert(name.to_string(), install.clone());
        self.save_scope_lock(&lock)?;
        Ok(install)
    }

    /// Attach an Installation to a bound Catalog Entry without rewriting files.
    pub fn attach_to_catalog(
        &self,
        name: &str,
        library: &str,
        kind: EntryKind,
    ) -> Result<InstallationLock> {
        self.lock_unlocked_to_catalog(
            name,
            &skill::CatalogSuggestion {
                library: library.to_string(),
                kind,
                exact_match: Some(true),
            },
        )
    }

    fn lock_unlocked_to_catalog(
        &self,
        name: &str,
        suggestion: &skill::CatalogSuggestion,
    ) -> Result<InstallationLock> {
        let config = self.load_scope_config()?;
        let project_skill = skill::skill_dir(&self.scope_skills_root(&config), name);
        let hash = skill::content_hash(&project_skill)?;
        let rows = self.list_skills(Some(&suggestion.library), &[], true, false)?;
        let entry = rows
            .into_iter()
            .find(|r| r.entry.name == name)
            .ok_or_else(|| {
                SkillzError::Message(format!(
                    "Catalog Entry `{name}` missing from library `{}`",
                    suggestion.library
                ))
            })?;
        let install = InstallationLock {
            library: suggestion.library.clone(),
            entry_kind: suggestion.kind,
            content_hash: hash,
            source: entry.entry.source,
            rev: None,
            branch: None,
            overlay_from_library: None,
            overlay_from_entry: None,
        };
        let mut lock = self.load_scope_lock()?;
        lock.installations.insert(name.to_string(), install.clone());
        self.save_scope_lock(&lock)?;
        Ok(install)
    }

    /// Abort if tracked files under `skills_dir` are dirty in git/jj.
    pub fn ensure_skills_dir_clean(&self) -> Result<()> {
        let config = self.load_scope_config()?;
        let repo = match self.scope {
            Scope::Project => self.paths.cwd.clone(),
            Scope::User => self.paths.home_dir.join(".agents"),
        };
        // Prefer Project/user skills root for pathspec; fall back to cwd repo.
        let repo_root = match vcs::infer_vcs(&self.paths.cwd) {
            Some(_) => self.paths.cwd.clone(),
            None => {
                if vcs::infer_vcs(&repo).is_some() {
                    repo
                } else {
                    return Ok(());
                }
            }
        };
        let skills_root = self.scope_skills_root(&config);
        let skills_rel = skills_root
            .strip_prefix(&repo_root)
            .unwrap_or(Path::new(&config.skills_dir))
            .to_string_lossy()
            .into_owned();
        let dirty = vcs_ops::dirty_tracked_under(&repo_root, &skills_rel)?;
        if dirty.is_empty() {
            return Ok(());
        }
        Err(SkillzError::Message(format!(
            "migrate refused: tracked files under `{skills_rel}` are dirty (commit or stash first):\n  - {}",
            dirty.join("\n  - ")
        )))
    }

    /// Build a migrate plan for the current scope (no writes).
    pub fn migrate_plan(&self, opts: &MigrateOptions) -> Result<MigratePlan> {
        self.ensure_skills_dir_clean()?;
        let config = self.load_scope_config()?;
        let skills_dir = self.scope_skills_root(&config);
        let lock = self.load_scope_lock()?;
        let drift: std::collections::BTreeSet<String> = self
            .detect_drift()?
            .into_iter()
            .map(|d| d.name)
            .collect();

        let (skillfile, skillfile_entries) = if opts.ignore_skillfile {
            (None, Vec::new())
        } else {
            let path = self.paths.cwd.join("Skillfile");
            if path.is_file() {
                let text = std::fs::read_to_string(&path).map_err(|e| {
                    SkillzError::Message(format!("failed to read {}: {e}", path.display()))
                })?;
                (Some(path), import::parse_skillfile(&text)?)
            } else {
                (None, Vec::new())
            }
        };
        let skillfile_by_name: std::collections::BTreeMap<String, String> = skillfile_entries
            .iter()
            .map(|e| (e.name.clone(), e.source.clone()))
            .collect();

        let on_disk = import::list_local_skill_names(&skills_dir)?;
        let mut warnings = Vec::new();
        for entry in &skillfile_entries {
            if !on_disk.iter().any(|n| n == &entry.name) {
                warnings.push(format!(
                    "Skillfile entry `{}` has no on-disk Installation; skipped (migrate does not fetch)",
                    entry.name
                ));
            }
        }

        let catalog_rows = self.list_skills(None, &[], false, false)?;
        let mut by_name: std::collections::BTreeMap<String, Vec<&ListRow>> =
            std::collections::BTreeMap::new();
        for row in &catalog_rows {
            by_name.entry(row.entry.name.clone()).or_default().push(row);
        }

        let mut rows = Vec::new();
        for name in on_disk {
            let skillfile_source = skillfile_by_name.get(&name).cloned();
            let mgmt = match lock.installations.get(&name) {
                None => MgmtState::Unmanaged,
                Some(rec) if rec.entry_kind == EntryKind::Local => {
                    if rec.source.is_some() {
                        MgmtState::LocalSource
                    } else {
                        MgmtState::LocalAuthored
                    }
                }
                Some(_) => MgmtState::LibraryLocked {
                    clean: !drift.contains(&name),
                },
            };

            let catalog = match by_name.get(&name).map(|v| v.as_slice()) {
                None | Some([]) => CatalogMatch::None,
                Some([row]) => {
                    let exact = match row.entry.kind {
                        EntryKind::Owned if row.entry.skill_path.join("SKILL.md").is_file() => {
                            let project_skill = skill::skill_dir(&skills_dir, &name);
                            match (
                                skill::content_hash(&project_skill),
                                skill::content_hash(&row.entry.skill_path),
                            ) {
                                (Ok(a), Ok(b)) => Some(a == b),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    CatalogMatch::Single {
                        library: row.entry.library.clone(),
                        kind: row.entry.kind,
                        exact_match: exact,
                    }
                }
                Some(_) => CatalogMatch::Ambiguous,
            };

            let (matrix, situation, recommended, alternatives) =
                Self::classify_migrate_row(opts, &mgmt, &catalog, skillfile_source.as_deref());

            rows.push(MigrateSkillPlan {
                name,
                matrix,
                mgmt,
                skillfile_source,
                catalog,
                recommended,
                situation,
                alternatives,
            });
        }

        Ok(MigratePlan {
            skills_dir,
            skillfile,
            rows,
            warnings,
        })
    }

    fn classify_migrate_row(
        opts: &MigrateOptions,
        mgmt: &MgmtState,
        catalog: &CatalogMatch,
        skillfile_source: Option<&str>,
    ) -> (
        &'static str,
        MigrateSituationClass,
        MigrateAction,
        Vec<MigrateAction>,
    ) {
        match (mgmt, catalog, skillfile_source) {
            (MgmtState::LibraryLocked { clean: true }, _, _) => {
                ("B1", MigrateSituationClass::Other, MigrateAction::NoOp, vec![])
            }
            (MgmtState::LibraryLocked { clean: false }, _, _) => (
                "B2",
                MigrateSituationClass::Other,
                MigrateAction::NeedsPrompt,
                vec![],
            ),
            (
                MgmtState::Unmanaged,
                CatalogMatch::Single {
                    library,
                    kind: EntryKind::Owned,
                    exact_match: Some(true),
                },
                _,
            ) => (
                if skillfile_source.is_some() { "A6" } else { "A3" },
                MigrateSituationClass::Other,
                MigrateAction::AttachIdentical {
                    library: library.clone(),
                },
                vec![],
            ),
            (
                MgmtState::Unmanaged,
                CatalogMatch::Single {
                    library,
                    kind: EntryKind::Owned,
                    exact_match: Some(false),
                },
                None,
            ) => (
                "A4",
                MigrateSituationClass::A4OwnedDiffers,
                MigrateAction::NeedsPrompt,
                vec![
                    MigrateAction::UseLibraryFiles {
                        library: library.clone(),
                    },
                    MigrateAction::PublishToLibrary {
                        library: library.clone(),
                    },
                    MigrateAction::KeepLocalAuthored,
                ],
            ),
            (
                MgmtState::Unmanaged,
                CatalogMatch::Single {
                    library,
                    kind: EntryKind::Owned,
                    exact_match: Some(false),
                },
                Some(src),
            ) => (
                "A7",
                MigrateSituationClass::A7OwnedDiffersWithSkillfile,
                MigrateAction::NeedsPrompt,
                vec![
                    MigrateAction::UseLibraryFiles {
                        library: library.clone(),
                    },
                    MigrateAction::PublishToLibrary {
                        library: library.clone(),
                    },
                    MigrateAction::SetLocalSource {
                        source: src.to_string(),
                    },
                ],
            ),
            (
                MgmtState::Unmanaged,
                CatalogMatch::Single {
                    library,
                    kind: EntryKind::Upstream,
                    ..
                },
                None,
            ) => (
                "A5",
                MigrateSituationClass::A5UpstreamUncompared,
                MigrateAction::NeedsPrompt,
                vec![
                    MigrateAction::AttachUpstream {
                        library: library.clone(),
                    },
                    MigrateAction::KeepLocalAuthored,
                ],
            ),
            (
                MgmtState::Unmanaged,
                CatalogMatch::Single {
                    library,
                    kind: EntryKind::Upstream,
                    ..
                },
                Some(src),
            ) => {
                // A8/A9: identity match vs mismatch (best-effort via source prefix).
                let cat_src = None::<String>; // identity compared at apply time when attaching
                let _ = cat_src;
                (
                    "A8",
                    MigrateSituationClass::Other,
                    MigrateAction::AttachUpstream {
                        library: library.clone(),
                    },
                    vec![MigrateAction::SetLocalSource {
                        source: src.to_string(),
                    }],
                )
            }
            (MgmtState::Unmanaged, CatalogMatch::Ambiguous, _) => (
                "A10",
                MigrateSituationClass::A10Ambiguous,
                MigrateAction::NeedsPrompt,
                vec![],
            ),
            (MgmtState::Unmanaged, CatalogMatch::None, Some(src)) => (
                "A2",
                MigrateSituationClass::A2SkillfileOnly,
                MigrateAction::NeedsPrompt,
                vec![
                    MigrateAction::SetLocalSource {
                        source: src.to_string(),
                    },
                    MigrateAction::KeepLocalAuthored,
                ],
            ),
            (MgmtState::Unmanaged, CatalogMatch::None, None) if opts.authored => (
                "A1",
                MigrateSituationClass::A1NoSignals,
                MigrateAction::SetLocalAuthored,
                vec![],
            ),
            (MgmtState::Unmanaged, CatalogMatch::None, None) => (
                "A1",
                MigrateSituationClass::A1NoSignals,
                MigrateAction::NeedsPrompt,
                vec![MigrateAction::SetLocalAuthored, MigrateAction::KeepLocalAuthored],
            ),
            (
                MgmtState::LocalAuthored,
                CatalogMatch::Single {
                    library,
                    kind: EntryKind::Owned,
                    exact_match: Some(true),
                },
                _,
            ) => (
                "B3",
                MigrateSituationClass::B3LocalToOwnedIdentical,
                MigrateAction::NeedsPrompt,
                vec![
                    MigrateAction::AttachIdentical {
                        library: library.clone(),
                    },
                    MigrateAction::KeepLocalAuthored,
                ],
            ),
            (
                MgmtState::LocalAuthored,
                CatalogMatch::Single {
                    library,
                    kind: EntryKind::Owned,
                    exact_match: Some(false),
                },
                _,
            ) => (
                "B4",
                MigrateSituationClass::B4LocalOwnedDiffers,
                MigrateAction::NeedsPrompt,
                vec![
                    MigrateAction::UseLibraryFiles {
                        library: library.clone(),
                    },
                    MigrateAction::PublishToLibrary {
                        library: library.clone(),
                    },
                    MigrateAction::KeepLocalAuthored,
                ],
            ),
            (MgmtState::LocalAuthored, _, Some(src)) => (
                "B5",
                MigrateSituationClass::B5LocalToSource,
                MigrateAction::NeedsPrompt,
                vec![
                    MigrateAction::SetLocalSource {
                        source: src.to_string(),
                    },
                    MigrateAction::KeepLocalAuthored,
                ],
            ),
            (MgmtState::LocalAuthored | MgmtState::LocalSource, CatalogMatch::None, None) => (
                "B8",
                MigrateSituationClass::Other,
                MigrateAction::NoOp,
                vec![],
            ),
            (
                MgmtState::LocalSource,
                CatalogMatch::Single {
                    library,
                    kind: EntryKind::Upstream,
                    ..
                },
                _,
            ) => (
                "B6",
                MigrateSituationClass::Other,
                MigrateAction::NeedsPrompt,
                vec![
                    MigrateAction::AttachUpstream {
                        library: library.clone(),
                    },
                    MigrateAction::KeepLocalAuthored,
                ],
            ),
            _ => (
                "?",
                MigrateSituationClass::Other,
                MigrateAction::NeedsPrompt,
                vec![],
            ),
        }
    }

    /// Apply migrate actions. When `no_prompt` is true, any NeedsPrompt aborts with no writes.
    /// `resolve` answers indeterminate rows; Cancel enables partial apply.
    pub fn migrate_apply(
        &self,
        plan: &MigratePlan,
        dry_run: bool,
        no_prompt: bool,
        mut resolve: Option<
            &mut dyn FnMut(&MigrateSkillPlan) -> Result<MigratePromptAnswer>,
        >,
    ) -> Result<Vec<(String, String)>> {
        if !dry_run {
            self.ensure_skills_dir_clean()?;
        }
        if no_prompt {
            let unclear: Vec<_> = plan
                .rows
                .iter()
                .filter(|r| !r.recommended.is_determinate())
                .map(|r| r.name.as_str())
                .collect();
            if !unclear.is_empty() {
                return Err(SkillzError::Message(format!(
                    "migrate --no-prompt aborted; indeterminate skill(s): {}",
                    unclear.join(", ")
                )));
            }
        }

        let mut class_answers: std::collections::HashMap<
            MigrateSituationClass,
            MigrateAction,
        > = std::collections::HashMap::new();
        let mut out = Vec::new();
        for row in &plan.rows {
            let action = if row.recommended.is_determinate() {
                row.recommended.clone()
            } else if let Some(ans) = class_answers.get(&row.situation) {
                ans.clone()
            } else if dry_run {
                MigrateAction::NeedsPrompt
            } else {
                let resolver = resolve.as_mut().ok_or_else(|| {
                    SkillzError::Message(
                        "migrate has indeterminate skills; pass a prompt resolver, --dry-run, or --no-prompt".into(),
                    )
                })?;
                match resolver(row)? {
                    MigratePromptAnswer::This(action) => action,
                    MigratePromptAnswer::AllRemaining(action) => {
                        class_answers.insert(row.situation, action.clone());
                        action
                    }
                    MigratePromptAnswer::Cancel => {
                        out.push((row.name.clone(), "cancelled (partial apply)".into()));
                        break;
                    }
                }
            };

            if matches!(action, MigrateAction::NeedsPrompt) {
                out.push((row.name.clone(), "needs prompt".into()));
                continue;
            }
            if !dry_run {
                self.apply_migrate_action(&row.name, &action)?;
            }
            out.push((row.name.clone(), action.label()));
        }
        Ok(out)
    }

    fn apply_migrate_action(&self, name: &str, action: &MigrateAction) -> Result<()> {
        match action {
            MigrateAction::NoOp | MigrateAction::KeepLocalAuthored => {
                if matches!(action, MigrateAction::KeepLocalAuthored) {
                    let _ = self.set_local_authored(name)?;
                }
                Ok(())
            }
            MigrateAction::SetLocalAuthored => {
                self.set_local_authored(name)?;
                Ok(())
            }
            MigrateAction::SetLocalSource { source } => {
                self.set_local_source(name, source)?;
                Ok(())
            }
            MigrateAction::AttachIdentical { library }
            | MigrateAction::AttachUpstream { library } => {
                let kind = if matches!(action, MigrateAction::AttachUpstream { .. }) {
                    EntryKind::Upstream
                } else {
                    EntryKind::Owned
                };
                self.attach_to_catalog(name, library, kind)?;
                Ok(())
            }
            MigrateAction::UseLibraryFiles { library } => {
                let rows = self.list_skills(Some(library), &[], true, false)?;
                let entry = rows
                    .into_iter()
                    .find(|r| r.entry.name == *name)
                    .ok_or_else(|| {
                        SkillzError::Message(format!(
                            "Catalog Entry `{name}` missing from library `{library}`"
                        ))
                    })?;
                if entry.entry.kind != EntryKind::Owned {
                    return Err(SkillzError::Message(format!(
                        "cannot overwrite from non-Owned Catalog Entry `{name}` in `{library}`"
                    )));
                }
                let config = self.load_scope_config()?;
                let dest = skill::skill_dir(&self.scope_skills_root(&config), name);
                skill::copy_skill_tree(&entry.entry.skill_path, &dest)?;
                self.attach_to_catalog(name, library, EntryKind::Owned)?;
                Ok(())
            }
            MigrateAction::PublishToLibrary { library } => {
                self.libcommit_authored(name, library, &[])?;
                Ok(())
            }
            MigrateAction::NeedsPrompt => Err(SkillzError::Message(
                "internal: cannot apply NeedsPrompt".into(),
            )),
        }
    }

    /// `migrate NAME --source`: Local+source for unmanaged / Local authored / update Local+source.
    pub fn migrate_set_source(&self, name: &str, source: &str) -> Result<InstallationLock> {
        skill::validate_skill_name(name)?;
        let lock = self.load_scope_lock()?;
        if let Some(rec) = lock.installations.get(name) {
            if rec.entry_kind != EntryKind::Local {
                return Err(SkillzError::Message(format!(
                    "refuse `migrate {name} --source`: skill is Library-backed (`{}`); detach is not supported in this slice",
                    rec.library
                )));
            }
        }
        self.set_local_source(name, source)
    }

    /// Detect Drift for skills present under skills_dir vs the Lock (current scope).
    pub fn detect_drift(&self) -> Result<Vec<DriftItem>> {
        let config = self.load_scope_config()?;
        let lock = self.load_scope_lock()?;
        let root = self.scope_skills_root(&config);
        let mut items = Self::drift_items_for(&root, &lock)?;
        self.append_branch_lock_status(&lock, &mut items)?;
        self.attach_unlocked_suggestions(&mut items)?;
        Ok(items)
    }

    pub fn detect_drift_in(&self, project_root: &Path) -> Result<Vec<DriftItem>> {
        let cfg_path = ProjectConfig::path(project_root);
        if !cfg_path.is_file() {
            return Err(SkillzError::Message(format!(
                "not a Managed Project: {}",
                project_root.display()
            )));
        }
        let config = ProjectConfig::load(project_root)?;
        let lock = ProjectLock::load(project_root)?;
        let root = project_root.join(&config.skills_dir);
        // Suggestions need App paths; attach only when cwd matches project.
        let mut items = Self::drift_items_for(&root, &lock)?;
        // Branch-lock pending checks need registry paths from this App.
        self.append_branch_lock_status(&lock, &mut items)?;
        if self.paths.cwd == project_root {
            self.attach_unlocked_suggestions(&mut items)?;
        }
        Ok(items)
    }

    fn append_branch_lock_status(
        &self,
        lock: &ProjectLock,
        items: &mut Vec<DriftItem>,
    ) -> Result<()> {
        let hard: std::collections::HashSet<String> = items
            .iter()
            .filter(|i| i.reason.fails_check())
            .map(|i| i.name.clone())
            .collect();
        for (name, rec) in &lock.installations {
            if hard.contains(name) {
                continue;
            }
            if let Some(reason) = self.classify_branch_lock(name, rec)? {
                items.push(DriftItem {
                    name: name.clone(),
                    reason,
                    suggestion: None,
                });
            }
        }
        items.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    fn attach_unlocked_suggestions(&self, items: &mut [DriftItem]) -> Result<()> {
        for item in items.iter_mut() {
            if item.reason == DriftReason::Unlocked {
                item.suggestion = self.suggest_catalog_match(&item.name)?;
            }
        }
        Ok(())
    }

    fn drift_items_for(root: &Path, lock: &ProjectLock) -> Result<Vec<DriftItem>> {
        let mut items = Vec::new();
        if root.is_dir() {
            for entry in std::fs::read_dir(root).map_err(|e| {
                SkillzError::Message(format!("failed to read {}: {e}", root.display()))
            })? {
                let entry = entry.map_err(|e| {
                    SkillzError::Message(format!("failed to read {}: {e}", root.display()))
                })?;
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                match lock.installations.get(&name) {
                    None => items.push(DriftItem {
                        name,
                        reason: DriftReason::Unlocked,
                        suggestion: None,
                    }),
                    // Local membership is managed without content-hash Drift.
                    Some(rec) if rec.entry_kind == EntryKind::Local => {}
                    Some(rec) => match skill::content_hash(&path) {
                        Ok(actual) if actual == rec.content_hash => {}
                        Ok(actual) => items.push(DriftItem {
                            name,
                            reason: DriftReason::HashMismatch {
                                locked: rec.content_hash.clone(),
                                actual,
                            },
                            suggestion: None,
                        }),
                        Err(_) => items.push(DriftItem {
                            name,
                            reason: DriftReason::MissingFiles,
                            suggestion: None,
                        }),
                    },
                }
            }
        }
        for name in lock.installations.keys() {
            let path = skill::skill_dir(root, name);
            if !path.is_dir() {
                items.push(DriftItem {
                    name: name.clone(),
                    reason: DriftReason::MissingFiles,
                    suggestion: None,
                });
            }
        }
        items.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(items)
    }

    /// Drift report for the requested [`DriftScan`] scope.
    ///
    /// - [`DriftScan::Current`]: this Managed Project (`cwd`) or user-global when
    ///   `App` scope is [`Scope::User`] — same surface as interactive `drift -i`.
    /// - [`DriftScan::Global`]: every registry Managed Project plus user-global.
    /// - [`DriftScan::Project`]: one explicit Managed Project path.
    pub fn drift_report(&self, scan: DriftScan) -> Result<Vec<ProjectDrift>> {
        match scan {
            DriftScan::Current => self.drift_report_current(),
            DriftScan::Project(path) => {
                let project = canonicalize_or_clone(&path);
                let items = self.detect_drift_in(&project)?;
                if items.is_empty() {
                    Ok(Vec::new())
                } else {
                    Ok(vec![ProjectDrift { project, items }])
                }
            }
            DriftScan::Global => self.drift_report_global(),
        }
    }

    fn drift_report_current(&self) -> Result<Vec<ProjectDrift>> {
        match self.scope {
            Scope::User => {
                let items = self.detect_drift()?;
                if items.is_empty() {
                    return Ok(Vec::new());
                }
                Ok(vec![ProjectDrift {
                    project: self.paths.user_skills_dir(),
                    items,
                }])
            }
            Scope::Project => {
                if !ProjectConfig::path(&self.paths.cwd).is_file() {
                    return Err(SkillzError::Message(
                        "not a Managed Project; pass -g/--global to scan all projects \
                         (and user-global), --user for user-global only, or --project PATH"
                            .into(),
                    ));
                }
                let project = canonicalize_or_clone(&self.paths.cwd);
                let items = self.detect_drift_in(&project)?;
                if items.is_empty() {
                    Ok(Vec::new())
                } else {
                    Ok(vec![ProjectDrift { project, items }])
                }
            }
        }
    }

    fn drift_report_global(&self) -> Result<Vec<ProjectDrift>> {
        let mut projects = Vec::new();
        let registry = self.load_registry()?;
        for p in registry.projects {
            let p = canonicalize_or_clone(&p);
            if !projects.iter().any(|x| x == &p) {
                projects.push(p);
            }
        }
        if ProjectConfig::path(&self.paths.cwd).is_file() {
            let cwd = canonicalize_or_clone(&self.paths.cwd);
            if !projects.iter().any(|p| p == &cwd) {
                projects.push(cwd);
            }
        }
        let mut out = Vec::new();
        for project in projects {
            let items = self.detect_drift_in(&project)?;
            if !items.is_empty() {
                out.push(ProjectDrift { project, items });
            }
        }
        if self.paths.user_config_path().is_file() {
            let user_app = self.clone().with_scope(Scope::User);
            let items = user_app.detect_drift()?;
            if !items.is_empty() {
                out.push(ProjectDrift {
                    project: self.paths.user_skills_dir(),
                    items,
                });
            }
        }
        Ok(out)
    }

    pub fn allow_drift_env() -> bool {
        matches!(
            std::env::var("SKILLZ_ALLOW_DRIFT").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
        )
    }

    /// Dry-run: describe Drift gates `hooks install` would write (no file changes).
    pub fn hooks_show(&self) -> Result<Vec<hooks::HookPlan>> {
        if self.scope == Scope::User {
            return Err(SkillzError::Message(
                "hooks show applies to Managed Projects, not user scope".into(),
            ));
        }
        self.require_managed()?;
        hooks::plan_hooks(&self.paths.cwd)
    }

    pub fn hooks_install(&self) -> Result<Vec<PathBuf>> {
        if self.scope == Scope::User {
            return Err(SkillzError::Message(
                "hooks install applies to Managed Projects, not user scope".into(),
            ));
        }
        self.require_managed()?;
        match infer_vcs(&self.paths.cwd) {
            Some(Vcs::Jj) => Ok(vec![hooks::install_jj_push_with_checks(
                &self.paths.cwd,
            )?]),
            Some(Vcs::Git) => Ok(vec![hooks::install_git_pre_commit(&self.paths.cwd)?]),
            None => Err(SkillzError::Message(
                "no hooks installed (need git or jj metadata)".into(),
            )),
        }
    }

    /// List Catalog Entries from bound Libraries (or a single `--library`).
    ///
    /// When `project_only` is true, only skills present in this Project's
    /// skills dir and/or recorded in the Project Lock are returned.
    ///
    /// Outside a Managed Project / user scope, falls back to the Catalog of
    /// `--library` or the registered Library that contains cwd (implies catalog).
    pub fn list_skills(
        &self,
        library_filter: Option<&str>,
        tags: &[String],
        dedupe: bool,
        project_only: bool,
    ) -> Result<Vec<ListRow>> {
        let config = match self.load_scope_config() {
            Ok(c) => c,
            Err(err) => {
                let name = self.resolve_library_name(library_filter).map_err(|_| err)?;
                return self.list_library_catalog(&name, tags, dedupe);
            }
        };
        let registry = self.load_registry()?;
        let tag_filters = catalog::normalize_tag_filters(tags);
        let project_names = if project_only {
            Some(self.project_skill_names(&config)?)
        } else {
            None
        };

        let bindings: Vec<&Binding> = if let Some(name) = library_filter {
            let b = config
                .bindings
                .iter()
                .find(|b| b.name == name)
                .ok_or_else(|| {
                    SkillzError::Message(format!(
                        "library `{name}` is not a Binding on this Managed Project"
                    ))
                })?;
            vec![b]
        } else {
            config.bindings.iter().collect()
        };

        let mut rows = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for binding in bindings {
            let Some(record) = registry.libraries.get(&binding.name) else {
                return Err(SkillzError::Message(format!(
                    "library `{}` is not in the local registry",
                    binding.name
                )));
            };
            let mut entries = catalog::list_owned_entries(&binding.name, &record.path)?;
            let owned_names: std::collections::BTreeSet<_> =
                entries.iter().map(|e| e.name.clone()).collect();
            for up in catalog::list_upstream_entries(&binding.name, &record.path)? {
                if !owned_names.contains(&up.name) {
                    entries.push(up);
                }
            }
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            for entry in entries {
                if let Some(names) = &project_names {
                    if !names.contains(&entry.name) {
                        continue;
                    }
                }
                if !catalog::entry_matches_tags(&entry, &tag_filters) {
                    continue;
                }
                if dedupe && !seen.insert(entry.name.clone()) {
                    continue;
                }
                let availability = self.availability_for(&config, &entry.name);
                rows.push(ListRow {
                    entry,
                    availability,
                });
            }
        }

        // Project-only: include on-disk / locked skills that are not (yet) in a Library.
        if let Some(names) = &project_names {
            let listed: std::collections::BTreeSet<_> =
                rows.iter().map(|r| r.entry.name.clone()).collect();
            let lock = self.load_scope_lock()?;
            let root = self.scope_skills_root(&config);
            for name in names {
                if listed.contains(name) || (dedupe && !seen.insert(name.clone())) {
                    continue;
                }
                let skill_path = skill::skill_dir(&root, name);
                let tags = if skill_path.join("SKILL.md").is_file() {
                    catalog::read_tags(&skill_path.join("SKILL.md")).unwrap_or_default()
                } else {
                    Vec::new()
                };
                if !tag_filters.is_empty()
                    && !tag_filters.iter().all(|t| tags.iter().any(|e| e == t))
                {
                    continue;
                }
                let (library, content_hash, kind, source) =
                    if let Some(install) = lock.installations.get(name) {
                        (
                            install.library.clone(),
                            install.content_hash.clone(),
                            install.entry_kind,
                            install.source.clone(),
                        )
                    } else {
                        (
                            "-".into(),
                            String::new(),
                            EntryKind::Owned,
                            None,
                        )
                    };
                if let Some(lib_filter) = library_filter {
                    if library != lib_filter {
                        continue;
                    }
                }
                let availability = self.availability_for(&config, name);
                rows.push(ListRow {
                    entry: CatalogEntry {
                        name: name.clone(),
                        library,
                        library_path: PathBuf::new(),
                        skill_path,
                        tags,
                        content_hash,
                        kind,
                        source,
                    },
                    availability,
                });
            }
            rows.sort_by(|a, b| a.entry.name.cmp(&b.entry.name));
        }
        Ok(rows)
    }

    /// List Owned + Upstream Catalog Entries for one registered Library (no Project required).
    pub fn list_library_catalog(
        &self,
        library_name: &str,
        tags: &[String],
        dedupe: bool,
    ) -> Result<Vec<ListRow>> {
        let registry = self.load_registry()?;
        let record = registry.libraries.get(library_name).ok_or_else(|| {
            SkillzError::Message(format!(
                "library `{library_name}` is not in the local registry"
            ))
        })?;
        let tag_filters = catalog::normalize_tag_filters(tags);
        let mut entries = catalog::list_owned_entries(library_name, &record.path)?;
        let owned_names: std::collections::BTreeSet<_> =
            entries.iter().map(|e| e.name.clone()).collect();
        for up in catalog::list_upstream_entries(library_name, &record.path)? {
            if !owned_names.contains(&up.name) {
                entries.push(up);
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let dummy = ProjectConfig::default();
        let mut rows = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for entry in entries {
            if !catalog::entry_matches_tags(&entry, &tag_filters) {
                continue;
            }
            if dedupe && !seen.insert(entry.name.clone()) {
                continue;
            }
            let availability = self.availability_for(&dummy, &entry.name);
            rows.push(ListRow { entry, availability });
        }
        Ok(rows)
    }

    fn project_skill_names(&self, config: &ProjectConfig) -> Result<std::collections::BTreeSet<String>> {
        let root = self.scope_skills_root(config);
        let mut names: std::collections::BTreeSet<String> =
            deps::list_skill_names(&root)?.into_iter().collect();
        let lock = self.load_scope_lock()?;
        names.extend(lock.installations.keys().cloned());
        Ok(names)
    }

    fn availability_for(&self, config: &ProjectConfig, name: &str) -> Availability {
        let mut avail = Availability::default();
        let project_skill = skill::skill_dir(&self.scope_skills_root(config), name);
        avail.project = project_skill.join("SKILL.md").is_file();
        avail.user = self
            .paths
            .user_skills_dir()
            .join(name)
            .join("SKILL.md")
            .is_file();
        for rel in [".agents/skills", ".cursor/skills", ".claude/skills"] {
            if rel == config.skills_dir {
                continue;
            }
            let path = self.paths.cwd.join(rel).join(name).join("SKILL.md");
            if path.is_file() {
                avail.other_roots.push(rel.to_string());
            }
        }
        avail
    }

    /// Missing sibling dependency warnings for a skill path.
    pub fn sibling_dep_warnings(
        &self,
        skill_path: &Path,
        known_siblings: &[String],
        present: &[String],
    ) -> Result<Vec<String>> {
        let deps = deps::detect_sibling_deps(skill_path, known_siblings)?;
        Ok(deps
            .into_iter()
            .filter(|d| !present.iter().any(|p| p == d))
            .collect())
    }

    /// Missing sibling deps for an installed skill (best-effort).
    pub fn missing_sibling_deps(
        &self,
        name: &str,
        library_filter: Option<&str>,
    ) -> Result<Vec<String>> {
        let config = self.load_scope_config()?;
        let dest = skill::skill_dir(&self.scope_skills_root(&config), name);
        if !dest.join("SKILL.md").is_file() {
            return Ok(Vec::new());
        }
        let catalog_names: Vec<String> = self
            .list_skills(library_filter, &[], true, false)?
            .into_iter()
            .map(|r| r.entry.name)
            .collect();
        let present = deps::list_skill_names(&self.scope_skills_root(&config))?;
        self.sibling_dep_warnings(&dest, &catalog_names, &present)
    }

    /// Forward sibling deps of `name` that are not in `also_removing` (still retained).
    pub fn forward_deps_remaining(
        &self,
        name: &str,
        also_removing: &[String],
    ) -> Result<Vec<String>> {
        let config = self.load_scope_config()?;
        let root = self.scope_skills_root(&config);
        let path = skill::skill_dir(&root, name);
        let known = deps::list_skill_names(&root)?;
        let deps = deps::detect_sibling_deps(&path, &known)?;
        Ok(deps
            .into_iter()
            .filter(|d| d != name && !also_removing.iter().any(|r| r == d))
            .collect())
    }

    /// Install a Catalog Entry into the Managed Project (copy + Lock).
    ///
    /// Errors if the skill directory already exists — use
    /// [`Self::add_skill_with_deps`] with `attach` / `from_library` to link or replace.
    pub fn add_skill(
        &self,
        name: &str,
        library_filter: Option<&str>,
    ) -> Result<(AddOutcome, Option<String>)> {
        self.add_skill_with_deps(name, library_filter, false, false, false)
            .map(|(outcome, warn, _)| (outcome, warn))
    }

    /// Install, attach, or replace-from-Catalog a skill.
    ///
    /// - Plain install (`attach=false`): skill dir must be missing.
    /// - `--attach`: Lock on-disk skill to Catalog Entry (no Library write); clears Local.
    /// - `--attach --from-library`: overwrite Installation from Catalog, then Lock.
    pub fn add_skill_with_deps(
        &self,
        name: &str,
        library_filter: Option<&str>,
        with_deps: bool,
        attach: bool,
        from_library: bool,
    ) -> Result<(AddOutcome, Option<String>, Vec<String>)> {
        skill::validate_skill_name(name)?;
        if from_library && !attach {
            return Err(SkillzError::Message(
                "`--from-library` requires `--attach` (link an on-disk skill, optionally replacing files from the Catalog)".into(),
            ));
        }
        let config = self.load_scope_config()?;
        let rows = self.list_skills(library_filter, &[], false, false)?;
        let matches: Vec<_> = rows.into_iter().filter(|r| r.entry.name == name).collect();
        if matches.is_empty() {
            return Err(SkillzError::Message(format!(
                "skill `{name}` not found in bound Libraries"
            )));
        }
        if matches.len() > 1 && library_filter.is_none() {
            let libs: Vec<_> = matches.iter().map(|m| m.entry.library.as_str()).collect();
            return Err(SkillzError::Message(format!(
                "skill `{name}` found in multiple Libraries ({}); pass --library",
                libs.join(", ")
            )));
        }
        let row = &matches[0];
        let warn = if !row.availability.project
            && (row.availability.user || !row.availability.other_roots.is_empty())
        {
            Some(format!(
                "skill `{name}` exists in {} but not in this Project; installing for collaborators",
                row.availability.summary()
            ))
        } else {
            None
        };

        let dest = skill::skill_dir(&self.scope_skills_root(&config), name);
        let outcome = if attach {
            self.add_attach_skill(&config, name, &row.entry, from_library)?
        } else if dest.exists() {
            return Err(SkillzError::Message(format!(
                "skill `{name}` already exists at {}; use `skillz add {name} --attach` to Lock the on-disk skill to a Catalog Entry, or `skillz add {name} --attach --from-library` to replace it from the Catalog",
                dest.display()
            )));
        } else if row.entry.kind == EntryKind::Upstream {
            self.add_upstream_installation(&config, name, &row.entry)?
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    SkillzError::Message(format!(
                        "failed to create {}: {e}",
                        parent.display()
                    ))
                })?;
            }
            skill::copy_skill_tree(&row.entry.skill_path, &dest)?;
            let install = self.write_owned_lock(name, &row.entry)?;
            AddOutcome::Installed(install)
        };

        // Sibling dependency detection (best-effort).
        let catalog_names: Vec<String> = self
            .list_skills(library_filter, &[], true, false)?
            .into_iter()
            .map(|r| r.entry.name)
            .collect();
        let present = deps::list_skill_names(&self.scope_skills_root(&config))?;
        let dep_scan_path = if dest.join("SKILL.md").is_file() {
            &dest
        } else {
            &row.entry.skill_path
        };
        let mut missing = self.sibling_dep_warnings(
            dep_scan_path,
            &catalog_names,
            &present,
        )?;
        let mut pulled = Vec::new();
        if with_deps {
            for dep in missing.clone() {
                if let Ok((dep_outcome, _, nested)) = self.add_skill_with_deps(
                    &dep,
                    library_filter.or(Some(&row.entry.library)),
                    true,
                    false,
                    false,
                ) {
                    let _ = dep_outcome;
                    pulled.push(dep);
                    pulled.extend(nested);
                }
            }
            missing.retain(|m| !pulled.iter().any(|p| p == m));
        }
        Ok((outcome, warn, missing))
    }

    /// `--attach` / `--attach --from-library`: Lock on-disk skill; never write Library.
    fn add_attach_skill(
        &self,
        config: &ProjectConfig,
        name: &str,
        entry: &CatalogEntry,
        from_library: bool,
    ) -> Result<AddOutcome> {
        let dest = skill::skill_dir(&self.scope_skills_root(config), name);
        if from_library {
            if entry.kind == EntryKind::Upstream {
                return self.add_upstream_installation(config, name, entry);
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    SkillzError::Message(format!(
                        "failed to create {}: {e}",
                        parent.display()
                    ))
                })?;
            }
            skill::copy_skill_tree(&entry.skill_path, &dest)?;
            let install = self.write_owned_lock(name, entry)?;
            return Ok(AddOutcome::Installed(install));
        }
        if !dest.join("SKILL.md").is_file() {
            return Err(SkillzError::Message(format!(
                "skill `{name}` not found under {}; `--attach` requires an on-disk skill (or pass `--from-library` to take Catalog files)",
                dest.display()
            )));
        }
        // Lock to Catalog Entry using on-disk hash; clears Local membership.
        let install = self.attach_to_catalog(name, &entry.library, entry.kind)?;
        Ok(AddOutcome::Installed(install))
    }

    /// Add every Catalog Entry matching tag filters.
    pub fn add_skills_by_tag(
        &self,
        tags: &[String],
        library_filter: Option<&str>,
    ) -> Result<Vec<(String, AddOutcome, Option<String>)>> {
        let rows = self.list_skills(library_filter, tags, true, false)?;
        let mut out = Vec::new();
        for row in rows {
            let (outcome, warn) = self.add_skill(&row.entry.name, Some(&row.entry.library))?;
            out.push((row.entry.name, outcome, warn));
        }
        Ok(out)
    }

    fn write_owned_lock(&self, name: &str, entry: &CatalogEntry) -> Result<InstallationLock> {
        let install = InstallationLock {
            library: entry.library.clone(),
            entry_kind: EntryKind::Owned,
            content_hash: entry.content_hash.clone(),
            source: None,
            rev: None,
            branch: None,
            overlay_from_library: None,
            overlay_from_entry: None,
        };
        let mut lock = self.load_scope_lock()?;
        lock.installations.insert(name.to_string(), install.clone());
        self.save_scope_lock(&lock)?;
        Ok(install)
    }

    fn add_upstream_installation(
        &self,
        config: &ProjectConfig,
        name: &str,
        entry: &CatalogEntry,
    ) -> Result<AddOutcome> {
        let source = entry.source.as_deref().ok_or_else(|| {
            SkillzError::Message(format!(
                "upstream Catalog Entry `{name}` is missing a source string"
            ))
        })?;
        let dest = skill::skill_dir(&self.scope_skills_root(config), name);
        let already = dest.join("SKILL.md").is_file();
        let prev_hash = if already {
            Some(skill::content_hash(&dest)?)
        } else {
            None
        };

        let (pinned_source, pinned_rev) = if source.starts_with("cmd:") {
            let mut parsed = source::CmdSource::parse(source)?;
            let version = parsed.validate_install_and_probe(&dest, name)?;
            parsed = parsed.with_rev(version.clone());
            (parsed.render(), Some(version))
        } else {
            let parsed = source::GithubSource::parse(source)?;
            let cache = self.paths.config_dir.join("cache");
            let (skill_path, pinned) = fetch::resolve_github_skill(&cache, &parsed, Some(name))?;
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    SkillzError::Message(format!(
                        "failed to create {}: {e}",
                        parent.display()
                    ))
                })?;
            }
            skill::copy_skill_tree(&skill_path, &dest)?;
            (pinned.render(), pinned.rev.clone())
        };

        let hash = skill::content_hash(&dest)?;
        let install = InstallationLock {
            library: entry.library.clone(),
            entry_kind: EntryKind::Upstream,
            content_hash: hash.clone(),
            source: Some(pinned_source),
            rev: pinned_rev,
            branch: None,
            overlay_from_library: None,
            overlay_from_entry: None,
        };
        let mut lock = self.load_scope_lock()?;
        lock.installations.insert(name.to_string(), install.clone());
        self.save_scope_lock(&lock)?;
        Ok(match prev_hash {
            Some(prev) if prev == hash => AddOutcome::AlreadyPresent(install),
            Some(_) => AddOutcome::Refreshed(install),
            None => AddOutcome::Installed(install),
        })
    }

    fn writable_library_path(&self, library_name: &str) -> Result<(ProjectConfig, PathBuf)> {
        let config = self.load_scope_config()?;
        let binding = config
            .bindings
            .iter()
            .find(|b| b.name == library_name)
            .ok_or_else(|| {
                SkillzError::Message(format!(
                    "library `{library_name}` is not a Binding on this Managed Project"
                ))
            })?;
        if binding.read_only {
            return Err(SkillzError::Message(format!(
                "library `{library_name}` is read-only; choose a writable Binding"
            )));
        }
        let registry = self.load_registry()?;
        let record = registry.libraries.get(library_name).ok_or_else(|| {
            SkillzError::Message(format!(
                "library `{library_name}` is not in the local registry"
            ))
        })?;
        if record.read_only {
            return Err(SkillzError::Message(format!(
                "library `{library_name}` is marked read-only in the registry"
            )));
        }
        Ok((config, record.path.clone()))
    }

    /// Stage an Upstream skill into the Project skills dir (Unlocked until managed).
    ///
    /// `source` is a filesystem path, `github:…` / HTTPS / SSH GitHub remote, or
    /// `cmd:…`. Optional `skill_name` enables nested name lookup / cmd install name
    /// (`skillz add --source github:owner/repo NAME`).
    pub fn fetch_skill(
        &self,
        source: &str,
        skill_name: Option<&str>,
    ) -> Result<(PathBuf, Option<String>)> {
        let config = self.load_scope_config()?;
        let skills_root = self.scope_skills_root(&config);
        std::fs::create_dir_all(&skills_root).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create {}: {e}",
                skills_root.display()
            ))
        })?;

        if source.starts_with("cmd:") {
            let name = skill_name.ok_or_else(|| {
                SkillzError::Message(
                    "`add --source cmd:…` requires a skill NAME".into(),
                )
            })?;
            skill::validate_skill_name(name)?;
            let dest = skill::skill_dir(&skills_root, name);
            let mut parsed = source::CmdSource::parse(source)?;
            let version = parsed.validate_install_and_probe(&dest, name)?;
            parsed = parsed.with_rev(version);
            let pinned = parsed.render();
            let sidecar = dest.join(".skillz-source");
            std::fs::write(&sidecar, &pinned).map_err(|e| {
                SkillzError::Message(format!("failed to write {}: {e}", sidecar.display()))
            })?;
            return Ok((dest, Some(pinned)));
        }

        if source::GithubSource::looks_like_remote(source) {
            let parsed = source::GithubSource::parse_input(source)?;
            let cache = self.paths.config_dir.join("cache");
            let (skill_path, pinned) =
                fetch::resolve_github_skill(&cache, &parsed, skill_name)?;
            let dest = fetch::stage_skill(&skills_root, &skill_path, skill_name)?;
            // Stash pinned source beside the skill for publish (sidecar).
            let sidecar = dest.join(".skillz-source");
            std::fs::write(&sidecar, pinned.render()).map_err(|e| {
                SkillzError::Message(format!("failed to write {}: {e}", sidecar.display()))
            })?;
            Ok((dest, Some(pinned.render())))
        } else {
            let path = PathBuf::from(source);
            let skill_path = if let Some(name) = skill_name {
                // Treat `source` as a repo/tree root for name lookup.
                let root = if path.is_absolute() {
                    path
                } else {
                    self.paths.cwd.join(path)
                };
                fetch::find_skill_by_name(&root, name)?
            } else {
                fetch::resolve_local_skill_path(&if path.is_absolute() {
                    path
                } else {
                    self.paths.cwd.join(path)
                })?
            };
            let dest = fetch::stage_skill(&skills_root, &skill_path, skill_name)?;
            Ok((dest, None))
        }
    }

    /// `add --source`: stage into skills_dir; with `local`, mark Local+source.
    ///
    /// Without `local`, the skill stays Unlocked (no Installation Lock).
    pub fn add_from_source(
        &self,
        source: &str,
        skill_name: Option<&str>,
        local: bool,
    ) -> Result<(PathBuf, Option<String>)> {
        let (dest, pinned) = self.fetch_skill(source, skill_name)?;
        if local {
            let name = dest
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    SkillzError::Message(format!(
                        "staged skill path has non-utf8 name: {}",
                        dest.display()
                    ))
                })?;
            let provenance = pinned.as_deref().ok_or_else(|| {
                SkillzError::Message(
                    "`add --source … --local` requires `github:` or `cmd:` provenance (filesystem path staging cannot use --local; use `new --local` for Local authored)".into(),
                )
            })?;
            self.set_local_source(name, provenance)?;
        }
        Ok((dest, pinned))
    }

    /// Publish managed skills to a Library (direct → main; review → Branch Lock).
    pub fn publish_skills(
        &self,
        names: &[String],
        library_name: &str,
        mode: PublishMode,
        message: Option<&str>,
    ) -> Result<PublishReport> {
        self.publish_with_options(
            names,
            library_name,
            &PublishOptions {
                mode,
                message: message.map(str::to_string),
                ..PublishOptions::default()
            },
        )
    }

    /// Publish with review push/PR flags and optional test seams.
    pub fn publish_with_options(
        &self,
        names: &[String],
        library_name: &str,
        opts: &PublishOptions,
    ) -> Result<PublishReport> {
        if names.is_empty() {
            return Err(SkillzError::Message(
                "provide one or more skill names to publish".into(),
            ));
        }
        let (config, lib_path) = self.writable_library_path(library_name)?;
        let meta = LibraryMeta::load(&lib_path)?;
        let contribution = meta.contribution;

        let lock = self.load_scope_lock()?;
        let catalog_names: std::collections::HashSet<String> = self
            .list_skills(Some(library_name), &[], true, false)?
            .into_iter()
            .map(|r| r.entry.name)
            .collect();

        #[derive(Clone)]
        enum Planned {
            CreateOwned,
            CreateUpstream { source: String },
            UpdateOwned,
            UpdateUpstream { source: String },
        }

        let mut plans: Vec<(String, Planned)> = Vec::new();
        for name in names {
            skill::validate_skill_name(name)?;
            let project_skill = skill::skill_dir(&self.scope_skills_root(&config), name);
            if !project_skill.join("SKILL.md").is_file() {
                return Err(SkillzError::Message(format!(
                    "skill `{name}` not found under {}",
                    config.skills_dir
                )));
            }
            let rec = lock.installations.get(name).ok_or_else(|| {
                SkillzError::Message(format!(
                    "skill `{name}` is unmanaged (Unlocked); mark Local (`new --local` / `add --source --local`), run `migrate`, or `add {name} --attach` before publish"
                ))
            })?;

            let in_catalog = catalog_names.contains(name);
            let plan = match rec.entry_kind {
                EntryKind::Local => {
                    if in_catalog {
                        return Err(SkillzError::Message(format!(
                            "skill `{name}` is Local but already exists in library `{library_name}` Catalog; use `skillz add {name} --attach` (or `--attach --from-library`) instead of publish"
                        )));
                    }
                    match &rec.source {
                        None => Planned::CreateOwned,
                        Some(src) => Planned::CreateUpstream {
                            source: src.clone(),
                        },
                    }
                }
                EntryKind::Owned => {
                    if rec.library != library_name {
                        return Err(SkillzError::Message(format!(
                            "skill `{name}` is Locked to library `{}`, not `{library_name}`",
                            rec.library
                        )));
                    }
                    Planned::UpdateOwned
                }
                EntryKind::Upstream => {
                    if rec.library != library_name {
                        return Err(SkillzError::Message(format!(
                            "skill `{name}` is Locked to library `{}`, not `{library_name}`",
                            rec.library
                        )));
                    }
                    let source = rec.source.clone().ok_or_else(|| {
                        SkillzError::Message(format!(
                            "skill `{name}` Upstream Lock is missing source provenance"
                        ))
                    })?;
                    Planned::UpdateUpstream { source }
                }
            };

            let creates = matches!(
                plan,
                Planned::CreateOwned | Planned::CreateUpstream { .. }
            );
            match opts.mode {
                PublishMode::UpdateOnly if creates => {
                    return Err(SkillzError::Message(format!(
                        "publish --update: `{name}` would create a Catalog Entry (not an update)"
                    )));
                }
                PublishMode::CreateOnly if in_catalog || !creates => {
                    return Err(SkillzError::Message(format!(
                        "publish --create-only: `{name}` already exists in the Catalog (or is not a Local create)"
                    )));
                }
                _ => {}
            }
            plans.push((name.clone(), plan));
        }

        let batch: std::collections::HashSet<&str> =
            names.iter().map(String::as_str).collect();
        let present = deps::list_skill_names(&self.scope_skills_root(&config))?;
        let mut missing_deps = Vec::new();
        for name in names {
            let path = skill::skill_dir(&self.scope_skills_root(&config), name);
            for dep in self.sibling_dep_warnings(&path, &present, &present)? {
                if !batch.contains(dep.as_str()) && !missing_deps.iter().any(|d| d == &dep) {
                    missing_deps.push(dep);
                }
            }
        }

        let review_branch = match contribution {
            Contribution::Review => Some(review_publish_branch_name(names)),
            Contribution::Direct => None,
        };
        let msg = opts
            .message
            .clone()
            .unwrap_or_else(|| "skillz publish".into());

        if opts.dry_run {
            let mut next_steps = Vec::new();
            let mut warnings = Vec::new();
            match contribution {
                Contribution::Direct => {
                    next_steps.push(format!(
                        "would commit {} skill(s) to library `{library_name}` main",
                        plans.len()
                    ));
                }
                Contribution::Review => {
                    let branch = review_branch.as_deref().unwrap();
                    next_steps.push(format!(
                        "would create Branch Lock `{branch}` for {} skill(s)",
                        plans.len()
                    ));
                    if opts.no_push {
                        warnings.push("would skip push (--no-push)".into());
                    } else {
                        next_steps.push(format!("would push `{branch}` to Library remote"));
                        if opts.no_pr {
                            warnings.push("would skip PR/MR (--no-pr)".into());
                        } else {
                            next_steps.push("would open PR/MR via gh/glab (best-effort)".into());
                        }
                    }
                }
            }
            let dry_branch = review_branch.clone();
            return Ok(PublishReport {
                library: library_name.to_string(),
                contribution,
                rev: "(dry-run)".into(),
                branch: review_branch,
                skills: plans
                    .iter()
                    .map(|(name, plan)| {
                        let (kind, created) = match plan {
                            Planned::CreateOwned => (EntryKind::Owned, true),
                            Planned::UpdateOwned => (EntryKind::Owned, false),
                            Planned::CreateUpstream { .. } => (EntryKind::Upstream, true),
                            Planned::UpdateUpstream { .. } => (EntryKind::Upstream, false),
                        };
                        PublishSkillResult {
                            name: name.clone(),
                            kind,
                            created,
                            lock: InstallationLock {
                                library: library_name.to_string(),
                                entry_kind: kind,
                                content_hash: String::new(),
                                source: None,
                                rev: None,
                                branch: dry_branch.clone(),
                                overlay_from_library: None,
                                overlay_from_entry: None,
                            },
                        }
                    })
                    .collect(),
                missing_deps,
                dry_run: true,
                pushed: false,
                pr_url: None,
                warnings,
                next_steps,
            });
        }

        let mut commit_paths: Vec<PathBuf> = Vec::new();
        let mut results: Vec<(String, bool, EntryKind, InstallationLock)> = Vec::new();

        for (name, plan) in &plans {
            let project_skill = skill::skill_dir(&self.scope_skills_root(&config), name);
            match plan {
                Planned::CreateOwned | Planned::UpdateOwned => {
                    let owned = lib_path.join(crate::library::SKILLS_DIR).join(name);
                    skill::copy_skill_tree(&project_skill, &owned)?;
                    commit_paths.push(owned);
                    let hash = skill::content_hash(&project_skill)?;
                    let install = InstallationLock {
                        library: library_name.to_string(),
                        entry_kind: EntryKind::Owned,
                        content_hash: hash,
                        source: None,
                        rev: None,
                        branch: None,
                        overlay_from_library: None,
                        overlay_from_entry: None,
                    };
                    let created = matches!(plan, Planned::CreateOwned);
                    results.push((name.clone(), created, EntryKind::Owned, install));
                }
                Planned::CreateUpstream { source }
                | Planned::UpdateUpstream { source } => {
                    let (catalog_source, lock_source, lock_rev) =
                        self.resolve_upstream_publish_source(name, &project_skill, source)?;
                    let mut meta = LibraryMeta::load(&lib_path)?;
                    let existing_tags = meta
                        .upstream
                        .iter()
                        .find(|e| e.name == *name)
                        .map(|e| e.tags.clone())
                        .unwrap_or_default();
                    meta.upsert_upstream(crate::library::UpstreamEntry {
                        name: name.clone(),
                        source: catalog_source,
                        tags: existing_tags,
                    });
                    meta.save(&lib_path)?;
                    commit_paths.push(lib_path.join(crate::library::LIBRARY_TOML));
                    let hash = skill::content_hash(&project_skill)?;
                    let install = InstallationLock {
                        library: library_name.to_string(),
                        entry_kind: EntryKind::Upstream,
                        content_hash: hash,
                        source: Some(lock_source),
                        rev: lock_rev,
                        branch: None,
                        overlay_from_library: None,
                        overlay_from_entry: None,
                    };
                    let created = matches!(plan, Planned::CreateUpstream { .. });
                    results.push((name.clone(), created, EntryKind::Upstream, install));
                }
            }
        }

        commit_paths.sort();
        commit_paths.dedup();
        let path_refs: Vec<&Path> = commit_paths.iter().map(Path::new).collect();

        let (rev, branch_for_lock) = match contribution {
            Contribution::Direct => {
                let rev = vcs_ops::commit_on_main(&lib_path, &msg, &path_refs)?;
                (rev, None)
            }
            Contribution::Review => {
                let branch = review_branch.clone().unwrap();
                let commit =
                    vcs_ops::commit_on_branch(&lib_path, &branch, &msg, &path_refs)?;
                (commit.rev, Some(commit.branch))
            }
        };

        let mut scope_lock = self.load_scope_lock()?;
        let mut skills = Vec::new();
        for (name, created, kind, mut install) in results {
            match contribution {
                Contribution::Direct => {
                    // Owned pins Library main tip; Upstream keeps source/@rev pin.
                    if kind == EntryKind::Owned {
                        install.rev = Some(rev.clone());
                    }
                    install.branch = None;
                }
                Contribution::Review => {
                    // Pending Branch Lock: all skills in the unit share tip + branch.
                    install.rev = Some(rev.clone());
                    install.branch = branch_for_lock.clone();
                }
            }
            scope_lock
                .installations
                .insert(name.clone(), install.clone());
            skills.push(PublishSkillResult {
                name,
                kind,
                created,
                lock: install,
            });
        }
        self.save_scope_lock(&scope_lock)?;

        let mut warnings = Vec::new();
        let mut pushed = false;
        let mut pr_url = None;
        let mut next_steps = Vec::new();

        match contribution {
            Contribution::Direct => {
                next_steps.push(format!(
                    "Library `{library_name}` main updated @{}",
                    &rev[..rev.len().min(12)]
                ));
                next_steps.push(
                    "Collaborators: pull Library; run `skillz update` / `skillz status` as needed"
                        .into(),
                );
            }
            Contribution::Review => {
                let branch = branch_for_lock.as_deref().unwrap();
                if opts.no_push {
                    warnings.push(format!(
                        "skipped push (--no-push); Branch Lock `{branch}`@{rev} is local-only until pushed"
                    ));
                    next_steps.push(format!(
                        "git -C {} push -u origin {branch}",
                        lib_path.display()
                    ));
                } else {
                    let push_result = match &opts.mock_push {
                        Some(r) => r.clone().map_err(SkillzError::Message),
                        None => vcs_ops::push_branch(&lib_path, branch, "origin"),
                    };
                    match push_result {
                        Ok(()) => {
                            pushed = true;
                            next_steps.push(format!("pushed `{branch}` to origin"));
                        }
                        Err(e) => {
                            warnings.push(format!(
                                "push failed ({e}); Branch Lock is local — push before collaborators can use it"
                            ));
                            next_steps.push(format!(
                                "git -C {} push -u origin {branch}",
                                lib_path.display()
                            ));
                        }
                    }
                }

                if pushed && !opts.no_pr {
                    let title = format!("skillz publish: {}", names.join(", "));
                    let pr_result = match &opts.mock_pr {
                        Some(r) => r.clone(),
                        None => vcs_ops::try_open_merge_request(&lib_path, branch, &title),
                    };
                    match pr_result {
                        Ok(Some(url)) => {
                            pr_url = Some(url.clone());
                            next_steps.push(format!("view PR/MR: {url}"));
                        }
                        Ok(None) => {
                            warnings.push(
                                "no gh/glab available to open PR/MR; use copy-paste next steps"
                                    .into(),
                            );
                            next_steps.extend(vcs_ops::copy_paste_open_pr_commands(
                                &lib_path, branch,
                            ));
                        }
                        Err(msg) => {
                            warnings.push(format!(
                                "PR/MR not opened ({msg}); publish succeeded after push"
                            ));
                            next_steps.extend(vcs_ops::copy_paste_open_pr_commands(
                                &lib_path, branch,
                            ));
                        }
                    }
                } else if opts.no_pr && pushed {
                    warnings.push("skipped PR/MR (--no-pr)".into());
                    next_steps.extend(vcs_ops::copy_paste_open_pr_commands(&lib_path, branch));
                }

                next_steps.push(
                    "Merge with a merge commit (not squash) so Branch Lock SHAs remain on main"
                        .into(),
                );
                next_steps.push(
                    "After merge: `skillz update <names…>` and/or `skillz status`".into(),
                );
            }
        }

        Ok(PublishReport {
            library: library_name.to_string(),
            contribution,
            rev,
            branch: branch_for_lock,
            skills,
            missing_deps,
            dry_run: false,
            pushed,
            pr_url,
            warnings,
            next_steps,
        })
    }

    fn resolve_upstream_publish_source(
        &self,
        name: &str,
        project_skill: &Path,
        source: &str,
    ) -> Result<(String, String, Option<String>)> {
        if source.starts_with("cmd:") {
            let mut parsed = source::CmdSource::parse(source)?;
            let version = parsed.validate_install_and_probe(project_skill, name)?;
            parsed = parsed.with_rev(version.clone());
            Ok((parsed.render_catalog(), parsed.render(), Some(version)))
        } else if source::GithubSource::looks_like_remote(source) {
            let parsed = source::GithubSource::parse_input(source)?;
            if parsed.rev.is_none() {
                return Err(SkillzError::Message(format!(
                    "skill `{name}` Upstream source must include @rev for publish"
                )));
            }
            let rendered = parsed.render();
            Ok((rendered.clone(), rendered, parsed.rev.clone()))
        } else {
            Err(SkillzError::Message(format!(
                "skill `{name}` has unsupported Upstream source `{source}` (expected github:… or cmd:…)"
            )))
        }
    }

    /// Formalize a tested Installation into a writable Library as an Owned Entry.
    pub fn libcommit_authored(
        &self,
        name: &str,
        library_name: &str,
        tags: &[String],
    ) -> Result<InstallationLock> {
        skill::validate_skill_name(name)?;
        let (config, lib_path) = self.writable_library_path(library_name)?;

        let project_skill = skill::skill_dir(&self.scope_skills_root(&config), name);
        if !project_skill.is_dir() {
            return Err(SkillzError::Message(format!(
                "skill `{name}` not found under {}",
                config.skills_dir
            )));
        }
        self.merge_tags_into_skill_dir(&project_skill, tags)?;
        let hash = skill::content_hash(&project_skill)?;
        let owned = lib_path.join(crate::library::SKILLS_DIR).join(name);
        skill::copy_skill_tree(&project_skill, &owned)?;

        let install = InstallationLock {
            library: library_name.to_string(),
            entry_kind: EntryKind::Owned,
            content_hash: hash,
            source: None,
            rev: None,
            branch: None,
            overlay_from_library: None,
            overlay_from_entry: None,
        };
        let mut lock = self.load_scope_lock()?;
        lock.installations.insert(name.to_string(), install.clone());
        self.save_scope_lock(&lock)?;
        Ok(install)
    }

    /// Edit path: push Installation into Library on a branch and Lock to that rev.
    ///
    /// If the desired Library is read-only, performs an Overlay Fork into the first
    /// writable Binding instead (unless `force_overlay_into` selects one).
    pub fn libcommit_branch(
        &self,
        name: &str,
        library_name: Option<&str>,
        message: Option<&str>,
        tags: &[String],
    ) -> Result<InstallationLock> {
        skill::validate_skill_name(name)?;
        let config = self.load_scope_config()?;
        let mut lock = self.load_scope_lock()?;
        let existing = lock.installations.get(name).cloned();
        let desired = library_name
            .map(|s| s.to_string())
            .or_else(|| existing.as_ref().map(|e| e.library.clone()))
            .or_else(|| {
                // Unlocked skills in drift -i: fall back to first writable Binding.
                self.first_writable_binding(&config)
                    .ok()
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                SkillzError::Message(format!(
                    "skill `{name}` has no Lock; pass --library or libcommit --as first"
                ))
            })?;

        let project_skill = skill::skill_dir(&self.scope_skills_root(&config), name);
        if !project_skill.is_dir() {
            return Err(SkillzError::Message(format!(
                "skill `{name}` not found under {}",
                config.skills_dir
            )));
        }
        self.merge_tags_into_skill_dir(&project_skill, tags)?;

        let (target_library, overlay_from) = match self.writable_library_path(&desired) {
            Ok(_) => (desired.clone(), None),
            Err(_) => {
                let writable = self.first_writable_binding(&config)?.to_string();
                let provenance = existing
                    .as_ref()
                    .and_then(|e| e.source.clone())
                    .unwrap_or_else(|| format!("{desired}/{name}"));
                (writable, Some((desired.clone(), provenance)))
            }
        };

        let (_config, lib_path) = self.writable_library_path(&target_library)?;
        let hash = skill::content_hash(&project_skill)?;
        let owned = lib_path.join(crate::library::SKILLS_DIR).join(name);
        skill::copy_skill_tree(&project_skill, &owned)?;

        let branch = format!("skillz/{name}");
        let msg = message.unwrap_or("skillz libcommit Branch Lock");
        let commit = vcs_ops::commit_on_branch(&lib_path, &branch, msg, &[&owned])?;

        let (overlay_from_library, overlay_from_entry) = match overlay_from {
            Some((lib, entry)) => (Some(lib), Some(entry)),
            None => (None, None),
        };

        let install = InstallationLock {
            library: target_library,
            entry_kind: EntryKind::Owned,
            content_hash: hash,
            source: existing.and_then(|e| e.source),
            rev: Some(commit.rev),
            branch: Some(commit.branch),
            overlay_from_library,
            overlay_from_entry,
        };
        lock.installations.insert(name.to_string(), install.clone());
        self.save_scope_lock(&lock)?;
        Ok(install)
    }

    fn first_writable_binding<'a>(&self, config: &'a ProjectConfig) -> Result<&'a str> {
        let registry = self.load_registry()?;
        for binding in &config.bindings {
            if binding.read_only {
                continue;
            }
            if let Some(rec) = registry.libraries.get(&binding.name) {
                if !rec.read_only {
                    return Ok(binding.name.as_str());
                }
            }
        }
        Err(SkillzError::Message(
            "no writable Library Binding available for Overlay Fork".into(),
        ))
    }

    /// Merge a Branch Lock into the Library main line.
    pub fn promote_skill(&self, name: &str) -> Result<String> {
        let config = self.load_scope_config()?;
        let lock = self.load_scope_lock()?;
        let install = lock.installations.get(name).ok_or_else(|| {
            SkillzError::Message(format!("skill `{name}` is not locked"))
        })?;
        let branch = install.branch.clone().ok_or_else(|| {
            SkillzError::Message(format!(
                "skill `{name}` has no Branch Lock to promote"
            ))
        })?;
        let registry = self.load_registry()?;
        let record = registry.libraries.get(&install.library).ok_or_else(|| {
            SkillzError::Message(format!(
                "library `{}` is not in the local registry",
                install.library
            ))
        })?;
        let _ = config;
        vcs_ops::promote_branch_to_main(&record.path, &branch)
    }

    /// Refresh Installation + Lock from Library Catalog main tip (remote-aware).
    ///
    /// Distinct from `publish -u` (Project → Library) and `add --attach` (link without refresh).
    pub fn update_skill(&self, name: &str) -> Result<InstallationLock> {
        skill::validate_skill_name(name)?;
        let config = self.load_scope_config()?;
        let mut lock = self.load_scope_lock()?;
        let install = lock.installations.get(name).cloned().ok_or_else(|| {
            SkillzError::Message(format!(
                "skill `{name}` is not Library-managed; `update` refreshes from Catalog main (use `add --attach` or `publish` first)"
            ))
        })?;
        if install.entry_kind == EntryKind::Local {
            return Err(SkillzError::Message(format!(
                "skill `{name}` is Local; `update` only refreshes Library-backed Installations"
            )));
        }
        let registry = self.load_registry()?;
        let record = registry.libraries.get(&install.library).ok_or_else(|| {
            SkillzError::Message(format!(
                "library `{}` is not in the local registry",
                install.library
            ))
        })?;
        vcs_ops::fetch_remote(&record.path, "origin")?;
        vcs_ops::checkout_main(&record.path)?;
        let main_rev = vcs_ops::main_rev(&record.path)?;

        let updated = match install.entry_kind {
            EntryKind::Owned => {
                let owned = record.path.join(crate::library::SKILLS_DIR).join(name);
                if !owned.is_dir() {
                    return Err(SkillzError::Message(format!(
                        "Owned Entry `{name}` missing on Library `{}` main at {}",
                        install.library,
                        owned.display()
                    )));
                }
                let dest = skill::skill_dir(&self.scope_skills_root(&config), name);
                skill::copy_skill_tree(&owned, &dest)?;
                let hash = skill::content_hash(&dest)?;
                InstallationLock {
                    library: install.library,
                    entry_kind: EntryKind::Owned,
                    content_hash: hash,
                    source: install.source,
                    rev: Some(main_rev),
                    branch: None,
                    overlay_from_library: install.overlay_from_library,
                    overlay_from_entry: install.overlay_from_entry,
                }
            }
            EntryKind::Upstream => {
                let meta = LibraryMeta::load(&record.path)?;
                let entry = meta
                    .upstream
                    .iter()
                    .find(|e| e.name == name)
                    .ok_or_else(|| {
                        SkillzError::Message(format!(
                            "Upstream Entry `{name}` missing from library `{}` catalog on main",
                            install.library
                        ))
                    })?;
                let catalog = CatalogEntry {
                    name: name.to_string(),
                    library: install.library.clone(),
                    library_path: record.path.clone(),
                    skill_path: PathBuf::new(),
                    content_hash: String::new(),
                    kind: EntryKind::Upstream,
                    source: Some(entry.source.clone()),
                    tags: entry.tags.clone(),
                };
                let outcome = self.add_upstream_installation(&config, name, &catalog)?;
                let mut refreshed = outcome.lock().clone();
                refreshed.branch = None;
                // Keep Upstream source pin; record Library main tip when no source rev.
                if refreshed.rev.is_none() {
                    refreshed.rev = Some(main_rev);
                }
                refreshed
            }
            EntryKind::Local => unreachable!(),
        };
        lock.installations.insert(name.to_string(), updated.clone());
        self.save_scope_lock(&lock)?;
        Ok(updated)
    }

    pub fn update_skills(&self, names: &[String]) -> Result<Vec<InstallationLock>> {
        let mut out = Vec::new();
        for name in names {
            out.push(self.update_skill(name)?);
        }
        Ok(out)
    }

    /// After promote, re-Lock to Library main and refresh the Installation.
    pub fn lock_main(&self, name: &str) -> Result<InstallationLock> {
        self.update_skill(name)
    }

    /// Formalize a fetched Installation as an Upstream Catalog Entry + Lock.
    pub fn libcommit_upstream(
        &self,
        name: &str,
        library_name: &str,
        source: Option<&str>,
        tags: &[String],
    ) -> Result<InstallationLock> {
        skill::validate_skill_name(name)?;
        let (config, lib_path) = self.writable_library_path(library_name)?;
        let project_skill = skill::skill_dir(&self.scope_skills_root(&config), name);
        if !project_skill.is_dir() {
            return Err(SkillzError::Message(format!(
                "skill `{name}` not found under {}",
                config.skills_dir
            )));
        }

        let source = match source {
            Some(s) => s.to_string(),
            None => {
                let sidecar = project_skill.join(".skillz-source");
                if sidecar.is_file() {
                    std::fs::read_to_string(&sidecar).map_err(|e| {
                        SkillzError::Message(format!(
                            "failed to read {}: {e}",
                            sidecar.display()
                        ))
                    })?
                    .trim()
                    .to_string()
                } else {
                    return Err(SkillzError::Message(
                        "upstream libcommit requires a source string (pass --source as github:… or cmd:…, or fetch via github:…)"
                            .into(),
                    ));
                }
            }
        };
        let (catalog_source, lock_source, lock_rev) = if source.starts_with("cmd:") {
            let mut parsed = source::CmdSource::parse(&source)?;
            // Eager validation: install template + version probe (ADR 0005).
            let version = parsed.validate_install_and_probe(&project_skill, name)?;
            parsed = parsed.with_rev(version.clone());
            (
                parsed.render_catalog(),
                parsed.render(),
                Some(version),
            )
        } else if source.starts_with("github:") {
            let parsed = source::GithubSource::parse(&source)?;
            if parsed.rev.is_none() {
                return Err(SkillzError::Message(
                    "upstream Lock source must include @rev".into(),
                ));
            }
            (source.clone(), source.clone(), parsed.rev.clone())
        } else {
            return Err(SkillzError::Message(format!(
                "unsupported upstream source `{source}` (expected github:… or cmd:…)"
            )));
        };

        // Upstream Catalog Entry is a reference — do not vendor into Library skills/.
        let mut meta = LibraryMeta::load(&lib_path)?;
        let existing_tags = meta
            .upstream
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.tags.clone())
            .unwrap_or_default();
        let merged_tags = catalog::merge_tags(&existing_tags, tags);
        meta.upsert_upstream(crate::library::UpstreamEntry {
            name: name.to_string(),
            source: catalog_source,
            tags: merged_tags,
        });
        meta.save(&lib_path)?;

        let hash = skill::content_hash(&project_skill)?;

        let install = InstallationLock {
            library: library_name.to_string(),
            entry_kind: EntryKind::Upstream,
            content_hash: hash,
            source: Some(lock_source),
            rev: lock_rev,
            branch: None,
            overlay_from_library: None,
            overlay_from_entry: None,
        };
        let mut lock = self.load_scope_lock()?;
        lock.installations.insert(name.to_string(), install.clone());
        self.save_scope_lock(&lock)?;
        Ok(install)
    }

    fn merge_tags_into_skill_dir(&self, skill_dir: &Path, tags: &[String]) -> Result<()> {
        if catalog::normalize_tag_filters(tags).is_empty() {
            return Ok(());
        }
        let md = skill_dir.join("SKILL.md");
        if !md.is_file() {
            return Err(SkillzError::Message(format!(
                "SKILL.md not found under {}",
                skill_dir.display()
            )));
        }
        catalog::merge_tags_into_frontmatter(&md, tags)?;
        Ok(())
    }

    /// Merge tags onto Library Catalog Entries (Owned SKILL.md or Upstream `library.toml`).
    ///
    /// Returns `(name, kind, merged_tags)` for each skill.
    pub fn tag_library_skills(
        &self,
        library_name: &str,
        names: &[String],
        tags: &[String],
    ) -> Result<Vec<(String, EntryKind, Vec<String>)>> {
        let assign = catalog::normalize_tag_filters(tags);
        if assign.is_empty() {
            return Err(SkillzError::Message(
                "provide at least one tag with --tag / -t".into(),
            ));
        }
        if names.is_empty() {
            return Err(SkillzError::Message(
                "provide one or more skill names".into(),
            ));
        }
        let lib_path = self.writable_registry_library(library_name)?;
        let owned = catalog::list_owned_entries(library_name, &lib_path)?;
        let mut meta = LibraryMeta::load(&lib_path)?;
        let mut out = Vec::new();
        let mut meta_dirty = false;

        for raw_name in names {
            skill::validate_skill_name(raw_name)?;
            if let Some(entry) = owned.iter().find(|e| e.name == *raw_name) {
                let merged =
                    catalog::merge_tags_into_frontmatter(&entry.skill_path.join("SKILL.md"), &assign)?;
                out.push((raw_name.clone(), EntryKind::Owned, merged));
                continue;
            }
            if let Some(up) = meta.upstream.iter_mut().find(|e| e.name == *raw_name) {
                let merged = catalog::merge_tags(&up.tags, &assign);
                up.tags = merged.clone();
                meta_dirty = true;
                out.push((raw_name.clone(), EntryKind::Upstream, merged));
                continue;
            }
            return Err(SkillzError::Message(format!(
                "skill `{raw_name}` not found in library `{library_name}` catalog"
            )));
        }
        if meta_dirty {
            meta.save(&lib_path)?;
        }
        Ok(out)
    }

    /// Ensure the config directory exists.
    pub fn ensure_config_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.paths.config_dir).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create config dir {}: {e}",
                self.paths.config_dir.display()
            ))
        })?;
        Ok(())
    }

    /// Create or adopt a Library skeleton and register it.
    ///
    /// When skill directories sit at the Library root (`*/SKILL.md`), pass
    /// `move_root_skills` to relocate them under `skills/`.
    pub fn library_init(
        &self,
        path: &Path,
        name: &str,
        move_root_skills: bool,
    ) -> Result<LibraryListEntry> {
        validate_library_name(name)?;
        let abs = resolve_path_against_cwd(&self.paths.cwd, path)?;
        init_library_skeleton(&abs, move_root_skills)?;
        self.register_library(name, &abs, false)
    }

    /// Register an existing Library checkout (optionally read-only).
    ///
    /// Requires an existing `library.toml`; does not invent Library metadata.
    pub fn library_register(
        &self,
        path: &Path,
        name: &str,
        read_only: bool,
    ) -> Result<LibraryListEntry> {
        validate_library_name(name)?;
        let abs = resolve_path_against_cwd(&self.paths.cwd, path)?;
        if !abs.is_dir() {
            return Err(SkillzError::Message(format!(
                "library path does not exist: {}",
                abs.display()
            )));
        }
        ensure_library_meta(&abs)?;
        self.register_library(name, &abs, read_only)
    }

    /// Registered Library whose root contains cwd, if any.
    pub fn library_covering_cwd(&self) -> Result<Option<(String, LibraryRecord)>> {
        let registry = self.load_registry()?;
        Ok(registry.library_covering_path(&self.paths.cwd))
    }

    /// Resolve `--library`, defaulting to the registered Library that contains cwd.
    pub fn resolve_library_name(&self, explicit: Option<&str>) -> Result<String> {
        if let Some(name) = explicit {
            validate_library_name(name)?;
            let registry = self.load_registry()?;
            if !registry.libraries.contains_key(name) {
                return Err(SkillzError::Message(format!(
                    "library `{name}` is not in the local registry"
                )));
            }
            return Ok(name.to_string());
        }
        if let Some((name, _)) = self.library_covering_cwd()? {
            return Ok(name);
        }
        Err(SkillzError::Message(
            "pass --library or run from inside a registered Library checkout".into(),
        ))
    }

    /// List registered Libraries with inferred VCS.
    pub fn library_list(&self) -> Result<Vec<LibraryListEntry>> {
        let registry = self.load_registry()?;
        let mut entries: Vec<_> = registry
            .libraries
            .iter()
            .map(|(name, rec)| LibraryListEntry {
                name: name.clone(),
                path: rec.path.clone(),
                read_only: rec.read_only,
                vcs: infer_vcs(&rec.path),
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    /// Resolve a registered Library by name; must be writable.
    pub fn writable_registry_library(&self, library_name: &str) -> Result<PathBuf> {
        let registry = self.load_registry()?;
        let record = registry.libraries.get(library_name).ok_or_else(|| {
            SkillzError::Message(format!(
                "library `{library_name}` is not in the local registry"
            ))
        })?;
        if record.read_only {
            return Err(SkillzError::Message(format!(
                "library `{library_name}` is marked read-only in the registry"
            )));
        }
        Ok(record.path.clone())
    }

    /// Discover skills in a GitHub repo for Library import (no catalog writes).
    pub fn library_import_discover(
        &self,
        source: &str,
        library_name: &str,
        path_filter: Option<&str>,
    ) -> Result<Vec<LibraryImportCandidate>> {
        let _ = self.writable_registry_library(library_name)?;
        let parsed = source::GithubSource::parse_input(source)?;
        let cache = self.paths.config_dir.join("cache");
        let repo = fetch::ensure_github_checkout(&cache, &parsed)?;
        let rev = fetch::git_head_rev(&repo)?.unwrap_or_else(|| "HEAD".into());

        let search_rel = parsed.join_path_prefix(path_filter);
        let search_root = match &search_rel {
            Some(rel) => {
                let p = repo.join(rel);
                if !p.is_dir() {
                    return Err(SkillzError::Message(format!(
                        "path `{rel}` not found in github:{}/{}",
                        parsed.owner, parsed.repo
                    )));
                }
                p
            }
            None => repo.clone(),
        };

        let discovered = fetch::discover_skills(&repo, &search_root)?;
        let mut out = Vec::new();
        for skill in discovered {
            let pinned = source::GithubSource {
                owner: parsed.owner.clone(),
                repo: parsed.repo.clone(),
                path: if skill.rel_path.is_empty() {
                    None
                } else {
                    Some(skill.rel_path.clone())
                },
                rev: Some(rev.clone()),
            };
            out.push(LibraryImportCandidate {
                name: skill.name,
                rel_path: skill.rel_path,
                source: pinned.render(),
            });
        }
        Ok(out)
    }

    /// Discover skills in a GitHub repo and upsert Upstream Catalog Entries.
    ///
    /// Does not materialize Project/`--user` Installations. Works against a
    /// registered writable Library by name (no Managed Project required).
    ///
    /// Default [`LibraryImportOnConflict::Fail`] aborts the entire import when
    /// any Owned name, different Upstream identity, or duplicate discovery name
    /// conflicts — writing nothing. Same-identity refreshes still succeed.
    pub fn library_import(
        &self,
        source: &str,
        library_name: &str,
        path_filter: Option<&str>,
        dry_run: bool,
        tags: &[String],
        name_override: Option<&str>,
        on_conflict: LibraryImportOnConflict,
    ) -> Result<LibraryImportReport> {
        self.library_import_filtered(
            source,
            library_name,
            path_filter,
            dry_run,
            None,
            tags,
            name_override,
            on_conflict,
            None,
        )
    }

    pub fn library_import_filtered(
        &self,
        source: &str,
        library_name: &str,
        path_filter: Option<&str>,
        dry_run: bool,
        only_sources: Option<&[String]>,
        tags: &[String],
        name_override: Option<&str>,
        on_conflict: LibraryImportOnConflict,
        mut resolve_conflict: Option<
            &mut dyn FnMut(&LibraryImportItem) -> Result<LibraryImportConflictResolution>,
        >,
    ) -> Result<LibraryImportReport> {
        let lib_path = self.writable_registry_library(library_name)?;
        let mut candidates = self.library_import_discover(source, library_name, path_filter)?;
        if let Some(only) = only_sources {
            let want: std::collections::BTreeSet<&str> = only.iter().map(|s| s.as_str()).collect();
            candidates.retain(|c| want.contains(c.source.as_str()));
        }
        apply_library_import_name_override(&mut candidates, name_override)?;
        let assign_tags = catalog::normalize_tag_filters(tags);

        let meta = LibraryMeta::load(&lib_path)?;
        let owned_names: std::collections::BTreeSet<String> =
            catalog::list_owned_entries(library_name, &lib_path)?
                .into_iter()
                .map(|e| e.name)
                .collect();

        // Phase 1: plan against the catalog snapshot (no writes yet).
        let mut planned: Vec<LibraryImportItem> = Vec::new();
        let mut seen_names: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();

        for skill in candidates {
            let pinned = source::GithubSource::parse(&skill.source)?;
            let source_str = skill.source.clone();
            let existing = meta.upstream.iter().find(|e| e.name == skill.name).cloned();
            let action = if owned_names.contains(&skill.name) {
                LibraryImportAction::SkipOwned
            } else if seen_names.contains(&skill.name) {
                LibraryImportAction::SkipCollision {
                    existing_source: format!("(earlier discovery of `{}`)", skill.name),
                }
            } else if let Some(ref existing) = existing {
                let existing_parsed = source::GithubSource::parse(&existing.source).ok();
                match existing_parsed {
                    Some(ex) if ex.identity_key() == pinned.identity_key() => {
                        let merged = catalog::merge_tags(&existing.tags, &assign_tags);
                        if existing.source == source_str && merged == existing.tags {
                            LibraryImportAction::SkipIdentical
                        } else {
                            LibraryImportAction::Update
                        }
                    }
                    _ => LibraryImportAction::SkipCollision {
                        existing_source: existing.source.clone(),
                    },
                }
            } else {
                LibraryImportAction::Add
            };

            if matches!(
                action,
                LibraryImportAction::Add
                    | LibraryImportAction::Update
                    | LibraryImportAction::SkipIdentical
            ) {
                seen_names.insert(skill.name.clone());
            }

            planned.push(LibraryImportItem {
                name: skill.name,
                source: source_str,
                action,
            });
        }

        let conflicts: Vec<&LibraryImportItem> = planned
            .iter()
            .filter(|i| i.action.is_conflict())
            .collect();

        // Phase 2: resolve conflicts per policy (still no writes).
        let mut apply_rows: Vec<(String, String, LibraryImportAction)> = Vec::new();
        match on_conflict {
            LibraryImportOnConflict::Fail if !conflicts.is_empty() => {
                let mut lines = vec![format!(
                    "library import aborted: {} conflict(s); no Catalog changes written",
                    conflicts.len()
                )];
                for c in &conflicts {
                    let detail = match &c.action {
                        LibraryImportAction::SkipOwned => {
                            "Owned Entry already exists".to_string()
                        }
                        LibraryImportAction::SkipCollision { existing_source } => {
                            format!("name collision with `{existing_source}`")
                        }
                        other => format!("{other:?}"),
                    };
                    lines.push(format!("  - {} ({}) — {detail}", c.name, c.source));
                }
                lines.push(
                    "re-run with `--on-conflict skip` or `--on-conflict prompt` to proceed"
                        .into(),
                );
                return Err(SkillzError::Message(lines.join("\n")));
            }
            LibraryImportOnConflict::Fail | LibraryImportOnConflict::Skip => {
                for item in &planned {
                    apply_rows.push((item.name.clone(), item.source.clone(), item.action.clone()));
                }
            }
            LibraryImportOnConflict::Prompt => {
                let resolver = resolve_conflict.as_mut().ok_or_else(|| {
                    SkillzError::Message(
                        "`--on-conflict prompt` requires an interactive resolver".into(),
                    )
                })?;
                for item in &planned {
                    if !item.action.is_conflict() {
                        apply_rows.push((
                            item.name.clone(),
                            item.source.clone(),
                            item.action.clone(),
                        ));
                        continue;
                    }
                    match resolver(item)? {
                        LibraryImportConflictResolution::Skip => {
                            apply_rows.push((
                                item.name.clone(),
                                item.source.clone(),
                                item.action.clone(),
                            ));
                        }
                        LibraryImportConflictResolution::Abort => {
                            return Err(SkillzError::Message(
                                "library import aborted during conflict prompt; no Catalog changes written"
                                    .into(),
                            ));
                        }
                        LibraryImportConflictResolution::Rename(new_name) => {
                            skill::validate_skill_name(&new_name)?;
                            if owned_names.contains(&new_name)
                                || meta.upstream.iter().any(|e| e.name == new_name)
                                || apply_rows.iter().any(|(n, _, a)| {
                                    n == &new_name
                                        && matches!(
                                            a,
                                            LibraryImportAction::Add
                                                | LibraryImportAction::Update
                                                | LibraryImportAction::SkipIdentical
                                        )
                                })
                            {
                                return Err(SkillzError::Message(format!(
                                    "cannot rename `{}` to `{new_name}`: name already in use",
                                    item.name
                                )));
                            }
                            apply_rows.push((
                                new_name,
                                item.source.clone(),
                                LibraryImportAction::Add,
                            ));
                        }
                    }
                }
            }
        }

        // Phase 3: apply atomically (or report-only for dry-run).
        let mut out_items = Vec::new();
        let mut meta = meta;
        for (name, source_str, action) in apply_rows {
            let pinned = source::GithubSource::parse(&source_str)?;
            let existing = meta.upstream.iter().find(|e| e.name == name).cloned();
            match &action {
                LibraryImportAction::Add | LibraryImportAction::Update => {
                    if !dry_run {
                        let tags = match &existing {
                            Some(ex)
                                if source::GithubSource::parse(&ex.source)
                                    .ok()
                                    .map(|p| p.identity_key() == pinned.identity_key())
                                    .unwrap_or(false) =>
                            {
                                catalog::merge_tags(&ex.tags, &assign_tags)
                            }
                            _ => assign_tags.clone(),
                        };
                        meta.upsert_upstream(crate::library::UpstreamEntry {
                            name: name.clone(),
                            source: source_str.clone(),
                            tags,
                        });
                    }
                }
                LibraryImportAction::SkipIdentical
                | LibraryImportAction::SkipCollision { .. }
                | LibraryImportAction::SkipOwned => {}
            }
            out_items.push(LibraryImportItem {
                name,
                source: source_str,
                action,
            });
        }

        if !dry_run {
            meta.save(&lib_path)?;
        }

        Ok(LibraryImportReport {
            library: library_name.to_string(),
            dry_run,
            items: out_items,
        })
    }

    fn register_library(
        &self,
        name: &str,
        abs_path: &Path,
        read_only: bool,
    ) -> Result<LibraryListEntry> {
        let mut registry = self.load_registry()?;
        registry.insert(
            name.to_string(),
            LibraryRecord {
                path: abs_path.to_path_buf(),
                read_only,
            },
        )?;
        self.save_registry(&registry)?;
        Ok(LibraryListEntry {
            name: name.to_string(),
            path: abs_path.to_path_buf(),
            read_only,
            vcs: infer_vcs(abs_path),
        })
    }
}

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn validate_library_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(SkillzError::Message(
            "library name must not be empty".into(),
        ));
    }
    if name
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    {
        return Err(SkillzError::Message(format!(
            "invalid library name `{name}` (use letters, digits, `-`, `_`)"
        )));
    }
    Ok(())
}

fn resolve_path_against_cwd(cwd: &Path, path: &Path) -> Result<PathBuf> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    if candidate.exists() {
        std::fs::canonicalize(&candidate).map_err(|e| {
            SkillzError::Message(format!(
                "failed to resolve path {}: {e}",
                candidate.display()
            ))
        })
    } else {
        // Parent may exist; normalize without requiring the leaf yet.
        if let Some(parent) = candidate.parent() {
            if parent.as_os_str().is_empty() {
                return Ok(candidate);
            }
            if parent.exists() {
                let parent = std::fs::canonicalize(parent).map_err(|e| {
                    SkillzError::Message(format!(
                        "failed to resolve path {}: {e}",
                        candidate.display()
                    ))
                })?;
                let leaf = candidate
                    .file_name()
                    .ok_or_else(|| SkillzError::Message("invalid library path".into()))?;
                return Ok(parent.join(leaf));
            }
        }
        Ok(candidate)
    }
}

/// Resolve the default skillz config directory (`$XDG_CONFIG_HOME/skillz` or
/// `~/.config/skillz`).
pub fn default_config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Path::new(&xdg).join("skillz");
    }
    dirs_home()
        .map(|h| h.join(".config").join("skillz"))
        .unwrap_or_else(|| PathBuf::from(".config/skillz"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn review_publish_branch_name(names: &[String]) -> String {
    if names.len() == 1 {
        return format!("skillz/{}", names[0]);
    }
    let joined = names.join("-");
    if joined.len() <= 60 {
        format!("skillz/{joined}")
    } else {
        format!("skillz/{}-plus-{}", names[0], names.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Temp-dir fixture harness: isolated config + project roots.
    pub struct Fixture {
        _root: tempfile::TempDir,
        pub config_dir: PathBuf,
        pub project_dir: PathBuf,
        pub libs_dir: PathBuf,
        pub home_dir: PathBuf,
    }

    impl Fixture {
        pub fn new() -> Self {
            let root = tempfile::tempdir().expect("tempdir");
            let config_dir = root.path().join("config");
            let project_dir = root.path().join("project");
            let libs_dir = root.path().join("libs");
            let home_dir = root.path().join("home");
            fs::create_dir_all(&config_dir).unwrap();
            fs::create_dir_all(&project_dir).unwrap();
            fs::create_dir_all(&libs_dir).unwrap();
            fs::create_dir_all(&home_dir).unwrap();
            Self {
                _root: root,
                config_dir,
                project_dir,
                libs_dir,
                home_dir,
            }
        }

        pub fn app(&self) -> App {
            App::new(SkillzPaths::with_home(
                &self.config_dir,
                &self.project_dir,
                &self.home_dir,
            ))
        }

        pub fn app_at(&self, project_dir: &Path) -> App {
            App::new(SkillzPaths::with_home(
                &self.config_dir,
                project_dir,
                &self.home_dir,
            ))
        }
    }

    #[test]
    fn status_reports_unmanaged_project_in_fixture() {
        let fx = Fixture::new();
        let report = fx.app().status().expect("status");
        assert!(!report.managed);
        assert_eq!(report.cwd, fx.project_dir);
        assert_eq!(report.config_dir, fx.config_dir);
        assert!(report.bindings.is_empty());
        assert!(report.message.contains("Not a Managed Project"));
    }

    #[test]
    fn project_init_writes_config_lock_and_registry() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fx.app().library_init(&lib, "personal", false).unwrap();

        let config = fx
            .app()
            .project_init(&["personal".into()], None)
            .expect("init");
        assert_eq!(config.skills_dir, DEFAULT_SKILLS_DIR);
        assert_eq!(config.bindings.len(), 1);
        assert_eq!(config.bindings[0].name, "personal");
        assert!(!config.bindings[0].read_only);
        assert!(config.bindings[0].url.starts_with("file://"));

        assert!(fx.project_dir.join(".skillz/skillz.toml").is_file());
        assert!(fx.project_dir.join(".skillz/skillz.lock").is_file());

        let reg = fx.app().load_registry().unwrap();
        assert_eq!(reg.projects.len(), 1);

        let report = fx.app().status().unwrap();
        assert!(report.managed);
        assert_eq!(report.bindings.len(), 1);
        assert!(report.installations.is_empty());
        assert_eq!(
            report.skills_dir.as_deref(),
            Some(DEFAULT_SKILLS_DIR)
        );
    }

    #[test]
    fn project_init_honors_skills_dir_and_ro_bind() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("company"), "company", false)
            .unwrap();
        let config = fx
            .app()
            .project_init(&["company:ro".into()], Some(".claude/skills"))
            .unwrap();
        assert_eq!(config.skills_dir, ".claude/skills");
        assert!(config.bindings[0].read_only);
    }

    #[test]
    fn project_init_fails_for_unknown_library() {
        let fx = Fixture::new();
        let err = fx
            .app()
            .project_init(&["missing".into()], None)
            .unwrap_err();
        assert!(err.to_string().contains("not in the local registry"));
    }

    #[test]
    fn local_authored_and_source_clear_unlocked_drift() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        fx.app().skill_new("local-only", &[]).unwrap();
        assert_eq!(
            fx.app().detect_drift().unwrap()[0].reason,
            DriftReason::Unlocked
        );

        let authored = fx.app().set_local_authored("local-only").unwrap();
        assert_eq!(authored.entry_kind, EntryKind::Local);
        assert_eq!(authored.library, LOCAL_LIBRARY);
        assert!(authored.content_hash.is_empty());
        assert!(authored.source.is_none());
        assert!(fx.app().detect_drift().unwrap().is_empty());

        let lock_text = fs::read_to_string(fx.project_dir.join(".skillz/skillz.lock")).unwrap();
        assert!(lock_text.contains("entry_kind = \"local\""));
        assert!(!lock_text.contains("content_hash"));

        // Edits to Local authored files are not hash Drift.
        fs::write(
            fx.project_dir
                .join(".agents/skills/local-only/SKILL.md"),
            "---\nname: local-only\n---\n# edited\n",
        )
        .unwrap();
        assert!(fx.app().detect_drift().unwrap().is_empty());

        fx.app().skill_new("local-src", &[]).unwrap();
        let sourced = fx
            .app()
            .set_local_source("local-src", "github:owner/repo/local-src@abc1234")
            .unwrap();
        assert_eq!(sourced.entry_kind, EntryKind::Local);
        assert_eq!(
            sourced.source.as_deref(),
            Some("github:owner/repo/local-src@abc1234")
        );
        assert_eq!(sourced.rev.as_deref(), Some("abc1234"));
        assert!(fx
            .app()
            .detect_drift()
            .unwrap()
            .iter()
            .all(|d| d.name != "local-src"));

        let err = fx
            .app()
            .set_local_source("local-src", "github:owner/repo/local-src")
            .unwrap_err();
        assert!(err.to_string().contains("@rev"));
    }

    #[test]
    fn local_membership_works_in_user_scope() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        let user = fx.app().with_scope(Scope::User);
        user.user_init(&["personal".into()]).unwrap();
        user.skill_new("home-skill", &[]).unwrap();
        assert_eq!(
            user.detect_drift().unwrap()[0].reason,
            DriftReason::Unlocked
        );
        user.set_local_authored("home-skill").unwrap();
        assert!(user.detect_drift().unwrap().is_empty());
        let lock_text = fs::read_to_string(fx.config_dir.join("user.lock")).unwrap();
        assert!(lock_text.contains("home-skill"));
        assert!(lock_text.contains("entry_kind = \"local\""));
    }

    #[test]
    fn migrate_mvp_authored_attach_noop_and_skillfile_skip() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        // Seed an Owned Catalog Entry for attach-identical.
        fx.app().skill_new("owned-skill", &[]).unwrap();
        fx.app()
            .libcommit_authored("owned-skill", "personal", &[])
            .unwrap();
        // Re-create as unmanaged Installation with identical content.
        let mut lock = ProjectLock::load(&fx.project_dir).unwrap();
        lock.installations.remove("owned-skill");
        lock.save(&fx.project_dir).unwrap();

        fx.app().skill_new("local-a1", &[]).unwrap();
        fs::write(
            fx.project_dir.join("Skillfile"),
            "github skill owner/repo skills/missing-only\n",
        )
        .unwrap();

        let plan = fx
            .app()
            .migrate_plan(&MigrateOptions {
                ignore_skillfile: false,
                authored: true,
            })
            .unwrap();
        assert!(plan
            .warnings
            .iter()
            .any(|w| w.contains("missing-only")));
        let a1 = plan.rows.iter().find(|r| r.name == "local-a1").unwrap();
        assert_eq!(a1.matrix, "A1");
        assert!(matches!(
            a1.recommended,
            MigrateAction::SetLocalAuthored
        ));
        let a3 = plan.rows.iter().find(|r| r.name == "owned-skill").unwrap();
        assert!(matches!(
            a3.recommended,
            MigrateAction::AttachIdentical { .. }
        ));

        let dry = fx
            .app()
            .migrate_apply(&plan, true, false, None)
            .unwrap();
        assert!(dry.iter().any(|(n, m)| n == "local-a1" && m.contains("Local")));
        assert!(fx.app().detect_drift().unwrap().iter().any(|d| d.name == "local-a1"));

        fx.app()
            .migrate_apply(&plan, false, false, None)
            .unwrap();
        assert!(fx
            .app()
            .detect_drift()
            .unwrap()
            .iter()
            .all(|d| d.name != "local-a1" && d.name != "owned-skill"));

        // B1: already attached owned-skill is no-op.
        let again = fx
            .app()
            .migrate_plan(&MigrateOptions {
                ignore_skillfile: true,
                authored: false,
            })
            .unwrap();
        let b1 = again.rows.iter().find(|r| r.name == "owned-skill").unwrap();
        assert_eq!(b1.matrix, "B1");
        assert!(matches!(b1.recommended, MigrateAction::NoOp));
    }

    #[test]
    fn interactive_migrate_uses_prompter_without_ratatui() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("pick-me", &[]).unwrap();
        let mut ui = SelectAllPrompter;
        let out = fx
            .app()
            .interactive_migrate(
                &MigrateOptions {
                    authored: false,
                    ignore_skillfile: true,
                },
                false,
                &mut ui,
            )
            .unwrap();
        assert!(!out.is_empty());
        assert!(fx.app().detect_drift().unwrap().is_empty());
    }

    #[test]
    fn migrate_no_prompt_aborts_indeterminate_without_writes() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("ambiguous", &[]).unwrap();
        let before = fs::read_to_string(fx.project_dir.join(".skillz/skillz.lock")).unwrap();
        let plan = fx
            .app()
            .migrate_plan(&MigrateOptions::default())
            .unwrap();
        assert!(plan
            .rows
            .iter()
            .any(|r| matches!(r.recommended, MigrateAction::NeedsPrompt)));
        let err = fx
            .app()
            .migrate_apply(&plan, false, true, None)
            .unwrap_err();
        assert!(err.to_string().contains("--no-prompt"));
        assert_eq!(
            before,
            fs::read_to_string(fx.project_dir.join(".skillz/skillz.lock")).unwrap()
        );
    }

    #[test]
    fn migrate_prompt_all_remaining_and_cancel_partial() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("one", &[]).unwrap();
        fx.app().skill_new("two", &[]).unwrap();
        fx.app().skill_new("three", &[]).unwrap();
        let plan = fx
            .app()
            .migrate_plan(&MigrateOptions {
                authored: false,
                ignore_skillfile: true,
            })
            .unwrap();
        let mut answers = vec![
            MigratePromptAnswer::AllRemaining(MigrateAction::SetLocalAuthored),
        ];
        let mut resolver = |row: &MigrateSkillPlan| {
            assert_eq!(row.situation, MigrateSituationClass::A1NoSignals);
            Ok(answers.remove(0))
        };
        let out = fx
            .app()
            .migrate_apply(&plan, false, false, Some(&mut resolver))
            .unwrap();
        assert_eq!(out.len(), 3);
        assert!(fx.app().detect_drift().unwrap().is_empty());

        // Cancel after first apply → partial.
        fx.app().skill_new("four", &[]).unwrap();
        fx.app().skill_new("five", &[]).unwrap();
        let plan2 = fx
            .app()
            .migrate_plan(&MigrateOptions {
                authored: false,
                ignore_skillfile: true,
            })
            .unwrap();
        let pending: Vec<_> = plan2
            .rows
            .iter()
            .filter(|r| matches!(r.recommended, MigrateAction::NeedsPrompt))
            .map(|r| r.name.clone())
            .collect();
        assert!(pending.len() >= 2);
        let mut answers2 = vec![
            MigratePromptAnswer::This(MigrateAction::SetLocalAuthored),
            MigratePromptAnswer::Cancel,
        ];
        let mut resolver2 = |_row: &MigrateSkillPlan| Ok(answers2.remove(0));
        let out2 = fx
            .app()
            .migrate_apply(&plan2, false, false, Some(&mut resolver2))
            .unwrap();
        assert!(out2.iter().any(|(_, m)| m.contains("Local authored")));
        assert!(out2.iter().any(|(_, m)| m.contains("cancelled")));
        assert!(fx
            .app()
            .detect_drift()
            .unwrap()
            .iter()
            .any(|d| d.name == "five" || d.name == "four"));
    }

    #[test]
    fn migrate_set_source_c1_c2_c4() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("src-skill", &[]).unwrap();
        let lock = fx
            .app()
            .migrate_set_source("src-skill", "github:owner/repo/src-skill@abc")
            .unwrap();
        assert_eq!(lock.entry_kind, EntryKind::Local);
        assert!(lock.source.as_ref().unwrap().contains("@abc"));

        fx.app().set_local_authored("src-skill").unwrap();
        let lock = fx
            .app()
            .migrate_set_source("src-skill", "github:owner/repo/src-skill@def")
            .unwrap();
        assert_eq!(lock.rev.as_deref(), Some("def"));

        fx.app()
            .libcommit_authored("src-skill", "personal", &[])
            .unwrap();
        let err = fx
            .app()
            .migrate_set_source("src-skill", "github:owner/repo/src-skill@ghi")
            .unwrap_err();
        assert!(err.to_string().contains("Library-backed"));
    }

    #[test]
    fn migrate_refuses_dirty_tracked_skills_dir() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        vcs_ops::init_git_repo(&fx.project_dir).unwrap();
        fx.app().skill_new("tracked", &[]).unwrap();
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&fx.project_dir)
            .args(["add", ".agents/skills/tracked"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&fx.project_dir)
            .args(["commit", "-m", "track skill"])
            .status()
            .unwrap();
        fs::write(
            fx.project_dir
                .join(".agents/skills/tracked/SKILL.md"),
            "---\nname: tracked\n---\n# dirty\n",
        )
        .unwrap();
        // Dirty outside skills_dir is OK to leave; gate only checks skills_dir.
        fs::write(fx.project_dir.join("README.md"), "hi\n").unwrap();
        let err = fx
            .app()
            .migrate_plan(&MigrateOptions::default())
            .unwrap_err();
        assert!(err.to_string().contains("dirty"));
        assert!(err.to_string().contains("tracked"));
    }

    #[test]
    fn new_then_libcommit_authored_clears_drift() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        let path = fx
            .app()
            .skill_new("demo-skill", &["process".into()])
            .unwrap();
        assert!(path.join("SKILL.md").is_file());

        let drift = fx.app().detect_drift().unwrap();
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].name, "demo-skill");
        assert_eq!(drift[0].reason, DriftReason::Unlocked);
        // Not yet in the Library Catalog.
        assert!(drift[0].suggestion.is_none());

        let lock = fx
            .app()
            .libcommit_authored("demo-skill", "personal", &[])
            .unwrap();
        assert_eq!(lock.library, "personal");
        assert_eq!(lock.entry_kind, EntryKind::Owned);
        assert!(fx
            .libs_dir
            .join("personal/skills/demo-skill/SKILL.md")
            .is_file());
        assert!(fx.app().detect_drift().unwrap().is_empty());

        let project_hash = skill::content_hash(&path).unwrap();
        assert_eq!(project_hash, lock.content_hash);
    }

    #[test]
    fn libcommit_and_tag_merge_owned_and_upstream_tags() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        fx.app().skill_new("demo-skill", &[]).unwrap();
        fx.app()
            .libcommit_authored(
                "demo-skill",
                "personal",
                &["process".into(), "tools".into()],
            )
            .unwrap();
        let owned_md = fx.libs_dir.join("personal/skills/demo-skill/SKILL.md");
        assert_eq!(
            catalog::read_tags(&owned_md).unwrap(),
            vec!["process".to_string(), "tools".to_string()]
        );

        let tagged = fx
            .app()
            .tag_library_skills(
                "personal",
                &["demo-skill".into()],
                &["elixir".into()],
            )
            .unwrap();
        assert_eq!(tagged[0].1, EntryKind::Owned);
        assert_eq!(
            tagged[0].2,
            vec![
                "process".to_string(),
                "tools".to_string(),
                "elixir".to_string()
            ]
        );

        // Upstream entry tags live in library.toml.
        let skill_dir = fx.project_dir.join(".agents/skills/up-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: up-skill\n---\n# u\n",
        )
        .unwrap();
        fs::write(
            skill_dir.join(".skillz-source"),
            "github:acme/skills/up-skill@abc1234",
        )
        .unwrap();
        fx.app()
            .libcommit_upstream(
                "up-skill",
                "personal",
                None,
                &["phoenix".into()],
            )
            .unwrap();
        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        let up = meta.upstream.iter().find(|u| u.name == "up-skill").unwrap();
        assert_eq!(up.tags, vec!["phoenix".to_string()]);

        let tagged_up = fx
            .app()
            .tag_library_skills("personal", &["up-skill".into()], &["otp".into()])
            .unwrap();
        assert_eq!(tagged_up[0].1, EntryKind::Upstream);
        assert_eq!(
            tagged_up[0].2,
            vec!["phoenix".to_string(), "otp".to_string()]
        );
    }

    #[test]
    fn unlocked_suggests_catalog_match_and_libcommit_without_lock() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app()
            .skill_new("demo-skill", &["process".into()])
            .unwrap();
        fx.app()
            .libcommit_authored("demo-skill", "personal", &[])
            .unwrap();

        // Simulate unlocked install that still matches the Library Owned tree.
        let mut lock = ProjectLock::load(&fx.project_dir).unwrap();
        lock.installations.clear();
        lock.save(&fx.project_dir).unwrap();

        let drift = fx.app().detect_drift().unwrap();
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].reason, DriftReason::Unlocked);
        let sug = drift[0].suggestion.as_ref().expect("catalog suggestion");
        assert_eq!(sug.library, "personal");
        assert_eq!(sug.exact_match, Some(true));

        // Drift -i attach must work without an existing Lock.
        let msg = fx
            .app()
            .apply_drift_action("demo-skill", DriftAction::Attach)
            .unwrap();
        assert!(msg.contains("attach"), "{msg}");
        assert!(fx.app().detect_drift().unwrap().is_empty());

        // Divergent content → differs.
        fs::write(
            fx.project_dir
                .join(".agents/skills/demo-skill/SKILL.md"),
            "---\nname: demo-skill\ntags: [process]\n---\n# changed\n",
        )
        .unwrap();
        let mut lock = ProjectLock::load(&fx.project_dir).unwrap();
        lock.installations.clear();
        lock.save(&fx.project_dir).unwrap();
        let drift = fx.app().detect_drift().unwrap();
        assert_eq!(
            drift[0].suggestion.as_ref().unwrap().exact_match,
            Some(false)
        );
    }

    #[test]
    fn libcommit_rejects_read_only_library() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("company"), "company", false)
            .unwrap();
        // Mark registry record read-only via add path: re-register after init is blocked,
        // so mutate registry directly.
        let mut reg = fx.app().load_registry().unwrap();
        reg.libraries.get_mut("company").unwrap().read_only = true;
        fx.app().save_registry(&reg).unwrap();
        fx.app()
            .project_init(&["company:ro".into()], None)
            .unwrap();
        fx.app().skill_new("x", &[]).unwrap();
        let err = fx.app().libcommit_authored("x", "company", &[]).unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }

    #[test]
    fn ensure_config_dir_creates_registry_parent() {
        let fx = Fixture::new();
        let nested = fx.config_dir.join("nested");
        let app = App::new(SkillzPaths::with_home(
            &nested,
            &fx.project_dir,
            &fx.home_dir,
        ));
        app.ensure_config_dir().unwrap();
        assert!(nested.is_dir());
        assert_eq!(app.paths().registry_path(), nested.join("registry.toml"));
    }

    #[test]
    fn remove_skill_deletes_installation_and_lock_keeps_library() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("demo", &[]).unwrap();
        fx.app().libcommit_authored("demo", "personal", &[]).unwrap();
        assert!(fx
            .libs_dir
            .join("personal/skills/demo/SKILL.md")
            .is_file());
        assert!(fx
            .project_dir
            .join(".agents/skills/demo/SKILL.md")
            .is_file());

        fx.app().remove_skill("demo").unwrap();
        assert!(!fx.project_dir.join(".agents/skills/demo").exists());
        let lock = ProjectLock::load(&fx.project_dir).unwrap();
        assert!(!lock.installations.contains_key("demo"));
        assert!(fx
            .libs_dir
            .join("personal/skills/demo/SKILL.md")
            .is_file());
    }

    #[test]
    fn reverse_deps_for_remove_skips_also_removing() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        let skills = fx.project_dir.join(".agents/skills");
        fs::create_dir_all(skills.join("grilling")).unwrap();
        fs::write(
            skills.join("grilling/SKILL.md"),
            "---\nname: grilling\n---\n",
        )
        .unwrap();
        fs::create_dir_all(skills.join("grill-with-docs")).unwrap();
        fs::write(
            skills.join("grill-with-docs/SKILL.md"),
            "---\nname: grill-with-docs\ndepends: [grilling]\n---\n",
        )
        .unwrap();

        let deps = fx
            .app()
            .reverse_deps_for_remove("grilling", &[])
            .unwrap();
        assert_eq!(deps, vec!["grill-with-docs".to_string()]);

        let deps = fx
            .app()
            .reverse_deps_for_remove("grilling", &["grill-with-docs".into()])
            .unwrap();
        assert!(deps.is_empty());

        let forward = fx
            .app()
            .forward_deps_remaining("grill-with-docs", &[])
            .unwrap();
        assert_eq!(forward, vec!["grilling".to_string()]);
        let forward = fx
            .app()
            .forward_deps_remaining("grill-with-docs", &["grilling".into()])
            .unwrap();
        assert!(forward.is_empty());

        let missing = fx
            .app()
            .missing_sibling_deps("grill-with-docs", None)
            .unwrap();
        // grilling is present on disk, so not missing.
        assert!(missing.is_empty());
    }

    #[test]
    fn interactive_add_uses_prompter_without_ratatui() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app()
            .skill_new("a", &["t".into()])
            .unwrap();
        fx.app().libcommit_authored("a", "personal", &[]).unwrap();
        fx.app()
            .skill_new("b", &["t".into()])
            .unwrap();
        fx.app().libcommit_authored("b", "personal", &[]).unwrap();
        fx.app()
            .skill_new("c", &["other".into()])
            .unwrap();
        fx.app().libcommit_authored("c", "personal", &[]).unwrap();
        fs::remove_dir_all(fx.project_dir.join(".agents/skills/a")).ok();
        fs::remove_dir_all(fx.project_dir.join(".agents/skills/b")).ok();
        fs::remove_dir_all(fx.project_dir.join(".agents/skills/c")).ok();
        let mut lock = ProjectLock::load(&fx.project_dir).unwrap();
        lock.installations.clear();
        lock.save(&fx.project_dir).unwrap();

        // Without --tag: all Catalog Entries available.
        let mut ui = SelectAllPrompter;
        let added = fx
            .app()
            .interactive_add_by_tag(&[], None, &mut ui)
            .unwrap();
        assert_eq!(added.len(), 3);

        // Optional --tag still pre-filters.
        fs::remove_dir_all(fx.project_dir.join(".agents/skills/a")).ok();
        fs::remove_dir_all(fx.project_dir.join(".agents/skills/b")).ok();
        fs::remove_dir_all(fx.project_dir.join(".agents/skills/c")).ok();
        let mut lock = ProjectLock::load(&fx.project_dir).unwrap();
        lock.installations.clear();
        lock.save(&fx.project_dir).unwrap();
        let mut ui = SelectAllPrompter;
        let added = fx
            .app()
            .interactive_add_by_tag(&["t".into()], None, &mut ui)
            .unwrap();
        assert_eq!(added.len(), 2);
    }

    #[test]
    fn project_import_skillfile_and_adopt_local() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        let skills = fx.project_dir.join(".agents/skills");
        fs::create_dir_all(skills.join("grilling")).unwrap();
        fs::write(
            skills.join("grilling/SKILL.md"),
            "---\nname: grilling\n---\n# g\n",
        )
        .unwrap();
        fs::create_dir_all(skills.join("local-only")).unwrap();
        fs::write(
            skills.join("local-only/SKILL.md"),
            "---\nname: local-only\n---\n# l\n",
        )
        .unwrap();
        fs::write(
            fx.project_dir.join("Skillfile"),
            "github  skill  mattpocock/skills  skills/productivity/grilling\n",
        )
        .unwrap();

        let plan = fx.app().project_import_dry_run().unwrap();
        assert_eq!(plan.skillfile_entries.len(), 1);
        assert!(plan.leftovers.iter().any(|n| n == "local-only"));

        let imported = fx
            .app()
            .project_import_from_skillfile("personal")
            .unwrap();
        assert_eq!(imported, vec!["grilling".to_string()]);

        let adopted = fx
            .app()
            .project_adopt_local(
                "personal",
                &["local-only=authored".into(), "grilling=skip".into()],
            )
            .unwrap();
        assert!(adopted.iter().any(|(n, m)| n == "local-only" && m.contains("authored")));
        // skipped leftovers remain unlocked Drift if not locked — grilling is locked.
        assert!(fx.app().detect_drift().unwrap().is_empty());
    }

    #[test]
    fn user_scope_init_list_add_and_libcommit() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        // Seed Library Catalog Entry via a temporary project.
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("demo", &["cli".into()]).unwrap();
        fx.app().libcommit_authored("demo", "personal", &[]).unwrap();

        let user = fx.app().with_scope(Scope::User);
        user.user_init(&["personal".into()]).unwrap();
        assert!(fx.config_dir.join("user.toml").is_file());
        assert!(fx.config_dir.join("user.lock").is_file());
        assert!(fx.home_dir.join(".agents/skills").is_dir());

        let status = user.status().unwrap();
        assert!(status.managed);
        assert!(status.message.contains("User scope"));

        let (outcome, _) = user.add_skill("demo", None).unwrap();
        assert!(matches!(outcome, AddOutcome::Installed(_)));
        assert!(fx
            .home_dir
            .join(".agents/skills/demo/SKILL.md")
            .is_file());
        assert!(user.detect_drift().unwrap().is_empty());
    }

    #[test]
    fn overlay_fork_when_library_read_only() {
        let fx = Fixture::new();
        let company = fx.libs_dir.join("company");
        let personal = fx.libs_dir.join("personal");
        fx.app().library_init(&company, "company", false).unwrap();
        fx.app().library_init(&personal, "personal", false).unwrap();
        vcs_ops::init_git_repo(&personal).unwrap();
        std::process::Command::new("git")
            .args(["-C", personal.to_str().unwrap(), "add", "."])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", personal.to_str().unwrap(), "commit", "-m", "init lib"])
            .status()
            .unwrap();

        // Mark company read-only in registry.
        let mut reg = fx.app().load_registry().unwrap();
        reg.libraries.get_mut("company").unwrap().read_only = true;
        fx.app().save_registry(&reg).unwrap();

        // Put an Owned Entry in company and bind both libraries.
        fs::create_dir_all(company.join("skills/grilling")).unwrap();
        fs::write(
            company.join("skills/grilling/SKILL.md"),
            "---\nname: grilling\n---\n# g\n",
        )
        .unwrap();
        fx.app()
            .project_init(&["company:ro".into(), "personal".into()], None)
            .unwrap();
        fx.app()
            .add_skill("grilling", Some("company"))
            .unwrap();

        let skill_md = fx
            .project_dir
            .join(".agents/skills/grilling/SKILL.md");
        let mut body = fs::read_to_string(&skill_md).unwrap();
        body.push_str("forked edit\n");
        fs::write(&skill_md, body).unwrap();

        let lock = fx
            .app()
            .libcommit_branch("grilling", Some("company"), Some("overlay"), &[])
            .unwrap();
        assert_eq!(lock.library, "personal");
        assert_eq!(lock.overlay_from_library.as_deref(), Some("company"));
        assert!(lock.overlay_from_entry.is_some());
        assert!(personal.join("skills/grilling/SKILL.md").is_file());
    }

    #[test]
    fn edit_libcommit_branch_promote_lock_main() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fx.app().library_init(&lib, "personal", false).unwrap();
        vcs_ops::init_git_repo(&lib).unwrap();
        // library.toml + skills need to be on main before branching.
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&lib)
            .args(["add", "."])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&lib)
            .args(["commit", "-m", "library skeleton"])
            .status()
            .unwrap();

        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("grilling", &[]).unwrap();
        fx.app()
            .libcommit_authored("grilling", "personal", &[])
            .unwrap();
        // Commit authored entry on main.
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&lib)
            .args(["add", "."])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&lib)
            .args(["commit", "-m", "add grilling"])
            .status()
            .unwrap();

        // Edit installation.
        let skill_md = fx
            .project_dir
            .join(".agents/skills/grilling/SKILL.md");
        let mut body = fs::read_to_string(&skill_md).unwrap();
        body.push_str("\nEdited in project.\n");
        fs::write(&skill_md, body).unwrap();
        assert!(!fx.app().detect_drift().unwrap().is_empty());

        let branch_lock = fx
            .app()
            .libcommit_branch("grilling", None, Some("tweak"), &[])
            .unwrap();
        assert!(branch_lock.branch.as_deref().unwrap().contains("grilling"));
        assert!(branch_lock.rev.is_some());
        let after_branch = fx.app().detect_drift().unwrap();
        assert!(
            after_branch.iter().all(|d| !d.reason.fails_check()),
            "Branch Lock should not be hard Drift: {after_branch:?}"
        );
        assert!(
            after_branch
                .iter()
                .any(|d| matches!(d.reason, DriftReason::PendingReview { .. })),
            "expected pending review: {after_branch:?}"
        );

        let main_rev = fx.app().promote_skill("grilling").unwrap();
        assert!(!main_rev.is_empty());

        let locked = fx.app().lock_main("grilling").unwrap();
        assert_eq!(locked.branch, None);
        assert_eq!(locked.rev.as_deref(), Some(main_rev.as_str()));
        assert!(fx.app().detect_drift().unwrap().is_empty());
    }

    #[test]
    fn drift_check_and_hooks_install() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("drifty", &[]).unwrap();

        let report = fx.app().drift_report(DriftScan::Current).unwrap();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].items[0].name, "drifty");

        fs::create_dir_all(fx.project_dir.join(".git/hooks")).unwrap();
        let hooks = fx.app().hooks_install().unwrap();
        assert!(!hooks.is_empty());
        let body = fs::read_to_string(&hooks[0]).unwrap();
        assert!(body.contains("skillz drift --check"));
        assert!(body.contains("SKILLZ_ALLOW_DRIFT"));
    }

    #[test]
    fn drift_current_stays_local_global_sees_other_projects() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("here-only", &[]).unwrap();

        let other = fx.project_dir.parent().unwrap().join("other");
        fs::create_dir_all(&other).unwrap();
        let other_app = fx.app_at(&other);
        other_app
            .project_init(&["personal".into()], None)
            .unwrap();
        other_app.skill_new("there-only", &[]).unwrap();

        let local = fx.app().drift_report(DriftScan::Current).unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].items.len(), 1);
        assert_eq!(local[0].items[0].name, "here-only");

        let global = fx.app().drift_report(DriftScan::Global).unwrap();
        let names: std::collections::BTreeSet<_> = global
            .iter()
            .flat_map(|p| p.items.iter().map(|i| i.name.as_str()))
            .collect();
        assert!(names.contains("here-only"));
        assert!(names.contains("there-only"));
        assert!(global.len() >= 2);
    }

    #[test]
    fn hooks_install_jj_push_with_checks_alias() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        // Colocated-looking tree: .git present, but jj wins — no pre-commit.
        fs::create_dir_all(fx.project_dir.join(".git/hooks")).unwrap();
        fs::create_dir_all(fx.project_dir.join(".jj/repo")).unwrap();
        let hooks = fx.app().hooks_install().unwrap();
        assert_eq!(hooks.len(), 1);
        assert!(hooks[0].ends_with("config.toml"));
        let body = fs::read_to_string(&hooks[0]).unwrap();
        assert!(body.contains("push-with-checks"));
        assert!(body.contains("skillz drift --check"));
        assert!(!body.contains("skillz push"));
        assert!(!fx.project_dir.join(".git/hooks/pre-commit").exists());
    }

    #[test]
    fn hooks_show_vcs_exclusive_dry_run() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        fs::create_dir_all(fx.project_dir.join(".git/hooks")).unwrap();
        let git_plans = fx.app().hooks_show().unwrap();
        assert_eq!(git_plans.len(), 1);
        assert_eq!(git_plans[0].kind, hooks::HookKind::GitPreCommit);
        assert_eq!(git_plans[0].status, hooks::HookPlanStatus::WouldCreate);
        assert!(git_plans[0].contents.contains("skillz drift --check"));
        assert!(!git_plans[0].path.exists());

        fs::create_dir_all(fx.project_dir.join(".jj/repo")).unwrap();
        let jj_plans = fx.app().hooks_show().unwrap();
        assert_eq!(jj_plans.len(), 1);
        assert_eq!(jj_plans[0].kind, hooks::HookKind::JjPushWithChecks);
        assert_eq!(jj_plans[0].status, hooks::HookPlanStatus::WouldCreate);
        assert!(jj_plans[0].contents.contains("push-with-checks"));
        assert!(!jj_plans[0].path.exists());

        fx.app().hooks_install().unwrap();
        let after = fx.app().hooks_show().unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].status, hooks::HookPlanStatus::AlreadyInstalled);
        assert!(!fx.project_dir.join(".git/hooks/pre-commit").exists());
    }

    #[test]
    fn add_from_source_stages_unlocked_and_local_variants() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        // Nested layout on disk → stage without --local → Unlocked.
        let upstream_root = fx.libs_dir.join("upstream-repo");
        let nested = upstream_root.join("skills").join("ecto-thinking");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("SKILL.md"),
            "---\nname: ecto-thinking\ndescription: x\n---\n# ecto\n",
        )
        .unwrap();

        let (dest, _) = fx
            .app()
            .add_from_source(upstream_root.to_str().unwrap(), Some("ecto-thinking"), false)
            .unwrap();
        assert!(dest.join("SKILL.md").is_file());
        let drift = fx.app().detect_drift().unwrap();
        assert!(
            drift.iter().any(|d| {
                d.name == "ecto-thinking" && d.reason == DriftReason::Unlocked
            }),
            "{drift:?}"
        );
        assert!(fx
            .app()
            .load_scope_lock()
            .unwrap()
            .installations
            .get("ecto-thinking")
            .is_none());

        // Path staging cannot use --local (needs github:/cmd: provenance).
        let err = fx
            .app()
            .add_from_source(upstream_root.to_str().unwrap(), Some("ecto-thinking"), true)
            .unwrap_err();
        assert!(err.to_string().contains("--local"), "{err}");

        // new without --local → Unlocked; with --local → Local authored.
        fx.app().skill_new("plain-new", &[]).unwrap();
        assert_eq!(
            fx.app().detect_drift().unwrap().iter().find(|d| d.name == "plain-new").unwrap().reason,
            DriftReason::Unlocked
        );
        fx.app()
            .skill_new_with_local("local-new", &[], true)
            .unwrap();
        let lock = fx.app().load_scope_lock().unwrap();
        let rec = lock.installations.get("local-new").unwrap();
        assert_eq!(rec.entry_kind, EntryKind::Local);
        assert_eq!(rec.library, LOCAL_LIBRARY);
        assert!(!fx
            .app()
            .detect_drift()
            .unwrap()
            .iter()
            .any(|d| d.name == "local-new"));

        // Still formalize staged skill as Upstream Catalog Entry.
        let up_lock = fx
            .app()
            .libcommit_upstream(
                "ecto-thinking",
                "personal",
                Some("github:acme/skills/skills/ecto-thinking@deadbeef"),
                &[],
            )
            .unwrap();
        assert_eq!(up_lock.entry_kind, EntryKind::Upstream);
        assert_eq!(
            up_lock.source.as_deref(),
            Some("github:acme/skills/skills/ecto-thinking@deadbeef")
        );
        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert_eq!(meta.upstream.len(), 1);
        assert!(!fx
            .libs_dir
            .join("personal/skills/ecto-thinking")
            .exists());
    }

    #[test]
    fn add_from_source_cmd_with_local() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        let bin_dir = fx.libs_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("fake-tool");
        write_fake_tool(&bin);
        let source = format!(
            "cmd:{} skill install --dir {{dir}}",
            bin.display()
        );

        let (dest, pinned) = fx
            .app()
            .add_from_source(&source, Some("fake-skill"), true)
            .unwrap();
        assert!(dest.join("SKILL.md").is_file());
        let pinned = pinned.expect("cmd pin");
        assert!(pinned.contains('@'), "{pinned}");
        let lock = fx.app().load_scope_lock().unwrap();
        let rec = lock.installations.get("fake-skill").unwrap();
        assert_eq!(rec.entry_kind, EntryKind::Local);
        assert_eq!(rec.library, LOCAL_LIBRARY);
        assert_eq!(rec.source.as_deref(), Some(pinned.as_str()));
        assert!(!fx
            .app()
            .detect_drift()
            .unwrap()
            .iter()
            .any(|d| d.name == "fake-skill"));
    }

    fn write_fake_tool(bin: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(
                bin,
                r#"#!/bin/sh
set -e
if [ "$1" = "--version" ]; then
  echo "fake-tool 9.9.9"
  exit 0
fi
if [ "$1" = "version" ]; then
  echo "fake-tool 9.9.9"
  exit 0
fi
if [ "$1" = "skill" ] && [ "$2" = "install" ] && [ "$3" = "--dir" ]; then
  dir="$4"
  mkdir -p "$dir"
  printf '%s\n' '---' 'name: fake-skill' 'description: from fake-tool' '---' '# fake' >"$dir/SKILL.md"
  exit 0
fi
echo "unexpected args: $*" >&2
exit 1
"#,
            )
            .unwrap();
            let mut perms = fs::metadata(bin).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(bin, perms).unwrap();
        }
        #[cfg(not(unix))]
        {
            let _ = bin;
            panic!("cmd: tool tests require unix");
        }
    }

    #[test]
    fn cmd_upstream_libcommit_validates_and_add_runs_template() {
        let fx = Fixture::new();
        let root = fx.libs_dir.parent().unwrap().to_path_buf();
        let tool = root.join("fake-tool");
        write_fake_tool(&tool);
        let tool_s = tool.display().to_string();

        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        // Seed Installation (as if the tool already ran once).
        let skill_dir = fx.project_dir.join(".agents/skills/fake-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: fake-skill\ndescription: seed\n---\n# seed\n",
        )
        .unwrap();

        let source = format!("cmd:{tool_s} skill install --dir {{dir}}");
        let lock = fx
            .app()
            .libcommit_upstream("fake-skill", "personal", Some(&source), &[])
            .unwrap();
        assert_eq!(lock.entry_kind, EntryKind::Upstream);
        assert_eq!(lock.rev.as_deref(), Some("9.9.9"));
        assert!(lock
            .source
            .as_deref()
            .unwrap()
            .ends_with("@9.9.9"));
        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert_eq!(meta.upstream.len(), 1);
        assert!(
            !meta.upstream[0].source.contains('@'),
            "catalog omits @rev: {}",
            meta.upstream[0].source
        );
        assert!(!fx
            .libs_dir
            .join("personal/skills/fake-skill")
            .exists());

        // Second Managed Project: add must re-run the tool template.
        let other = root.join("other-project");
        fs::create_dir_all(&other).unwrap();
        fx.app_at(&other)
            .project_init(&["personal".into()], None)
            .unwrap();
        let (outcome, _, _) = fx
            .app_at(&other)
            .add_skill_with_deps("fake-skill", None, false, false, false)
            .unwrap();
        match outcome {
            AddOutcome::Installed(l) | AddOutcome::Refreshed(l) | AddOutcome::AlreadyPresent(l) => {
                assert_eq!(l.rev.as_deref(), Some("9.9.9"));
            }
        }
        assert!(other
            .join(".agents/skills/fake-skill/SKILL.md")
            .is_file());
    }

    #[test]
    fn cmd_upstream_libcommit_rejects_missing_tool() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("ghost", &[]).unwrap();
        let err = fx
            .app()
            .libcommit_upstream(
                "ghost",
                "personal",
                Some("cmd:/no/such/tool-xyz skill install --dir {dir}"),
                &[],
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("failed") || err.to_string().contains("No such"),
            "{err}"
        );
        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert!(meta.upstream.is_empty());
    }

    #[test]
    fn list_and_add_from_library_with_tags_and_second_project() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app()
            .skill_new("demo-skill", &["elixir".into(), "phoenix".into()])
            .unwrap();
        fx.app()
            .libcommit_authored("demo-skill", "personal", &[])
            .unwrap();

        // Remove project install to simulate a fresh project consuming the Library.
        fs::remove_dir_all(fx.project_dir.join(".agents/skills/demo-skill")).unwrap();
        let mut lock = ProjectLock::load(&fx.project_dir).unwrap();
        lock.installations.clear();
        lock.save(&fx.project_dir).unwrap();

        let listed = fx
            .app()
            .list_skills(None, &["elixir".into()], false, false)
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].entry.name, "demo-skill");
        assert!(!listed[0].availability.project);

        // Default project-only list is empty until installed again.
        let project_listed = fx
            .app()
            .list_skills(None, &[], false, true)
            .unwrap();
        assert!(project_listed.is_empty());

        // Warn path: present only in user skills.
        fs::create_dir_all(fx.home_dir.join(".agents/skills/demo-skill")).unwrap();
        fs::write(
            fx.home_dir.join(".agents/skills/demo-skill/SKILL.md"),
            "---\nname: demo-skill\n---\n",
        )
        .unwrap();

        let (outcome, warn) = fx.app().add_skill("demo-skill", None).unwrap();
        assert!(matches!(outcome, AddOutcome::Installed(_)));
        assert!(warn.unwrap().contains("user"));
        assert!(fx
            .project_dir
            .join(".agents/skills/demo-skill/SKILL.md")
            .is_file());

        // Second Managed Project can add the same Catalog Entry.
        let project2 = fx.libs_dir.join("project2");
        fs::create_dir_all(&project2).unwrap();
        fx.app_at(&project2)
            .project_init(&["personal".into()], None)
            .unwrap();
        let (outcome2, _) = fx
            .app_at(&project2)
            .add_skill("demo-skill", Some("personal"))
            .unwrap();
        assert!(matches!(outcome2, AddOutcome::Installed(_)));
        assert!(project2
            .join(".agents/skills/demo-skill/SKILL.md")
            .is_file());

        // Skill dir already exists → error with attach / from-library hint.
        let err = fx.app().add_skill("demo-skill", None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("already exists"), "{msg}");
        assert!(msg.contains("--attach"), "{msg}");
        assert!(msg.contains("--from-library"), "{msg}");
    }

    #[test]
    fn add_attach_and_from_library() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("demo", &[]).unwrap();
        fx.app()
            .libcommit_authored("demo", "personal", &[])
            .unwrap();

        let project_skill = fx.project_dir.join(".agents/skills/demo");
        let lib_skill = fx.libs_dir.join("personal/skills/demo");
        let lib_before = fs::read_to_string(lib_skill.join("SKILL.md")).unwrap();

        // Local membership then attach: Lock to Catalog, clear Local, leave Library untouched.
        fx.app().set_local_authored("demo").unwrap();
        assert_eq!(
            fx.app()
                .load_scope_lock()
                .unwrap()
                .installations
                .get("demo")
                .unwrap()
                .entry_kind,
            EntryKind::Local
        );
        // Divergent on-disk content vs Catalog.
        fs::write(
            project_skill.join("SKILL.md"),
            "---\nname: demo\ndescription: local edit\n---\n# local\n",
        )
        .unwrap();
        let local_hash = skill::content_hash(&project_skill).unwrap();

        let (attached, _) = fx
            .app()
            .add_skill_with_deps("demo", Some("personal"), false, true, false)
            .map(|(o, w, _)| (o, w))
            .unwrap();
        assert!(matches!(attached, AddOutcome::Installed(_)));
        let lock = fx.app().load_scope_lock().unwrap();
        let rec = lock.installations.get("demo").unwrap();
        assert_eq!(rec.library, "personal");
        assert_eq!(rec.entry_kind, EntryKind::Owned);
        assert_eq!(rec.content_hash, local_hash);
        assert_ne!(rec.library, LOCAL_LIBRARY);
        assert_eq!(
            fs::read_to_string(lib_skill.join("SKILL.md")).unwrap(),
            lib_before,
            "attach must not write Project tree into Library"
        );
        assert!(
            fs::read_to_string(project_skill.join("SKILL.md"))
                .unwrap()
                .contains("local edit"),
            "attach without --from-library keeps on-disk files"
        );

        // --attach --from-library replaces Installation from Catalog.
        let (replaced, _) = fx
            .app()
            .add_skill_with_deps("demo", Some("personal"), false, true, true)
            .map(|(o, w, _)| (o, w))
            .unwrap();
        assert!(matches!(replaced, AddOutcome::Installed(_)));
        let after = fs::read_to_string(project_skill.join("SKILL.md")).unwrap();
        assert_eq!(after, lib_before);
        let lock = fx.app().load_scope_lock().unwrap();
        let rec = lock.installations.get("demo").unwrap();
        assert_eq!(rec.entry_kind, EntryKind::Owned);
        assert_eq!(
            rec.content_hash,
            skill::content_hash(&lib_skill).unwrap()
        );
    }

    #[test]
    fn publish_direct_refuses_unmanaged_infers_modes_and_batches() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fx.app().library_init(&lib, "personal", false).unwrap();
        vcs_ops::init_git_repo(&lib).unwrap();
        // Commit library.toml so main has content before publish.
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "add", "library.toml", "skills"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "commit", "-m", "skeleton"])
            .status()
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        // Unmanaged / Unlocked refused.
        fx.app().skill_new("orphan", &[]).unwrap();
        let err = fx
            .app()
            .publish_skills(
                &["orphan".into()],
                "personal",
                PublishMode::Auto,
                None,
            )
            .unwrap_err();
        assert!(err.to_string().contains("unmanaged"), "{err}");
        assert!(err.to_string().contains("--attach") || err.to_string().contains("migrate"), "{err}");

        // Local authored create.
        fx.app()
            .skill_new_with_local("alpha", &[], true)
            .unwrap();
        fx.app()
            .skill_new_with_local("beta", &[], true)
            .unwrap();
        // Sibling dep notify: beta depends on grilling (not in batch).
        fs::write(
            fx.project_dir.join(".agents/skills/beta/SKILL.md"),
            "---\nname: beta\ndepends: [grilling]\n---\n# beta\n",
        )
        .unwrap();

        let report = fx
            .app()
            .publish_skills(
                &["alpha".into(), "beta".into()],
                "personal",
                PublishMode::Auto,
                Some("batch create"),
            )
            .unwrap();
        assert_eq!(report.skills.len(), 2);
        assert!(report.skills.iter().all(|s| s.created));
        assert!(report.skills.iter().all(|s| s.kind == EntryKind::Owned));
        assert!(report.missing_deps.iter().any(|d| d == "grilling"));
        assert!(lib.join("skills/alpha/SKILL.md").is_file());
        assert!(lib.join("skills/beta/SKILL.md").is_file());
        // One main commit unit — both locks share the same rev.
        assert_eq!(
            report.skills[0].lock.rev.as_deref(),
            report.skills[1].lock.rev.as_deref()
        );
        assert_eq!(report.skills[0].lock.rev.as_deref(), Some(report.rev.as_str()));

        // Local + Catalog hit → attach hint.
        fx.app()
            .skill_new_with_local("alpha-local", &[], true)
            .unwrap();
        // Rename trick: create Local named alpha while Catalog has alpha — use set_local after removing lock then re-scaffold? Simpler: mark alpha Local again after publish cleared Local.
        fx.app().set_local_authored("alpha").unwrap();
        let err = fx
            .app()
            .publish_skills(&["alpha".into()], "personal", PublishMode::Auto, None)
            .unwrap_err();
        assert!(err.to_string().contains("--attach"), "{err}");

        // Re-lock alpha as Library-backed for update tests.
        fx.app()
            .add_skill_with_deps("alpha", Some("personal"), false, true, true)
            .unwrap();
        fs::write(
            fx.project_dir.join(".agents/skills/alpha/SKILL.md"),
            "---\nname: alpha\ndescription: edited\n---\n# edited\n",
        )
        .unwrap();

        // --create-only fails when Catalog exists.
        let err = fx
            .app()
            .publish_skills(
                &["alpha".into()],
                "personal",
                PublishMode::CreateOnly,
                None,
            )
            .unwrap_err();
        assert!(err.to_string().contains("create-only"), "{err}");

        // --update succeeds for Library-backed; fails for Local create.
        let updated = fx
            .app()
            .publish_skills(
                &["alpha".into()],
                "personal",
                PublishMode::UpdateOnly,
                Some("update alpha"),
            )
            .unwrap();
        assert_eq!(updated.skills.len(), 1);
        assert!(!updated.skills[0].created);
        assert!(
            fs::read_to_string(lib.join("skills/alpha/SKILL.md"))
                .unwrap()
                .contains("edited")
        );

        fx.app()
            .skill_new_with_local("gamma", &[], true)
            .unwrap();
        let err = fx
            .app()
            .publish_skills(
                &["gamma".into()],
                "personal",
                PublishMode::UpdateOnly,
                None,
            )
            .unwrap_err();
        assert!(err.to_string().contains("--update"), "{err}");

        // Infer Upstream from Local+source.
        fx.app().skill_new("upskill", &[]).unwrap();
        fx.app()
            .set_local_source(
                "upskill",
                "github:acme/skills/skills/upskill@deadbeef",
            )
            .unwrap();
        let up = fx
            .app()
            .publish_skills(
                &["upskill".into()],
                "personal",
                PublishMode::CreateOnly,
                Some("upstream"),
            )
            .unwrap();
        assert!(up.skills[0].created);
        assert_eq!(up.skills[0].kind, EntryKind::Upstream);
        let meta = LibraryMeta::load(&lib).unwrap();
        assert!(meta.upstream.iter().any(|e| e.name == "upskill"));
    }

    #[test]
    fn update_refreshes_from_main_tip_after_pending_clears() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fx.app().library_init(&lib, "personal", false).unwrap();
        vcs_ops::init_git_repo(&lib).unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "add", "."])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "commit", "-m", "skeleton"])
            .status()
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app()
            .skill_new_with_local("demo", &[], true)
            .unwrap();
        // Direct publish to main.
        let mut meta = LibraryMeta::load(&lib).unwrap();
        meta.contribution = Contribution::Direct;
        meta.save(&lib).unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "add", "library.toml"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "commit", "-m", "contribution"])
            .status()
            .unwrap();
        fx.app()
            .publish_skills(&["demo".into()], "personal", PublishMode::Auto, None)
            .unwrap();

        // Simulate review-style pending then merge onto main via branch + promote.
        fs::write(
            fx.project_dir.join(".agents/skills/demo/SKILL.md"),
            "---\nname: demo\n---\n# edited\n",
        )
        .unwrap();
        let branch_lock = fx
            .app()
            .libcommit_branch("demo", None, Some("tweak"), &[])
            .unwrap();
        assert!(branch_lock.branch.is_some());
        let main_rev = fx.app().promote_skill("demo").unwrap();
        // update (not lock --main) clears pending and refreshes Installation.
        let updated = fx.app().update_skill("demo").unwrap();
        assert_eq!(updated.branch, None);
        assert_eq!(updated.rev.as_deref(), Some(main_rev.as_str()));
        assert!(
            fs::read_to_string(fx.project_dir.join(".agents/skills/demo/SKILL.md"))
                .unwrap()
                .contains("edited")
        );
        let drift = fx.app().detect_drift().unwrap();
        assert!(
            !drift.iter().any(|d| d.name == "demo"),
            "update should clear pending/hard drift for demo: {drift:?}"
        );

        // Distinct refusal for Local.
        fx.app()
            .skill_new_with_local("local-only", &[], true)
            .unwrap();
        let err = fx.app().update_skill("local-only").unwrap_err();
        assert!(err.to_string().contains("Local"), "{err}");
    }

    #[test]
    fn pending_review_visible_but_check_soft_and_squash_message() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("team");
        fx.app().library_init(&lib, "team", false).unwrap();
        let mut meta = LibraryMeta::load(&lib).unwrap();
        meta.contribution = Contribution::Review;
        meta.save(&lib).unwrap();
        vcs_ops::init_git_repo(&lib).unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "add", "."])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "commit", "-m", "skeleton"])
            .status()
            .unwrap();
        fx.app()
            .project_init(&["team".into()], None)
            .unwrap();
        fx.app()
            .skill_new_with_local("pending", &[], true)
            .unwrap();
        let report = fx
            .app()
            .publish_with_options(
                &["pending".into()],
                "team",
                &PublishOptions {
                    no_push: true,
                    no_pr: true,
                    ..PublishOptions::default()
                },
            )
            .unwrap();
        assert!(report.branch.is_some());

        let status = fx.app().status().unwrap();
        assert!(
            status.pending_review.iter().any(|n| n == "pending"),
            "{:?}",
            status.pending_review
        );

        let drift = fx.app().detect_drift().unwrap();
        assert!(
            drift.iter().any(|d| {
                d.name == "pending"
                    && matches!(d.reason, DriftReason::PendingReview { .. })
                    && !d.reason.fails_check()
            }),
            "{drift:?}"
        );
        // Pending-only must not count as hard Drift for --check.
        assert!(
            !drift.iter().any(|d| d.reason.fails_check()),
            "pending-only should be soft: {drift:?}"
        );

        // Squash fallout: delete the review branch so neither local nor remote exists.
        let branch = report.branch.clone().unwrap();
        let rev = report.rev.clone();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "checkout", "main"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "branch", "-D", &branch])
            .status()
            .unwrap();
        // Keep Lock pointing at orphaned rev.
        let mut lock = fx.app().load_scope_lock().unwrap();
        let rec = lock.installations.get_mut("pending").unwrap();
        rec.branch = Some(branch);
        rec.rev = Some(rev);
        fx.app().save_scope_lock(&lock).unwrap();

        let drift = fx.app().detect_drift().unwrap();
        let squash = drift
            .iter()
            .find(|d| d.name == "pending")
            .expect("squash item");
        match &squash.reason {
            DriftReason::SquashFallout { recovery, .. } => {
                assert!(recovery.contains("update") || recovery.contains("publish"), "{recovery}");
                assert!(!squash.reason.fails_check());
            }
            other => panic!("expected SquashFallout, got {other:?}"),
        }

        // Missing Lock SHA errors clearly after remote consult.
        let mut lock = fx.app().load_scope_lock().unwrap();
        lock.installations.get_mut("pending").unwrap().rev =
            Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".into());
        lock.installations.get_mut("pending").unwrap().branch =
            Some("skillz/missing".into());
        fx.app().save_scope_lock(&lock).unwrap();
        let err = fx.app().detect_drift().unwrap_err();
        assert!(
            err.to_string().contains("not present") || err.to_string().contains("deadbeef"),
            "{err}"
        );
    }

    #[test]
    fn publish_review_branch_unit_push_pr_dry_run() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("team");
        fx.app().library_init(&lib, "team", false).unwrap();
        let mut meta = LibraryMeta::load(&lib).unwrap();
        meta.contribution = Contribution::Review;
        meta.save(&lib).unwrap();
        vcs_ops::init_git_repo(&lib).unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "add", "library.toml", "skills"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", lib.to_str().unwrap(), "commit", "-m", "skeleton"])
            .status()
            .unwrap();
        fx.app()
            .project_init(&["team".into()], None)
            .unwrap();

        fx.app()
            .skill_new_with_local("one", &[], true)
            .unwrap();
        fx.app()
            .skill_new_with_local("two", &[], true)
            .unwrap();

        // Dry-run: no Branch Lock files / locks written.
        let dry = fx
            .app()
            .publish_with_options(
                &["one".into(), "two".into()],
                "team",
                &PublishOptions {
                    dry_run: true,
                    ..PublishOptions::default()
                },
            )
            .unwrap();
        assert!(dry.dry_run);
        assert_eq!(dry.branch.as_deref(), Some("skillz/one-two"));
        assert!(dry.next_steps.iter().any(|s| s.contains("Branch Lock")));
        assert!(dry.next_steps.iter().any(|s| s.contains("push")));
        assert!(!lib.join("skills/one/SKILL.md").is_file());
        assert!(fx
            .app()
            .load_scope_lock()
            .unwrap()
            .installations
            .get("one")
            .unwrap()
            .entry_kind
            == EntryKind::Local);

        // Real publish with mocked push + forge soft-fail.
        let report = fx
            .app()
            .publish_with_options(
                &["one".into(), "two".into()],
                "team",
                &PublishOptions {
                    mock_push: Some(Ok(())),
                    mock_pr: Some(Err("gh auth required".into())),
                    ..PublishOptions::default()
                },
            )
            .unwrap();
        assert_eq!(report.contribution, Contribution::Review);
        assert_eq!(report.branch.as_deref(), Some("skillz/one-two"));
        assert!(report.pushed);
        assert!(report.pr_url.is_none());
        assert!(report.warnings.iter().any(|w| w.contains("gh auth")));
        assert!(report.next_steps.iter().any(|s| s.contains("gh ")));
        assert!(report.next_steps.iter().any(|s| s.contains("merge commit")));
        assert_eq!(report.skills.len(), 2);
        assert_eq!(
            report.skills[0].lock.rev.as_deref(),
            report.skills[1].lock.rev.as_deref()
        );
        assert_eq!(
            report.skills[0].lock.branch.as_deref(),
            Some("skillz/one-two")
        );
        assert!(lib.join("skills/one/SKILL.md").is_file());

        // --no-push / --no-pr
        fx.app()
            .skill_new_with_local("three", &[], true)
            .unwrap();
        let no_net = fx
            .app()
            .publish_with_options(
                &["three".into()],
                "team",
                &PublishOptions {
                    no_push: true,
                    no_pr: true,
                    ..PublishOptions::default()
                },
            )
            .unwrap();
        assert!(!no_net.pushed);
        assert!(no_net.warnings.iter().any(|w| w.contains("--no-push")));
        assert_eq!(no_net.branch.as_deref(), Some("skillz/three"));
    }

    #[test]
    fn library_init_creates_skeleton_and_registry() {
        let fx = Fixture::new();
        let lib_path = fx.libs_dir.join("personal");
        let entry = fx
            .app()
            .library_init(&lib_path, "personal", false)
            .expect("init");

        assert_eq!(entry.name, "personal");
        assert!(lib_path.join("library.toml").is_file());
        assert!(lib_path.join("skills").is_dir());
        assert!(!entry.read_only);

        let meta = LibraryMeta::load(&lib_path).unwrap();
        assert_eq!(meta.version, 1);
        assert_eq!(meta.contribution, Contribution::Direct);
        let toml_text = fs::read_to_string(lib_path.join("library.toml")).unwrap();
        assert!(toml_text.contains("version = 1"));
        assert!(toml_text.contains("contribution = \"direct\""));
        assert!(!toml_text.contains("name"));

        let listed = fx.app().library_list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "personal");
        assert_eq!(listed[0].path, entry.path);

        let reg_text = fs::read_to_string(fx.config_dir.join("registry.toml")).unwrap();
        assert!(reg_text.contains("personal"));
        assert!(reg_text.contains("path"));
    }

    #[test]
    fn library_meta_contribution_load_default_reject() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fs::create_dir_all(lib.join("skills")).unwrap();

        // Missing contribution → direct (no migrate required).
        fs::write(lib.join("library.toml"), "version = 1\n").unwrap();
        let meta = LibraryMeta::load(&lib).unwrap();
        assert_eq!(meta.contribution, Contribution::Direct);

        // Explicit review loads.
        fs::write(
            lib.join("library.toml"),
            "version = 1\ncontribution = \"review\"\n",
        )
        .unwrap();
        let meta = LibraryMeta::load(&lib).unwrap();
        assert_eq!(meta.contribution, Contribution::Review);

        // Invalid value fails clearly.
        fs::write(
            lib.join("library.toml"),
            "version = 1\ncontribution = \"push\"\n",
        )
        .unwrap();
        let err = LibraryMeta::load(&lib).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid"), "{msg}");
        assert!(
            msg.contains("contribution") || msg.contains("direct") || msg.contains("review"),
            "{msg}"
        );

        // library init writes contribution=direct.
        let init_lib = fx.libs_dir.join("fresh");
        fx.app()
            .library_init(&init_lib, "fresh", false)
            .expect("init");
        let init_meta = LibraryMeta::load(&init_lib).unwrap();
        assert_eq!(init_meta.contribution, Contribution::Direct);
        let text = fs::read_to_string(init_lib.join("library.toml")).unwrap();
        assert!(text.contains("contribution = \"direct\""));
    }

    #[test]
    fn library_register_registers_existing_git_repo_read_only() {
        let fx = Fixture::new();
        let lib_path = fx.libs_dir.join("company");
        fs::create_dir_all(lib_path.join("skills")).unwrap();
        fs::create_dir_all(lib_path.join(".git")).unwrap();
        LibraryMeta::new().save(&lib_path).unwrap();

        let entry = fx
            .app()
            .library_register(&lib_path, "company", true)
            .expect("register");
        assert!(entry.read_only);
        assert_eq!(entry.vcs, Some(Vcs::Git));

        let listed = fx.app().library_list().unwrap();
        assert_eq!(listed[0].vcs, Some(Vcs::Git));
        assert!(listed[0].read_only);
    }

    #[test]
    fn library_register_infers_jj() {
        let fx = Fixture::new();
        let lib_path = fx.libs_dir.join("jj-lib");
        fs::create_dir_all(lib_path.join("skills")).unwrap();
        fs::create_dir_all(lib_path.join(".jj")).unwrap();
        LibraryMeta::new().save(&lib_path).unwrap();

        let entry = fx
            .app()
            .library_register(&lib_path, "jjlib", false)
            .unwrap();
        assert_eq!(entry.vcs, Some(Vcs::Jj));
        assert!(lib_path.join("library.toml").is_file());
    }

    #[test]
    fn library_register_requires_library_toml() {
        let fx = Fixture::new();
        let lib_path = fx.libs_dir.join("bare");
        fs::create_dir_all(lib_path.join("skills")).unwrap();
        let err = fx
            .app()
            .library_register(&lib_path, "bare", false)
            .unwrap_err();
        assert!(err.to_string().contains("no library.toml"));
    }

    #[test]
    fn library_init_adopts_skills_and_moves_root_skills() {
        let fx = Fixture::new();
        let adopt = fx.libs_dir.join("adopt");
        let skill = adopt.join("skills/demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: demo\n---\n").unwrap();
        fx.app()
            .library_init(&adopt, "adopt", false)
            .expect("adopt skills/");
        assert!(skill.join("SKILL.md").is_file());

        let rooted = fx.libs_dir.join("rooted");
        let root_skill = rooted.join("demo");
        fs::create_dir_all(&root_skill).unwrap();
        fs::write(root_skill.join("SKILL.md"), "---\nname: demo\n---\n").unwrap();
        let err = fx
            .app()
            .library_init(&rooted, "rooted", false)
            .unwrap_err();
        assert!(err.to_string().contains("--move"));
        fx.app()
            .library_init(&rooted, "rooted", true)
            .expect("move root skills");
        assert!(rooted.join("skills/demo/SKILL.md").is_file());
        assert!(!root_skill.exists());
    }

    #[test]
    fn library_init_refuses_existing_library_toml() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fx.app().library_init(&lib, "personal", false).unwrap();
        let err = fx
            .app()
            .library_init(&lib, "other", false)
            .unwrap_err();
        assert!(err.to_string().contains("library register"));
    }

    #[test]
    fn resolve_library_name_defaults_to_cwd_library() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fx.app().library_init(&lib, "personal", false).unwrap();
        let nested = lib.join("skills");
        let name = fx
            .app_at(&nested)
            .resolve_library_name(None)
            .expect("cwd library");
        assert_eq!(name, "personal");
        let err = fx.app().resolve_library_name(None).unwrap_err();
        assert!(err.to_string().contains("pass --library"));
    }

    #[test]
    fn list_skills_from_library_cwd_without_project() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fx.app().library_init(&lib, "personal", false).unwrap();
        let mut meta = LibraryMeta::new();
        meta.upstream = vec![crate::library::UpstreamEntry {
            name: "demo".into(),
            source: "github:owner/repo/demo@abc".into(),
            tags: vec!["tools".into()],
        }];
        meta.save(&lib).unwrap();

        let rows = fx
            .app_at(&lib)
            .list_skills(None, &[], false, true)
            .expect("list from library cwd");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].entry.name, "demo");
    }

    #[test]
    fn library_list_is_registry_catalog_is_entries() {
        let fx = Fixture::new();
        let lib = fx.libs_dir.join("personal");
        fx.app().library_init(&lib, "personal", false).unwrap();
        let mut meta = LibraryMeta::new();
        meta.upstream = vec![
            crate::library::UpstreamEntry {
                name: "demo".into(),
                source: "github:owner/repo/demo@abc".into(),
                tags: vec!["tools".into()],
            },
            crate::library::UpstreamEntry {
                name: "other".into(),
                source: "github:owner/repo/other@abc".into(),
                tags: vec!["misc".into()],
            },
        ];
        meta.save(&lib).unwrap();

        let registry = fx.app().library_list().unwrap();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry[0].name, "personal");

        let all = fx
            .app_at(&lib)
            .list_library_catalog("personal", &[], false)
            .unwrap();
        assert_eq!(all.len(), 2);

        let filtered = fx
            .app()
            .list_library_catalog("personal", &["tools".into()], false)
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].entry.name, "demo");

        let via_cwd = fx
            .app_at(&lib)
            .resolve_library_name(None)
            .unwrap();
        assert_eq!(via_cwd, "personal");
    }

    #[test]
    fn library_init_rejects_duplicate_name() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("a"), "personal", false)
            .unwrap();
        let err = fx
            .app()
            .library_init(&fx.libs_dir.join("b"), "personal", false)
            .unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    /// Seed skillz github cache with a local git repo (avoids network).
    fn seed_github_cache(fx: &Fixture, owner: &str, repo: &str) -> String {
        let dest = fx
            .config_dir
            .join("cache")
            .join("github")
            .join(owner)
            .join(repo);
        fs::create_dir_all(dest.join("skills/engineering/grilling")).unwrap();
        fs::create_dir_all(dest.join("skills/productivity/focus")).unwrap();
        fs::write(
            dest.join("skills/engineering/grilling/SKILL.md"),
            "---\nname: grilling\n---\n# g\n",
        )
        .unwrap();
        fs::write(
            dest.join("skills/productivity/focus/SKILL.md"),
            "---\nname: focus\n---\n# f\n",
        )
        .unwrap();
        // Duplicate name under another tree for collision-within-batch coverage.
        fs::create_dir_all(dest.join("skills/other/grilling")).unwrap();
        fs::write(
            dest.join("skills/other/grilling/SKILL.md"),
            "---\nname: grilling\n---\n# other\n",
        )
        .unwrap();

        std::process::Command::new("git")
            .args(["-C"])
            .arg(&dest)
            .args(["init"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&dest)
            .args(["config", "user.email", "test@example.com"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&dest)
            .args(["config", "user.name", "test"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&dest)
            .args(["add", "."])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&dest)
            .args(["commit", "-m", "init"])
            .status()
            .unwrap();
        fetch::git_head_rev(&dest)
            .unwrap()
            .expect("rev")
    }

    #[test]
    fn library_import_discovers_upserts_dry_run_path_and_collision() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        let rev = seed_github_cache(&fx, "mattpocock", "skills");

        // Dry-run with skip policy leaves library.toml unchanged and reports conflicts.
        let before = fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap();
        let dry = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                None,
                true,
                &[],
                None,
                LibraryImportOnConflict::Skip,
            )
            .unwrap();
        assert!(dry.dry_run);
        assert!(dry.items.iter().any(|i| i.name == "grilling"));
        assert!(dry.items.iter().any(|i| i.name == "focus"));
        assert!(dry.items.iter().any(|i| matches!(
            i.action,
            LibraryImportAction::SkipCollision { .. }
        )));
        let after_dry = fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap();
        assert_eq!(before, after_dry);

        // Fail-closed default aborts when discovery has duplicate names.
        let fail = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                None,
                false,
                &[],
                None,
                LibraryImportOnConflict::Fail,
            )
            .unwrap_err();
        assert!(fail.to_string().contains("conflict"));
        assert_eq!(
            before,
            fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap()
        );

        // HTTPS form + path filter (no conflicts).
        let filtered = fx
            .app()
            .library_import(
                "https://github.com/mattpocock/skills",
                "personal",
                Some("skills/engineering"),
                false,
                &["elixir".into()],
                None,
                LibraryImportOnConflict::Fail,
            )
            .unwrap();
        assert_eq!(
            filtered
                .items
                .iter()
                .filter(|i| matches!(i.action, LibraryImportAction::Add))
                .count(),
            1
        );
        assert_eq!(filtered.items[0].name, "grilling");
        assert_eq!(
            filtered.items[0].source,
            format!("github:mattpocock/skills/skills/engineering/grilling@{rev}")
        );

        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert_eq!(meta.upstream.len(), 1);
        assert_eq!(meta.upstream[0].name, "grilling");
        assert_eq!(meta.upstream[0].tags, vec!["elixir".to_string()]);

        // Skip policy: focus added; duplicate grilling soft-skipped; same-identity tag merge.
        let full = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                None,
                false,
                &["phoenix".into()],
                None,
                LibraryImportOnConflict::Skip,
            )
            .unwrap();
        let focus = full.items.iter().find(|i| i.name == "focus").unwrap();
        assert!(matches!(focus.action, LibraryImportAction::Add));
        let grilling_actions: Vec<_> = full
            .items
            .iter()
            .filter(|i| i.name == "grilling")
            .map(|i| i.action.clone())
            .collect();
        assert!(grilling_actions.iter().any(|a| matches!(
            a,
            LibraryImportAction::SkipIdentical | LibraryImportAction::Update
        )));
        assert!(grilling_actions
            .iter()
            .any(|a| matches!(a, LibraryImportAction::SkipCollision { .. })));

        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert_eq!(meta.upstream.len(), 2);
        let grilling = meta.upstream.iter().find(|u| u.name == "grilling").unwrap();
        assert!(grilling.tags.contains(&"elixir".to_string()));
        assert!(grilling.tags.contains(&"phoenix".to_string()));
        let focus_up = meta.upstream.iter().find(|u| u.name == "focus").unwrap();
        assert_eq!(focus_up.tags, vec!["phoenix".to_string()]);

        assert!(!fx.project_dir.join(".agents/skills").exists());
        assert!(!fx.home_dir.join(".agents/skills").exists());

        // Different Upstream identity: Fail aborts; Skip soft-skips.
        let mut meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        meta.upsert_upstream(crate::library::UpstreamEntry {
            name: "focus".into(),
            source: "github:other/repo/focus@abc".into(),
            tags: vec!["phoenix".into()],
        });
        meta.save(&fx.libs_dir.join("personal")).unwrap();
        let before_collide =
            fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap();
        let fail_collide = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                Some("skills/productivity"),
                false,
                &[],
                None,
                LibraryImportOnConflict::Fail,
            )
            .unwrap_err();
        assert!(fail_collide.to_string().contains("conflict"));
        assert_eq!(
            before_collide,
            fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap()
        );
        let collide = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                Some("skills/productivity"),
                false,
                &[],
                None,
                LibraryImportOnConflict::Skip,
            )
            .unwrap();
        assert!(matches!(
            collide.items[0].action,
            LibraryImportAction::SkipCollision { .. }
        ));
    }

    #[test]
    fn library_import_prompt_renames_then_applies_atomically() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        let rev = seed_github_cache(&fx, "mattpocock", "skills");
        let mut meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        meta.upsert_upstream(crate::library::UpstreamEntry {
            name: "grilling".into(),
            source: "github:other/repo/grilling@abc".into(),
            tags: vec![],
        });
        meta.save(&fx.libs_dir.join("personal")).unwrap();

        let mut resolutions = vec![LibraryImportConflictResolution::Rename(
            "renamed-grilling".into(),
        )];
        let mut resolver =
            |item: &LibraryImportItem| -> Result<LibraryImportConflictResolution> {
                assert_eq!(item.name, "grilling");
                Ok(resolutions.remove(0))
            };
        let report = fx
            .app()
            .library_import_filtered(
                "github:mattpocock/skills",
                "personal",
                Some("skills/engineering"),
                false,
                None,
                &[],
                None,
                LibraryImportOnConflict::Prompt,
                Some(&mut resolver),
            )
            .unwrap();
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].name, "renamed-grilling");
        assert!(matches!(report.items[0].action, LibraryImportAction::Add));
        assert_eq!(
            report.items[0].source,
            format!("github:mattpocock/skills/skills/engineering/grilling@{rev}")
        );
        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert!(meta.upstream.iter().any(|u| u.name == "renamed-grilling"));
        assert!(meta.upstream.iter().any(|u| u.name == "grilling"));
    }

    #[test]
    fn library_import_prompt_abort_writes_nothing() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        let _ = seed_github_cache(&fx, "mattpocock", "skills");
        let before = fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap();
        let mut resolver =
            |_item: &LibraryImportItem| Ok(LibraryImportConflictResolution::Abort);
        let err = fx
            .app()
            .library_import_filtered(
                "github:mattpocock/skills",
                "personal",
                None,
                false,
                None,
                &[],
                None,
                LibraryImportOnConflict::Prompt,
                Some(&mut resolver),
            )
            .unwrap_err();
        assert!(err.to_string().contains("aborted"));
        assert_eq!(
            before,
            fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap()
        );
    }

    #[test]
    fn library_import_name_override_avoids_collision() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        let rev = seed_github_cache(&fx, "mattpocock", "skills");

        // Existing catalog entry occupies the discovered skill name.
        let mut meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        meta.upsert_upstream(crate::library::UpstreamEntry {
            name: "grilling".into(),
            source: "github:other/repo/grilling@abc".into(),
            tags: vec![],
        });
        meta.save(&fx.libs_dir.join("personal")).unwrap();

        let err = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                None,
                true,
                &[],
                Some("renamed-grilling"),
                crate::LibraryImportOnConflict::Fail)
            .unwrap_err();
        assert!(err.to_string().contains("exactly one skill"));

        let report = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                Some("skills/engineering/grilling"),
                false,
                &["import".into()],
                Some("renamed-grilling"),
                crate::LibraryImportOnConflict::Fail)
            .unwrap();
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].name, "renamed-grilling");
        assert!(matches!(report.items[0].action, LibraryImportAction::Add));
        assert_eq!(
            report.items[0].source,
            format!("github:mattpocock/skills/skills/engineering/grilling@{rev}")
        );

        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert!(meta.upstream.iter().any(|u| u.name == "grilling"));
        let renamed = meta
            .upstream
            .iter()
            .find(|u| u.name == "renamed-grilling")
            .unwrap();
        assert_eq!(
            renamed.source,
            format!("github:mattpocock/skills/skills/engineering/grilling@{rev}")
        );
        assert_eq!(renamed.tags, vec!["import".to_string()]);
    }

    #[test]
    fn library_import_does_not_require_managed_project() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        let _ = seed_github_cache(&fx, "acme", "skills");
        // cwd project is unmanaged — import still works.
        assert!(!fx.project_dir.join(".skillz/skillz.toml").is_file());
        let report = fx
            .app()
            .library_import(
                "github:acme/skills",
                "personal",
                Some("skills/engineering"),
                false,
                &[],
                None,
                LibraryImportOnConflict::Fail,
            )
            .unwrap();
        assert!(!report.items.is_empty());
    }

    #[test]
    fn interactive_library_import_selects_subset_and_cancel() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal", false)
            .unwrap();
        let _ = seed_github_cache(&fx, "mattpocock", "skills");

        let before = fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap();
        let err = fx
            .app()
            .interactive_library_import(
                "github:mattpocock/skills",
                "personal",
                None,
                &[],
                None,
                LibraryImportOnConflict::Fail,
                None,
                &mut CancelPrompter,
            )
            .unwrap_err();
        assert!(err.to_string().contains("cancelled"));
        assert_eq!(
            before,
            fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap()
        );

        // Path filter + select first only (grilling under engineering).
        let mut ui = SelectFirstNPrompter { n: 1 };
        let report = fx
            .app()
            .interactive_library_import(
                "github:mattpocock/skills",
                "personal",
                Some("skills/engineering"),
                &["process".into()],
                None,
                LibraryImportOnConflict::Fail,
                None,
                &mut ui,
            )
            .unwrap();
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].name, "grilling");
        assert!(matches!(report.items[0].action, LibraryImportAction::Add));
        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert_eq!(meta.upstream.len(), 1);
        assert_eq!(meta.upstream[0].name, "grilling");
        assert_eq!(meta.upstream[0].tags, vec!["process".to_string()]);
        assert!(!fx.project_dir.join(".agents/skills").exists());
    }
}
