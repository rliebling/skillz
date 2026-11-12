//! Upstream source grammars: `github:…` and Tool-Provided `cmd:…`.

use std::path::Path;
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
/// Placeholders in the template: `{dir}` (Installation path), `{name}` (skill name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdSource {
    /// Install command template (no `cmd:` prefix; may contain `{dir}` / `{name}`).
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

    fn substitute(&self, dir: &Path, name: &str) -> String {
        self.template
            .replace("{dir}", &dir.display().to_string())
            .replace("{name}", name)
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
    /// Runs the install template into `dir`, requires `SKILL.md`, then returns the
    /// probed tool version for the Lock.
    pub fn validate_install_and_probe(&self, dir: &Path, name: &str) -> Result<String> {
        self.run_install(dir, name)?;
        if !dir.join("SKILL.md").is_file() {
            return Err(SkillzError::Message(format!(
                "cmd: install for `{name}` did not create SKILL.md under {}",
                dir.display()
            )));
        }
        self.probe_version()
    }

    /// Materialize the skill into `dir` by running the install template.
    pub fn run_install(&self, dir: &Path, name: &str) -> Result<()> {
        if let Some(parent) = dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SkillzError::Message(format!(
                    "failed to create {}: {e}",
                    parent.display()
                ))
            })?;
        }
        std::fs::create_dir_all(dir).map_err(|e| {
            SkillzError::Message(format!("failed to create {}: {e}", dir.display()))
        })?;
        let rendered = self.substitute(dir, name);
        let argv = split_argv(&rendered)?;
        let _ = run_argv(
            &argv,
            &format!("cmd: install `{}`", self.template),
        )?;
        Ok(())
    }
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
}
