//! Fetch Upstream skills into a Project skills directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::skill;
use crate::source::GithubSource;
use crate::{Result, SkillzError};

/// A skill directory discovered by walking for `SKILL.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub name: String,
    pub abs_path: PathBuf,
    /// Path relative to the walk root's repo root, using `/` separators.
    pub rel_path: String,
}

/// Locate a skill directory by name under `root` (nested layouts OK).
pub fn find_skill_by_name(root: &Path, skill_name: &str) -> Result<PathBuf> {
    let mut matches = Vec::new();
    find_skill_inner(root, skill_name, &mut matches)?;
    match matches.len() {
        0 => Err(SkillzError::Message(format!(
            "skill `{skill_name}` not found under {}",
            root.display()
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(SkillzError::Message(format!(
            "skill `{skill_name}` matched multiple paths under {}",
            root.display()
        ))),
    }
}

fn find_skill_inner(dir: &Path, skill_name: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let entry = entry.map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", dir.display()))
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == skill_name && path.join("SKILL.md").is_file() {
            out.push(path.clone());
        }
        // Skip VCS metadata when walking.
        if name == ".git" || name == ".jj" {
            continue;
        }
        find_skill_inner(&path, skill_name, out)?;
    }
    Ok(())
}

/// Walk `search_root` for directories containing `SKILL.md`.
///
/// If `search_root` itself contains `SKILL.md`, it is included (so a source URL
/// pointing at a single skill directory works). `repo_root` is used only to
/// compute `rel_path` (must be an ancestor of each hit).
pub fn discover_skills(repo_root: &Path, search_root: &Path) -> Result<Vec<DiscoveredSkill>> {
    let mut out = Vec::new();
    if search_root.join("SKILL.md").is_file() {
        let name = search_root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "skill".into());
        let rel = search_root
            .strip_prefix(repo_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| name.clone());
        out.push(DiscoveredSkill {
            name,
            abs_path: search_root.to_path_buf(),
            rel_path: rel,
        });
    }
    discover_skills_inner(repo_root, search_root, &mut out)?;
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

fn discover_skills_inner(
    repo_root: &Path,
    dir: &Path,
    out: &mut Vec<DiscoveredSkill>,
) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let entry = entry.map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", dir.display()))
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == ".jj" {
            continue;
        }
        if path.join("SKILL.md").is_file() {
            let rel = path
                .strip_prefix(repo_root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());
            out.push(DiscoveredSkill {
                name: name.clone(),
                abs_path: path.clone(),
                rel_path: rel,
            });
        }
        discover_skills_inner(repo_root, &path, out)?;
    }
    Ok(())
}

pub fn git_head_rev(repo: &Path) -> Result<Option<String>> {
    git_rev_parse_commit(repo, "HEAD")
}

/// Resolve `rev` to a commit object id, if git can see it in `repo`.
fn git_rev_parse_commit(repo: &Path, rev: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["rev-parse", "--verify", &format!("{rev}^{{commit}}")])
        .output()
        .map_err(|e| SkillzError::Message(format!("git rev-parse failed: {e}")))?;
    if !output.status.success() {
        return Ok(None);
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if id.is_empty() { None } else { Some(id) })
}

fn git_checkout_detached(repo: &Path, rev: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["checkout", "--detach", rev])
        .status()
        .map_err(|e| SkillzError::Message(format!("git checkout failed: {e}")))?;
    if !status.success() {
        return Err(SkillzError::Message(format!(
            "failed to checkout rev `{rev}` in {}",
            repo.display()
        )));
    }
    Ok(())
}

/// Fetch `rev` into an existing cache clone and detach HEAD onto it.
///
/// `git fetch origin <tag>` often updates only `FETCH_HEAD` without creating a
/// local tag ref. Resolving via object id / `FETCH_HEAD` avoids treating the
/// tag name as a pathspec (`git checkout --detach <unresolved>`).
fn fetch_and_checkout_rev(repo: &Path, rev: &str) -> Result<()> {
    let fetch_ok = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["fetch", "--depth", "1", "origin", rev])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let target = match git_rev_parse_commit(repo, rev)? {
        Some(id) => id,
        None if fetch_ok => git_rev_parse_commit(repo, "FETCH_HEAD")?.ok_or_else(|| {
            SkillzError::Message(format!(
                "fetched `{rev}` but could not resolve FETCH_HEAD in {}",
                repo.display()
            ))
        })?,
        None => {
            return Err(SkillzError::Message(format!(
                "failed to fetch or resolve rev `{rev}` in {}",
                repo.display()
            )));
        }
    };
    git_checkout_detached(repo, &target)
}

/// Resolve a local filesystem path to a skill directory.
pub fn resolve_local_skill_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| SkillzError::Message(format!("cwd: {e}")))?
            .join(path)
    };
    if path.join("SKILL.md").is_file() {
        return Ok(path);
    }
    if path.is_dir() {
        // If it's a repo root with a single obvious skill, still require name or SKILL.md.
        return Err(SkillzError::Message(format!(
            "path {} does not contain SKILL.md; pass the skill directory",
            path.display()
        )));
    }
    Err(SkillzError::Message(format!(
        "skill path not found: {}",
        path.display()
    )))
}

/// Ensure a shallow clone of the GitHub repo exists under the skillz cache.
pub fn ensure_github_checkout(cache_root: &Path, source: &GithubSource) -> Result<PathBuf> {
    let dest = cache_root
        .join("github")
        .join(&source.owner)
        .join(&source.repo);
    if dest.join(".git").exists() || dest.join(".jj").is_dir() {
        if let Some(rev) = &source.rev {
            fetch_and_checkout_rev(&dest, rev)?;
        }
        return Ok(dest);
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create cache dir {}: {e}",
                parent.display()
            ))
        })?;
    }
    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(rev) = &source.rev {
        cmd.arg("--branch").arg(rev);
    }
    cmd.arg(source.clone_url()).arg(&dest);
    let status = cmd
        .status()
        .map_err(|e| SkillzError::Message(format!("failed to run git clone: {e}")))?;
    if !status.success() {
        return Err(SkillzError::Message(format!(
            "git clone failed for {}",
            source.clone_url()
        )));
    }
    Ok(dest)
}

pub fn resolve_github_skill(
    cache_root: &Path,
    source: &GithubSource,
    skill_name: Option<&str>,
) -> Result<(PathBuf, GithubSource)> {
    let repo = ensure_github_checkout(cache_root, source)?;
    let rev = git_head_rev(&repo)?.unwrap_or_else(|| "HEAD".into());
    let skill_path = if let Some(path) = &source.path {
        let candidate = repo.join(path);
        if candidate.join("SKILL.md").is_file() {
            candidate
        } else if let Some(name) = skill_name {
            find_skill_by_name(&candidate, name)?
        } else {
            return Err(SkillzError::Message(format!(
                "no SKILL.md at {} in github:{}/{}",
                path, source.owner, source.repo
            )));
        }
    } else if let Some(name) = skill_name {
        find_skill_by_name(&repo, name)?
    } else if repo.join("SKILL.md").is_file() {
        repo.clone()
    } else {
        return Err(SkillzError::Message(format!(
            "provide a skill name for github:{}/{} or include /path in the source",
            source.owner, source.repo
        )));
    };

    let rel = skill_path
        .strip_prefix(&repo)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let pinned = source.clone().with_path(rel).with_rev(rev);
    Ok((skill_path, pinned))
}

/// Copy a resolved skill into the Project skills dir using the skill's directory name.
pub fn stage_skill(project_skills: &Path, skill_path: &Path, name_override: Option<&str>) -> Result<PathBuf> {
    let name = name_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            skill_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "skill".into())
        });
    skill::validate_skill_name(&name)?;
    let dest = skill::skill_dir(project_skills, &name);
    skill::copy_skill_tree(skill_path, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::GithubSource;

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-C"])
            .arg(repo)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed in {}", repo.display());
    }

    #[test]
    fn discover_skills_finds_nested_and_respects_search_root() {
        let root = tempfile::tempdir().unwrap();
        let eng = root.path().join("skills/engineering/grilling");
        let prod = root.path().join("skills/productivity/focus");
        fs::create_dir_all(&eng).unwrap();
        fs::create_dir_all(&prod).unwrap();
        fs::write(eng.join("SKILL.md"), "# g\n").unwrap();
        fs::write(prod.join("SKILL.md"), "# f\n").unwrap();

        let all = discover_skills(root.path(), root.path()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "grilling");
        assert_eq!(all[0].rel_path, "skills/engineering/grilling");

        let filtered = discover_skills(
            root.path(),
            &root.path().join("skills/engineering"),
        )
        .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "grilling");

        let leaf = discover_skills(root.path(), &eng).unwrap();
        assert_eq!(leaf.len(), 1);
        assert_eq!(leaf[0].name, "grilling");
        assert_eq!(leaf[0].rel_path, "skills/engineering/grilling");
    }

    #[test]
    fn ensure_github_checkout_resolves_tag_left_on_fetch_head() {
        let tmp = tempfile::tempdir().unwrap();
        let upstream = tmp.path().join("upstream");
        fs::create_dir_all(&upstream).unwrap();
        fs::write(upstream.join("SKILL.md"), "# tagged\n").unwrap();
        git(&upstream, &["init"]);
        git(&upstream, &["config", "user.email", "test@example.com"]);
        git(&upstream, &["config", "user.name", "test"]);
        git(&upstream, &["add", "."]);
        git(&upstream, &["commit", "-m", "init"]);
        git(&upstream, &["tag", "v2.1.0"]);
        let tagged = git_rev_parse_commit(&upstream, "v2.1.0")
            .unwrap()
            .expect("tagged rev");

        // Second commit on main so HEAD != tag after a plain clone.
        fs::write(upstream.join("SKILL.md"), "# later\n").unwrap();
        git(&upstream, &["add", "."]);
        git(&upstream, &["commit", "-m", "later"]);

        let cache = tmp.path().join("cache");
        let dest = cache.join("github/acme/skills");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let status = Command::new("git")
            .args(["clone", upstream.to_str().unwrap(), dest.to_str().unwrap()])
            .status()
            .unwrap();
        assert!(status.success());
        // Match production failure mode: tag fetch lands on FETCH_HEAD only.
        git(&dest, &["tag", "-d", "v2.1.0"]);
        assert!(git_rev_parse_commit(&dest, "v2.1.0").unwrap().is_none());

        let source = GithubSource {
            owner: "acme".into(),
            repo: "skills".into(),
            path: None,
            rev: Some("v2.1.0".into()),
        };
        let checked = ensure_github_checkout(&cache, &source).unwrap();
        assert_eq!(checked, dest);
        assert_eq!(git_head_rev(&dest).unwrap().as_deref(), Some(tagged.as_str()));
    }
}
