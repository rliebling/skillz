//! On-disk Library skeleton (`library.toml` + `skills/`).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Result, SkillzError};

pub const LIBRARY_TOML: &str = "library.toml";
pub const SKILLS_DIR: &str = "skills";

/// Upstream Catalog Entry recorded in `library.toml` (reference, not Owned tree).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamEntry {
    pub name: String,
    pub source: String,
}

/// Versioned Library metadata stored in the Library repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryMeta {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream: Vec<UpstreamEntry>,
}

impl LibraryMeta {
    pub fn load(library_root: &Path) -> Result<Self> {
        let path = library_root.join(LIBRARY_TOML);
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

    pub fn save(&self, library_root: &Path) -> Result<()> {
        let path = library_root.join(LIBRARY_TOML);
        let text = toml::to_string_pretty(self).map_err(|e| {
            SkillzError::Message(format!("failed to serialize library.toml: {e}"))
        })?;
        fs::write(&path, text).map_err(|e| {
            SkillzError::Message(format!("failed to write {}: {e}", path.display()))
        })?;
        Ok(())
    }

    pub fn upsert_upstream(&mut self, entry: UpstreamEntry) {
        if let Some(existing) = self.upstream.iter_mut().find(|e| e.name == entry.name) {
            *existing = entry;
        } else {
            self.upstream.push(entry);
        }
    }
}

/// Create a new Library skeleton at `path` with the given name.
pub fn init_library_skeleton(path: &Path, name: &str) -> Result<()> {
    if path.exists() {
        if path.join(LIBRARY_TOML).is_file() {
            return Err(SkillzError::Message(format!(
                "library already exists at {}",
                path.display()
            )));
        }
        if path.read_dir().map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", path.display()))
        })?
        .next()
        .is_some()
        {
            return Err(SkillzError::Message(format!(
                "refusing to init library in non-empty directory {}",
                path.display()
            )));
        }
    } else {
        fs::create_dir_all(path).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create library dir {}: {e}",
                path.display()
            ))
        })?;
    }

    let skills = path.join(SKILLS_DIR);
    fs::create_dir_all(&skills).map_err(|e| {
        SkillzError::Message(format!(
            "failed to create skills dir {}: {e}",
            skills.display()
        ))
    })?;

    LibraryMeta {
        name: name.to_string(),
        upstream: Vec::new(),
    }
    .save(path)?;

    Ok(())
}

/// Ensure an existing checkout looks like a Library (has `library.toml`).
pub fn ensure_library_meta(path: &Path, name: &str) -> Result<LibraryMeta> {
    let meta_path = path.join(LIBRARY_TOML);
    if meta_path.is_file() {
        let meta = LibraryMeta::load(path)?;
        if meta.name != name {
            return Err(SkillzError::Message(format!(
                "library at {} is named `{}`, not `{name}`",
                path.display(),
                meta.name
            )));
        }
        return Ok(meta);
    }
    if !path.is_dir() {
        return Err(SkillzError::Message(format!(
            "library path does not exist: {}",
            path.display()
        )));
    }
    let skills = path.join(SKILLS_DIR);
    if !skills.is_dir() {
        fs::create_dir_all(&skills).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create skills dir {}: {e}",
                skills.display()
            ))
        })?;
    }
    let meta = LibraryMeta {
        name: name.to_string(),
        upstream: Vec::new(),
    };
    meta.save(path)?;
    Ok(meta)
}
