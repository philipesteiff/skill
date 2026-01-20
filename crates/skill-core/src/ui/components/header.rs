use ratatui::layout::{Alignment, Rect};
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Widget};

use crate::ui::theme;

pub struct Header<'a> {
    title: &'a str,
}

impl<'a> Header<'a> {
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let text = Span::from(self.title).style(theme::accent_style()).bold();
        Paragraph::new(text).alignment(Alignment::Left).render(area, buf);
    }
}
