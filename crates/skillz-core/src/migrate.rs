//! Migrate planner: unmanaged → managed recommendations.

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

/// Recommended / chosen migrate action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrateAction {
    SetLocalAuthored,
    SetLocalSource { source: String },
    AttachIdentical { library: String },
    AttachUpstream { library: String },
    UseLibraryFiles { library: String },
    PublishToLibrary { library: String },
    KeepLocalAuthored,
    NoOp,
    /// Indeterminate — requires prompt (or `--no-prompt` abort).
    NeedsPrompt,
}

/// Situation class for “all remaining” prompt answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MigrateSituationClass {
    A1NoSignals,
    A2SkillfileOnly,
    A4OwnedDiffers,
    A5UpstreamUncompared,
    A7OwnedDiffersWithSkillfile,
    A9UpstreamIdentityMismatch,
    A10Ambiguous,
    B3LocalToOwnedIdentical,
    B4LocalOwnedDiffers,
    B5LocalToSource,
    Other,
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
    pub situation: MigrateSituationClass,
    /// Alternatives when the row needs a prompt.
    pub alternatives: Vec<MigrateAction>,
}

/// Full migrate plan for a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratePlan {
    pub skills_dir: PathBuf,
    pub skillfile: Option<PathBuf>,
    pub rows: Vec<MigrateSkillPlan>,
    pub warnings: Vec<String>,
}

/// How a prompt resolver answers one indeterminate skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigratePromptAnswer {
    /// Apply this action for this skill only.
    This(MigrateAction),
    /// Apply this action for this skill and all remaining in the same situation class.
    AllRemaining(MigrateAction),
    /// Stop prompting; keep already-applied skills (partial apply).
    Cancel,
}

impl MigrateAction {
    pub fn label(&self) -> String {
        match self {
            Self::SetLocalAuthored => "set Local authored".into(),
            Self::SetLocalSource { source } => format!("set Local + source (`{source}`)"),
            Self::AttachIdentical { library } => {
                format!("attach identical (`{library}`)")
            }
            Self::AttachUpstream { library } => format!("attach Upstream (`{library}`)"),
            Self::UseLibraryFiles { library } => {
                format!("use Library files (`{library}`)")
            }
            Self::PublishToLibrary { library } => {
                format!("publish local → Library (`{library}`)")
            }
            Self::KeepLocalAuthored => "keep Local authored".into(),
            Self::NoOp => "no-op (already managed)".into(),
            Self::NeedsPrompt => "needs prompt".into(),
        }
    }

    pub fn is_determinate(&self) -> bool {
        !matches!(self, Self::NeedsPrompt)
    }
}

impl MigrateSituationClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A1NoSignals => "A1",
            Self::A2SkillfileOnly => "A2",
            Self::A4OwnedDiffers => "A4",
            Self::A5UpstreamUncompared => "A5",
            Self::A7OwnedDiffersWithSkillfile => "A7",
            Self::A9UpstreamIdentityMismatch => "A9",
            Self::A10Ambiguous => "A10",
            Self::B3LocalToOwnedIdentical => "B3",
            Self::B4LocalOwnedDiffers => "B4",
            Self::B5LocalToSource => "B5",
            Self::Other => "?",
        }
    }
}
