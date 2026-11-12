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

fn format_tags_line(tags: &[String]) -> String {
    format!(
        "tags: [{}]",
        tags.iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Replace or insert the `tags:` line in SKILL.md frontmatter; returns the new file text.
pub fn set_tags_in_frontmatter(text: &str, tags: &[String]) -> Result<String> {
    let tags_line = format_tags_line(tags);
    if !text.starts_with("---") {
        return Ok(format!("---\n{tags_line}\n---\n{text}"));
    }
    let rest = text.strip_prefix("---").unwrap();
    let Some(end) = rest.find("\n---") else {
        return Err(SkillzError::Message(
            "SKILL.md frontmatter is not closed with ---".into(),
        ));
    };
    let fm = &rest[..end];
    let body = &rest[end + 4..]; // skip "\n---"
    let mut new_fm = String::new();
    let mut replaced = false;
    for line in fm.lines() {
        if line.trim().starts_with("tags:") {
            if !replaced {
                new_fm.push_str(&tags_line);
                new_fm.push('\n');
                replaced = true;
            }
        } else {
            new_fm.push_str(line);
            new_fm.push('\n');
        }
    }
    if !replaced {
        new_fm.push_str(&tags_line);
        new_fm.push('\n');
    }
    Ok(format!("---\n{new_fm}---{body}"))
}

/// Merge `incoming` tags into SKILL.md frontmatter (additive). Returns the merged tag list.
pub fn merge_tags_into_frontmatter(skill_md: &Path, incoming: &[String]) -> Result<Vec<String>> {
    let incoming = normalize_tag_filters(incoming);
    if incoming.is_empty() {
        return read_tags(skill_md);
    }
    let text = fs::read_to_string(skill_md).map_err(|e| {
        SkillzError::Message(format!("failed to read {}: {e}", skill_md.display()))
    })?;
    let merged = merge_tags(&parse_tags_from_frontmatter(&text), &incoming);
    let new_text = set_tags_in_frontmatter(&text, &merged)?;
    if new_text != text {
        fs::write(skill_md, new_text).map_err(|e| {
            SkillzError::Message(format!("failed to write {}: {e}", skill_md.display()))
        })?;
    }
    Ok(merged)
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
    fn set_and_merge_tags_in_frontmatter() {
        let text = "---\nname: x\n---\n# body\n";
        let with_tags = set_tags_in_frontmatter(text, &["process".into()]).unwrap();
        assert_eq!(
            parse_tags_from_frontmatter(&with_tags),
            vec!["process".to_string()]
        );
        assert!(with_tags.contains("# body"));

        let dir = tempfile::tempdir().unwrap();
        let md = dir.path().join("SKILL.md");
        fs::write(&md, "---\nname: x\ntags: [\"a\"]\n---\n").unwrap();
        let merged = merge_tags_into_frontmatter(&md, &["b".into(), "a".into()]).unwrap();
        assert_eq!(merged, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(read_tags(&md).unwrap(), merged);
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
