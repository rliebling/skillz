//! Catalog discovery over Library Owned Entries.

use std::fs;
use std::path::{Path, PathBuf};

use crate::library::SKILLS_DIR;
use crate::skill::{self, EntryKind};
use crate::{Result, SkillzError};

/// A Catalog Entry discovered in a bound Library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: String,
    pub library: String,
    pub library_path: PathBuf,
    pub skill_path: PathBuf,
    pub tags: Vec<String>,
    pub content_hash: String,
    pub kind: EntryKind,
    /// Upstream source string when [`kind`] is [`EntryKind::Upstream`].
    pub source: Option<String>,
}

/// Where a skill's files are already present for the current scope.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Availability {
    pub project: bool,
    pub user: bool,
    pub other_roots: Vec<String>,
}

impl Availability {
    pub fn is_empty(&self) -> bool {
        !self.project && !self.user && self.other_roots.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.project {
            parts.push("project".to_string());
        }
        if self.user {
            parts.push("user".to_string());
        }
        for root in &self.other_roots {
            parts.push(root.clone());
        }
        if parts.is_empty() {
            "-".into()
        } else {
            parts.join(",")
        }
    }
}

/// List Owned Catalog Entries under a Library's `skills/` tree.
pub fn list_owned_entries(library_name: &str, library_path: &Path) -> Result<Vec<CatalogEntry>> {
    let skills = library_path.join(SKILLS_DIR);
    if !skills.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&skills).map_err(|e| {
        SkillzError::Message(format!("failed to read {}: {e}", skills.display()))
    })? {
        let entry = entry.map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", skills.display()))
        })?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let skill_path = entry.path();
        if !skill_path.join("SKILL.md").is_file() {
            continue;
        }
        let tags = read_tags(&skill_path.join("SKILL.md"))?;
        let content_hash = skill::content_hash(&skill_path)?;
        entries.push(CatalogEntry {
            name,
            library: library_name.to_string(),
            library_path: library_path.to_path_buf(),
            skill_path,
            tags,
            content_hash,
            kind: EntryKind::Owned,
            source: None,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Best-effort frontmatter tags from SKILL.md (`tags: [a, b]` or `tags: ["a"]`).
pub fn read_tags(skill_md: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(skill_md).map_err(|e| {
        SkillzError::Message(format!("failed to read {}: {e}", skill_md.display()))
    })?;
    Ok(parse_tags_from_frontmatter(&text))
}

fn parse_tags_from_frontmatter(text: &str) -> Vec<String> {
    let Some(rest) = text.strip_prefix("---") else {
        return Vec::new();
    };
    let Some(end) = rest.find("\n---") else {
        return Vec::new();
    };
    let fm = &rest[..end];
    for line in fm.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("tags:") else {
            continue;
        };
        let value = value.trim();
        if let Some(inner) = value.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            return inner
                .split(',')
                .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Vec::new()
}

/// Expand `--tag` args that may include comma-separated lists; AND semantics.
pub fn normalize_tag_filters(raw: &[String]) -> Vec<String> {
    let mut tags = Vec::new();
    for item in raw {
        for part in item.split(',') {
            let t = part.trim();
            if !t.is_empty() && !tags.iter().any(|x| x == t) {
                tags.push(t.to_string());
            }
        }
    }
    tags
}

pub fn entry_matches_tags(entry: &CatalogEntry, required: &[String]) -> bool {
    required.iter().all(|t| entry.tags.iter().any(|e| e == t))
}

/// Case-insensitive substring match against name and tags (general search).
pub fn entry_matches_search(entry: &CatalogEntry, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    if entry.name.to_lowercase().contains(&q) {
        return true;
    }
    entry.tags.iter().any(|t| t.to_lowercase().contains(&q))
}

/// Merge `incoming` into `existing` (union, preserving existing order then new).
pub fn merge_tags(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut out = existing.to_vec();
    for tag in normalize_tag_filters(incoming) {
        if !out.iter().any(|t| t == &tag) {
            out.push(tag);
        }
    }
    out
}

/// List Upstream Catalog Entries from `library.toml`.
pub fn list_upstream_entries(
    library_name: &str,
    library_path: &Path,
) -> Result<Vec<CatalogEntry>> {
    use crate::library::LibraryMeta;
    let meta_path = library_path.join(crate::library::LIBRARY_TOML);
    if !meta_path.is_file() {
        return Ok(Vec::new());
    }
    let meta = LibraryMeta::load(library_path)?;
    let mut entries = Vec::new();
    for up in meta.upstream {
        // Upstream is a reference — no Owned tree. Use a stable sentinel path.
        let skill_path = library_path.join(SKILLS_DIR).join(&up.name);
        entries.push(CatalogEntry {
            name: up.name,
            library: library_name.to_string(),
            library_path: library_path.to_path_buf(),
            skill_path,
            tags: up.tags,
            // Source string identity stands in until Installation is hashed.
            content_hash: format!("upstream:{}", up.source),
            kind: EntryKind::Upstream,
            source: Some(up.source),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tags_and_normalize_filters() {
        let text = "---\nname: x\ntags: [\"elixir\", phoenix]\n---\n";
        assert_eq!(
            parse_tags_from_frontmatter(text),
            vec!["elixir".to_string(), "phoenix".to_string()]
        );
        assert_eq!(
            normalize_tag_filters(&["elixir,phoenix".into(), "otp".into()]),
            vec!["elixir".to_string(), "phoenix".to_string(), "otp".to_string()]
        );
    }

    #[test]
    fn search_and_merge_tags() {
        let entry = CatalogEntry {
            name: "ecto-thinking".into(),
            library: "personal".into(),
            library_path: PathBuf::from("/tmp"),
            skill_path: PathBuf::from("/tmp/skills/ecto-thinking"),
            tags: vec!["elixir".into(), "phoenix".into()],
            content_hash: "abc".into(),
            kind: EntryKind::Owned,
            source: None,
        };
        assert!(entry_matches_search(&entry, "ecto"));
        assert!(entry_matches_search(&entry, "PHOENIX"));
        assert!(!entry_matches_search(&entry, "rust"));
        assert_eq!(
            merge_tags(&["elixir".into()], &["phoenix".into(), "elixir".into()]),
            vec!["elixir".to_string(), "phoenix".to_string()]
        );
    }
}
