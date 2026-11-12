//! `-i` orchestration over [`crate::ui::Prompter`] (testable without Ratatui).
//!
//! Drift review uses a dedicated board UI in the CLI crate; add/import still use Prompter.

use crate::ui::{Prompter, SelectItem};
use crate::{AddOutcome, AdoptAs, App, LibraryImportReport, Result};

impl App {
    /// Interactive add: multi-select among Catalog Entries.
    ///
    /// `tags` optionally pre-filters (same AND semantics as `list --tag`).
    /// The TUI supports further `/` search and `tag:` filters.
    /// Returns `(name, outcome)` for each selected skill.
    pub fn interactive_add_by_tag(
        &self,
        tags: &[String],
        library_filter: Option<&str>,
        prompler: &mut dyn Prompter,
    ) -> Result<Vec<(String, AddOutcome)>> {
        let rows = self.list_skills(library_filter, tags, true, false)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let items: Vec<SelectItem> = rows
            .iter()
            .map(|r| {
                let tags_suffix = if r.entry.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", r.entry.tags.join(","))
                };
                SelectItem {
                    label: format!("{} ({}){tags_suffix}", r.entry.name, r.entry.library),
                    search_text: format!("{} {}", r.entry.name, r.entry.library),
                    tags: r.entry.tags.clone(),
                }
            })
            .collect();
        let selected = prompler.multi_select_filterable("Add skills", &items)?;
        let mut added = Vec::new();
        for idx in selected {
            let row = &rows[idx];
            let (outcome, _) = self.add_skill(&row.entry.name, Some(&row.entry.library))?;
            added.push((row.entry.name.clone(), outcome));
        }
        Ok(added)
    }

    /// Interactive Library import: multi-select discovered skills, then upsert.
    pub fn interactive_library_import(
        &self,
        source: &str,
        library_name: &str,
        path_filter: Option<&str>,
        tags: &[String],
        prompler: &mut dyn Prompter,
    ) -> Result<LibraryImportReport> {
        let candidates = self.library_import_discover(source, library_name, path_filter)?;
        if candidates.is_empty() {
            return Ok(LibraryImportReport {
                library: library_name.to_string(),
                dry_run: false,
                items: Vec::new(),
            });
        }
        let labels: Vec<String> = candidates
            .iter()
            .map(|c| {
                if c.rel_path.is_empty() {
                    c.name.clone()
                } else {
                    format!("{}  {}", c.name, c.rel_path)
                }
            })
            .collect();
        let selected = prompler.multi_select("Import Upstream skills", &labels)?;
        let sources: Vec<String> = selected
            .into_iter()
            .filter_map(|i| candidates.get(i).map(|c| c.source.clone()))
            .collect();
        if sources.is_empty() {
            return Ok(LibraryImportReport {
                library: library_name.to_string(),
                dry_run: false,
                items: Vec::new(),
            });
        }
        self.library_import_filtered(
            source,
            library_name,
            path_filter,
            false,
            Some(&sources),
            tags,
        )
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
}
