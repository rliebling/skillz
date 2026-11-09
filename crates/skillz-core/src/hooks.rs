//! Install git pre-commit Drift gates.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::vcs::{infer_vcs, Vcs};
use crate::{Result, SkillzError};

const PRE_COMMIT_MARKER: &str = "# skillz drift gate";

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

pub fn install_git_pre_commit(project_root: &Path) -> Result<std::path::PathBuf> {
    let git_dir = resolve_git_dir(project_root)?;
    let hooks = git_dir.join("hooks");
    fs::create_dir_all(&hooks).map_err(|e| {
        SkillzError::Message(format!(
            "failed to create hooks dir {}: {e}",
            hooks.display()
        ))
    })?;
    let hook = hooks.join("pre-commit");
    if hook.is_file() {
        let existing = fs::read_to_string(&hook).unwrap_or_default();
        if existing.contains(PRE_COMMIT_MARKER) {
            return Ok(hook);
        }
        // Preserve prior hook by chaining.
        let backup = hooks.join("pre-commit.skillz-prev");
        fs::rename(&hook, &backup).map_err(|e| {
            SkillzError::Message(format!(
                "failed to move existing pre-commit aside: {e}"
            ))
        })?;
        let chained = format!(
            "#!/bin/sh\n{PRE_COMMIT_BODY}\nstatus=$?\nif [ $status -ne 0 ]; then exit $status; fi\n\"$(dirname \"$0\")/pre-commit.skillz-prev\" \"$@\"\n"
        );
        write_executable(&hook, &chained)?;
        return Ok(hook);
    }
    let script = format!("#!/bin/sh\n{PRE_COMMIT_BODY}");
    write_executable(&hook, &script)?;
    Ok(hook)
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

/// Install repo-local `jj push-with-checks` alias (does not wrap plain `jj git push`).
pub fn install_jj_push_with_checks(project_root: &Path) -> Result<std::path::PathBuf> {
    if infer_vcs(project_root) != Some(Vcs::Jj) {
        return Err(SkillzError::Message(format!(
            "not a jj repo: {}",
            project_root.display()
        )));
    }
    let config_path = project_root.join(".jj").join("repo").join("config.toml");
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SkillzError::Message(format!(
                "failed to create {}: {e}",
                parent.display()
            ))
        })?;
    }
    let existing = if config_path.is_file() {
        fs::read_to_string(&config_path).unwrap_or_default()
    } else {
        String::new()
    };
    if existing.contains("push-with-checks") {
        return Ok(config_path);
    }
    let alias = r#"
# skillz drift gate
[aliases]
push-with-checks = ["util", "exec", "--", "sh", "-c", "if [ \"${SKILLZ_ALLOW_DRIFT:-}\" = 1 ] || [ \"${SKILLZ_ALLOW_DRIFT:-}\" = true ]; then jj git push \"$@\"; else skillz drift --check --project \"$PWD\" && jj git push \"$@\"; fi", ""]
"#;
    let mut new_contents = existing;
    if !new_contents.ends_with('\n') && !new_contents.is_empty() {
        new_contents.push('\n');
    }
    new_contents.push_str(alias);
    fs::write(&config_path, new_contents).map_err(|e| {
        SkillzError::Message(format!(
            "failed to write {}: {e}",
            config_path.display()
        ))
    })?;
    Ok(config_path)
}

fn resolve_git_dir(project_root: &Path) -> Result<std::path::PathBuf> {
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
