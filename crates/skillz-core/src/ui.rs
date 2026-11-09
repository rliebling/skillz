//! Thin interactive UI boundary (ADR 0004) so CLI/TUI/backends can swap.

use crate::{Result, SkillzError};

/// Interactive prompts used by `-i` flows. Core stays testable with fakes.
pub trait Prompter {
    fn multi_select(&mut self, title: &str, items: &[String]) -> Result<Vec<usize>>;
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
