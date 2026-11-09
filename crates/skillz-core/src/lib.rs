//! Application command API for skillz.
//!
//! CLI and future TUI share this seam.

use std::path::{Path, PathBuf};

use thiserror::Error;

pub mod catalog;
pub mod fetch;
pub mod library;
pub mod project;
pub mod registry;
pub mod skill;
pub mod source;
pub mod vcs;

pub use catalog::{Availability, CatalogEntry};
pub use library::{ensure_library_meta, init_library_skeleton, LibraryMeta};
pub use project::{Binding, ProjectConfig, ProjectLock, DEFAULT_SKILLS_DIR};
pub use registry::{LibraryRecord, Registry};
pub use skill::{DriftItem, DriftReason, EntryKind, InstallationLock};
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

/// A list row including Availability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    pub entry: CatalogEntry,
    pub availability: Availability,
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
}

impl App {
    pub fn new(paths: SkillzPaths) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &SkillzPaths {
        &self.paths
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
        let lock = ProjectLock::load(&self.paths.cwd)?;
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

    fn skills_root(&self, config: &ProjectConfig) -> PathBuf {
        self.paths.cwd.join(&config.skills_dir)
    }

    /// Scaffold a new Authored skill in the Project skills dir (Drift until libcommit).
    pub fn skill_new(&self, name: &str, tags: &[String]) -> Result<PathBuf> {
        let config = self.require_managed()?;
        let root = self.skills_root(&config);
        skill::scaffold_skill(&root, name, tags)
    }

    /// Detect Drift for skills present under skills_dir vs the Project Lock.
    pub fn detect_drift(&self) -> Result<Vec<DriftItem>> {
        let config = self.require_managed()?;
        let lock = ProjectLock::load(&self.paths.cwd)?;
        let root = self.skills_root(&config);
        let mut items = Vec::new();
        if root.is_dir() {
            for entry in std::fs::read_dir(&root).map_err(|e| {
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
            let path = skill::skill_dir(&root, name);
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

    /// List Catalog Entries from bound Libraries (or a single `--library`).
    pub fn list_skills(
        &self,
        library_filter: Option<&str>,
        tags: &[String],
        dedupe: bool,
    ) -> Result<Vec<ListRow>> {
        let config = self.require_managed()?;
        let registry = self.load_registry()?;
        let tag_filters = catalog::normalize_tag_filters(tags);

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
            for entry in catalog::list_owned_entries(&binding.name, &record.path)? {
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
        Ok(rows)
    }

    fn availability_for(&self, config: &ProjectConfig, name: &str) -> Availability {
        let mut avail = Availability::default();
        let project_skill = skill::skill_dir(&self.skills_root(config), name);
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

    /// Install a Catalog Entry into the Managed Project (copy + Lock).
    pub fn add_skill(
        &self,
        name: &str,
        library_filter: Option<&str>,
    ) -> Result<(AddOutcome, Option<String>)> {
        skill::validate_skill_name(name)?;
        let config = self.require_managed()?;
        let rows = self.list_skills(library_filter, &[], false)?;
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

        let dest = skill::skill_dir(&self.skills_root(&config), name);
        let outcome = if dest.join("SKILL.md").is_file() {
            let existing = skill::content_hash(&dest)?;
            if existing == row.entry.content_hash {
                let mut lock = ProjectLock::load(&self.paths.cwd)?;
                let install = InstallationLock {
                    library: row.entry.library.clone(),
                    entry_kind: EntryKind::Owned,
                    content_hash: row.entry.content_hash.clone(),
                    source: None,
                };
                lock.installations
                    .insert(name.to_string(), install.clone());
                lock.save(&self.paths.cwd)?;
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
        Ok((outcome, warn))
    }

    /// Add every Catalog Entry matching tag filters.
    pub fn add_skills_by_tag(
        &self,
        tags: &[String],
        library_filter: Option<&str>,
    ) -> Result<Vec<(String, AddOutcome, Option<String>)>> {
        let rows = self.list_skills(library_filter, tags, true)?;
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
        };
        let mut lock = ProjectLock::load(&self.paths.cwd)?;
        lock.installations.insert(name.to_string(), install.clone());
        lock.save(&self.paths.cwd)?;
        Ok(install)
    }

    fn writable_library_path(&self, library_name: &str) -> Result<(ProjectConfig, PathBuf)> {
        let config = self.require_managed()?;
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
        let config = self.require_managed()?;
        let skills_root = self.skills_root(&config);
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

        let project_skill = skill::skill_dir(&self.skills_root(&config), name);
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
        };
        let mut lock = ProjectLock::load(&self.paths.cwd)?;
        lock.installations.insert(name.to_string(), install.clone());
        lock.save(&self.paths.cwd)?;
        Ok(install)
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
        let project_skill = skill::skill_dir(&self.skills_root(&config), name);
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
        });
        meta.save(&lib_path)?;

        let hash = skill::content_hash(&project_skill)?;

        let install = InstallationLock {
            library: library_name.to_string(),
            entry_kind: EntryKind::Upstream,
            content_hash: hash,
            source: Some(source),
        };
        let mut lock = ProjectLock::load(&self.paths.cwd)?;
        lock.installations.insert(name.to_string(), install.clone());
        lock.save(&self.paths.cwd)?;
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
            .list_skills(None, &["elixir".into()], false)
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].entry.name, "demo-skill");
        assert!(!listed[0].availability.project);

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
}
