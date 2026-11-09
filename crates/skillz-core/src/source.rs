//! Upstream `github:owner/repo[/path][@rev]` source grammar.

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
        let (path, rev) = if let Some(kind) = segments.first().copied() {
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
    fn join_path_prefix_combines() {
        let s = GithubSource::parse("github:o/r/skills").unwrap();
        assert_eq!(
            s.join_path_prefix(Some("engineering")).as_deref(),
            Some("skills/engineering")
        );
        assert_eq!(join_repo_paths(None, Some("a/b")).as_deref(), Some("a/b"));
    }
}
