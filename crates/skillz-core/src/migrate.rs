//! Migrate planner: unmanaged → managed recommendations (MVP safe applies).

use std::path::PathBuf;

use crate::skill::EntryKind;

/// Options for [`crate::App::migrate_plan`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrateOptions {
    pub ignore_skillfile: bool,
    /// Default A1-class (no catalog, no source) outcomes to Local authored.
    pub authored: bool,
}

/// Management state of one on-disk skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MgmtState {
    Unmanaged,
    LocalAuthored,
    LocalSource,
    LibraryLocked { clean: bool },
}

/// Catalog match against bound Libraries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogMatch {
    None,
    Single {
        library: String,
        kind: EntryKind,
        exact_match: Option<bool>,
    },
    Ambiguous,
}

/// Recommended migrate action (MVP subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrateAction {
    /// A1 — Local authored membership.
    SetLocalAuthored,
    /// A3/A6 — attach identical Owned (Lock only; keep files).
    AttachIdentical { library: String },
    /// B1 — already Library-locked and clean.
    NoOp,
    /// Deferred to prompt / later beads.
    NeedsPrompt,
}

/// One planned skill row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateSkillPlan {
    pub name: String,
    pub matrix: &'static str,
    pub mgmt: MgmtState,
    pub skillfile_source: Option<String>,
    pub catalog: CatalogMatch,
    pub recommended: MigrateAction,
}

/// Full migrate plan for a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratePlan {
    pub skills_dir: PathBuf,
    pub skillfile: Option<PathBuf>,
    pub rows: Vec<MigrateSkillPlan>,
    pub warnings: Vec<String>,
}

impl MigrateAction {
    pub fn label(&self) -> String {
        match self {
            Self::SetLocalAuthored => "set Local authored".into(),
            Self::AttachIdentical { library } => format!("attach identical Owned (`{library}`)"),
            Self::NoOp => "no-op (already managed)".into(),
            Self::NeedsPrompt => "needs prompt".into(),
        }
    }
}
