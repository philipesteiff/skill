use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};
use std::io::{self, Stdout};

pub type UiTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn setup_inline_terminal() -> Result<UiTerminal> {
    enable_raw_mode()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let viewport_height = size()?.1.saturating_sub(1).max(1);
    let terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(viewport_height),
        },
    )?;
    Ok(terminal)
}

pub fn teardown_terminal(terminal: &mut UiTerminal) -> Result<()> {
    disable_raw_mode()?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn safe_area(area: Rect) -> Rect {
    Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(1),
        area.height.saturating_sub(1),
    )
}
