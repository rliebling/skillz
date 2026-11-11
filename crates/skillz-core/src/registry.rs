//! User-global Library registry (`~/.config/skillz/registry.toml`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Result, SkillzError};

/// One registered Library: local path plus optional read-only flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryRecord {
    pub path: PathBuf,
    #[serde(default)]
    pub read_only: bool,
}

/// Machine-local registry of Libraries (and later Managed Project paths).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub libraries: BTreeMap<String, LibraryRecord>,
    /// Managed Project absolute paths (used by Drift; filled by later tickets).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<PathBuf>,
}

impl Registry {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).map_err(|e| {
            SkillzError::Message(format!("failed to read registry {}: {e}", path.display()))
        })?;
        toml::from_str(&text).map_err(|e| {
            SkillzError::Message(format!("invalid registry {}: {e}", path.display()))
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SkillzError::Message(format!(
                    "failed to create registry dir {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| {
            SkillzError::Message(format!("failed to serialize registry: {e}"))
        })?;
        fs::write(path, text).map_err(|e| {
            SkillzError::Message(format!("failed to write registry {}: {e}", path.display()))
        })?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&LibraryRecord> {
        self.libraries.get(name)
    }

    pub fn insert(&mut self, name: String, record: LibraryRecord) -> Result<()> {
        if self.libraries.contains_key(&name) {
            return Err(SkillzError::Message(format!(
                "library `{name}` is already registered"
            )));
        }
        self.libraries.insert(name, record);
        Ok(())
    }

    /// Find the registered Library whose root contains `path` (longest match wins).
    pub fn library_covering_path(&self, path: &Path) -> Option<(String, LibraryRecord)> {
        let target = canonicalize_or_clone(path);
        let mut best: Option<(String, LibraryRecord, usize)> = None;
        for (name, rec) in &self.libraries {
            let lib = canonicalize_or_clone(&rec.path);
            if path_is_within(&target, &lib) {
                let len = lib.as_os_str().len();
                let replace = match &best {
                    None => true,
                    Some((_, _, best_len)) => len > *best_len,
                };
                if replace {
                    best = Some((name.clone(), rec.clone(), len));
                }
            }
        }
        best.map(|(name, rec, _)| (name, rec))
    }
}

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_registry_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.toml");
        let mut reg = Registry::default();
        reg.insert(
            "personal".into(),
            LibraryRecord {
                path: PathBuf::from("/tmp/personal"),
                read_only: false,
            },
        )
        .unwrap();
        reg.save(&path).unwrap();
        let loaded = Registry::load(&path).unwrap();
        assert_eq!(loaded, reg);
    }

    #[test]
    fn library_covering_path_prefers_longest_match() {
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().join("libs");
        let inner = outer.join("personal");
        std::fs::create_dir_all(inner.join("skills")).unwrap();
        let mut reg = Registry::default();
        reg.insert(
            "libs".into(),
            LibraryRecord {
                path: outer.clone(),
                read_only: false,
            },
        )
        .unwrap();
        reg.insert(
            "personal".into(),
            LibraryRecord {
                path: inner.clone(),
                read_only: false,
            },
        )
        .unwrap();
        let (name, _) = reg
            .library_covering_path(&inner.join("skills"))
            .expect("cover");
        assert_eq!(name, "personal");
        assert!(reg.library_covering_path(&dir.path().join("other")).is_none());
    }
}
