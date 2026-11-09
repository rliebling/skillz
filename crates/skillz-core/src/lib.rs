//! Application command API for skillz.
//!
//! CLI and future TUI share this seam.

use std::path::{Path, PathBuf};

use thiserror::Error;

pub mod library;
pub mod registry;
pub mod vcs;

pub use library::{ensure_library_meta, init_library_skeleton, LibraryMeta};
pub use registry::{LibraryRecord, Registry};
pub use vcs::{infer_vcs, Vcs};

/// Filesystem roots skillz resolves against for a given invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillzPaths {
    /// Overridden home / config root (tests inject a temp dir).
    pub config_dir: PathBuf,
    /// Current working directory used for Project discovery.
    pub cwd: PathBuf,
}

impl SkillzPaths {
    pub fn new(config_dir: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
            cwd: cwd.into(),
        }
    }

    pub fn registry_path(&self) -> PathBuf {
        self.config_dir.join("registry.toml")
    }
}

/// Errors returned by the application command API.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SkillzError {
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, SkillzError>;

/// Snapshot returned by [`App::status`] (stub until Managed Project work).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    pub cwd: PathBuf,
    pub config_dir: PathBuf,
    pub managed: bool,
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

    /// Stub status: reports whether a Managed Project config exists.
    pub fn status(&self) -> Result<StatusReport> {
        let managed = self
            .paths
            .cwd
            .join(".skillz")
            .join("skillz.toml")
            .is_file();
        let message = if managed {
            "Managed Project (config present; domain status not implemented yet)".to_string()
        } else {
            "Not a Managed Project (no .skillz/skillz.toml)".to_string()
        };
        Ok(StatusReport {
            cwd: self.paths.cwd.clone(),
            config_dir: self.paths.config_dir.clone(),
            managed,
            message,
        })
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
    }

    impl Fixture {
        pub fn new() -> Self {
            let root = tempfile::tempdir().expect("tempdir");
            let config_dir = root.path().join("config");
            let project_dir = root.path().join("project");
            let libs_dir = root.path().join("libs");
            fs::create_dir_all(&config_dir).unwrap();
            fs::create_dir_all(&project_dir).unwrap();
            fs::create_dir_all(&libs_dir).unwrap();
            Self {
                _root: root,
                config_dir,
                project_dir,
                libs_dir,
            }
        }

        pub fn app(&self) -> App {
            App::new(SkillzPaths::new(&self.config_dir, &self.project_dir))
        }
    }

    #[test]
    fn status_reports_unmanaged_project_in_fixture() {
        let fx = Fixture::new();
        let report = fx.app().status().expect("status");
        assert!(!report.managed);
        assert_eq!(report.cwd, fx.project_dir);
        assert_eq!(report.config_dir, fx.config_dir);
        assert!(report.message.contains("Not a Managed Project"));
    }

    #[test]
    fn status_detects_managed_config_file() {
        let fx = Fixture::new();
        let cfg = fx.project_dir.join(".skillz");
        fs::create_dir_all(&cfg).unwrap();
        fs::write(cfg.join("skillz.toml"), "skills_dir = \".agents/skills\"\n").unwrap();

        let report = fx.app().status().expect("status");
        assert!(report.managed);
        assert!(report.message.contains("Managed Project"));
    }

    #[test]
    fn ensure_config_dir_creates_registry_parent() {
        let fx = Fixture::new();
        let nested = fx.config_dir.join("nested");
        let app = App::new(SkillzPaths::new(&nested, &fx.project_dir));
        app.ensure_config_dir().unwrap();
        assert!(nested.is_dir());
        assert_eq!(app.paths().registry_path(), nested.join("registry.toml"));
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
