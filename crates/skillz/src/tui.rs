//! Ratatui-backed interactive UI (thin boundary over skillz-core).

use std::io::{self, stdout};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;
use skillz_core::{App, DriftAction, DriftItem, Prompter, Result, SelectItem, SkillzError};

const DRIFT_HELP: &[&str] = &[
    "Navigate",
    "  j / ↓          move down",
    "  k / ↑          move up",
    "",
    "Actions (per selected skill)",
    "  ← → / [ ] / h l   cycle planned action",
    "  s                 skip",
    "  c                 libcommit (Branch Lock / Overlay Fork)",
    "  p                 promote Branch Lock to Library main",
    "  r                 mark remove (dep options if others depend on it)",
    "",
    "Apply / quit",
    "  Enter             confirm removals (if any) then apply actions",
    "  q / Esc           quit without further applies",
    "",
    "Remove keeps the Library Catalog Entry; only Project files + Lock go away.",
    "",
    "Press ? or Esc to close this help.",
];

const LIST_MULTI_HELP: &[&str] = &[
    "Multi-select",
    "  j / ↓          move down",
    "  k / ↑          move up",
    "  Space          toggle current item",
    "  /              filter (name/tags; tag:foo or #foo for tags)",
    "  Esc            clear filter / exit filter mode",
    "  Enter          confirm selection",
    "  q / Esc        cancel (when not filtering)",
    "",
    "Press ? or Esc to close this help.",
];

const LIST_CHOOSE_HELP: &[&str] = &[
    "Choose one",
    "  j / ↓          move down",
    "  k / ↑          move up",
    "  /              filter",
    "  Enter          select highlighted item",
    "  q / Esc        cancel",
    "",
    "Press ? or Esc to close this help.",
];

pub struct RatatuiPrompter;

impl Prompter for RatatuiPrompter {
    fn multi_select(&mut self, title: &str, items: &[String]) -> Result<Vec<usize>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<SelectItem> = items.iter().map(SelectItem::from_label).collect();
        run_list(title, &rows, true)
    }

    fn multi_select_filterable(
        &mut self,
        title: &str,
        items: &[SelectItem],
    ) -> Result<Vec<usize>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        run_list(title, items, true)
    }

    fn choose(&mut self, title: &str, options: &[String]) -> Result<usize> {
        if options.is_empty() {
            return Err(SkillzError::Message("no options".into()));
        }
        let rows: Vec<SelectItem> = options.iter().map(SelectItem::from_label).collect();
        let selected = run_list(title, &rows, false)?;
        selected
            .into_iter()
            .next()
            .ok_or_else(|| SkillzError::Message("nothing selected".into()))
    }
}

#[derive(Clone)]
struct DriftRow {
    item: DriftItem,
    action: DriftAction,
    /// Result after apply, if any.
    outcome: Option<String>,
}

#[derive(Clone)]
enum Modal {
    /// Mark-time: retained skills depend on the skill being removed.
    RemoveDeps {
        index: usize,
        dependants: Vec<String>,
        forward_deps: Vec<String>,
    },
    /// Apply-time: confirm the full remove set.
    ConfirmApplyRemoves {
        names: Vec<String>,
        /// name -> dependants still retained (should be empty if mark-time handled)
        reverse_notes: Vec<(String, Vec<String>)>,
        /// name -> forward deps that will remain installed
        forward_notes: Vec<(String, Vec<String>)>,
    },
}

/// Scrollable Drift board: set an action per skill, then apply.
pub fn run_drift_board(app: &App) -> Result<()> {
    let items = app.detect_drift()?;
    if items.is_empty() {
        println!("no drift");
        return Ok(());
    }

    enable_raw_mode().map_err(|e| SkillzError::Message(format!("tui: {e}")))?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| SkillzError::Message(format!("tui: {e}")))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| SkillzError::Message(format!("tui: {e}")))?;

    let mut rows: Vec<DriftRow> = items
        .into_iter()
        .map(|item| DriftRow {
            item,
            action: DriftAction::Skip,
            outcome: None,
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(0));
    let mut modal: Option<Modal> = None;
    let mut show_help = false;
    let mut status_line = String::from(
        "j/k move · ←/→ action · r remove · enter apply · ? help · q quit",
    );

    let result = loop {
        terminal
            .draw(|frame| {
                let area = frame.area();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Min(3),
                        Constraint::Length(1),
                    ])
                    .split(area);

                let header = Paragraph::new(Line::from(vec![
                    Span::styled(
                        "skillz drift",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  — review Installations that disagree with Locks"),
                ]));
                frame.render_widget(header, chunks[0]);

                let name_width = rows
                    .iter()
                    .map(|r| r.item.name.len())
                    .max()
                    .unwrap_or(8)
                    .max(8);
                let status_width = rows
                    .iter()
                    .map(|r| r.item.reason.label().len())
                    .max()
                    .unwrap_or(8)
                    .max(12);

                let list_items: Vec<ListItem> = rows
                    .iter()
                    .map(|row| {
                        let action_label = row
                            .outcome
                            .as_deref()
                            .unwrap_or_else(|| row.action.label());
                        let action_style = match (&row.outcome, row.action) {
                            (Some(o), _) if o.starts_with("error") => {
                                Style::default().fg(Color::Red)
                            }
                            (Some(_), _) => Style::default().fg(Color::Green),
                            (_, DriftAction::Remove) => Style::default().fg(Color::Red),
                            (_, DriftAction::Libcommit | DriftAction::Promote) => {
                                Style::default().fg(Color::Cyan)
                            }
                            _ => Style::default().fg(Color::DarkGray),
                        };
                        let line = Line::from(vec![
                            Span::raw(format!(" {:<w$}  ", row.item.name, w = name_width)),
                            Span::styled(
                                format!("{:<sw$}", row.item.reason.label(), sw = status_width),
                                Style::default().fg(Color::Yellow),
                            ),
                            Span::raw("  "),
                            Span::styled(format!("[{action_label}]"), action_style),
                        ]);
                        ListItem::new(line)
                    })
                    .collect();

                let list = List::new(list_items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" NAME / DRIFT STATUS / ACTION "),
                    )
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                    .highlight_symbol("▸ ");
                frame.render_stateful_widget(list, chunks[1], &mut state);

                let footer = Paragraph::new(status_line.as_str());
                frame.render_widget(footer, chunks[2]);

                if show_help {
                    draw_help_modal(frame, area, "drift keys", DRIFT_HELP);
                } else if let Some(Modal::RemoveDeps {
                    index,
                    dependants,
                    forward_deps,
                }) = &modal
                {
                    let name = &rows[*index].item.name;
                    let mut body = format!(
                        "`{name}` is referenced by retained skill(s):\n  {}\n",
                        dependants.join(", ")
                    );
                    if !forward_deps.is_empty() {
                        body.push_str(&format!(
                            "\n`{name}` itself depends on (will remain):\n  {}\n",
                            forward_deps.join(", ")
                        ));
                    }
                    body.push_str(
                        "\n[a] also mark dependants for remove\n[r] remove anyway (only this skill)\n[n] cancel",
                    );
                    draw_options_modal(
                        frame,
                        area,
                        &format!("Remove `{name}` — dependency warning"),
                        &body,
                    );
                } else if let Some(Modal::ConfirmApplyRemoves {
                    names,
                    reverse_notes,
                    forward_notes,
                }) = &modal
                {
                    let mut body = format!("Remove {} skill(s) from this Project:\n", names.len());
                    for n in names {
                        body.push_str(&format!("  - {n}\n"));
                    }
                    body.push_str("\nLibrary Catalog Entries are kept.\n");
                    if !reverse_notes.is_empty() {
                        body.push_str("\nStill referenced by retained skills:\n");
                        for (n, deps) in reverse_notes {
                            body.push_str(&format!("  {n} ← {}\n", deps.join(", ")));
                        }
                    }
                    if !forward_notes.is_empty() {
                        body.push_str("\nDependencies that will remain installed:\n");
                        for (n, deps) in forward_notes {
                            body.push_str(&format!("  {n} → {}\n", deps.join(", ")));
                        }
                    }
                    draw_confirm_modal(
                        frame,
                        area,
                        "Confirm removals",
                        &body,
                    );
                }
            })
            .map_err(|e| SkillzError::Message(format!("tui: {e}")))?;

        if let Event::Key(key) =
            event::read().map_err(|e| SkillzError::Message(format!("tui: {e}")))?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if show_help {
                // Any key dismisses help (including ?).
                show_help = false;
                continue;
            }

            if let Some(Modal::RemoveDeps {
                index,
                dependants,
                ..
            }) = modal.clone()
            {
                match key.code {
                    KeyCode::Char('?') => {
                        show_help = true;
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        rows[index].action = DriftAction::Remove;
                        rows[index].outcome = None;
                        for dep_name in &dependants {
                            if let Some(row) = rows.iter_mut().find(|r| r.item.name == *dep_name) {
                                row.action = DriftAction::Remove;
                                row.outcome = None;
                            }
                        }
                        status_line = format!(
                            "queued remove for `{}` + dependants — press enter to apply",
                            rows[index].item.name
                        );
                        modal = None;
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Enter => {
                        rows[index].action = DriftAction::Remove;
                        rows[index].outcome = None;
                        status_line = format!(
                            "queued remove for `{}` anyway — press enter to apply",
                            rows[index].item.name
                        );
                        modal = None;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
                        status_line = "remove cancelled".into();
                        modal = None;
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(Modal::ConfirmApplyRemoves { .. }) = &modal {
                match key.code {
                    KeyCode::Char('?') => {
                        show_help = true;
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        modal = None;
                        let (applied, errors) = apply_pending_actions(app, &mut rows);
                        status_line = format!(
                            "applied {applied} action(s), {errors} error(s) — ? help · q quit"
                        );
                        if applied == 0 && errors == 0 {
                            status_line =
                                "nothing to apply (set actions with ←/→ or r) — ? help · q quit"
                                    .into();
                        }
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
                        status_line = "apply cancelled".into();
                        modal = None;
                    }
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Char('?') => {
                    show_help = true;
                }
                KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                KeyCode::Down | KeyCode::Char('j') => {
                    let i = state.selected().unwrap_or(0);
                    state.select(Some((i + 1).min(rows.len().saturating_sub(1))));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let i = state.selected().unwrap_or(0);
                    state.select(Some(i.saturating_sub(1)));
                }
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(']') => {
                    if let Some(i) = state.selected() {
                        let next = rows[i].action.next();
                        if next == DriftAction::Remove {
                            queue_remove(app, &mut rows, i, &mut modal, &mut status_line)?;
                        } else {
                            rows[i].action = next;
                            rows[i].outcome = None;
                        }
                    }
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('[') => {
                    if let Some(i) = state.selected() {
                        let prev = rows[i].action.prev();
                        if prev == DriftAction::Remove {
                            queue_remove(app, &mut rows, i, &mut modal, &mut status_line)?;
                        } else {
                            rows[i].action = prev;
                            rows[i].outcome = None;
                        }
                    }
                }
                KeyCode::Char('s') => {
                    if let Some(i) = state.selected() {
                        rows[i].action = DriftAction::Skip;
                        rows[i].outcome = None;
                    }
                }
                KeyCode::Char('c') => {
                    if let Some(i) = state.selected() {
                        rows[i].action = DriftAction::Libcommit;
                        rows[i].outcome = None;
                    }
                }
                KeyCode::Char('p') => {
                    if let Some(i) = state.selected() {
                        rows[i].action = DriftAction::Promote;
                        rows[i].outcome = None;
                    }
                }
                KeyCode::Char('r') => {
                    if let Some(i) = state.selected() {
                        queue_remove(app, &mut rows, i, &mut modal, &mut status_line)?;
                    }
                }
                KeyCode::Enter => {
                    let pending_removes: Vec<String> = rows
                        .iter()
                        .filter(|r| r.action == DriftAction::Remove && r.outcome.is_none())
                        .map(|r| r.item.name.clone())
                        .collect();
                    if !pending_removes.is_empty() {
                        modal = Some(build_apply_removes_modal(app, &rows, &pending_removes)?);
                    } else {
                        let (applied, errors) = apply_pending_actions(app, &mut rows);
                        status_line = format!(
                            "applied {applied} action(s), {errors} error(s) — ? help · q quit"
                        );
                        if applied == 0 && errors == 0 {
                            status_line =
                                "nothing to apply (set actions with ←/→ or r) — ? help · q quit"
                                    .into();
                        }
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

fn draw_help_modal(frame: &mut ratatui::Frame<'_>, area: Rect, title: &str, lines: &[&str]) {
    let width = area.width.min(72).max(44);
    let height = (lines.len() as u16)
        .saturating_add(2)
        .min(area.height.saturating_sub(2))
        .max(8);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let modal = Rect {
        x,
        y,
        width,
        height,
    };
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" help: {title} "))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let text: Vec<Line> = lines.iter().map(|l| Line::from(*l)).collect();
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }),
        inner,
    );
}

fn queue_remove(
    app: &App,
    rows: &mut [DriftRow],
    index: usize,
    modal: &mut Option<Modal>,
    status_line: &mut String,
) -> Result<()> {
    let name = rows[index].item.name.clone();
    let also_removing: Vec<String> = rows
        .iter()
        .enumerate()
        .filter(|(i, r)| *i == index || r.action == DriftAction::Remove)
        .map(|(_, r)| r.item.name.clone())
        .collect();
    let dependants = app.reverse_deps_for_remove(&name, &also_removing)?;
    let forward_deps = app.forward_deps_remaining(&name, &also_removing)?;
    if dependants.is_empty() {
        rows[index].action = DriftAction::Remove;
        rows[index].outcome = None;
        *status_line = if forward_deps.is_empty() {
            format!("queued remove for `{name}` — press enter to apply")
        } else {
            format!(
                "queued remove for `{name}` (leaves deps: {}) — press enter to apply",
                forward_deps.join(", ")
            )
        };
        *modal = None;
    } else {
        *modal = Some(Modal::RemoveDeps {
            index,
            dependants,
            forward_deps,
        });
    }
    Ok(())
}

fn build_apply_removes_modal(
    app: &App,
    rows: &[DriftRow],
    names: &[String],
) -> Result<Modal> {
    let mut reverse_notes = Vec::new();
    let mut forward_notes = Vec::new();
    for name in names {
        let reverse = app.reverse_deps_for_remove(name, names)?;
        if !reverse.is_empty() {
            reverse_notes.push((name.clone(), reverse));
        }
        let forward = app.forward_deps_remaining(name, names)?;
        if !forward.is_empty() {
            forward_notes.push((name.clone(), forward));
        }
    }
    let _ = rows;
    Ok(Modal::ConfirmApplyRemoves {
        names: names.to_vec(),
        reverse_notes,
        forward_notes,
    })
}

fn apply_pending_actions(app: &App, rows: &mut [DriftRow]) -> (usize, usize) {
    let mut applied = 0;
    let mut errors = 0;
    for row in rows.iter_mut() {
        if row.action == DriftAction::Skip {
            continue;
        }
        if row.outcome.is_some() {
            continue;
        }
        match app.apply_drift_action(&row.item.name, row.action) {
            Ok(msg) => {
                row.outcome = Some(msg.to_string());
                applied += 1;
            }
            Err(e) => {
                row.outcome = Some(format!("error: {e}"));
                errors += 1;
            }
        }
    }
    (applied, errors)
}

fn draw_options_modal(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    body: &str,
) {
    draw_text_modal(frame, area, " options ", Color::Yellow, title, body, false);
}

fn draw_confirm_modal(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    body: &str,
) {
    draw_text_modal(frame, area, " confirm ", Color::Red, title, body, true);
}

fn draw_text_modal(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    border_title: &str,
    border_color: Color,
    title: &str,
    body: &str,
    yn_footer: bool,
) {
    let width = area.width.min(78).max(44);
    let extra_lines = body.lines().count().saturating_sub(1) as u16;
    let height = (8 + extra_lines).min(area.height.saturating_sub(2)).max(8);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let modal = Rect {
        x,
        y,
        width,
        height,
    };
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(border_title)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let mut lines = vec![
        Line::from(Span::styled(title, Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
    ];
    for part in body.split('\n') {
        let style = if part.starts_with('[') {
            Style::default().fg(Color::Cyan)
        } else if part.contains("referenced")
            || part.contains("depends on")
            || part.contains("Still referenced")
            || part.contains("will remain")
        {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(part.to_string(), style)));
    }
    if yn_footer {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "[y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" yes   "),
            Span::styled(
                "[n]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" cancel   "),
            Span::styled(
                "[?]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" help"),
        ]));
    }
    let text = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(text, inner);
}

fn run_list(title: &str, items: &[SelectItem], multi: bool) -> Result<Vec<usize>> {
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
    let mut show_help = false;
    let mut filter = String::new();
    let mut filter_mode = false;
    let help_body = if multi {
        LIST_MULTI_HELP
    } else {
        LIST_CHOOSE_HELP
    };
    let result = loop {
        let visible: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.matches_filter(&filter))
            .map(|(i, _)| i)
            .collect();
        if state.selected().map(|i| i >= visible.len()).unwrap_or(true) {
            state.select(if visible.is_empty() {
                None
            } else {
                Some(0)
            });
        }

        terminal
            .draw(|frame| {
                let area = frame.area();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Min(3),
                        Constraint::Length(1),
                    ])
                    .split(area);

                let help = if multi {
                    "space toggle · / filter · enter confirm · ? help · q cancel"
                } else {
                    "enter select · / filter · ? help · q cancel"
                };
                let header = Paragraph::new(Line::from(format!("{title}  ({help})")));
                frame.render_widget(header, chunks[0]);

                let rows: Vec<ListItem> = visible
                    .iter()
                    .map(|&orig| {
                        let item = &items[orig];
                        let mark = if multi {
                            if checked[orig] {
                                "[x]"
                            } else {
                                "[ ]"
                            }
                        } else {
                            "   "
                        };
                        ListItem::new(format!("{mark} {}", item.label))
                    })
                    .collect();
                let list_title = if filter.is_empty() {
                    format!("skillz ({}/{})", visible.len(), items.len())
                } else {
                    format!(
                        "skillz filter `{filter}` ({}/{})",
                        visible.len(),
                        items.len()
                    )
                };
                let list = List::new(rows)
                    .block(Block::default().borders(Borders::ALL).title(list_title))
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
                frame.render_stateful_widget(list, chunks[1], &mut state);

                let footer = if filter_mode {
                    format!("filter> {filter}_   (Esc clear · Enter apply)")
                } else if filter.is_empty() {
                    String::new()
                } else {
                    format!("filter: {filter}  (/ edit · Esc clear)")
                };
                frame.render_widget(Paragraph::new(footer), chunks[2]);

                if show_help {
                    let label = if multi { "multi-select" } else { "choose" };
                    draw_help_modal(frame, area, label, help_body);
                }
            })
            .map_err(|e| SkillzError::Message(format!("tui: {e}")))?;

        if let Event::Key(key) =
            event::read().map_err(|e| SkillzError::Message(format!("tui: {e}")))?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if show_help {
                show_help = false;
                continue;
            }
            if filter_mode {
                match key.code {
                    KeyCode::Esc => {
                        filter.clear();
                        filter_mode = false;
                    }
                    KeyCode::Enter => {
                        filter_mode = false;
                    }
                    KeyCode::Backspace => {
                        filter.pop();
                    }
                    KeyCode::Char(c) => {
                        filter.push(c);
                    }
                    _ => {}
                }
                continue;
            }
            match key.code {
                KeyCode::Char('?') => {
                    show_help = true;
                }
                KeyCode::Char('/') => {
                    filter_mode = true;
                }
                KeyCode::Esc if !filter.is_empty() => {
                    filter.clear();
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    break Err(SkillzError::Message("cancelled".into()));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if visible.is_empty() {
                        continue;
                    }
                    let i = state.selected().unwrap_or(0);
                    state.select(Some((i + 1).min(visible.len() - 1)));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let i = state.selected().unwrap_or(0);
                    state.select(Some(i.saturating_sub(1)));
                }
                KeyCode::Char(' ') if multi => {
                    if let Some(vi) = state.selected() {
                        if let Some(&orig) = visible.get(vi) {
                            checked[orig] = !checked[orig];
                        }
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
                    } else if let Some(vi) = state.selected() {
                        if let Some(&orig) = visible.get(vi) {
                            break Ok(vec![orig]);
                        }
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
