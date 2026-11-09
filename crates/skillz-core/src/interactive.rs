//! `-i` orchestration over [`crate::ui::Prompter`] (testable without Ratatui).

use crate::ui::Prompter;
use crate::{AdoptAs, App, DriftReason, Result, SkillzError};

impl App {
    /// Interactive add-by-tag: multi-select among matching Catalog Entries.
    pub fn interactive_add_by_tag(
        &self,
        tags: &[String],
        library_filter: Option<&str>,
        prompler: &mut dyn Prompter,
    ) -> Result<Vec<String>> {
        let rows = self.list_skills(library_filter, tags, true)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let labels: Vec<String> = rows
            .iter()
            .map(|r| format!("{} ({})", r.entry.name, r.entry.library))
            .collect();
        let selected = prompler.multi_select("Add skills", &labels)?;
        let mut added = Vec::new();
        for idx in selected {
            let row = &rows[idx];
            self.add_skill(&row.entry.name, Some(&row.entry.library))?;
            added.push(row.entry.name.clone());
        }
        Ok(added)
    }

    /// Interactive import adopt for leftovers.
    pub fn interactive_import_adopt(
        &self,
        library_name: &str,
        prompler: &mut dyn Prompter,
    ) -> Result<Vec<(String, String)>> {
        let plan = self.project_import_dry_run()?;
        let mut results = Vec::new();
        let options = vec![
            "upstream".to_string(),
            "authored".to_string(),
            "skip".to_string(),
        ];
        for name in &plan.leftovers {
            let choice = prompler.choose(&format!("Adopt `{name}`"), &options)?;
            let adopt = match choice {
                0 => AdoptAs::Upstream,
                1 => AdoptAs::Authored,
                _ => AdoptAs::Skip,
            };
            let spec = match adopt {
                AdoptAs::Upstream => format!("{name}=upstream"),
                AdoptAs::Authored => format!("{name}=authored"),
                AdoptAs::Skip => format!("{name}=skip"),
            };
            let one = self.project_adopt_local(library_name, &[spec])?;
            results.extend(one);
        }
        Ok(results)
    }

    /// Interactive drift next-actions for the current scope.
    pub fn interactive_drift(&self, prompler: &mut dyn Prompter) -> Result<()> {
        let items = self.detect_drift()?;
        if items.is_empty() {
            return Ok(());
        }
        let options = vec![
            "libcommit (branch/overlay)".to_string(),
            "promote".to_string(),
            "skip".to_string(),
        ];
        for item in items {
            let title = format!(
                "Drift `{}` ({})",
                item.name,
                match item.reason {
                    DriftReason::Unlocked => "unlocked",
                    DriftReason::MissingFiles => "missing",
                    DriftReason::HashMismatch { .. } => "hash mismatch",
                }
            );
            let choice = prompler.choose(&title, &options)?;
            match choice {
                0 => {
                    let _ = self.libcommit_branch(&item.name, None, Some("interactive libcommit"))?;
                }
                1 => {
                    let _ = self.promote_skill(&item.name).map_err(|e| {
                        SkillzError::Message(format!(
                            "promote `{}` failed: {e}",
                            item.name
                        ))
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}
