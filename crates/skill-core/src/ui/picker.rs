use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use std::time::Duration;

use crate::ui::terminal::UiTerminal;
use crate::ui::theme;

pub fn pick_from_list(
    terminal: &mut UiTerminal,
    title: &str,
    items: &[String],
) -> Result<Option<usize>> {
    if items.is_empty() {
        return Ok(None);
    }

    let mut state = ListState::default();
    state.select(Some(0));

    loop {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Min(1),
                    ]
                    .as_ref(),
                )
                .split(frame.area());

            let header = Paragraph::new(Line::from(vec![
                Span::from(title).style(theme::accent_style()).bold(),
            ]));
            frame.render_widget(header, chunks[0]);

            let help = Paragraph::new(Line::from(
                Span::from("Up/Down: move  Enter: select  Esc/q: cancel").dim(),
            ));
            frame.render_widget(help, chunks[1]);

            let list_items: Vec<ListItem> = items
                .iter()
                .map(|item| ListItem::new(item.as_str()))
                .collect();

            let list = List::new(list_items)
                .highlight_style(
                    theme::accent_style()
                        .add_modifier(Modifier::BOLD)
                        .add_modifier(Modifier::REVERSED),
                )
                .highlight_symbol("> ");

            frame.render_stateful_widget(list, chunks[2], &mut state);
        })?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Up => {
                    let next = match state.selected() {
                        Some(0) | None => items.len() - 1,
                        Some(idx) => idx - 1,
                    };
                    state.select(Some(next));
                }
                KeyCode::Down => {
                    let next = match state.selected() {
                        Some(idx) if idx + 1 < items.len() => idx + 1,
                        _ => 0,
                    };
                    state.select(Some(next));
                }
                KeyCode::Enter => {
                    return Ok(state.selected());
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    return Ok(None);
                }
                _ => {}
            }
        }
    }
}
