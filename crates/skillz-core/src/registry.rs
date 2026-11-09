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
}
