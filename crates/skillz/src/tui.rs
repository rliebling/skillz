//! Ratatui-backed interactive prompts (thin boundary over skillz-core::Prompter).

use std::io::{self, stdout};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use skillz_core::{Prompter, Result, SkillzError};

pub struct RatatuiPrompter;

impl Prompter for RatatuiPrompter {
    fn multi_select(&mut self, title: &str, items: &[String]) -> Result<Vec<usize>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        run_list(title, items, true)
    }

    fn choose(&mut self, title: &str, options: &[String]) -> Result<usize> {
        if options.is_empty() {
            return Err(SkillzError::Message("no options".into()));
        }
        let selected = run_list(title, options, false)?;
        selected
            .into_iter()
            .next()
            .ok_or_else(|| SkillzError::Message("nothing selected".into()))
    }
}

fn run_list(title: &str, items: &[String], multi: bool) -> Result<Vec<usize>> {
    enable_raw_mode().map_err(|e| SkillzError::Message(format!("tui: {e}")))?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| SkillzError::Message(format!("tui: {e}")))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| SkillzError::Message(format!("tui: {e}")))?;

    let mut state = ListState::default();
    state.select(Some(0));
    let mut checked = vec![false; items.len()];
    let result = loop {
        terminal
            .draw(|frame| {
                let area = frame.area();
                let help = if multi {
                    "space toggle · enter confirm · q cancel"
                } else {
                    "enter select · q cancel"
                };
                let header = Paragraph::new(Line::from(format!("{title}  ({help})")));
                frame.render_widget(header, area);
                let list_area = ratatui::layout::Rect {
                    x: area.x,
                    y: area.y.saturating_add(1),
                    width: area.width,
                    height: area.height.saturating_sub(1),
                };
                let rows: Vec<ListItem> = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let mark = if multi {
                            if checked[i] {
                                "[x]"
                            } else {
                                "[ ]"
                            }
                        } else {
                            "   "
                        };
                        ListItem::new(format!("{mark} {item}"))
                    })
                    .collect();
                let list = List::new(rows)
                    .block(Block::default().borders(Borders::ALL).title("skillz"))
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
                frame.render_stateful_widget(list, list_area, &mut state);
            })
            .map_err(|e| SkillzError::Message(format!("tui: {e}")))?;

        if let Event::Key(key) = event::read().map_err(|e| SkillzError::Message(format!("tui: {e}")))?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    break Err(SkillzError::Message("cancelled".into()));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let i = state.selected().unwrap_or(0);
                    state.select(Some((i + 1).min(items.len() - 1)));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let i = state.selected().unwrap_or(0);
                    state.select(Some(i.saturating_sub(1)));
                }
                KeyCode::Char(' ') if multi => {
                    if let Some(i) = state.selected() {
                        checked[i] = !checked[i];
                    }
                }
                KeyCode::Enter => {
                    if multi {
                        let selected: Vec<_> = checked
                            .iter()
                            .enumerate()
                            .filter_map(|(i, c)| c.then_some(i))
                            .collect();
                        break Ok(selected);
                    } else if let Some(i) = state.selected() {
                        break Ok(vec![i]);
                    }
                }
                _ => {}
            }
        }
    };

    disable_raw_mode().ok();
    execute!(io::stdout(), LeaveAlternateScreen).ok();
    result
}
