//! Best-effort sibling skill dependency heuristics.

use std::fs;
use std::path::Path;

use crate::{Result, SkillzError};

/// Detect likely sibling skill names referenced by a skill body.
///
/// Heuristics (best-effort): YAML `depends` / `skills` frontmatter lists,
/// markdown links to `../name/` or `/name`, and bare sibling directory names
/// appearing as `/skill-name` tokens in the body.
pub fn detect_sibling_deps(skill_path: &Path, known_siblings: &[String]) -> Result<Vec<String>> {
    let skill_md = skill_path.join("SKILL.md");
    if !skill_md.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&skill_md).map_err(|e| {
        SkillzError::Message(format!("failed to read {}: {e}", skill_md.display()))
    })?;
    let self_name = skill_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut found = Vec::new();
    push_unique(&mut found, parse_frontmatter_deps(&text));

    for sib in known_siblings {
        if sib == &self_name {
            continue;
        }
        if references_sibling(&text, sib) {
            push_unique(&mut found, vec![sib.clone()]);
        }
    }
    Ok(found)
}

fn parse_frontmatter_deps(text: &str) -> Vec<String> {
    let Some(rest) = text.strip_prefix("---") else {
        return Vec::new();
    };
    let Some(end) = rest.find("\n---") else {
        return Vec::new();
    };
    let fm = &rest[..end];
    let mut out = Vec::new();
    for key in ["depends", "dependencies", "skills", "requires"] {
        for line in fm.lines() {
            let line = line.trim();
            let Some(value) = line.strip_prefix(&format!("{key}:")) else {
                continue;
            };
            let value = value.trim();
            if let Some(inner) = value.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                for part in inner.split(',') {
                    let t = part.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !t.is_empty() {
                        out.push(t);
                    }
                }
            }
        }
    }
    out
}

fn references_sibling(text: &str, name: &str) -> bool {
    let patterns = [
        format!("/{name}"),
        format!("/{name}/"),
        format!("../{name}"),
        format!("../{name}/"),
        format!("]({name})"),
        format!("](./{name}"),
        format!("`{name}`"),
        format!("\"{name}\""),
    ];
    patterns.iter().any(|p| text.contains(p))
}

fn push_unique(out: &mut Vec<String>, items: Vec<String>) {
    for item in items {
        if !out.iter().any(|x| x == &item) {
            out.push(item);
        }
    }
}

/// Find retained skills that appear to depend on `target_name` (reverse of
/// [`detect_sibling_deps`]).
pub fn find_dependants(
    skills_root: &Path,
    target_name: &str,
    retained_names: &[String],
) -> Result<Vec<String>> {
    let mut dependants = Vec::new();
    let known = vec![target_name.to_string()];
    for name in retained_names {
        if name == target_name {
            continue;
        }
        let path = skills_root.join(name);
        if !path.join("SKILL.md").is_file() {
            continue;
        }
        let deps = detect_sibling_deps(&path, &known)?;
        if deps.iter().any(|d| d == target_name) {
            push_unique(&mut dependants, vec![name.clone()]);
        }
    }
    dependants.sort();
    Ok(dependants)
}

pub fn list_skill_names(skills_root: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    if !skills_root.is_dir() {
        return Ok(names);
    }
    for entry in fs::read_dir(skills_root).map_err(|e| {
        SkillzError::Message(format!(
            "failed to read {}: {e}",
            skills_root.display()
        ))
    })? {
        let entry = entry.map_err(|e| {
            SkillzError::Message(format!(
                "failed to read {}: {e}",
                skills_root.display()
            ))
        })?;
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
            && entry.path().join("SKILL.md").is_file()
        {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_grill_with_docs_siblings() {
        let root = tempfile::tempdir().unwrap();
        let skill = root.path().join("grill-with-docs");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            r#"---
name: grill-with-docs
depends: [grilling, domain-modeling]
---

See `/grilling` and `../domain-modeling/` for prerequisites.
"#,
        )
        .unwrap();
        let deps = detect_sibling_deps(
            &skill,
            &[
                "grilling".into(),
                "domain-modeling".into(),
                "unrelated".into(),
            ],
        )
        .unwrap();
        assert!(deps.contains(&"grilling".to_string()));
        assert!(deps.contains(&"domain-modeling".to_string()));
        assert!(!deps.contains(&"unrelated".to_string()));
    }

    #[test]
    fn find_dependants_respects_retained_set() {
        let root = tempfile::tempdir().unwrap();
        let grilling = root.path().join("grilling");
        let docs = root.path().join("grill-with-docs");
        let other = root.path().join("other");
        fs::create_dir_all(&grilling).unwrap();
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(grilling.join("SKILL.md"), "---\nname: grilling\n---\n").unwrap();
        fs::write(
            docs.join("SKILL.md"),
            "---\nname: grill-with-docs\ndepends: [grilling]\n---\n",
        )
        .unwrap();
        fs::write(other.join("SKILL.md"), "---\nname: other\n---\n").unwrap();

        let retained = vec!["grill-with-docs".into(), "other".into()];
        let deps = find_dependants(root.path(), "grilling", &retained).unwrap();
        assert_eq!(deps, vec!["grill-with-docs".to_string()]);

        // Both queued for remove → no warn about the pair.
        let retained_neither = vec!["other".into()];
        let deps = find_dependants(root.path(), "grilling", &retained_neither).unwrap();
        assert!(deps.is_empty());
    }
}
