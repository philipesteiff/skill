use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use std::collections::HashSet;
use std::io::IsTerminal;
use std::time::Duration;

use crate::ui::components::{Footer, Header};
use crate::ui::terminal::{UiTerminal, safe_area, setup_inline_terminal, teardown_terminal};
use crate::ui::theme;

#[derive(Debug, Clone)]
pub struct BrowseItem {
    pub name: String,
    pub description: String,
    pub updated_at: String,
    pub tags: Vec<String>,
    pub path: String,
    pub installed: bool,
}

#[derive(Debug, Clone)]
pub enum BrowseSelection {
    All,
    List(Vec<String>),
    Delete(Vec<String>),
    Cancel,
}

#[derive(Debug, Clone, Copy)]
pub enum BrowseMode {
    Install,
    Installed,
}

enum InputMode {
    Normal,
    Search,
}

pub fn run_browse_ui(
    title: String,
    items: &[BrowseItem],
    initial_selected: &HashSet<String>,
    initial_query: Option<&str>,
    mode: BrowseMode,
) -> Result<BrowseSelection> {
    if !std::io::stdout().is_terminal() {
        return Err(anyhow!("browse requires an interactive terminal"));
    }

    let mut terminal = setup_inline_terminal()?;
    let mut state = BrowseState::new(items, initial_selected, initial_query, mode);
    let result = run_loop(&mut terminal, &title, items, &mut state);
    teardown_terminal(&mut terminal)?;
    println!();
    result
}

struct BrowseState {
    filter: String,
    filtered: Vec<usize>,
    selected: HashSet<String>,
    list_state: ListState,
    mode: InputMode,
    browse_mode: BrowseMode,
}

impl BrowseState {
    fn new(
        items: &[BrowseItem],
        initial_selected: &HashSet<String>,
        initial_query: Option<&str>,
        browse_mode: BrowseMode,
    ) -> Self {
        let filter = initial_query.unwrap_or("").to_string();
        let mut state = Self {
            filter,
            filtered: Vec::new(),
            selected: initial_selected.clone(),
            list_state: ListState::default(),
            mode: InputMode::Normal,
            browse_mode,
        };
        state.refresh_filter(items);
        state
    }

    fn refresh_filter(&mut self, items: &[BrowseItem]) {
        let query = self.filter.trim().to_lowercase();
        self.filtered = items
            .iter()
            .enumerate()
            .filter(|(_, item)| matches_query(item, &query))
            .map(|(idx, _)| idx)
            .collect();
        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else if self.list_state.selected().is_none()
            || self
                .list_state
                .selected()
                .is_some_and(|idx| idx >= self.filtered.len())
        {
            self.list_state.select(Some(0));
        }
    }

    fn selected_paths(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }
}

fn matches_query(item: &BrowseItem, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let name = item.name.to_lowercase();
    let description = item.description.to_lowercase();
    let tags = item.tags.join(" ").to_lowercase();
    name.contains(query) || description.contains(query) || tags.contains(query)
}

fn run_loop(
    terminal: &mut UiTerminal,
    title: &str,
    items: &[BrowseItem],
    state: &mut BrowseState,
) -> Result<BrowseSelection> {
    loop {
        terminal.draw(|frame| {
            let area = safe_area(frame.area());
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Min(1),
                        Constraint::Length(2),
                    ]
                    .as_ref(),
                )
                .split(area);

            frame.render_widget(Header::new(title), chunks[0]);
            let search_line = render_search_line(state);
            frame.render_widget(search_line, chunks[1]);

            let list_items = build_list(items, state);
            let list = List::new(list_items)
                .highlight_style(
                    theme::accent_style()
                        .add_modifier(ratatui::style::Modifier::BOLD)
                        .add_modifier(ratatui::style::Modifier::REVERSED),
                )
                .highlight_symbol("> ");
            frame.render_stateful_widget(list, chunks[2], &mut state.list_state);

            let footer = match state.browse_mode {
                BrowseMode::Installed => Footer::new(vec![
                    ("↑/↓", "move"),
                    ("Space", "select"),
                    ("A", "select all"),
                    ("R", "unselect all"),
                    ("Enter", "delete selected"),
                    ("/", "search"),
                    ("Esc", "close"),
                ]),
                BrowseMode::Install => Footer::new(vec![
                    ("↑/↓", "move"),
                    ("Space", "select"),
                    ("A", "select all"),
                    ("R", "unselect all"),
                    ("Enter", "install"),
                    ("/", "search"),
                    ("Esc", "cancel"),
                ]),
            };
            frame.render_widget(footer, chunks[3]);
        })?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            match state.mode {
                InputMode::Normal => {
                    if let Some(selection) = handle_normal_mode(items, state, key)? {
                        return Ok(selection);
                    }
                }
                InputMode::Search => {
                    handle_search_mode(items, state, key)?;
                }
            }
        }
    }
}

fn render_search_line(state: &BrowseState) -> Paragraph<'static> {
    let mut spans = vec![Span::from("Search: ").dim()];
    let query = if state.filter.is_empty() {
        "type / to search".to_string()
    } else {
        state.filter.clone()
    };
    let query_span = match state.mode {
        InputMode::Search => Span::from(query).style(theme::accent_style()),
        InputMode::Normal => Span::from(query).dim(),
    };
    spans.push(query_span);
    Paragraph::new(Line::from(spans))
}

fn build_list(items: &[BrowseItem], state: &BrowseState) -> Vec<ListItem<'static>> {
    state
        .filtered
        .iter()
        .map(|idx| {
            let item = &items[*idx];
            let selected = state.selected.contains(&item.path);
            let checkbox = if selected { "[x] " } else { "[ ] " };
            let mut spans = vec![Span::from(checkbox).dim()];
            spans.push(Span::from(item.name.clone()).bold());
            if !item.description.is_empty() {
                spans.push(Span::from(" — ").dim());
                spans.push(Span::from(item.description.clone()));
            }
            if !item.updated_at.is_empty() {
                spans.push(Span::from(format!(" ({})", item.updated_at)).dim());
            }
            if item.installed {
                spans.push(Span::from(" • installed").green());
            }
            if !item.tags.is_empty() {
                let tag_text = item
                    .tags
                    .iter()
                    .map(|tag| format!("#{tag}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                spans.push(Span::from(" ").dim());
                spans.push(Span::from(tag_text).dim());
            }
            ListItem::new(Line::from(spans))
        })
        .collect()
}

fn handle_normal_mode(
    items: &[BrowseItem],
    state: &mut BrowseState,
    key: KeyEvent,
) -> Result<Option<BrowseSelection>> {
    match key.code {
        KeyCode::Up => {
            let next = match state.list_state.selected() {
                Some(0) | None => state.filtered.len().saturating_sub(1),
                Some(idx) => idx.saturating_sub(1),
            };
            state.list_state.select(if state.filtered.is_empty() {
                None
            } else {
                Some(next)
            });
        }
        KeyCode::Down => {
            let next = match state.list_state.selected() {
                Some(idx) if idx + 1 < state.filtered.len() => idx + 1,
                _ => 0,
            };
            state.list_state.select(if state.filtered.is_empty() {
                None
            } else {
                Some(next)
            });
        }
        KeyCode::Char(' ') => {
            toggle_selected(items, state);
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            select_all(items, state);
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            state.selected.clear();
        }
        KeyCode::Char('/') => {
            state.mode = InputMode::Search;
        }
        KeyCode::Enter => {
            if matches!(state.browse_mode, BrowseMode::Installed) {
                let selected = state.selected_paths();
                if !selected.is_empty() {
                    return Ok(Some(BrowseSelection::Delete(selected)));
                }
                return Ok(Some(BrowseSelection::Cancel));
            }
            let selected = state.selected_paths();
            if selected.is_empty() {
                return Ok(Some(BrowseSelection::Cancel));
            }
            return Ok(Some(BrowseSelection::List(selected)));
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            return Ok(Some(BrowseSelection::Cancel));
        }
        _ => {}
    }
    Ok(None)
}

fn handle_search_mode(items: &[BrowseItem], state: &mut BrowseState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            state.mode = InputMode::Normal;
        }
        KeyCode::Enter => {
            state.mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            state.filter.pop();
            state.refresh_filter(items);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.filter.clear();
            state.refresh_filter(items);
        }
        KeyCode::Char(c) => {
            state.filter.push(c);
            state.refresh_filter(items);
        }
        _ => {}
    }
    Ok(())
}

fn toggle_selected(items: &[BrowseItem], state: &mut BrowseState) {
    let Some(selected_idx) = state.list_state.selected() else {
        return;
    };
    let Some(item_idx) = state.filtered.get(selected_idx) else {
        return;
    };
    let item = &items[*item_idx];
    if state.selected.contains(&item.path) {
        state.selected.remove(&item.path);
    } else {
        state.selected.insert(item.path.clone());
    }
}

fn select_all(items: &[BrowseItem], state: &mut BrowseState) {
    state.selected = items.iter().map(|item| item.path.clone()).collect();
}
