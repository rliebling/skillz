//! Import from Skillfile and adopt local leftovers.

use std::fs;
use std::path::{Path, PathBuf};

use crate::source::GithubSource;
use crate::{Result, SkillzError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillfileEntry {
    pub name: String,
    pub source: String,
    pub owner_repo: String,
    pub path_in_repo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    pub skills_dir: PathBuf,
    pub skillfile: Option<PathBuf>,
    pub skillfile_entries: Vec<SkillfileEntry>,
    pub local_skills: Vec<String>,
    pub leftovers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdoptAs {
    Upstream,
    Authored,
    Skip,
}

pub fn detect_skills_dir(project_root: &Path) -> PathBuf {
    for candidate in [".agents/skills", ".claude/skills", ".cursor/skills"] {
        let p = project_root.join(candidate);
        if p.is_dir() {
            return p;
        }
    }
    project_root.join(".agents/skills")
}

pub fn parse_skillfile(text: &str) -> Result<Vec<SkillfileEntry>> {
    let mut entries = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.first() == Some(&"install") {
            continue;
        }
        if parts.first() != Some(&"github") || parts.get(1) != Some(&"skill") {
            continue;
        }
        // github skill <owner/repo[@ref]> <path> [ref]
        if parts.len() < 4 {
            return Err(SkillzError::Message(format!(
                "Skillfile line {}: expected `github skill <owner/repo> <path>`",
                lineno + 1
            )));
        }
        let owner_repo = parts[2].to_string();
        let path_in_repo = parts[3].to_string();
        let name = path_in_repo
            .rsplit('/')
            .next()
            .unwrap_or(&path_in_repo)
            .to_string();
        let (repo, rev) = match owner_repo.split_once('@') {
            Some((r, rev)) => (r.to_string(), Some(rev.to_string())),
            None => (owner_repo.clone(), parts.get(4).map(|s| s.to_string())),
        };
        let mut source = format!("github:{repo}/{path_in_repo}");
        if let Some(rev) = rev {
            source.push('@');
            source.push_str(&rev);
        }
        // Validate grammar when rev present; without rev catalog form is OK.
        if source.contains('@') {
            GithubSource::parse(&source)?;
        } else {
            let _ = GithubSource::parse(&format!("{source}@placeholder"))?;
        }
        entries.push(SkillfileEntry {
            name,
            source,
            owner_repo: repo,
            path_in_repo,
        });
    }
    Ok(entries)
}

pub fn list_local_skill_names(skills_dir: &Path) -> Result<Vec<String>> {
    if !skills_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(skills_dir).map_err(|e| {
        SkillzError::Message(format!("failed to read {}: {e}", skills_dir.display()))
    })? {
        let entry = entry.map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", skills_dir.display()))
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

pub fn build_plan(project_root: &Path) -> Result<ImportPlan> {
    let skills_dir = detect_skills_dir(project_root);
    let skillfile_path = project_root.join("Skillfile");
    let (skillfile, skillfile_entries) = if skillfile_path.is_file() {
        let text = fs::read_to_string(&skillfile_path).map_err(|e| {
            SkillzError::Message(format!(
                "failed to read {}: {e}",
                skillfile_path.display()
            ))
        })?;
        (Some(skillfile_path), parse_skillfile(&text)?)
    } else {
        (None, Vec::new())
    };
    let local_skills = list_local_skill_names(&skills_dir)?;
    let skillfile_names: std::collections::BTreeSet<_> =
        skillfile_entries.iter().map(|e| e.name.clone()).collect();
    let leftovers = local_skills
        .iter()
        .filter(|n| !skillfile_names.contains(*n))
        .cloned()
        .collect();
    Ok(ImportPlan {
        skills_dir,
        skillfile,
        skillfile_entries,
        local_skills,
        leftovers,
    })
}

pub fn parse_adopt_spec(spec: &str) -> Result<(String, AdoptAs)> {
    let (name, kind) = spec
        .split_once('=')
        .ok_or_else(|| SkillzError::Message(format!(
            "invalid --adopt-local `{spec}`; expected name=upstream|authored|skip"
        )))?;
    let adopt = match kind {
        "upstream" => AdoptAs::Upstream,
        "authored" => AdoptAs::Authored,
        "skip" => AdoptAs::Skip,
        other => {
            return Err(SkillzError::Message(format!(
                "unknown adopt kind `{other}` (use upstream|authored|skip)"
            )));
        }
    };
    Ok((name.to_string(), adopt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pocock_style_skillfile() {
        let text = r#"
install  cursor  local
github  skill  mattpocock/skills  skills/productivity/grilling
github  skill  owner/repo@abc  path/to/ecto-thinking
"#;
        let entries = parse_skillfile(text).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "grilling");
        assert_eq!(
            entries[0].source,
            "github:mattpocock/skills/skills/productivity/grilling"
        );
        assert_eq!(entries[1].name, "ecto-thinking");
        assert!(entries[1].source.ends_with("@abc"));
    }
}
