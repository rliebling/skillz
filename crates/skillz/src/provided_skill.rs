//! Tool-Provided agent skill embedded in the skillz binary.
//!
//! `skillz skill install --dir {dir}` writes `SKILL.md` into the Installation
//! directory (ADR 0005 `cmd:` contract). `skillz skill show` prints the same
//! text for human inspection.

use std::path::{Path, PathBuf};

/// Canonical skill body shipped with this binary.
pub const SKILL_MD: &str = include_str!("../skills/skillz/SKILL.md");

const SKILL_NAME: &str = "skillz";

/// Default Installation directory for a bare `skillz skill install`
/// (`~/.agents/skills/skillz`).
pub fn default_install_dir() -> PathBuf {
    dirs_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agents")
        .join("skills")
        .join(SKILL_NAME)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Path where `install` writes `SKILL.md` under `dir`.
pub fn skill_md_path(dir: &Path) -> PathBuf {
    dir.join("SKILL.md")
}

/// Write embedded `SKILL.md` into `dir` (the Installation path).
pub fn install(dir: &Path) -> skillz_core::Result<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| {
        skillz_core::SkillzError::Message(format!(
            "failed to create {}: {e}",
            dir.display()
        ))
    })?;
    let target = skill_md_path(dir);
    std::fs::write(&target, SKILL_MD).map_err(|e| {
        skillz_core::SkillzError::Message(format!(
            "failed to write {}: {e}",
            target.display()
        ))
    })?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn skill_md_has_frontmatter_name() {
        assert!(SKILL_MD.contains("name: skillz"));
        assert!(SKILL_MD.contains("# Managing skills with skillz"));
    }

    #[test]
    fn install_writes_skill_md_directly_under_dir() {
        let dir = tempfile_dir();
        let target = install(&dir).unwrap();
        assert_eq!(target, skill_md_path(&dir));
        assert_eq!(fs::read_to_string(&target).unwrap(), SKILL_MD);
    }

    #[test]
    fn skill_md_path_is_dir_join_skill_md() {
        let dir = PathBuf::from("/tmp/skillz-install");
        assert_eq!(skill_md_path(&dir), dir.join("SKILL.md"));
    }

    fn tempfile_dir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "skillz-provided-skill-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
