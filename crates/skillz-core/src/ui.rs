//! Thin interactive UI boundary (ADR 0004) so CLI/TUI/backends can swap.

use crate::{Result, SkillzError};

/// One row in a filterable multi-select list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    pub label: String,
    /// Free-text haystack (usually name + library).
    pub search_text: String,
    pub tags: Vec<String>,
}

impl SelectItem {
    pub fn from_label(label: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            search_text: label.clone(),
            label,
            tags: Vec::new(),
        }
    }

    /// Match a filter query: `tag:foo` / `#foo` match tags only; otherwise
    /// case-insensitive substring against search_text and tags.
    pub fn matches_filter(&self, query: &str) -> bool {
        let q = query.trim();
        if q.is_empty() {
            return true;
        }
        let lower = q.to_lowercase();
        if let Some(tag_q) = lower
            .strip_prefix("tag:")
            .or_else(|| lower.strip_prefix('#'))
        {
            let tag_q = tag_q.trim();
            if tag_q.is_empty() {
                return !self.tags.is_empty();
            }
            return self
                .tags
                .iter()
                .any(|t| t.to_lowercase().contains(tag_q));
        }
        if self.search_text.to_lowercase().contains(&lower) {
            return true;
        }
        self.tags
            .iter()
            .any(|t| t.to_lowercase().contains(&lower))
    }
}

/// Interactive prompts used by `-i` flows. Core stays testable with fakes.
pub trait Prompter {
    fn multi_select(&mut self, title: &str, items: &[String]) -> Result<Vec<usize>>;
    /// Filterable multi-select; default maps to [`Self::multi_select`] on labels.
    fn multi_select_filterable(
        &mut self,
        title: &str,
        items: &[SelectItem],
    ) -> Result<Vec<usize>> {
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        self.multi_select(title, &labels)
    }
    fn choose(&mut self, title: &str, options: &[String]) -> Result<usize>;
}

/// Non-interactive prompler for tests: selects all / first.
#[derive(Debug, Default)]
pub struct SelectAllPrompter;

impl Prompter for SelectAllPrompter {
    fn multi_select(&mut self, _title: &str, items: &[String]) -> Result<Vec<usize>> {
        Ok((0..items.len()).collect())
    }

    fn choose(&mut self, _title: &str, options: &[String]) -> Result<usize> {
        if options.is_empty() {
            return Err(SkillzError::Message("no options".into()));
        }
        Ok(0)
    }
}

/// Test prompler that selects only the first N items (or cancels with empty).
#[derive(Debug)]
pub struct SelectFirstNPrompter {
    pub n: usize,
}

impl Prompter for SelectFirstNPrompter {
    fn multi_select(&mut self, _title: &str, items: &[String]) -> Result<Vec<usize>> {
        Ok((0..self.n.min(items.len())).collect())
    }

    fn choose(&mut self, _title: &str, options: &[String]) -> Result<usize> {
        if options.is_empty() {
            return Err(SkillzError::Message("no options".into()));
        }
        Ok(0)
    }
}

/// Test prompler that cancels multi-select (same as TUI quit).
#[derive(Debug, Default)]
pub struct CancelPrompter;

impl Prompter for CancelPrompter {
    fn multi_select(&mut self, _title: &str, _items: &[String]) -> Result<Vec<usize>> {
        Err(SkillzError::Message("cancelled".into()))
    }

    fn choose(&mut self, _title: &str, _options: &[String]) -> Result<usize> {
        Err(SkillzError::Message("cancelled".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_item_filter_general_and_tag() {
        let item = SelectItem {
            label: "ecto-thinking (personal) [elixir,phoenix]".into(),
            search_text: "ecto-thinking personal".into(),
            tags: vec!["elixir".into(), "phoenix".into()],
        };
        assert!(item.matches_filter(""));
        assert!(item.matches_filter("ecto"));
        assert!(item.matches_filter("ELIXIR"));
        assert!(item.matches_filter("tag:phoenix"));
        assert!(item.matches_filter("#eli"));
        assert!(!item.matches_filter("tag:rust"));
        assert!(!item.matches_filter("missing"));
    }
}
