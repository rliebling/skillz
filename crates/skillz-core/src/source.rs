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

    pub fn with_rev(mut self, rev: impl Into<String>) -> Self {
        self.rev = Some(rev.into());
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        self.path = if path.is_empty() { None } else { Some(path) };
        self
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
}
