use ratatui::style::{Color, Style};

pub fn accent_style() -> Style {
    Style::new().fg(Color::Cyan)
}

pub fn success_style() -> Style {
    Style::new().fg(Color::Green)
}

pub fn error_style() -> Style {
    Style::new().fg(Color::Red)
}
