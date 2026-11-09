//! Infer whether a path is a git or jj checkout.

use std::path::Path;

/// Version-control system detected for a Library or Project checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vcs {
    Jj,
    Git,
}

impl Vcs {
    pub fn as_str(self) -> &'static str {
        match self {
            Vcs::Jj => "jj",
            Vcs::Git => "git",
        }
    }
}

/// Infer VCS from on-disk metadata. Prefer jj when `.jj/` is present.
pub fn infer_vcs(path: &Path) -> Option<Vcs> {
    if path.join(".jj").is_dir() {
        return Some(Vcs::Jj);
    }
    if path.join(".git").exists() {
        return Some(Vcs::Git);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn prefers_jj_when_both_present() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".jj")).unwrap();
        fs::create_dir_all(root.path().join(".git")).unwrap();
        assert_eq!(infer_vcs(root.path()), Some(Vcs::Jj));
    }

    #[test]
    fn detects_git_only() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".git")).unwrap();
        assert_eq!(infer_vcs(root.path()), Some(Vcs::Git));
    }

    #[test]
    fn none_without_metadata() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(infer_vcs(root.path()), None);
    }
}
