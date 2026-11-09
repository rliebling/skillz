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
pub mod project;
pub mod registry;
pub mod skill;
pub mod source;
pub mod ui;
pub mod vcs;
pub mod vcs_ops;

pub use import::{AdoptAs, ImportPlan, SkillfileEntry};
pub use ui::{
    CancelPrompter, Prompter, SelectAllPrompter, SelectFirstNPrompter, SelectItem,
};

pub use catalog::{Availability, CatalogEntry};
pub use library::{ensure_library_meta, init_library_skeleton, LibraryMeta};
pub use project::{Binding, ProjectConfig, ProjectLock, DEFAULT_SKILLS_DIR};
pub use registry::{LibraryRecord, Registry};
pub use skill::{DriftAction, DriftItem, DriftReason, EntryKind, InstallationLock};
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

/// A list row including Availability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    pub entry: CatalogEntry,
    pub availability: Availability,
}

/// Drift for one Managed Project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDrift {
    pub project: PathBuf,
    pub items: Vec<DriftItem>,
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
                    message: "User scope not initialized (no user.toml)".to_string(),
                });
            }
            let config = self.load_scope_config()?;
            let lock = self.load_scope_lock()?;
            let installations = lock.installation_names();
            let message = format!(
                "User scope; {} binding(s); {} installation(s)",
                config.bindings.len(),
                installations.len()
            );
            return Ok(StatusReport {
                cwd: self.paths.user_skills_dir(),
                config_dir: self.paths.config_dir.clone(),
                managed: true,
                skills_dir: Some(config.skills_dir),
                bindings: config.bindings,
                installations,
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
                message: "Not a Managed Project (no .skillz/skillz.toml)".to_string(),
            });
        }
        let config = ProjectConfig::load(&self.paths.cwd)?;
        let lock = self.load_scope_lock()?;
        let installations = lock.installation_names();
        let message = format!(
            "Managed Project; {} binding(s); {} installation(s)",
            config.bindings.len(),
            installations.len()
        );
        Ok(StatusReport {
            cwd: self.paths.cwd.clone(),
            config_dir: self.paths.config_dir.clone(),
            managed: true,
            skills_dir: Some(config.skills_dir),
            bindings: config.bindings,
            installations,
            message,
        })
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
            self.libcommit_upstream(&entry.name, library_name, Some(&source))?;
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
                    self.libcommit_authored(&name, library_name)?;
                    results.push((name, "adopted as authored".into()));
                }
                AdoptAs::Upstream => {
                    let source = format!("github:local/import/{name}@adopted");
                    self.libcommit_upstream(&name, library_name, Some(&source))?;
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

    /// Scaffold a new Authored skill in the Project skills dir (Drift until libcommit).
    pub fn skill_new(&self, name: &str, tags: &[String]) -> Result<PathBuf> {
        let config = self.load_scope_config()?;
        let root = self.scope_skills_root(&config);
        skill::scaffold_skill(&root, name, tags)
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
            DriftAction::Libcommit => {
                self.libcommit_branch(name, None, Some("interactive libcommit"))?;
                Ok("libcommitted")
            }
            DriftAction::Promote => {
                self.promote_skill(name)?;
                Ok("promoted")
            }
            DriftAction::Remove => {
                self.remove_skill_files(name)?;
                Ok("removed")
            }
        }
    }

    /// Detect Drift for skills present under skills_dir vs the Lock (current scope).
    pub fn detect_drift(&self) -> Result<Vec<DriftItem>> {
        let config = self.load_scope_config()?;
        let lock = self.load_scope_lock()?;
        let root = self.scope_skills_root(&config);
        Self::drift_items_for(&root, &lock)
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
        Self::drift_items_for(&root, &lock)
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
                    }),
                    Some(rec) => match skill::content_hash(&path) {
                        Ok(actual) if actual == rec.content_hash => {}
                        Ok(actual) => items.push(DriftItem {
                            name,
                            reason: DriftReason::HashMismatch {
                                locked: rec.content_hash.clone(),
                                actual,
                            },
                        }),
                        Err(_) => items.push(DriftItem {
                            name,
                            reason: DriftReason::MissingFiles,
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
                });
            }
        }
        items.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(items)
    }

    /// Drift across Managed Projects recorded in the user registry.
    pub fn drift_report(&self, project_filter: Option<&Path>) -> Result<Vec<ProjectDrift>> {
        let mut projects = Vec::new();
        if let Some(p) = project_filter {
            projects.push(canonicalize_or_clone(p));
        } else {
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
        }
        let mut out = Vec::new();
        for project in projects {
            let items = self.detect_drift_in(&project)?;
            if !items.is_empty() {
                out.push(ProjectDrift { project, items });
            }
        }
        if project_filter.is_none() && self.paths.user_config_path().is_file() {
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

    pub fn hooks_install(&self) -> Result<Vec<PathBuf>> {
        if self.scope == Scope::User {
            return Err(SkillzError::Message(
                "hooks install applies to Managed Projects, not user scope".into(),
            ));
        }
        self.require_managed()?;
        let mut installed = Vec::new();
        match hooks::install_git_pre_commit(&self.paths.cwd) {
            Ok(path) => installed.push(path),
            Err(err) => {
                // jj-only checkouts may still get the alias below.
                if infer_vcs(&self.paths.cwd) != Some(Vcs::Jj) {
                    return Err(err);
                }
            }
        }
        if infer_vcs(&self.paths.cwd) == Some(Vcs::Jj) {
            installed.push(hooks::install_jj_push_with_checks(&self.paths.cwd)?);
        }
        if installed.is_empty() {
            return Err(SkillzError::Message(
                "no hooks installed (need git and/or jj metadata)".into(),
            ));
        }
        Ok(installed)
    }

    /// List Catalog Entries from bound Libraries (or a single `--library`).
    ///
    /// When `project_only` is true, only skills present in this Project's
    /// skills dir and/or recorded in the Project Lock are returned.
    pub fn list_skills(
        &self,
        library_filter: Option<&str>,
        tags: &[String],
        dedupe: bool,
        project_only: bool,
    ) -> Result<Vec<ListRow>> {
        let config = self.load_scope_config()?;
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
    pub fn add_skill(
        &self,
        name: &str,
        library_filter: Option<&str>,
    ) -> Result<(AddOutcome, Option<String>)> {
        self.add_skill_with_deps(name, library_filter, false)
            .map(|(outcome, warn, _)| (outcome, warn))
    }

    pub fn add_skill_with_deps(
        &self,
        name: &str,
        library_filter: Option<&str>,
        with_deps: bool,
    ) -> Result<(AddOutcome, Option<String>, Vec<String>)> {
        skill::validate_skill_name(name)?;
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
        let outcome = if row.entry.kind == EntryKind::Upstream {
            self.add_upstream_installation(&config, name, &row.entry)?
        } else if dest.join("SKILL.md").is_file() {
            let existing = skill::content_hash(&dest)?;
            if existing == row.entry.content_hash {
                let mut lock = self.load_scope_lock()?;
                let install = InstallationLock {
                    library: row.entry.library.clone(),
                    entry_kind: EntryKind::Owned,
                    content_hash: row.entry.content_hash.clone(),
                    source: None,
                    rev: None,
                    branch: None,
                    overlay_from_library: None,
                    overlay_from_entry: None,
                };
                lock.installations
                    .insert(name.to_string(), install.clone());
                self.save_scope_lock(&lock)?;
                AddOutcome::AlreadyPresent(install)
            } else {
                skill::copy_skill_tree(&row.entry.skill_path, &dest)?;
                let install = self.write_owned_lock(name, &row.entry)?;
                AddOutcome::Refreshed(install)
            }
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
                if let Ok((dep_outcome, _, nested)) =
                    self.add_skill_with_deps(&dep, library_filter.or(Some(&row.entry.library)), true)
                {
                    let _ = dep_outcome;
                    pulled.push(dep);
                    pulled.extend(nested);
                }
            }
            missing.retain(|m| !pulled.iter().any(|p| p == m));
        }
        Ok((outcome, warn, missing))
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
        let parsed = source::GithubSource::parse(source)?;
        let cache = self.paths.config_dir.join("cache");
        let (skill_path, pinned) = fetch::resolve_github_skill(&cache, &parsed, Some(name))?;
        let dest = skill::skill_dir(&self.scope_skills_root(config), name);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SkillzError::Message(format!(
                    "failed to create {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let already = dest.join("SKILL.md").is_file();
        let prev_hash = if already {
            Some(skill::content_hash(&dest)?)
        } else {
            None
        };
        skill::copy_skill_tree(&skill_path, &dest)?;
        let hash = skill::content_hash(&dest)?;
        let install = InstallationLock {
            library: entry.library.clone(),
            entry_kind: EntryKind::Upstream,
            content_hash: hash.clone(),
            source: Some(pinned.render()),
            rev: pinned.rev.clone(),
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

    /// Fetch an Upstream skill into the Project skills dir (Drift until libcommit).
    ///
    /// `source` is a filesystem path or `github:owner/repo…`. Optional `skill_name`
    /// enables nested name lookup (`skillz fetch github:owner/repo NAME`).
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

        if source.starts_with("github:") {
            let parsed = source::GithubSource::parse(source)?;
            let cache = self.paths.config_dir.join("cache");
            let (skill_path, pinned) =
                fetch::resolve_github_skill(&cache, &parsed, skill_name)?;
            let dest = fetch::stage_skill(&skills_root, &skill_path, skill_name)?;
            // Stash pinned source beside the skill for libcommit (sidecar).
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

    /// Formalize a tested Installation into a writable Library as an Owned Entry.
    pub fn libcommit_authored(
        &self,
        name: &str,
        library_name: &str,
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
    ) -> Result<InstallationLock> {
        skill::validate_skill_name(name)?;
        let config = self.load_scope_config()?;
        let mut lock = self.load_scope_lock()?;
        let existing = lock.installations.get(name).cloned();
        let desired = library_name
            .map(|s| s.to_string())
            .or_else(|| existing.as_ref().map(|e| e.library.clone()))
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

    /// After promote, re-Lock to Library main and refresh the Installation.
    pub fn lock_main(&self, name: &str) -> Result<InstallationLock> {
        let config = self.load_scope_config()?;
        let mut lock = self.load_scope_lock()?;
        let install = lock.installations.get(name).cloned().ok_or_else(|| {
            SkillzError::Message(format!("skill `{name}` is not locked"))
        })?;
        let registry = self.load_registry()?;
        let record = registry.libraries.get(&install.library).ok_or_else(|| {
            SkillzError::Message(format!(
                "library `{}` is not in the local registry",
                install.library
            ))
        })?;
        let main_rev = vcs_ops::main_rev(&record.path)?;
        let owned = record.path.join(crate::library::SKILLS_DIR).join(name);
        if !owned.is_dir() {
            return Err(SkillzError::Message(format!(
                "Owned Entry `{name}` missing on Library main at {}",
                owned.display()
            )));
        }
        let dest = skill::skill_dir(&self.scope_skills_root(&config), name);
        skill::copy_skill_tree(&owned, &dest)?;
        let hash = skill::content_hash(&dest)?;
        let updated = InstallationLock {
            library: install.library,
            entry_kind: install.entry_kind,
            content_hash: hash,
            source: install.source,
            rev: Some(main_rev),
            branch: None,
            overlay_from_library: install.overlay_from_library,
            overlay_from_entry: install.overlay_from_entry,
        };
        lock.installations.insert(name.to_string(), updated.clone());
        self.save_scope_lock(&lock)?;
        Ok(updated)
    }

    /// Formalize a fetched Installation as an Upstream Catalog Entry + Lock.
    pub fn libcommit_upstream(
        &self,
        name: &str,
        library_name: &str,
        source: Option<&str>,
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
                        "upstream libcommit requires a source string (pass --source or fetch via github:…)"
                            .into(),
                    ));
                }
            }
        };
        // Validate grammar when it looks like github.
        if source.starts_with("github:") {
            let parsed = source::GithubSource::parse(&source)?;
            if parsed.rev.is_none() {
                return Err(SkillzError::Message(
                    "upstream Lock source must include @rev".into(),
                ));
            }
        }

        // Upstream Catalog Entry is a reference — do not vendor into Library skills/.
        let mut meta = LibraryMeta::load(&lib_path)?;
        meta.upsert_upstream(crate::library::UpstreamEntry {
            name: name.to_string(),
            source: source.clone(),
            tags: Vec::new(),
        });
        meta.save(&lib_path)?;

        let hash = skill::content_hash(&project_skill)?;

        let install = InstallationLock {
            library: library_name.to_string(),
            entry_kind: EntryKind::Upstream,
            content_hash: hash,
            source: Some(source),
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

    /// Create a Library skeleton and register it.
    pub fn library_init(&self, path: &Path, name: &str) -> Result<LibraryListEntry> {
        validate_library_name(name)?;
        let abs = resolve_path_against_cwd(&self.paths.cwd, path)?;
        init_library_skeleton(&abs, name)?;
        self.register_library(name, &abs, false)
    }

    /// Register an existing Library checkout (optionally read-only).
    pub fn library_add(
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
        ensure_library_meta(&abs, name)?;
        self.register_library(name, &abs, read_only)
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
    /// When `only_sources` is `Some`, only candidates whose canonical source
    /// strings appear in that list are considered (order follows discovery).
    pub fn library_import(
        &self,
        source: &str,
        library_name: &str,
        path_filter: Option<&str>,
        dry_run: bool,
        tags: &[String],
    ) -> Result<LibraryImportReport> {
        self.library_import_filtered(source, library_name, path_filter, dry_run, None, tags)
    }

    pub fn library_import_filtered(
        &self,
        source: &str,
        library_name: &str,
        path_filter: Option<&str>,
        dry_run: bool,
        only_sources: Option<&[String]>,
        tags: &[String],
    ) -> Result<LibraryImportReport> {
        let lib_path = self.writable_registry_library(library_name)?;
        let mut candidates = self.library_import_discover(source, library_name, path_filter)?;
        if let Some(only) = only_sources {
            let want: std::collections::BTreeSet<&str> = only.iter().map(|s| s.as_str()).collect();
            candidates.retain(|c| want.contains(c.source.as_str()));
        }
        let assign_tags = catalog::normalize_tag_filters(tags);

        let mut meta = LibraryMeta::load(&lib_path)?;
        let owned_names: std::collections::BTreeSet<String> =
            catalog::list_owned_entries(library_name, &lib_path)?
                .into_iter()
                .map(|e| e.name)
                .collect();

        let mut items = Vec::new();
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
                            name: skill.name.clone(),
                            source: source_str.clone(),
                            tags,
                        });
                    }
                    seen_names.insert(skill.name.clone());
                }
                LibraryImportAction::SkipIdentical => {
                    seen_names.insert(skill.name.clone());
                }
                LibraryImportAction::SkipCollision { .. } | LibraryImportAction::SkipOwned => {}
            }

            items.push(LibraryImportItem {
                name: skill.name,
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
            items,
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
        fx.app().library_init(&lib, "personal").unwrap();

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
            .library_init(&fx.libs_dir.join("company"), "company")
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
    fn new_then_libcommit_authored_clears_drift() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal")
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

        let lock = fx
            .app()
            .libcommit_authored("demo-skill", "personal")
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
    fn libcommit_rejects_read_only_library() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("company"), "company")
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
        let err = fx.app().libcommit_authored("x", "company").unwrap_err();
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
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("demo", &[]).unwrap();
        fx.app().libcommit_authored("demo", "personal").unwrap();
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
            .library_init(&fx.libs_dir.join("personal"), "personal")
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
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app()
            .skill_new("a", &["t".into()])
            .unwrap();
        fx.app().libcommit_authored("a", "personal").unwrap();
        fx.app()
            .skill_new("b", &["t".into()])
            .unwrap();
        fx.app().libcommit_authored("b", "personal").unwrap();
        fx.app()
            .skill_new("c", &["other".into()])
            .unwrap();
        fx.app().libcommit_authored("c", "personal").unwrap();
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
            .library_init(&fx.libs_dir.join("personal"), "personal")
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
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        // Seed Library Catalog Entry via a temporary project.
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("demo", &["cli".into()]).unwrap();
        fx.app().libcommit_authored("demo", "personal").unwrap();

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
        fx.app().library_init(&company, "company").unwrap();
        fx.app().library_init(&personal, "personal").unwrap();
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
            .libcommit_branch("grilling", Some("company"), Some("overlay"))
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
        fx.app().library_init(&lib, "personal").unwrap();
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
            .libcommit_authored("grilling", "personal")
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
            .libcommit_branch("grilling", None, Some("tweak"))
            .unwrap();
        assert!(branch_lock.branch.as_deref().unwrap().contains("grilling"));
        assert!(branch_lock.rev.is_some());
        assert!(fx.app().detect_drift().unwrap().is_empty());

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
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app().skill_new("drifty", &[]).unwrap();

        let report = fx.app().drift_report(None).unwrap();
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
    fn hooks_install_jj_push_with_checks_alias() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fs::create_dir_all(fx.project_dir.join(".jj/repo")).unwrap();
        let hooks = fx.app().hooks_install().unwrap();
        let jj_cfg = hooks
            .iter()
            .find(|p| p.ends_with("config.toml"))
            .expect("jj config");
        let body = fs::read_to_string(jj_cfg).unwrap();
        assert!(body.contains("push-with-checks"));
        assert!(body.contains("skillz drift --check"));
        assert!(!body.contains("skillz push"));
    }

    #[test]
    fn fetch_local_nested_then_libcommit_upstream() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();

        // Nested "Pocock-style" layout on disk.
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
            .fetch_skill(upstream_root.to_str().unwrap(), Some("ecto-thinking"))
            .unwrap();
        assert!(dest.join("SKILL.md").is_file());
        let drift = fx.app().detect_drift().unwrap();
        assert!(drift.iter().any(|d| d.name == "ecto-thinking"));

        let lock = fx
            .app()
            .libcommit_upstream(
                "ecto-thinking",
                "personal",
                Some("github:acme/skills/skills/ecto-thinking@deadbeef"),
            )
            .unwrap();
        assert_eq!(lock.entry_kind, EntryKind::Upstream);
        assert_eq!(
            lock.source.as_deref(),
            Some("github:acme/skills/skills/ecto-thinking@deadbeef")
        );
        let meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        assert_eq!(meta.upstream.len(), 1);
        assert!(fx.app().detect_drift().unwrap().is_empty());
        // Upstream is a reference — not vendored under Library skills/.
        assert!(!fx
            .libs_dir
            .join("personal/skills/ecto-thinking")
            .exists());
    }

    #[test]
    fn list_and_add_from_library_with_tags_and_second_project() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        fx.app()
            .project_init(&["personal".into()], None)
            .unwrap();
        fx.app()
            .skill_new("demo-skill", &["elixir".into(), "phoenix".into()])
            .unwrap();
        fx.app()
            .libcommit_authored("demo-skill", "personal")
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

        // Already present → no-op/refresh.
        let (again, _) = fx.app().add_skill("demo-skill", None).unwrap();
        assert!(matches!(again, AddOutcome::AlreadyPresent(_)));
    }

    #[test]
    fn library_init_creates_skeleton_and_registry() {
        let fx = Fixture::new();
        let lib_path = fx.libs_dir.join("personal");
        let entry = fx
            .app()
            .library_init(&lib_path, "personal")
            .expect("init");

        assert_eq!(entry.name, "personal");
        assert!(lib_path.join("library.toml").is_file());
        assert!(lib_path.join("skills").is_dir());
        assert!(!entry.read_only);

        let meta = LibraryMeta::load(&lib_path).unwrap();
        assert_eq!(meta.name, "personal");

        let listed = fx.app().library_list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "personal");
        assert_eq!(listed[0].path, entry.path);

        let reg_text = fs::read_to_string(fx.config_dir.join("registry.toml")).unwrap();
        assert!(reg_text.contains("personal"));
        assert!(reg_text.contains("path"));
    }

    #[test]
    fn library_add_registers_existing_git_repo_read_only() {
        let fx = Fixture::new();
        let lib_path = fx.libs_dir.join("company");
        fs::create_dir_all(lib_path.join("skills")).unwrap();
        fs::create_dir_all(lib_path.join(".git")).unwrap();
        LibraryMeta {
            name: "company".into(),
            upstream: Vec::new(),
        }
        .save(&lib_path)
        .unwrap();

        let entry = fx
            .app()
            .library_add(&lib_path, "company", true)
            .expect("add");
        assert!(entry.read_only);
        assert_eq!(entry.vcs, Some(Vcs::Git));

        let listed = fx.app().library_list().unwrap();
        assert_eq!(listed[0].vcs, Some(Vcs::Git));
        assert!(listed[0].read_only);
    }

    #[test]
    fn library_add_infers_jj() {
        let fx = Fixture::new();
        let lib_path = fx.libs_dir.join("jj-lib");
        fs::create_dir_all(&lib_path).unwrap();
        fs::create_dir_all(lib_path.join(".jj")).unwrap();

        let entry = fx.app().library_add(&lib_path, "jjlib", false).unwrap();
        assert_eq!(entry.vcs, Some(Vcs::Jj));
        assert!(lib_path.join("library.toml").is_file());
        assert!(lib_path.join("skills").is_dir());
    }

    #[test]
    fn library_init_rejects_duplicate_name() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("a"), "personal")
            .unwrap();
        let err = fx
            .app()
            .library_init(&fx.libs_dir.join("b"), "personal")
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
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        let rev = seed_github_cache(&fx, "mattpocock", "skills");

        // Dry-run leaves library.toml unchanged.
        let before = fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap();
        let dry = fx
            .app()
            .library_import("github:mattpocock/skills", "personal", None, true, &[])
            .unwrap();
        assert!(dry.dry_run);
        assert!(dry.items.iter().any(|i| i.name == "grilling"));
        assert!(dry.items.iter().any(|i| i.name == "focus"));
        let after_dry = fs::read_to_string(fx.libs_dir.join("personal/library.toml")).unwrap();
        assert_eq!(before, after_dry);

        // HTTPS form + path filter.
        let filtered = fx
            .app()
            .library_import(
                "https://github.com/mattpocock/skills",
                "personal",
                Some("skills/engineering"),
                false,
                &["elixir".into()],
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

        // Full import: focus added; other grilling collides with existing / earlier.
        // New tag on re-import merges onto grilling.
        let full = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                None,
                false,
                &["phoenix".into()],
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

        // No Project / user skills materialization.
        assert!(!fx.project_dir.join(".agents/skills").exists());
        assert!(!fx.home_dir.join(".agents/skills").exists());

        // Name collision with different source skips.
        let mut meta = LibraryMeta::load(&fx.libs_dir.join("personal")).unwrap();
        meta.upsert_upstream(crate::library::UpstreamEntry {
            name: "focus".into(),
            source: "github:other/repo/focus@abc".into(),
            tags: vec!["phoenix".into()],
        });
        meta.save(&fx.libs_dir.join("personal")).unwrap();
        let collide = fx
            .app()
            .library_import(
                "github:mattpocock/skills",
                "personal",
                Some("skills/productivity"),
                false,
                &[],
            )
            .unwrap();
        assert!(matches!(
            collide.items[0].action,
            LibraryImportAction::SkipCollision { .. }
        ));
    }

    #[test]
    fn library_import_does_not_require_managed_project() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal")
            .unwrap();
        let _ = seed_github_cache(&fx, "acme", "skills");
        // cwd project is unmanaged — import still works.
        assert!(!fx.project_dir.join(".skillz/skillz.toml").is_file());
        let report = fx
            .app()
            .library_import("github:acme/skills", "personal", None, false, &[])
            .unwrap();
        assert!(!report.items.is_empty());
    }

    #[test]
    fn interactive_library_import_selects_subset_and_cancel() {
        let fx = Fixture::new();
        fx.app()
            .library_init(&fx.libs_dir.join("personal"), "personal")
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
