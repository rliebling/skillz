//! Install and preview git/jj Drift gates.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::vcs::{infer_vcs, Vcs};
use crate::{Result, SkillzError};

const PRE_COMMIT_MARKER: &str = "# skillz drift gate";
const JJ_ALIAS_NAME: &str = "push-with-checks";

const PRE_COMMIT_BODY: &str = r#"# skillz drift gate
# Blocks commits when Managed Project skill Installations drift from Locks.
# Escape: SKILLZ_ALLOW_DRIFT=1 git commit …

if [ "${SKILLZ_ALLOW_DRIFT:-}" = "1" ] || [ "${SKILLZ_ALLOW_DRIFT:-}" = "true" ]; then
  exit 0
fi

if ! command -v skillz >/dev/null 2>&1; then
  echo "skillz: warning: skillz not on PATH; skipping drift pre-commit check" >&2
  exit 0
fi

skillz drift --check --project "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
"#;

const JJ_ALIAS_LINE: &str = r#"push-with-checks = ["util", "exec", "--", "sh", "-c", "if [ \"${SKILLZ_ALLOW_DRIFT:-}\" = 1 ] || [ \"${SKILLZ_ALLOW_DRIFT:-}\" = true ]; then jj git push \"$@\"; else skillz drift --check --project \"$PWD\" && jj git push \"$@\"; fi", ""]"#;

/// Kind of Drift gate `hooks install` can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookKind {
    GitPreCommit,
    JjPushWithChecks,
}

impl HookKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HookKind::GitPreCommit => "git pre-commit",
            HookKind::JjPushWithChecks => "jj push-with-checks alias",
        }
    }
}

/// Whether the planned gate is already present on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPlanStatus {
    /// Target file does not exist yet; install would create it.
    WouldCreate,
    /// Skillz gate is already present; install is a no-op.
    AlreadyInstalled,
    /// Existing non-skillz pre-commit would be chained behind the gate.
    WouldChain,
    /// Existing jj config would gain the alias stanza.
    WouldAppend,
}

impl HookPlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            HookPlanStatus::WouldCreate => "would create",
            HookPlanStatus::AlreadyInstalled => "already installed",
            HookPlanStatus::WouldChain => "would chain with existing hook",
            HookPlanStatus::WouldAppend => "would append alias",
        }
    }
}

/// Dry-run description of one Drift gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookPlan {
    pub kind: HookKind,
    pub path: PathBuf,
    pub status: HookPlanStatus,
    /// Contents install would write (full file) or the alias stanza to append.
    pub contents: String,
}

/// Plan Drift gates for a Managed Project root without writing files.
///
/// VCS-exclusive: jj (including colocated) → only the push-with-checks alias;
/// git-only → only the pre-commit hook. Never both.
pub fn plan_hooks(project_root: &Path) -> Result<Vec<HookPlan>> {
    match infer_vcs(project_root) {
        Some(Vcs::Jj) => Ok(vec![plan_jj_push_with_checks(project_root)?]),
        Some(Vcs::Git) => Ok(vec![plan_git_pre_commit(project_root)?]),
        None => Err(SkillzError::Message(
            "no hooks applicable (need git or jj metadata)".into(),
        )),
    }
}

pub fn install_git_pre_commit(project_root: &Path) -> Result<PathBuf> {
    let plan = plan_git_pre_commit(project_root)?;
    if plan.status == HookPlanStatus::AlreadyInstalled {
        return Ok(plan.path);
    }
    if let Some(parent) = plan.path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create hooks dir {}: {e}",
                parent.display()
            ))
        })?;
    }
    if plan.status == HookPlanStatus::WouldChain {
        let backup = plan
            .path
            .parent()
            .expect("pre-commit has parent")
            .join("pre-commit.skillz-prev");
        fs::rename(&plan.path, &backup).map_err(|e| {
            SkillzError::Message(format!(
                "failed to move existing pre-commit aside: {e}"
            ))
        })?;
    }
    write_executable(&plan.path, &plan.contents)?;
    Ok(plan.path)
}

/// Install repo-local `jj push-with-checks` alias (does not wrap plain `jj git push`).
pub fn install_jj_push_with_checks(project_root: &Path) -> Result<PathBuf> {
    let plan = plan_jj_push_with_checks(project_root)?;
    if plan.status == HookPlanStatus::AlreadyInstalled {
        return Ok(plan.path);
    }
    if let Some(parent) = plan.path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create {}: {e}",
                parent.display()
            ))
        })?;
    }
    let existing = if plan.path.is_file() {
        fs::read_to_string(&plan.path).unwrap_or_default()
    } else {
        String::new()
    };
    let mut new_contents = existing;
    if !new_contents.ends_with('\n') && !new_contents.is_empty() {
        new_contents.push('\n');
    }
    new_contents.push_str(&plan.contents);
    if !new_contents.ends_with('\n') {
        new_contents.push('\n');
    }
    fs::write(&plan.path, new_contents).map_err(|e| {
        SkillzError::Message(format!(
            "failed to write {}: {e}",
            plan.path.display()
        ))
    })?;
    Ok(plan.path)
}

fn plan_git_pre_commit(project_root: &Path) -> Result<HookPlan> {
    let git_dir = resolve_git_dir(project_root)?;
    let hook = git_dir.join("hooks").join("pre-commit");
    if hook.is_file() {
        let existing = fs::read_to_string(&hook).unwrap_or_default();
        if existing.contains(PRE_COMMIT_MARKER) {
            return Ok(HookPlan {
                kind: HookKind::GitPreCommit,
                path: hook,
                status: HookPlanStatus::AlreadyInstalled,
                contents: existing,
            });
        }
        let chained = format!(
            "#!/bin/sh\n{PRE_COMMIT_BODY}\nstatus=$?\nif [ $status -ne 0 ]; then exit $status; fi\n\"$(dirname \"$0\")/pre-commit.skillz-prev\" \"$@\"\n"
        );
        return Ok(HookPlan {
            kind: HookKind::GitPreCommit,
            path: hook,
            status: HookPlanStatus::WouldChain,
            contents: chained,
        });
    }
    Ok(HookPlan {
        kind: HookKind::GitPreCommit,
        path: hook,
        status: HookPlanStatus::WouldCreate,
        contents: format!("#!/bin/sh\n{PRE_COMMIT_BODY}"),
    })
}

fn plan_jj_push_with_checks(project_root: &Path) -> Result<HookPlan> {
    if infer_vcs(project_root) != Some(Vcs::Jj) {
        return Err(SkillzError::Message(format!(
            "not a jj repo: {}",
            project_root.display()
        )));
    }
    let config_path = project_root.join(".jj").join("repo").join("config.toml");
    let existing = if config_path.is_file() {
        fs::read_to_string(&config_path).unwrap_or_default()
    } else {
        String::new()
    };
    let stanza = format!("# skillz drift gate\n[aliases]\n{JJ_ALIAS_LINE}\n");
    if existing.contains(JJ_ALIAS_NAME) {
        return Ok(HookPlan {
            kind: HookKind::JjPushWithChecks,
            path: config_path,
            status: HookPlanStatus::AlreadyInstalled,
            contents: stanza,
        });
    }
    let status = if config_path.is_file() {
        HookPlanStatus::WouldAppend
    } else {
        HookPlanStatus::WouldCreate
    };
    Ok(HookPlan {
        kind: HookKind::JjPushWithChecks,
        path: config_path,
        status,
        contents: stanza,
    })
}

fn write_executable(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).map_err(|e| {
        SkillzError::Message(format!("failed to write {}: {e}", path.display()))
    })?;
    let mut perms = fs::metadata(path)
        .map_err(|e| SkillzError::Message(format!("stat {}: {e}", path.display())))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).map_err(|e| {
        SkillzError::Message(format!(
            "failed to chmod {}: {e}",
            path.display()
        ))
    })?;
    Ok(())
}

fn resolve_git_dir(project_root: &Path) -> Result<PathBuf> {
    match infer_vcs(project_root) {
        Some(Vcs::Git) | Some(Vcs::Jj) => {}
        None => {
            return Err(SkillzError::Message(format!(
                "no git/jj metadata under {}",
                project_root.display()
            )));
        }
    }
    let git_path = project_root.join(".git");
    if git_path.is_dir() {
        return Ok(git_path);
    }
    if git_path.is_file() {
        // gitfile / colocated jj: `gitdir: ...`
        let text = fs::read_to_string(&git_path).map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", git_path.display()))
        })?;
        if let Some(dir) = text.strip_prefix("gitdir:") {
            let dir = dir.trim();
            let path = Path::new(dir);
            let abs = if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_root.join(path)
            };
            return Ok(abs);
        }
    }
    // jj without colocated .git — look under .jj
    let jj_git = project_root.join(".jj").join("repo").join("store").join("git");
    if jj_git.is_dir() {
        return Ok(jj_git);
    }
    Err(SkillzError::Message(format!(
        "could not locate git dir for hooks under {}",
        project_root.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn plan_git_would_create() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".git/hooks")).unwrap();
        let plan = plan_git_pre_commit(root.path()).unwrap();
        assert_eq!(plan.kind, HookKind::GitPreCommit);
        assert_eq!(plan.status, HookPlanStatus::WouldCreate);
        assert!(plan.contents.contains("skillz drift --check"));
        assert!(!plan.path.exists());
    }

    #[test]
    fn plan_jj_would_create_alias_stanza() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".jj/repo")).unwrap();
        let plan = plan_jj_push_with_checks(root.path()).unwrap();
        assert_eq!(plan.kind, HookKind::JjPushWithChecks);
        assert_eq!(plan.status, HookPlanStatus::WouldCreate);
        assert!(plan.contents.contains("push-with-checks"));
        assert!(plan.contents.contains("skillz drift --check"));
    }
}
