//! Fetch Upstream skills into a Project skills directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::skill;
use crate::source::GithubSource;
use crate::{Result, SkillzError};

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
        // Fetch updates when rev requested; best-effort.
        if let Some(rev) = &source.rev {
            let _ = Command::new("git")
                .args(["-C"])
                .arg(&dest)
                .args(["fetch", "--depth", "1", "origin", rev])
                .status();
            let status = Command::new("git")
                .args(["-C"])
                .arg(&dest)
                .args(["checkout", "--detach", rev])
                .status()
                .map_err(|e| SkillzError::Message(format!("git checkout failed: {e}")))?;
            if !status.success() {
                return Err(SkillzError::Message(format!(
                    "failed to checkout rev `{rev}` in {}",
                    dest.display()
                )));
            }
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

fn git_head_rev(repo: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| SkillzError::Message(format!("git rev-parse failed: {e}")))?;
    if !output.status.success() {
        return Ok(None);
    }
    let rev = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if rev.is_empty() { None } else { Some(rev) })
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
