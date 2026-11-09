//! Managed Project config and lock files under `.skillz/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::registry::LibraryRecord;
use crate::vcs::{infer_vcs, Vcs};
use crate::{Result, SkillzError};

pub const SKILLZ_DIR: &str = ".skillz";
pub const SKILLZ_TOML: &str = "skillz.toml";
pub const SKILLZ_LOCK: &str = "skillz.lock";
pub const DEFAULT_SKILLS_DIR: &str = ".agents/skills";

/// One Library Binding recorded in a Managed Project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub read_only: bool,
}

/// Versioned Managed Project config (`PROJECT/.skillz/skillz.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default = "default_skills_dir")]
    pub skills_dir: String,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

fn default_skills_dir() -> String {
    DEFAULT_SKILLS_DIR.to_string()
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            skills_dir: default_skills_dir(),
            bindings: Vec::new(),
        }
    }
}

impl ProjectConfig {
    pub fn path(project_root: &Path) -> PathBuf {
        project_root.join(SKILLZ_DIR).join(SKILLZ_TOML)
    }

    pub fn load(project_root: &Path) -> Result<Self> {
        let path = Self::path(project_root);
        let text = fs::read_to_string(&path).map_err(|e| {
            SkillzError::Message(format!(
                "failed to read {}: {e}",
                path.display()
            ))
        })?;
        toml::from_str(&text).map_err(|e| {
            SkillzError::Message(format!("invalid {}: {e}", path.display()))
        })
    }

    pub fn save(&self, project_root: &Path) -> Result<()> {
        let dir = project_root.join(SKILLZ_DIR);
        fs::create_dir_all(&dir).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create {}: {e}",
                dir.display()
            ))
        })?;
        let path = Self::path(project_root);
        let text = toml::to_string_pretty(self).map_err(|e| {
            SkillzError::Message(format!("failed to serialize skillz.toml: {e}"))
        })?;
        fs::write(&path, text).map_err(|e| {
            SkillzError::Message(format!("failed to write {}: {e}", path.display()))
        })?;
        Ok(())
    }
}

/// Lock file for Installations (`PROJECT/.skillz/skillz.lock`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectLock {
    /// Map of skill name → lock records (empty until later tickets).
    #[serde(default)]
    pub installations: std::collections::BTreeMap<String, toml::Value>,
}

impl ProjectLock {
    pub fn path(project_root: &Path) -> PathBuf {
        project_root.join(SKILLZ_DIR).join(SKILLZ_LOCK)
    }

    pub fn load(project_root: &Path) -> Result<Self> {
        let path = Self::path(project_root);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path).map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", path.display()))
        })?;
        toml::from_str(&text).map_err(|e| {
            SkillzError::Message(format!("invalid {}: {e}", path.display()))
        })
    }

    pub fn save(&self, project_root: &Path) -> Result<()> {
        let dir = project_root.join(SKILLZ_DIR);
        fs::create_dir_all(&dir).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create {}: {e}",
                dir.display()
            ))
        })?;
        let path = Self::path(project_root);
        let text = toml::to_string_pretty(self).map_err(|e| {
            SkillzError::Message(format!("failed to serialize skillz.lock: {e}"))
        })?;
        fs::write(&path, text).map_err(|e| {
            SkillzError::Message(format!("failed to write {}: {e}", path.display()))
        })?;
        Ok(())
    }

    pub fn installation_names(&self) -> Vec<String> {
        self.installations.keys().cloned().collect()
    }
}

/// Parse `--bind name` or `--bind name:ro` tokens.
pub fn parse_bind_spec(spec: &str) -> Result<(String, bool)> {
    let spec = spec.trim();
    if let Some(name) = spec.strip_suffix(":ro") {
        if name.is_empty() {
            return Err(SkillzError::Message(
                "invalid --bind value: missing library name before `:ro`".into(),
            ));
        }
        return Ok((name.to_string(), true));
    }
    if let Some(name) = spec.strip_suffix(":rw") {
        if name.is_empty() {
            return Err(SkillzError::Message(
                "invalid --bind value: missing library name before `:rw`".into(),
            ));
        }
        return Ok((name.to_string(), false));
    }
    if spec.is_empty() {
        return Err(SkillzError::Message(
            "invalid --bind value: empty".into(),
        ));
    }
    Ok((spec.to_string(), false))
}

/// Resolve a Library's stable remote URL for a Binding.
pub fn library_remote_url(library_path: &Path) -> Result<String> {
    match infer_vcs(library_path) {
        Some(Vcs::Jj) => {
            if let Some(url) = jj_origin_url(library_path)? {
                return Ok(url);
            }
        }
        Some(Vcs::Git) => {
            if let Some(url) = git_origin_url(library_path)? {
                return Ok(url);
            }
        }
        None => {}
    }
    // Local-only Libraries: stable file URL from absolute path.
    let abs = fs::canonicalize(library_path).unwrap_or_else(|_| library_path.to_path_buf());
    Ok(format!("file://{}", abs.display()))
}

fn git_origin_url(path: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| SkillzError::Message(format!("failed to run git: {e}")))?;
    if !output.status.success() {
        return Ok(None);
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() {
        Ok(None)
    } else {
        Ok(Some(url))
    }
}

fn jj_origin_url(path: &Path) -> Result<Option<String>> {
    let output = Command::new("jj")
        .args(["-R"])
        .arg(path)
        .args(["git", "remote", "list"])
        .output()
        .map_err(|e| SkillzError::Message(format!("failed to run jj: {e}")))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap_or("");
        let url = parts.next().unwrap_or("");
        if name == "origin" && !url.is_empty() {
            return Ok(Some(url.to_string()));
        }
    }
    // Fall back to first remote if origin missing.
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let _name = parts.next();
        if let Some(url) = parts.next() {
            if !url.is_empty() {
                return Ok(Some(url.to_string()));
            }
        }
    }
    Ok(None)
}

/// Build Bindings from `--bind` specs using the local registry.
pub fn bindings_from_specs(
    specs: &[String],
    libraries: &std::collections::BTreeMap<String, LibraryRecord>,
) -> Result<Vec<Binding>> {
    if specs.is_empty() {
        return Err(SkillzError::Message(
            "at least one --bind is required".into(),
        ));
    }
    let mut bindings = Vec::with_capacity(specs.len());
    let mut seen = std::collections::BTreeSet::new();
    for spec in specs {
        let (name, flag_ro) = parse_bind_spec(spec)?;
        if !seen.insert(name.clone()) {
            return Err(SkillzError::Message(format!(
                "duplicate binding `{name}`"
            )));
        }
        let Some(record) = libraries.get(&name) else {
            return Err(SkillzError::Message(format!(
                "library `{name}` is not in the local registry; run `skillz library add` or `skillz library init` first"
            )));
        };
        let read_only = flag_ro || record.read_only;
        let url = library_remote_url(&record.path)?;
        bindings.push(Binding {
            name,
            url,
            read_only,
        });
    }
    Ok(bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bind_ro_and_default() {
        assert_eq!(
            parse_bind_spec("personal").unwrap(),
            ("personal".into(), false)
        );
        assert_eq!(
            parse_bind_spec("company:ro").unwrap(),
            ("company".into(), true)
        );
        assert_eq!(
            parse_bind_spec("team:rw").unwrap(),
            ("team".into(), false)
        );
    }
}
