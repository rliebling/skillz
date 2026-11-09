//! Application command API for skillz.
//!
//! CLI and future TUI share this seam; domain behaviour lands here in later
//! tickets. Scaffold only: paths + a `status` stub for the fixture harness.

use std::path::{Path, PathBuf};

use thiserror::Error;

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

    /// Ensure the config directory exists (used by later registry commands).
    pub fn ensure_config_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.paths.config_dir).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create config dir {}: {e}",
                self.paths.config_dir.display()
            ))
        })?;
        Ok(())
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
    struct Fixture {
        _root: tempfile::TempDir,
        pub config_dir: PathBuf,
        pub project_dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().expect("tempdir");
            let config_dir = root.path().join("config");
            let project_dir = root.path().join("project");
            fs::create_dir_all(&config_dir).unwrap();
            fs::create_dir_all(&project_dir).unwrap();
            Self {
                _root: root,
                config_dir,
                project_dir,
            }
        }

        fn app(&self) -> App {
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
}
