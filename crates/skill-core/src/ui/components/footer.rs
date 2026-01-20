use ratatui::layout::{Alignment, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

pub struct Footer<'a> {
    keys: Vec<(&'a str, &'a str)>,
}

impl<'a> Footer<'a> {
    pub fn new(keys: Vec<(&'a str, &'a str)>) -> Self {
        Self { keys }
    }
}

impl Widget for Footer<'_> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let spans: Vec<Span> = self
            .keys
            .iter()
            .enumerate()
            .flat_map(|(i, (key, desc))| {
                let mut s = vec![
                    Span::from(*key).bold(),
                    Span::from(": "),
                    Span::from(*desc).dim(),
                ];
                if i < self.keys.len() - 1 {
                    s.push(Span::from("  "));
                }
                s
            })
            .collect();
        
        let line = Line::from(spans);
        Paragraph::new(line)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}
