//! Skill trees on disk: scaffold, hash, copy.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Result, SkillzError};

/// Recorded Lock for one Installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallationLock {
    pub library: String,
    pub entry_kind: EntryKind,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Owned,
    Upstream,
}

/// Why a skill is considered Drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftReason {
    Unlocked,
    MissingFiles,
    HashMismatch { locked: String, actual: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftItem {
    pub name: String,
    pub reason: DriftReason,
}

pub fn skill_dir(skills_root: &Path, name: &str) -> PathBuf {
    skills_root.join(name)
}

pub fn validate_skill_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(SkillzError::Message("skill name must not be empty".into()));
    }
    if name
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    {
        return Err(SkillzError::Message(format!(
            "invalid skill name `{name}` (use letters, digits, `-`, `_`)"
        )));
    }
    Ok(())
}

/// Scaffold an Owned-ready skill under `skills_root/name/`.
pub fn scaffold_skill(skills_root: &Path, name: &str, tags: &[String]) -> Result<PathBuf> {
    validate_skill_name(name)?;
    let dir = skill_dir(skills_root, name);
    if dir.exists() {
        return Err(SkillzError::Message(format!(
            "skill already exists at {}",
            dir.display()
        )));
    }
    fs::create_dir_all(&dir).map_err(|e| {
        SkillzError::Message(format!("failed to create {}: {e}", dir.display()))
    })?;
    let tags_line = if tags.is_empty() {
        String::new()
    } else {
        format!(
            "tags: [{}]\n",
            tags.iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let body = format!(
        r#"---
name: {name}
description: TODO — describe this skill
{tags_line}---

# {name}

Describe what this skill does and when to use it.
"#
    );
    let skill_md = dir.join("SKILL.md");
    fs::write(&skill_md, body).map_err(|e| {
        SkillzError::Message(format!("failed to write {}: {e}", skill_md.display()))
    })?;
    Ok(dir)
}

/// SHA-256 hex digest of all files under a skill directory (stable path order).
pub fn content_hash(skill_path: &Path) -> Result<String> {
    if !skill_path.is_dir() {
        return Err(SkillzError::Message(format!(
            "skill path is not a directory: {}",
            skill_path.display()
        )));
    }
    let mut files = list_files_recursive(skill_path)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in &files {
        let abs = skill_path.join(rel);
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        let bytes = fs::read(&abs).map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", abs.display()))
        })?;
        hasher.update(&bytes);
        hasher.update([0u8]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

/// Copy a skill tree into a Library Owned Entry path.
pub fn copy_skill_tree(from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        fs::remove_dir_all(to).map_err(|e| {
            SkillzError::Message(format!("failed to replace {}: {e}", to.display()))
        })?;
    }
    fs::create_dir_all(to).map_err(|e| {
        SkillzError::Message(format!("failed to create {}: {e}", to.display()))
    })?;
    let files = list_files_recursive(from)?;
    for rel in files {
        let src = from.join(&rel);
        let dst = to.join(&rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SkillzError::Message(format!(
                    "failed to create {}: {e}",
                    parent.display()
                ))
            })?;
        }
        fs::copy(&src, &dst).map_err(|e| {
            SkillzError::Message(format!(
                "failed to copy {} → {}: {e}",
                src.display(),
                dst.display()
            ))
        })?;
    }
    Ok(())
}

fn list_files_recursive(root: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    list_files_inner(root, root, &mut out)?;
    Ok(out)
}

fn list_files_inner(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| {
        SkillzError::Message(format!("failed to read {}: {e}", dir.display()))
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", dir.display()))
        })?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| {
            SkillzError::Message(format!("failed to stat {}: {e}", path.display()))
        })?;
        if ft.is_dir() {
            list_files_inner(root, &path, out)?;
        } else if ft.is_file() {
            let rel = path.strip_prefix(root).map_err(|_| {
                SkillzError::Message(format!(
                    "path {} is not under {}",
                    path.display(),
                    root.display()
                ))
            })?;
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_changes_with_content() {
        let root = tempfile::tempdir().unwrap();
        let skill = root.path().join("demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "hello").unwrap();
        let h1 = content_hash(&skill).unwrap();
        let h2 = content_hash(&skill).unwrap();
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
        fs::write(skill.join("SKILL.md"), "hello!").unwrap();
        assert_ne!(h1, content_hash(&skill).unwrap());
    }
}
