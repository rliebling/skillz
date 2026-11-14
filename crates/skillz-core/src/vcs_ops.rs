//! Shell-out helpers for committing Library Branch Locks (git / jj).

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use crate::vcs::{infer_vcs, Vcs};
use crate::{Result, SkillzError};

/// Soft advisories printed at most once per process key (e.g. per library path).
static SOFT_WARNS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

fn soft_warn(key: String, message: String) {
    let mut map = match SOFT_WARNS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if map.insert(key, message.clone()).is_none() {
        eprintln!("warning: {message}");
    }
}

#[cfg(test)]
pub(crate) fn clear_soft_warns_for_test() {
    match SOFT_WARNS.lock() {
        Ok(mut g) => g.clear(),
        Err(e) => e.into_inner().clear(),
    }
}

#[cfg(test)]
pub(crate) fn soft_warn_messages_for_test() -> Vec<String> {
    match SOFT_WARNS.lock() {
        Ok(g) => g.values().cloned().collect(),
        Err(e) => e.into_inner().values().cloned().collect(),
    }
}

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

/// Check out Library main tip (after fetch) so Catalog files match remote.
pub fn checkout_main(repo: &Path) -> Result<()> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => {
            let main = git_main_branch(repo)?;
            // Prefer remote-tracking tip when present.
            let remote_main = format!("origin/{main}");
            if run(repo, &["rev-parse", "--verify", &remote_main]).is_ok() {
                run(repo, &["checkout", "-B", &main, &remote_main])?;
            } else {
                run(repo, &["checkout", &main])?;
            }
            Ok(())
        }
        Some(Vcs::Jj) => {
            let main = jj_main_bookmark(repo)?;
            run_jj(repo, &["new", "-r", &format!("bookmarks({main})")])?;
            Ok(())
        }
        None => Ok(()),
    }
}

/// Fetch from `origin` (or configured remote) so ancestry checks see remote main.
///
/// Local-only Libraries (no remotes) skip fetch. Missing remotes / missing
/// named remote emit at most one soft warning per repo per process.
pub fn fetch_remote(repo: &Path, remote: &str) -> Result<()> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => {
            if !git_remote_exists(repo, remote) {
                warn_skip_fetch(repo, remote, git_has_any_remote(repo));
                return Ok(());
            }
            let _ = run_git_quiet(repo, &["fetch", remote, "--prune"]);
            Ok(())
        }
        Some(Vcs::Jj) => {
            if !jj_remote_exists(repo, remote) {
                warn_skip_fetch(repo, remote, jj_has_any_remote(repo));
                return Ok(());
            }
            let _ = run_jj_quiet(repo, &["git", "fetch", "--remote", remote]);
            Ok(())
        }
        None => Ok(()),
    }
}

fn warn_skip_fetch(repo: &Path, remote: &str, has_any: bool) {
    let path = repo.display();
    let (key, message) = if has_any {
        (
            format!("no-remote-named:{path}"),
            format!("library at {path} has no remote named `{remote}`; skipping fetch"),
        )
    } else {
        (
            format!("no-remotes:{path}"),
            format!("library at {path} has no git remotes; skipping fetch"),
        )
    };
    soft_warn(key, message);
}

/// True if `rev` is an ancestor of Library main (inclusive).
pub fn is_ancestor_of_main(repo: &Path, rev: &str) -> Result<bool> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => {
            let main = git_main_branch(repo)?;
            let status = Command::new("git")
                .args(["-C"])
                .arg(repo)
                .args(["merge-base", "--is-ancestor", rev, &main])
                .status()
                .map_err(|e| SkillzError::Message(format!("git merge-base failed: {e}")))?;
            Ok(status.success())
        }
        Some(Vcs::Jj) => {
            let main = jj_main_bookmark(repo)?;
            // `jj log -r 'rev & ::main'` non-empty ⇒ ancestor (or equal).
            let out = stdout_jj(
                repo,
                &[
                    "log",
                    "-r",
                    &format!("{rev} & ::bookmarks({main})"),
                    "-T",
                    "commit_id",
                    "--no-graph",
                ],
            )?;
            Ok(!out.trim().is_empty())
        }
        None => Err(SkillzError::Message(
            "cannot check ancestry without git/jj".into(),
        )),
    }
}

/// Ensure `rev` exists locally; try fetching it from origin when missing.
pub fn ensure_rev(repo: &Path, rev: &str) -> Result<()> {
    match infer_vcs(repo) {
        Some(Vcs::Git) => {
            if run_git_quiet(repo, &["cat-file", "-e", &format!("{rev}^{{commit}}")]).is_ok() {
                return Ok(());
            }
            if git_has_any_remote(repo) {
                fetch_remote(repo, "origin")?;
                if run_git_quiet(repo, &["cat-file", "-e", &format!("{rev}^{{commit}}")]).is_ok()
                {
                    return Ok(());
                }
                // Last resort: fetch the SHA directly (only when origin exists).
                if git_remote_exists(repo, "origin") {
                    let _ = run_git_quiet(repo, &["fetch", "origin", rev]);
                    if run_git_quiet(repo, &["cat-file", "-e", &format!("{rev}^{{commit}}")])
                        .is_ok()
                    {
                        return Ok(());
                    }
                }
            }
            Err(SkillzError::Message(format!(
                "Lock rev `{rev}` is not present locally or on origin; fetch the Branch Lock branch or re-publish"
            )))
        }
        Some(Vcs::Jj) => {
            if jj_rev_exists(repo, rev) {
                return Ok(());
            }
            if jj_has_any_remote(repo) {
                fetch_remote(repo, "origin")?;
                if jj_rev_exists(repo, rev) {
                    return Ok(());
                }
            }
            Err(SkillzError::Message(format!(
                "Lock rev `{rev}` is not present in the jj repo; fetch or re-publish"
            )))
        }
        None => Ok(()),
    }
}

fn jj_rev_exists(repo: &Path, rev: &str) -> bool {
    stdout_jj(repo, &["log", "-r", rev, "-T", "commit_id", "--no-graph"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Whether `refs/remotes/origin/<branch>` (or jj bookmark) exists.
pub fn remote_branch_exists(repo: &Path, branch: &str) -> bool {
    match infer_vcs(repo) {
        Some(Vcs::Git) => stdout(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{branch}"),
            ],
        )
        .is_ok(),
        // jj bookmarks are local until pushed; treat presence as remote-or-local.
        Some(Vcs::Jj) => jj_bookmark_exists(repo, branch),
        None => false,
    }
}

/// Whether the branch/bookmark exists locally.
pub fn local_branch_exists(repo: &Path, branch: &str) -> bool {
    match infer_vcs(repo) {
        Some(Vcs::Git) => stdout(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .is_ok(),
        Some(Vcs::Jj) => jj_bookmark_exists(repo, branch),
        None => false,
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
        if stdout(repo, &["rev-parse", "--verify", "--quiet", candidate]).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    soft_warn(
        format!("no-main:{}", repo.display()),
        format!(
            "library at {} has no `main` or `master` branch; pending-review checks use local history only",
            repo.display()
        ),
    );
    Ok("main".into())
}

fn jj_main_bookmark(repo: &Path) -> Result<String> {
    for candidate in ["main", "master"] {
        if jj_bookmark_exists(repo, candidate) {
            return Ok(candidate.to_string());
        }
    }
    soft_warn(
        format!("no-main:{}", repo.display()),
        format!(
            "library at {} has no `main` or `master` bookmark; pending-review checks use local history only",
            repo.display()
        ),
    );
    Ok("main".into())
}

fn jj_bookmark_exists(repo: &Path, name: &str) -> bool {
    // `jj bookmark list NAME` exits 0 with empty stdout when missing (and may
    // warn on stderr) — capture output and require a non-empty listing.
    stdout_jj(repo, &["bookmark", "list", name])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

fn git_remote_exists(repo: &Path, remote: &str) -> bool {
    stdout(repo, &["remote", "get-url", remote]).is_ok()
}

fn git_has_any_remote(repo: &Path) -> bool {
    stdout(repo, &["remote"])
        .map(|s| s.lines().any(|l| !l.trim().is_empty()))
        .unwrap_or(false)
}

fn jj_remote_exists(repo: &Path, remote: &str) -> bool {
    match stdout_jj(repo, &["git", "remote", "list"]) {
        Ok(s) => s.lines().any(|line| {
            line.split_whitespace()
                .next()
                .is_some_and(|name| name == remote)
        }),
        Err(_) => false,
    }
}

fn jj_has_any_remote(repo: &Path) -> bool {
    stdout_jj(repo, &["git", "remote", "list"])
        .map(|s| s.lines().any(|l| !l.trim().is_empty()))
        .unwrap_or(false)
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

/// Like [`run`], but captures stdout/stderr so probe/best-effort calls stay quiet.
fn run_git_quiet(repo: &Path, args: &[&str]) -> Result<()> {
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

/// Like [`run_jj`], but captures stdout/stderr so probe/best-effort calls stay quiet.
fn run_jj_quiet(repo: &Path, args: &[&str]) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::sync::Mutex;

    /// Soft-warn map is process-global; serialize tests that assert on it.
    static SOFT_WARN_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn init_jj_colocate(repo: &Path) {
        fs::create_dir_all(repo).unwrap();
        let status = Command::new("jj")
            .args(["git", "init", "--colocate"])
            .current_dir(repo)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("jj git init");
        assert!(status.success(), "jj git init --colocate failed");
    }

    fn msgs_for(repo: &Path) -> Vec<String> {
        let needle = repo.display().to_string();
        soft_warn_messages_for_test()
            .into_iter()
            .filter(|m| m.contains(&needle))
            .collect()
    }

    #[test]
    fn fetch_remote_no_remotes_warns_once_git() {
        let _guard = SOFT_WARN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_soft_warns_for_test();
        let tmp = temp_dir();
        let repo = tmp.path().join("lib");
        init_git_repo(&repo).unwrap();
        assert!(!git_has_any_remote(&repo));

        for _ in 0..3 {
            fetch_remote(&repo, "origin").unwrap();
        }

        let msgs = msgs_for(&repo);
        assert_eq!(msgs.len(), 1, "expected one soft warn, got {msgs:?}");
        assert!(
            msgs[0].contains("no git remotes"),
            "unexpected message: {}",
            msgs[0]
        );
    }

    #[test]
    fn main_missing_warns_once_git() {
        let _guard = SOFT_WARN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_soft_warns_for_test();
        let tmp = temp_dir();
        let repo = tmp.path().join("lib");
        fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init"]).unwrap();
        run(&repo, &["config", "user.email", "skillz@example.com"]).unwrap();
        run(&repo, &["config", "user.name", "skillz"]).unwrap();
        // Detached empty commit, no main/master branch name.
        run(&repo, &["commit", "--allow-empty", "-m", "init"]).unwrap();
        let head = stdout(&repo, &["rev-parse", "HEAD"]).unwrap();
        run_git_quiet(&repo, &["checkout", "--detach", &head]).unwrap();
        // Delete the default branch ref if present so neither main nor master exists.
        let _ = run_git_quiet(&repo, &["branch", "-D", "main"]);
        let _ = run_git_quiet(&repo, &["branch", "-D", "master"]);

        for _ in 0..3 {
            let _ = git_main_branch(&repo).unwrap();
        }

        let msgs = msgs_for(&repo);
        assert_eq!(msgs.len(), 1, "expected one soft warn, got {msgs:?}");
        assert!(
            msgs[0].contains("no `main` or `master` branch"),
            "unexpected message: {}",
            msgs[0]
        );
    }

    #[test]
    fn fetch_remote_and_main_missing_warn_once_each_jj() {
        if !which_bin("jj") {
            return;
        }
        let _guard = SOFT_WARN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_soft_warns_for_test();
        let tmp = temp_dir();
        let repo = tmp.path().join("lib");
        init_jj_colocate(&repo);
        // Fresh jj repo: no remotes, no main bookmark.
        assert!(!jj_has_any_remote(&repo));
        assert!(!jj_bookmark_exists(&repo, "main"));

        for _ in 0..3 {
            fetch_remote(&repo, "origin").unwrap();
            let _ = jj_main_bookmark(&repo).unwrap();
            let _ = is_ancestor_of_main(&repo, "root()").unwrap();
        }

        let msgs = msgs_for(&repo);
        assert_eq!(
            msgs.len(),
            2,
            "expected one no-remotes + one no-main warn, got {msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("no git remotes")),
            "missing remotes warn: {msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("no `main` or `master` bookmark")),
            "missing main warn: {msgs:?}"
        );
    }

    #[test]
    fn jj_bookmark_exists_false_for_missing_name() {
        if !which_bin("jj") {
            return;
        }
        let tmp = temp_dir();
        let repo = tmp.path().join("lib");
        init_jj_colocate(&repo);
        assert!(!jj_bookmark_exists(&repo, "main"));
        assert!(!jj_bookmark_exists(&repo, "does-not-exist"));
    }
}
