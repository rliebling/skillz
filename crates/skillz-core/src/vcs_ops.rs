//! Shell-out helpers for committing Library Branch Locks (git / jj).

use std::path::Path;
use std::process::Command;

use crate::vcs::{infer_vcs, Vcs};
use crate::{Result, SkillzError};

pub struct BranchCommit {
    pub branch: String,
    pub rev: String,
}

/// Write working tree changes on a new branch and return the commit rev.
pub fn commit_on_branch(
    repo: &Path,
    branch: &str,
    message: &str,
    paths: &[&Path],
) -> Result<BranchCommit> {
    match infer_vcs(repo) {
        Some(Vcs::Jj) => commit_jj(repo, branch, message, paths),
        Some(Vcs::Git) => commit_git(repo, branch, message, paths),
        None => Err(SkillzError::Message(format!(
            "library at {} has no git/jj metadata; init VCS before Branch Lock libcommit",
            repo.display()
        ))),
    }
}

/// Commit staged Library paths on main (direct contribution publish unit).
pub fn commit_on_main(repo: &Path, message: &str, paths: &[&Path]) -> Result<String> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => {
            let main = git_main_branch(repo)?;
            run(repo, &["checkout", &main])?;
            for path in paths {
                let rel = path
                    .strip_prefix(repo)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                run(repo, &["add", "--", &rel])?;
            }
            run(repo, &["commit", "-m", message])?;
            stdout(repo, &["rev-parse", "HEAD"])
        }
        Some(Vcs::Jj) => {
            let main = jj_main_bookmark(repo)?;
            for path in paths {
                let _ = path;
            }
            run_jj(repo, &["describe", "-m", message])?;
            run_jj(repo, &["bookmark", "set", &main, "-r", "@"])?;
            run_jj(repo, &["new"])?;
            main_rev(repo)
        }
        None => Err(SkillzError::Message(format!(
            "library at {} has no git/jj metadata; init VCS before `publish`",
            repo.display()
        ))),
    }
}

fn commit_git(repo: &Path, branch: &str, message: &str, paths: &[&Path]) -> Result<BranchCommit> {
    run(repo, &["checkout", "-B", branch])?;
    for path in paths {
        let rel = path
            .strip_prefix(repo)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        run(repo, &["add", "--", &rel])?;
    }
    run(repo, &["commit", "-m", message])?;
    let rev = stdout(repo, &["rev-parse", "HEAD"])?;
    Ok(BranchCommit {
        branch: branch.to_string(),
        rev,
    })
}

fn commit_jj(repo: &Path, branch: &str, message: &str, paths: &[&Path]) -> Result<BranchCommit> {
    // Stage by describing a new change on a bookmark.
    for path in paths {
        let _ = path; // jj tracks working copy automatically
    }
    run_jj(repo, &["describe", "-m", message])?;
    run_jj(repo, &["bookmark", "set", branch, "-r", "@"])?;
    // Advance to leave the commit immutable for locking.
    run_jj(repo, &["new"])?;
    let rev = stdout_jj(repo, &["log", "-r", &format!("bookmarks({branch})"), "-T", "commit_id", "--no-graph"])?;
    let rev = rev.lines().next().unwrap_or("").trim().to_string();
    if rev.is_empty() {
        return Err(SkillzError::Message(
            "failed to resolve jj bookmark rev".into(),
        ));
    }
    Ok(BranchCommit {
        branch: branch.to_string(),
        rev,
    })
}

/// Merge a skill branch into the main line of a Library repo.
pub fn promote_branch_to_main(repo: &Path, branch: &str) -> Result<String> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => {
            let main = git_main_branch(repo)?;
            run(repo, &["checkout", &main])?;
            run(repo, &["merge", "--no-ff", "-m", &format!("Promote {branch}"), branch])?;
            stdout(repo, &["rev-parse", "HEAD"])
        }
        Some(Vcs::Jj) => {
            let main = jj_main_bookmark(repo)?;
            run_jj(
                repo,
                &[
                    "bookmark",
                    "set",
                    &main,
                    "-r",
                    &format!("bookmarks({branch})"),
                ],
            )?;
            stdout_jj(
                repo,
                &[
                    "log",
                    "-r",
                    &format!("bookmarks({main})"),
                    "-T",
                    "commit_id",
                    "--no-graph",
                ],
            )
            .map(|s| s.lines().next().unwrap_or("").trim().to_string())
        }
        None => Err(SkillzError::Message(
            "cannot promote without git/jj".into(),
        )),
    }
}

pub fn checkout_rev(repo: &Path, rev: &str) -> Result<()> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => run(repo, &["checkout", "--detach", rev]),
        Some(Vcs::Jj) => {
            run_jj(repo, &["new", "-r", rev])?;
            Ok(())
        }
        None => Ok(()),
    }
}

/// Push a local branch to `remote` (best-effort; caller may soft-fail).
pub fn push_branch(repo: &Path, branch: &str, remote: &str) -> Result<()> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => run(repo, &["push", "-u", remote, branch]),
        Some(Vcs::Jj) => run_jj(
            repo,
            &["git", "push", "--remote", remote, "--bookmark", branch],
        ),
        None => Err(SkillzError::Message(format!(
            "library at {} has no git/jj metadata; cannot push",
            repo.display()
        ))),
    }
}

/// Best-effort PR/MR open via `gh` or `glab`. Returns `Ok(None)` if neither CLI exists.
pub fn try_open_merge_request(
    repo: &Path,
    branch: &str,
    title: &str,
) -> std::result::Result<Option<String>, String> {
    if which_bin("gh") {
        let output = Command::new("gh")
            .args([
                "pr",
                "create",
                "--head",
                branch,
                "--title",
                title,
                "--body",
                "Created by skillz publish (review Library).",
            ])
            .current_dir(repo)
            .output()
            .map_err(|e| format!("gh failed to start: {e}"))?;
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !url.is_empty() {
                return Ok(Some(url));
            }
            return Ok(Some("(gh pr create succeeded)".into()));
        }
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("gh pr create exited {}", output.status)
        } else {
            err
        });
    }
    if which_bin("glab") {
        let output = Command::new("glab")
            .args([
                "mr",
                "create",
                "--source-branch",
                branch,
                "--title",
                title,
                "--description",
                "Created by skillz publish (review Library).",
                "--yes",
            ])
            .current_dir(repo)
            .output()
            .map_err(|e| format!("glab failed to start: {e}"))?;
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !url.is_empty() {
                return Ok(Some(url));
            }
            return Ok(Some("(glab mr create succeeded)".into()));
        }
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("glab mr create exited {}", output.status)
        } else {
            err
        });
    }
    Ok(None)
}

/// Copy-paste commands when forge CLI is missing or auth failed.
pub fn copy_paste_open_pr_commands(repo: &Path, branch: &str) -> Vec<String> {
    vec![
        format!(
            "gh -C {} pr create --head {branch} --fill",
            repo.display()
        ),
        format!(
            "glab -C {} mr create --source-branch {branch} --fill",
            repo.display()
        ),
        format!(
            "# or open the host compare URL for branch `{branch}` against main"
        ),
    ]
}

fn which_bin(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn main_rev(repo: &Path) -> Result<String> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => {
            let main = git_main_branch(repo)?;
            stdout(repo, &["rev-parse", &main])
        }
        Some(Vcs::Jj) => {
            let main = jj_main_bookmark(repo)?;
            stdout_jj(
                repo,
                &[
                    "log",
                    "-r",
                    &format!("bookmarks({main})"),
                    "-T",
                    "commit_id",
                    "--no-graph",
                ],
            )
            .map(|s| s.lines().next().unwrap_or("").trim().to_string())
        }
        None => Err(SkillzError::Message("no VCS main rev".into())),
    }
}

fn git_main_branch(repo: &Path) -> Result<String> {
    for candidate in ["main", "master"] {
        if run(repo, &["rev-parse", "--verify", candidate]).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    Ok("main".into())
}

fn jj_main_bookmark(repo: &Path) -> Result<String> {
    for candidate in ["main", "master"] {
        if run_jj(repo, &["bookmark", "list", candidate]).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    Ok("main".into())
}

fn run(repo: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(args)
        .status()
        .map_err(|e| SkillzError::Message(format!("git failed: {e}")))?;
    if !status.success() {
        return Err(SkillzError::Message(format!(
            "git {} failed in {}",
            args.join(" "),
            repo.display()
        )));
    }
    Ok(())
}

fn stdout(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| SkillzError::Message(format!("git failed: {e}")))?;
    if !output.status.success() {
        return Err(SkillzError::Message(format!(
            "git {} failed in {}",
            args.join(" "),
            repo.display()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_jj(repo: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("jj")
        .args(["-R"])
        .arg(repo)
        .args(args)
        .status()
        .map_err(|e| SkillzError::Message(format!("jj failed: {e}")))?;
    if !status.success() {
        return Err(SkillzError::Message(format!(
            "jj {} failed in {}",
            args.join(" "),
            repo.display()
        )));
    }
    Ok(())
}

fn stdout_jj(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("jj")
        .args(["-R"])
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| SkillzError::Message(format!("jj failed: {e}")))?;
    if !output.status.success() {
        return Err(SkillzError::Message(format!(
            "jj {} failed in {}",
            args.join(" "),
            repo.display()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Snapshot the working copy when using jj (`jj status`); no-op for git/none.
pub fn snapshot_working_copy(repo: &Path) -> Result<()> {
    match infer_vcs(repo) {
        Some(Vcs::Jj) => run_jj(repo, &["status"]),
        Some(Vcs::Git) | None => Ok(()),
    }
}

/// Tracked files under `skills_rel` (repo-relative) that differ from HEAD/@-.
///
/// Untracked paths are ignored so brand-new skill trees can be migrated.
pub fn dirty_tracked_under(repo: &Path, skills_rel: &str) -> Result<Vec<String>> {
    let skills_rel = skills_rel.trim_matches('/');
    match infer_vcs(repo) {
        Some(Vcs::Git) => dirty_tracked_git(repo, skills_rel),
        Some(Vcs::Jj) => {
            snapshot_working_copy(repo)?;
            dirty_tracked_jj(repo, skills_rel)
        }
        None => Ok(Vec::new()),
    }
}

fn under_skills_prefix(path: &str, skills_rel: &str) -> bool {
    path == skills_rel || path.starts_with(&format!("{skills_rel}/"))
}

fn dirty_tracked_git(repo: &Path, skills_rel: &str) -> Result<Vec<String>> {
    // `-uno` omits untracked; still shows modified/deleted/staged tracked files.
    let output = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["status", "--porcelain=v1", "-uno", "--", skills_rel])
        .output()
        .map_err(|e| SkillzError::Message(format!("git status failed: {e}")))?;
    if !output.status.success() {
        return Err(SkillzError::Message(format!(
            "git status failed in {}",
            repo.display()
        )));
    }
    let mut dirty = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        let path = line[3..].trim();
        // Rename lines: `R  old -> new`
        let path = path
            .rsplit_once(" -> ")
            .map(|(_, new)| new)
            .unwrap_or(path);
        if under_skills_prefix(path, skills_rel) {
            dirty.push(path.to_string());
        }
    }
    dirty.sort();
    dirty.dedup();
    Ok(dirty)
}

fn dirty_tracked_jj(repo: &Path, skills_rel: &str) -> Result<Vec<String>> {
    let parent_files = stdout_jj(repo, &["file", "list", "-r", "@-"])?;
    let tracked: std::collections::BTreeSet<&str> = parent_files.lines().collect();
    let changed = stdout_jj(repo, &["diff", "--name-only", "--from", "@-"])?;
    let mut dirty = Vec::new();
    for path in changed.lines() {
        if under_skills_prefix(path, skills_rel) && tracked.contains(path) {
            dirty.push(path.to_string());
        }
    }
    dirty.sort();
    dirty.dedup();
    Ok(dirty)
}

/// Initialize a bare-minimum git repo with an empty main commit (tests).
pub fn init_git_repo(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).ok();
    run(path, &["init"])?;
    run(path, &["config", "user.email", "skillz@example.com"])?;
    run(path, &["config", "user.name", "skillz"])?;
    // Empty commit so main exists.
    run(path, &["commit", "--allow-empty", "-m", "init"])?;
    // Ensure branch named main.
    let _ = run(path, &["branch", "-M", "main"]);
    Ok(())
}
