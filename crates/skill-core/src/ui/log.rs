use anyhow::Result;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Wrap};
use std::io::IsTerminal;

use crate::output::Output;
use crate::ui::terminal::{UiTerminal, safe_area, setup_inline_terminal, teardown_terminal};
use crate::ui::theme;

pub struct LogUi {
    inner: LogUiInner,
}

impl LogUi {
    pub fn new(context: impl Into<String>) -> Result<Self> {
        let context = context.into();
        if std::io::stdout().is_terminal() {
            Ok(Self {
                inner: LogUiInner::Tui(TuiLogUi::new(context)),
            })
        } else {
            Ok(Self {
                inner: LogUiInner::Plain(PlainLogUi::new(context)),
            })
        }
    }

    pub fn render(&mut self) -> Result<()> {
        match &mut self.inner {
            LogUiInner::Tui(inner) => inner.render(),
            LogUiInner::Plain(inner) => inner.render(),
        }
    }

    pub fn finish(mut self) -> Result<()> {
        match &mut self.inner {
            LogUiInner::Tui(inner) => inner.finish(),
            LogUiInner::Plain(inner) => inner.finish(),
        }
    }
}

impl Output for LogUi {
    fn line(&mut self, message: impl Into<String>) -> Result<()> {
        match &mut self.inner {
            LogUiInner::Tui(inner) => inner.line(message),
            LogUiInner::Plain(inner) => inner.line(message),
        }
    }
}

enum LogUiInner {
    Tui(TuiLogUi),
    Plain(PlainLogUi),
}

struct TuiLogUi {
    terminal: Option<UiTerminal>,
    context: String,
    lines: Vec<String>,
}

impl TuiLogUi {
    fn new(context: String) -> Self {
        Self {
            terminal: None,
            context,
            lines: Vec::new(),
        }
    }

    fn ensure_terminal(&mut self) -> Result<()> {
        if self.terminal.is_none() {
            let mut terminal = setup_inline_terminal()?;
            terminal.clear()?;
            self.terminal = Some(terminal);
        }
        Ok(())
    }

    fn render(&mut self) -> Result<()> {
        self.ensure_terminal()?;
        let context = self.context.clone();
        let lines = self.lines.clone();
        let terminal = self.terminal.as_mut().expect("terminal initialized");
        terminal.draw(|frame| {
            let area = safe_area(frame.area());
            render_log(frame, area, &context, &lines);
        })?;
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        if self.terminal.is_none() {
            return Ok(());
        }
        self.render()?;
        let terminal = self.terminal.as_mut().expect("terminal initialized");
        teardown_terminal(terminal)
    }

    fn line(&mut self, message: impl Into<String>) -> Result<()> {
        self.lines.push(message.into());
        self.render()
    }
}

struct PlainLogUi {
    context: String,
}

impl PlainLogUi {
    fn new(context: String) -> Self {
        Self { context }
    }

    fn render(&mut self) -> Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }

    fn line(&mut self, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        println!("{}: {}", self.context, message);
        Ok(())
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
