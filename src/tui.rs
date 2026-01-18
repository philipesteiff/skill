use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use std::io::{self, Stdout};
use std::time::Duration;

pub fn pick_from_list(title: &str, items: &[String]) -> Result<Option<usize>> {
    if items.is_empty() {
        return Ok(None);
    }

    let mut terminal = setup_terminal()?;
    let result = run_list(&mut terminal, title, items);
    teardown_terminal(terminal)?;
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn teardown_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_list(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    title: &str,
    items: &[String],
) -> Result<Option<usize>> {
    let mut state = ListState::default();
    state.select(Some(0));

    loop {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
                .split(frame.area());

            let help = Paragraph::new("Up/Down: move  Enter: select  Esc/q: cancel")
                .block(Block::default().borders(Borders::ALL).title("Controls"));
            frame.render_widget(help, chunks[0]);

            let list_items: Vec<ListItem> = items
                .iter()
                .map(|item| ListItem::new(item.as_str()))
                .collect();

            let list = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title(title))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            frame.render_stateful_widget(list, chunks[1], &mut state);
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
