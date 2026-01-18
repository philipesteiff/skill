use anyhow::Result;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Wrap};

use crate::output::Output;
use crate::ui::terminal::{UiTerminal, safe_area, setup_inline_terminal, teardown_terminal};
use crate::ui::theme;

pub struct LogUi {
    terminal: UiTerminal,
    context: String,
    lines: Vec<String>,
}

impl LogUi {
    pub fn new(context: impl Into<String>) -> Result<Self> {
        Ok(Self {
            terminal: setup_inline_terminal()?,
            context: context.into(),
            lines: Vec::new(),
        })
    }

    pub fn render(&mut self) -> Result<()> {
        let context = self.context.clone();
        let lines = self.lines.clone();
        self.terminal.draw(|frame| {
            let area = safe_area(frame.area());
            render_log(frame, area, &context, &lines);
        })?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.render()?;
        teardown_terminal(&mut self.terminal)
    }
}

impl Output for LogUi {
    fn line(&mut self, message: impl Into<String>) -> Result<()> {
        self.lines.push(message.into());
        self.render()
    }
}

fn render_log(frame: &mut ratatui::Frame, area: Rect, context: &str, lines: &[String]) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
        .split(area);

    let header = Line::from(vec![
        Span::from(context).style(theme::accent_style()).bold(),
    ]);
    frame.render_widget(Paragraph::new(header).alignment(Alignment::Left), chunks[0]);

    if lines.is_empty() {
        let placeholder = Paragraph::new("Working...")
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });
        frame.render_widget(placeholder, chunks[1]);
        return;
    }

    let items = lines
        .iter()
        .map(|line| ListItem::new(Line::from(line.as_str())))
        .collect::<Vec<_>>();
    let list = List::new(items);
    frame.render_widget(list, chunks[1]);
}
