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
