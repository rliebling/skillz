//! Upstream source grammars: `github:…` and Tool-Provided `cmd:…`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Result, SkillzError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubSource {
    pub owner: String,
    pub repo: String,
    /// Optional path within the repo to a skill (or skill parent tree).
    pub path: Option<String>,
    pub rev: Option<String>,
}

impl GithubSource {
    pub fn parse(raw: &str) -> Result<Self> {
        let Some(rest) = raw.strip_prefix("github:") else {
            return Err(SkillzError::Message(format!(
                "expected github:owner/repo…, got `{raw}`"
            )));
        };
        Self::from_owner_repo_rest(rest)
    }

    /// Accept `github:…` or `https://github.com/owner/repo…` and normalize.
    pub fn parse_input(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if raw.starts_with("github:") {
            return Self::parse(raw);
        }
        Self::parse_https_url(raw)
    }

    fn parse_https_url(raw: &str) -> Result<Self> {
        let rest = raw
            .strip_prefix("https://github.com/")
            .or_else(|| raw.strip_prefix("http://github.com/"))
            .ok_or_else(|| {
                SkillzError::Message(format!(
                    "expected github:owner/repo… or https://github.com/owner/repo…, got `{raw}`"
                ))
            })?;
        let rest = rest.trim_start_matches('/');
        // Accept github-style `@rev` on HTTPS URLs too:
        // `https://github.com/o/r@v2.1.0`, `https://github.com/o/r/@v2.1.0`.
        let (rest, at_rev) = match rest.rsplit_once('@') {
            Some((path, rev)) if !rev.is_empty() && !rev.contains('/') => {
                (path.trim_end_matches('/'), Some(rev.to_string()))
            }
            _ => (rest, None),
        };
        let mut segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() < 2 {
            return Err(SkillzError::Message(format!(
                "github URL missing owner/repo: `{raw}`"
            )));
        }
        let owner = segments.remove(0).to_string();
        let mut repo = segments.remove(0).to_string();
        if let Some(stripped) = repo.strip_suffix(".git") {
            repo = stripped.to_string();
        }
        // Drop trailing .git-only form; then optional /tree|/blob|/commit/REV[/path]
        let (path, tree_rev) = if let Some(kind) = segments.first().copied() {
            match kind {
                "tree" | "blob" | "commit" if segments.len() >= 2 => {
                    let rev = segments[1].to_string();
                    let path_segs = &segments[2..];
                    let path = if path_segs.is_empty() {
                        None
                    } else {
                        Some(path_segs.join("/"))
                    };
                    (path, Some(rev))
                }
                _ => {
                    // Treat remaining as path (no rev in URL).
                    (Some(segments.join("/")), None)
                }
            }
        } else {
            (None, None)
        };
        // Prefer GitHub UI `/tree|blob|commit/REV` when present; else `@rev`.
        let rev = tree_rev.or(at_rev);
        Ok(Self {
            owner,
            repo,
            path,
            rev,
        })
    }

    fn from_owner_repo_rest(rest: &str) -> Result<Self> {
        let (path_part, rev) = match rest.rsplit_once('@') {
            Some((path, rev)) if !rev.is_empty() && !rev.contains('/') => {
                (path, Some(rev.to_string()))
            }
            _ => (rest, None),
        };
        let mut segments = path_part.split('/').filter(|s| !s.is_empty());
        let owner = segments
            .next()
            .ok_or_else(|| SkillzError::Message("github source missing owner".into()))?
            .to_string();
        let repo = segments
            .next()
            .ok_or_else(|| SkillzError::Message("github source missing repo".into()))?
            .to_string();
        let path_segments: Vec<_> = segments.map(|s| s.to_string()).collect();
        let path = if path_segments.is_empty() {
            None
        } else {
            Some(path_segments.join("/"))
        };
        Ok(Self {
            owner,
            repo,
            path,
            rev,
        })
    }

    pub fn cache_key(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    pub fn clone_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.repo)
    }

    /// Render canonical source string (rev included when present).
    pub fn render(&self) -> String {
        let mut s = format!("github:{}/{}", self.owner, self.repo);
        if let Some(path) = &self.path {
            s.push('/');
            s.push_str(path);
        }
        if let Some(rev) = &self.rev {
            s.push('@');
            s.push_str(rev);
        }
        s
    }

    /// Identity without revision (owner/repo[/path]) for collision checks.
    pub fn identity_key(&self) -> String {
        let mut s = format!("{}/{}", self.owner, self.repo);
        if let Some(path) = &self.path {
            s.push('/');
            s.push_str(path);
        }
        s
    }

    pub fn with_rev(mut self, rev: impl Into<String>) -> Self {
        self.rev = Some(rev.into());
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        self.path = if path.is_empty() { None } else { Some(path) };
        self
    }

    /// Join an additional discovery prefix onto this source's path.
    pub fn join_path_prefix(&self, extra: Option<&str>) -> Option<String> {
        join_repo_paths(self.path.as_deref(), extra)
    }
}

/// Tool-Provided Upstream source: command template that materializes a skill.
///
/// Grammar: `cmd:<template>[|version:<version-cmd>][@<tool-version>]`
///
/// Placeholders in the template:
/// - `{dir}` — Installation path (`…/skills/<name>`); install-dir tools write
///   `SKILL.md` here.
/// - `{skills_dir}` — parent of `{dir}` (the skills root); skills-root tools
///   create `<skills_dir>/<name>/SKILL.md`.
/// - `{name}` — skill name.
///
/// After the tool runs, [`normalize_cmd_install`] flattens a nested
/// `{dir}/{name}/` tree into the Installation when present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdSource {
    /// Install command template (no `cmd:` prefix; may contain placeholders).
    pub template: String,
    /// Optional version probe command; default is `{argv0} --version`.
    pub version_cmd: Option<String>,
    /// Observational tool version (Locks only; catalogs omit this).
    pub rev: Option<String>,
}

impl CmdSource {
    pub fn parse(raw: &str) -> Result<Self> {
        let Some(rest) = raw.strip_prefix("cmd:") else {
            return Err(SkillzError::Message(format!(
                "expected cmd:<template>…, got `{raw}`"
            )));
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(SkillzError::Message("cmd: source is missing a template".into()));
        }

        let (body, rev) = match rest.rsplit_once('@') {
            Some((body, rev))
                if !rev.is_empty()
                    && !rev.contains(|c: char| c.is_whitespace())
                    && !rev.contains('|')
                    && !body.is_empty() =>
            {
                (body, Some(rev.to_string()))
            }
            _ => (rest, None),
        };

        let (template, version_cmd) = match body.split_once("|version:") {
            Some((template, version_cmd)) => {
                let template = template.trim();
                let version_cmd = version_cmd.trim();
                if template.is_empty() {
                    return Err(SkillzError::Message(
                        "cmd: source is missing a template before |version:".into(),
                    ));
                }
                if version_cmd.is_empty() {
                    return Err(SkillzError::Message(
                        "cmd: |version: override is empty".into(),
                    ));
                }
                (template.to_string(), Some(version_cmd.to_string()))
            }
            None => (body.trim().to_string(), None),
        };

        if template.is_empty() {
            return Err(SkillzError::Message("cmd: source is missing a template".into()));
        }

        Ok(Self {
            template,
            version_cmd,
            rev,
        })
    }

    pub fn render_catalog(&self) -> String {
        let mut s = format!("cmd:{}", self.template);
        if let Some(version_cmd) = &self.version_cmd {
            s.push_str("|version:");
            s.push_str(version_cmd);
        }
        s
    }

    pub fn render(&self) -> String {
        let mut s = self.render_catalog();
        if let Some(rev) = &self.rev {
            s.push('@');
            s.push_str(rev);
        }
        s
    }

    pub fn with_rev(mut self, rev: impl Into<String>) -> Self {
        self.rev = Some(rev.into());
        self
    }

    fn substitute(&self, dir: &Path, name: &str) -> Result<String> {
        let mut rendered = self
            .template
            .replace("{dir}", &dir.display().to_string())
            .replace("{name}", name);
        if rendered.contains("{skills_dir}") {
            let skills_dir = dir.parent().filter(|p| !p.as_os_str().is_empty()).ok_or_else(
                || {
                    SkillzError::Message(format!(
                        "Installation path {} has no parent; cannot expand {{skills_dir}}",
                        dir.display()
                    ))
                },
            )?;
            rendered = rendered.replace("{skills_dir}", &skills_dir.display().to_string());
        }
        Ok(rendered)
    }

    fn version_probe_command(&self) -> Result<String> {
        if let Some(cmd) = &self.version_cmd {
            return Ok(cmd.clone());
        }
        let argv0 = split_argv(&self.template)?
            .into_iter()
            .next()
            .ok_or_else(|| SkillzError::Message("cmd: template has no argv0".into()))?;
        Ok(format!("{argv0} --version"))
    }

    /// Run the version probe and normalize stdout into a Lock `@rev` token.
    pub fn probe_version(&self) -> Result<String> {
        let cmd = self.version_probe_command()?;
        let output = run_argv(&split_argv(&cmd)?, &format!("version probe `{cmd}`"))?;
        normalize_tool_version(&output).ok_or_else(|| {
            SkillzError::Message(format!(
                "version probe `{cmd}` produced no usable version on stdout"
            ))
        })
    }

    /// Eagerly validate install + version probe (Library catalog / libcommit).
    ///
    /// Runs the install template into `dir`, normalizes nested layouts, requires
    /// `SKILL.md`, then returns the probed tool version for the Lock.
    pub fn validate_install_and_probe(&self, dir: &Path, name: &str) -> Result<String> {
        self.run_install(dir, name)?;
        self.probe_version()
    }

    /// Materialize the skill into `dir` by running the install template, then
    /// normalize so `SKILL.md` lands at the Installation root.
    pub fn run_install(&self, dir: &Path, name: &str) -> Result<()> {
        if let Some(parent) = dir.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SkillzError::Message(format!(
                    "failed to create {}: {e}",
                    parent.display()
                ))
            })?;
        }
        fs::create_dir_all(dir).map_err(|e| {
            SkillzError::Message(format!("failed to create {}: {e}", dir.display()))
        })?;
        let rendered = self.substitute(dir, name)?;
        let argv = split_argv(&rendered)?;
        let _ = run_argv(
            &argv,
            &format!("cmd: install `{}`", self.template),
        )?;
        normalize_cmd_install(dir, name)
    }
}

/// Ensure `dir/SKILL.md` exists after a `cmd:` install.
///
/// Skills-root tools given `{dir}` (or leftovers beside a nested write) produce
/// `{dir}/{name}/SKILL.md`. When that nest exists it is treated as the tool's
/// fresh output and hoisted into `dir`.
pub fn normalize_cmd_install(dir: &Path, name: &str) -> Result<()> {
    let nest = dir.join(name);
    let nested_skill = nest.join("SKILL.md");
    if nested_skill.is_file() {
        hoist_tree_into(&nest, dir)?;
        fs::remove_dir_all(&nest).map_err(|e| {
            SkillzError::Message(format!(
                "failed to remove nested install {}: {e}",
                nest.display()
            ))
        })?;
    }
    if !dir.join("SKILL.md").is_file() {
        return Err(SkillzError::Message(format!(
            "cmd: install for `{name}` did not create SKILL.md under {} \
             (also checked {}/{name}/SKILL.md for skills-root layout)",
            dir.display(),
            dir.display()
        )));
    }
    Ok(())
}

/// Copy every file/dir from `from` into `to` (overwriting), preserving relative paths.
fn hoist_tree_into(from: &Path, to: &Path) -> Result<()> {
    let files = list_files_under(from)?;
    for rel in files {
        let src = from.join(&rel);
        let dst = to.join(&rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SkillzError::Message(format!(
                    "failed to create {}: {e}",
                    parent.display()
                ))
            })?;
        }
        if dst.exists() {
            if dst.is_dir() {
                fs::remove_dir_all(&dst).map_err(|e| {
                    SkillzError::Message(format!(
                        "failed to replace {}: {e}",
                        dst.display()
                    ))
                })?;
            } else {
                fs::remove_file(&dst).map_err(|e| {
                    SkillzError::Message(format!(
                        "failed to replace {}: {e}",
                        dst.display()
                    ))
                })?;
            }
        }
        fs::copy(&src, &dst).map_err(|e| {
            SkillzError::Message(format!(
                "failed to hoist {} → {}: {e}",
                src.display(),
                dst.display()
            ))
        })?;
    }
    Ok(())
}

fn list_files_under(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    fn walk(dir: &Path, prefix: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        let entries = fs::read_dir(dir).map_err(|e| {
            SkillzError::Message(format!("failed to read {}: {e}", dir.display()))
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                SkillzError::Message(format!("failed to read dir entry: {e}"))
            })?;
            let path = entry.path();
            let rel = prefix.join(entry.file_name());
            if path.is_dir() {
                walk(&path, &rel, out)?;
            } else {
                out.push(rel);
            }
        }
        Ok(())
    }
    walk(root, Path::new(""), &mut out)?;
    Ok(out)
}

fn split_argv(command: &str) -> Result<Vec<String>> {
    let parts: Vec<String> = command
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    if parts.is_empty() {
        return Err(SkillzError::Message(
            "command template is empty after placeholder substitution".into(),
        ));
    }
    Ok(parts)
}

fn run_argv(argv: &[String], label: &str) -> Result<String> {
    let output = Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .map_err(|e| {
            SkillzError::Message(format!(
                "{label} failed to start `{}`: {e}",
                argv[0]
            ))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        return Err(SkillzError::Message(format!(
            "{label} failed ({}): {detail}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Prefer a leading digit token (semver-ish); otherwise slugify the first line.
pub fn normalize_tool_version(output: &str) -> Option<String> {
    let line = output.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    for tok in line.split_whitespace().rev() {
        let tok = tok.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-');
        let tok = match tok.strip_prefix('v') {
            Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => rest,
            _ => tok,
        };
        if tok.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return Some(tok.to_string());
        }
    }
    let slug: String = line
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();
    if slug.is_empty() {
        None
    } else {
        Some(slug)
    }
}

pub fn join_repo_paths(base: Option<&str>, extra: Option<&str>) -> Option<String> {
    match (base.filter(|s| !s.is_empty()), extra.filter(|s| !s.is_empty())) {
        (None, None) => None,
        (Some(a), None) => Some(a.trim_matches('/').to_string()),
        (None, Some(b)) => Some(b.trim_matches('/').to_string()),
        (Some(a), Some(b)) => Some(format!(
            "{}/{}",
            a.trim_matches('/'),
            b.trim_matches('/')
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forms() {
        let s = GithubSource::parse("github:owner/repo").unwrap();
        assert_eq!(s.owner, "owner");
        assert_eq!(s.repo, "repo");
        assert!(s.path.is_none());
        assert!(s.rev.is_none());

        let s = GithubSource::parse("github:o/r/skills/grilling@abc").unwrap();
        assert_eq!(s.path.as_deref(), Some("skills/grilling"));
        assert_eq!(s.rev.as_deref(), Some("abc"));
        assert_eq!(s.render(), "github:o/r/skills/grilling@abc");
    }

    #[test]
    fn normalizes_https_urls() {
        let s = GithubSource::parse_input("https://github.com/mattpocock/skills").unwrap();
        assert_eq!(s.render(), "github:mattpocock/skills");

        let s = GithubSource::parse_input("https://github.com/mattpocock/skills.git").unwrap();
        assert_eq!(s.owner, "mattpocock");
        assert_eq!(s.repo, "skills");

        let s = GithubSource::parse_input(
            "https://github.com/mattpocock/skills/tree/main/skills/engineering",
        )
        .unwrap();
        assert_eq!(s.path.as_deref(), Some("skills/engineering"));
        assert_eq!(s.rev.as_deref(), Some("main"));
        assert_eq!(
            s.render(),
            "github:mattpocock/skills/skills/engineering@main"
        );
    }

    #[test]
    fn normalizes_https_at_rev_and_tree_tag_urls() {
        let s = GithubSource::parse_input("https://github.com/Leonxlnx/taste-skill@v2.1.0").unwrap();
        assert_eq!(s.owner, "Leonxlnx");
        assert_eq!(s.repo, "taste-skill");
        assert!(s.path.is_none());
        assert_eq!(s.rev.as_deref(), Some("v2.1.0"));
        assert_eq!(s.render(), "github:Leonxlnx/taste-skill@v2.1.0");

        let s =
            GithubSource::parse_input("https://github.com/Leonxlnx/taste-skill/@v2.1.0").unwrap();
        assert_eq!(s.repo, "taste-skill");
        assert!(s.path.is_none());
        assert_eq!(s.rev.as_deref(), Some("v2.1.0"));

        let s = GithubSource::parse_input(
            "https://github.com/Leonxlnx/taste-skill/tree/v2.1.0/skills",
        )
        .unwrap();
        assert_eq!(s.path.as_deref(), Some("skills"));
        assert_eq!(s.rev.as_deref(), Some("v2.1.0"));
        assert_eq!(
            s.render(),
            "github:Leonxlnx/taste-skill/skills@v2.1.0"
        );
    }

    #[test]
    fn join_path_prefix_combines() {
        let s = GithubSource::parse("github:o/r/skills").unwrap();
        assert_eq!(
            s.join_path_prefix(Some("engineering")).as_deref(),
            Some("skills/engineering")
        );
        assert_eq!(join_repo_paths(None, Some("a/b")).as_deref(), Some("a/b"));
    }

    #[test]
    fn parses_cmd_template_version_and_rev() {
        let s = CmdSource::parse("cmd:rexpad skill install --dir {dir}").unwrap();
        assert_eq!(s.template, "rexpad skill install --dir {dir}");
        assert!(s.version_cmd.is_none());
        assert!(s.rev.is_none());
        assert_eq!(s.render_catalog(), "cmd:rexpad skill install --dir {dir}");

        let s = CmdSource::parse(
            "cmd:rexpad skill install --dir {dir}|version:rexpad version@1.2.3",
        )
        .unwrap();
        assert_eq!(s.version_cmd.as_deref(), Some("rexpad version"));
        assert_eq!(s.rev.as_deref(), Some("1.2.3"));
        assert_eq!(
            s.render(),
            "cmd:rexpad skill install --dir {dir}|version:rexpad version@1.2.3"
        );
        assert_eq!(
            s.render_catalog(),
            "cmd:rexpad skill install --dir {dir}|version:rexpad version"
        );
    }

    #[test]
    fn normalize_tool_version_prefers_digit_token() {
        assert_eq!(
            normalize_tool_version("fake-tool 9.9.9\n").as_deref(),
            Some("9.9.9")
        );
        assert_eq!(normalize_tool_version("v2.0.0").as_deref(), Some("2.0.0"));
    }

    #[test]
    fn substitute_expands_skills_dir() {
        let s = CmdSource::parse("cmd:tool install --dir {skills_dir} --name {name}").unwrap();
        let rendered = s
            .substitute(Path::new("/tmp/skills/demo"), "demo")
            .unwrap();
        assert_eq!(rendered, "tool install --dir /tmp/skills --name demo");
    }

    #[test]
    fn substitute_skills_dir_requires_parent() {
        let s = CmdSource::parse("cmd:tool install --dir {skills_dir}").unwrap();
        let err = s.substitute(Path::new("demo"), "demo").unwrap_err();
        assert!(
            err.to_string().contains("{skills_dir}"),
            "unexpected err: {err}"
        );
    }

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).unwrap();
        }
    }

    #[test]
    fn install_dir_tool_writes_flat_skill_md() {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("install-flat.sh");
        write_executable(
            &script,
            "#!/bin/sh\n# usage: install-flat.sh --dir DIR\ndir=\"$2\"\nprintf '%s\\n' '---' 'name: demo' '---' 'flat' > \"$dir/SKILL.md\"\n",
        );
        let install = root.path().join("skills").join("demo");
        let src = format!(
            "cmd:{} --dir {{dir}}|version:echo 1.0.0",
            script.display()
        );
        let parsed = CmdSource::parse(&src).unwrap();
        parsed.run_install(&install, "demo").unwrap();
        assert_eq!(
            fs::read_to_string(install.join("SKILL.md")).unwrap().contains("flat"),
            true
        );
        assert!(!install.join("demo").exists());
    }

    #[test]
    fn skills_root_tool_with_dir_hoists_nested_tree() {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("install-nest.sh");
        write_executable(
            &script,
            "#!/bin/sh\n# skills-root style given Installation path: nest under DIR/NAME\ndir=\"$2\"\nname=\"$4\"\nmkdir -p \"$dir/$name\"\nprintf '%s\\n' '---' \"name: $name\" '---' 'nested' > \"$dir/$name/SKILL.md\"\n",
        );
        let install = root.path().join("skills").join("demo");
        // Pre-existing flat file that would previously mask the nest.
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("SKILL.md"), "stale flat\n").unwrap();
        let src = format!(
            "cmd:{} --dir {{dir}} --name {{name}}|version:echo 1.0.0",
            script.display()
        );
        let parsed = CmdSource::parse(&src).unwrap();
        parsed.run_install(&install, "demo").unwrap();
        let body = fs::read_to_string(install.join("SKILL.md")).unwrap();
        assert!(body.contains("nested"), "expected hoisted body, got {body}");
        assert!(!body.contains("stale"), "stale flat should be overwritten");
        assert!(
            !install.join("demo").exists(),
            "nested directory should be removed"
        );
    }

    #[test]
    fn skills_root_tool_with_skills_dir_writes_flat() {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("install-root.sh");
        write_executable(
            &script,
            "#!/bin/sh\n# skills-root style: create DIR/NAME/SKILL.md\nroot=\"$2\"\nname=\"$4\"\nmkdir -p \"$root/$name\"\nprintf '%s\\n' '---' \"name: $name\" '---' 'from-root' > \"$root/$name/SKILL.md\"\n",
        );
        let install = root.path().join("skills").join("demo");
        let src = format!(
            "cmd:{} --dir {{skills_dir}} --name {{name}}|version:echo 1.0.0",
            script.display()
        );
        let parsed = CmdSource::parse(&src).unwrap();
        parsed.run_install(&install, "demo").unwrap();
        let body = fs::read_to_string(install.join("SKILL.md")).unwrap();
        assert!(body.contains("from-root"), "got {body}");
        assert!(!install.join("demo").exists());
    }

    #[test]
    fn normalize_errors_when_neither_flat_nor_nested() {
        let root = tempfile::tempdir().unwrap();
        let install = root.path().join("skills").join("demo");
        fs::create_dir_all(&install).unwrap();
        let err = normalize_cmd_install(&install, "demo").unwrap_err();
        assert!(
            err.to_string().contains("did not create SKILL.md"),
            "unexpected err: {err}"
        );
    }
}
