//! On-disk Library skeleton (`library.toml` + `skills/`).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Result, SkillzError};

pub const LIBRARY_TOML: &str = "library.toml";
pub const SKILLS_DIR: &str = "skills";
/// Current `library.toml` schema version written by skillz.
pub const LIBRARY_META_VERSION: u32 = 1;

fn default_library_version() -> u32 {
    LIBRARY_META_VERSION
}

/// Upstream Catalog Entry recorded in `library.toml` (reference, not Owned tree).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamEntry {
    pub name: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Versioned Library metadata stored in the Library repo.
///
/// Registry Binding name is authoritative; `library.toml` does not carry `name`.
/// Legacy files with `name` and/or missing `version` still load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryMeta {
    #[serde(default = "default_library_version")]
    pub version: u32,
    /// Legacy field from older schemas; accepted on load, never written back.
    #[serde(default, skip_serializing)]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream: Vec<UpstreamEntry>,
}

impl LibraryMeta {
    pub fn new() -> Self {
        Self {
            version: LIBRARY_META_VERSION,
            name: None,
            upstream: Vec::new(),
        }
    }

    pub fn load(library_root: &Path) -> Result<Self> {
        let path = library_root.join(LIBRARY_TOML);
        let text = fs::read_to_string(&path).map_err(|e| {
            SkillzError::Message(format!(
                "failed to read {}: {e}",
                path.display()
            ))
        })?;
        let meta: Self = toml::from_str(&text).map_err(|e| {
            SkillzError::Message(format!("invalid {}: {e}", path.display()))
        })?;
        if meta.version != LIBRARY_META_VERSION {
            return Err(SkillzError::Message(format!(
                "unsupported library.toml version {} at {} (supported: {LIBRARY_META_VERSION})",
                meta.version,
                path.display()
            )));
        }
        Ok(meta)
    }

    pub fn save(&self, library_root: &Path) -> Result<()> {
        let path = library_root.join(LIBRARY_TOML);
        // Always persist current schema (drop legacy `name`, pin version).
        let to_write = Self {
            version: LIBRARY_META_VERSION,
            name: None,
            upstream: self.upstream.clone(),
        };
        let text = toml::to_string_pretty(&to_write).map_err(|e| {
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

impl Default for LibraryMeta {
    fn default() -> Self {
        Self::new()
    }
}

/// Immediate child directories of `path` that contain `SKILL.md` (excluding `skills/`).
pub fn root_level_skill_dirs(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| {
        SkillzError::Message(format!("failed to read {}: {e}", path.display()))
    })? {
        let entry = entry.map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", path.display()))
        })?;
        let child = entry.path();
        if !child.is_dir() {
            continue;
        }
        if entry.file_name() == SKILLS_DIR {
            continue;
        }
        if child.join("SKILL.md").is_file() {
            out.push(child);
        }
    }
    out.sort();
    Ok(out)
}

fn move_root_skills_into_skills_dir(path: &Path, root_skills: &[PathBuf]) -> Result<()> {
    let skills = path.join(SKILLS_DIR);
    fs::create_dir_all(&skills).map_err(|e| {
        SkillzError::Message(format!(
            "failed to create skills dir {}: {e}",
            skills.display()
        ))
    })?;
    for src in root_skills {
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                SkillzError::Message(format!(
                    "skill directory has non-utf8 name: {}",
                    src.display()
                ))
            })?;
        let dest = skills.join(name);
        if dest.exists() {
            return Err(SkillzError::Message(format!(
                "cannot move `{}` into skills/: `{}` already exists",
                src.display(),
                dest.display()
            )));
        }
        fs::rename(src, &dest).map_err(|e| {
            SkillzError::Message(format!(
                "failed to move {} → {}: {e}",
                src.display(),
                dest.display()
            ))
        })?;
    }
    Ok(())
}

/// Create or adopt a Library skeleton at `path`.
///
/// - Refuses if `library.toml` already exists (use `library register`).
/// - Adopts an existing `skills/*/SKILL.md` layout without moving.
/// - Root-level `*/SKILL.md` directories require `move_root_skills` to relocate under `skills/`.
pub fn init_library_skeleton(path: &Path, move_root_skills: bool) -> Result<()> {
    if path.join(LIBRARY_TOML).is_file() {
        return Err(SkillzError::Message(format!(
            "library already exists at {} (found library.toml); use `skillz library register` to add it to this machine's registry",
            path.display()
        )));
    }

    if path.exists() {
        if !path.is_dir() {
            return Err(SkillzError::Message(format!(
                "library path is not a directory: {}",
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

    let root_skills = root_level_skill_dirs(path)?;
    if !root_skills.is_empty() {
        if !move_root_skills {
            let names: Vec<_> = root_skills
                .iter()
                .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
                .collect();
            return Err(SkillzError::Message(format!(
                "found skill directories at the Library root ({}); pass `--move` to relocate them under `skills/`, or rearrange them first",
                names.join(", ")
            )));
        }
        move_root_skills_into_skills_dir(path, &root_skills)?;
    }

    let skills = path.join(SKILLS_DIR);
    fs::create_dir_all(&skills).map_err(|e| {
        SkillzError::Message(format!(
            "failed to create skills dir {}: {e}",
            skills.display()
        ))
    })?;

    LibraryMeta::new().save(path)?;
    Ok(())
}

/// Load an existing Library (`library.toml` must already be present).
pub fn ensure_library_meta(path: &Path) -> Result<LibraryMeta> {
    if !path.is_dir() {
        return Err(SkillzError::Message(format!(
            "library path does not exist: {}",
            path.display()
        )));
    }
    if !path.join(LIBRARY_TOML).is_file() {
        return Err(SkillzError::Message(format!(
            "no library.toml at {}; run `skillz library init` to create a Library, then register — `library register` does not invent metadata",
            path.display()
        )));
    }
    LibraryMeta::load(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_writes_version_omits_name() {
        let dir = TempDir::new().unwrap();
        LibraryMeta::new().save(dir.path()).unwrap();
        let text = fs::read_to_string(dir.path().join(LIBRARY_TOML)).unwrap();
        assert!(text.contains("version = 1"));
        assert!(!text.contains("name"));
    }

    #[test]
    fn load_migrates_legacy_name_and_missing_version() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(LIBRARY_TOML),
            "name = \"legacy\"\n\n[[upstream]]\nname = \"demo\"\nsource = \"github:o/r/demo@abc\"\n",
        )
        .unwrap();
        let meta = LibraryMeta::load(dir.path()).unwrap();
        assert_eq!(meta.version, 1);
        assert_eq!(meta.upstream.len(), 1);
        meta.save(dir.path()).unwrap();
        let text = fs::read_to_string(dir.path().join(LIBRARY_TOML)).unwrap();
        assert!(text.contains("version = 1"));
        assert!(!text.contains("name = \"legacy\""));
    }

    #[test]
    fn load_rejects_unsupported_version() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(LIBRARY_TOML), "version = 99\n").unwrap();
        let err = LibraryMeta::load(dir.path()).unwrap_err();
        assert!(err.to_string().contains("unsupported library.toml version 99"));
    }

    #[test]
    fn init_adopts_skills_layout_without_move() {
        let dir = TempDir::new().unwrap();
        let skill = dir.path().join("skills/demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: demo\n---\n").unwrap();
        init_library_skeleton(dir.path(), false).unwrap();
        assert!(dir.path().join(LIBRARY_TOML).is_file());
        assert!(skill.join("SKILL.md").is_file());
    }

    #[test]
    fn init_requires_move_for_root_skills() {
        let dir = TempDir::new().unwrap();
        let skill = dir.path().join("demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: demo\n---\n").unwrap();
        let err = init_library_skeleton(dir.path(), false).unwrap_err();
        assert!(err.to_string().contains("--move"));
        init_library_skeleton(dir.path(), true).unwrap();
        assert!(dir.path().join("skills/demo/SKILL.md").is_file());
        assert!(!skill.exists());
    }

    #[test]
    fn init_refuses_existing_library_toml() {
        let dir = TempDir::new().unwrap();
        LibraryMeta::new().save(dir.path()).unwrap();
        let err = init_library_skeleton(dir.path(), false).unwrap_err();
        assert!(err.to_string().contains("library register"));
    }

    #[test]
    fn ensure_requires_existing_meta() {
        let dir = TempDir::new().unwrap();
        let err = ensure_library_meta(dir.path()).unwrap_err();
        assert!(err.to_string().contains("no library.toml"));
        LibraryMeta::new().save(dir.path()).unwrap();
        ensure_library_meta(dir.path()).unwrap();
    }
}
