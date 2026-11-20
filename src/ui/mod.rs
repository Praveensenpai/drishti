//! Terminal user interface.

pub mod browser;
pub mod detail;
pub mod help;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

use crate::app::{App, Screen};

/// Colours used across the interface.
pub struct Theme;

impl Theme {
    pub const ACCENT: Color = Color::Cyan;
    pub const GOOD: Color = Color::Green;
    pub const WARN: Color = Color::Yellow;
    pub const BAD: Color = Color::Red;
    pub const MUTED: Color = Color::DarkGray;
    pub const TEXT: Color = Color::Gray;

    /// A block with a rounded border.
    pub fn block(title: &str) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" {title} "))
    }

    /// A dimmed block used for popups.
    pub fn popup(title: &str) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::ACCENT))
            .title(format!(" {title} "))
    }

    /// Style for a value that passes a filter.
    pub fn good() -> Style {
        Style::default().fg(Theme::GOOD)
    }

    /// Style for a value that is rejected.
    pub fn bad() -> Style {
        Style::default().fg(Theme::BAD)
    }

    /// Style for secondary information.
    pub fn muted() -> Style {
        Style::default().fg(Theme::MUTED)
    }

    /// Style for the focused element.
    pub fn focus() -> Style {
        Style::default()
            .fg(Theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    }
}

/// Draw the current screen.
pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    match app.screen {
        Screen::Browser => browser::draw(frame, app),
        Screen::Detail => detail::draw(frame, app),
        Screen::Help => help::draw(frame, app),
        Screen::Language => browser::draw_language(frame, app),
    }

    // Draw search overlay on top of whatever screen is active.
    if app.search_open {
        // Render a search status line using a full-width paragraph
        // that appears at the top of the browser area.
        let search_text = if app.search_draft.is_empty() {
            " /  — start typing to search  —  esc to close  ↑↓ Nav  ↵ Jump  ? Help"
        } else {
            &format!(
                "fuzzy search: '{}' ({} results)  —  esc to close  ↑↓ Nav  ↵ Jump  ? Help",
                app.search_draft, app.search_results.len()
            )
        };
        let search_line = Line::from(Span::styled(
            search_text,
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        // Render it as a widget block at the top.
        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .style(Style::default().bg(Color::DarkGray));
        // Use Paragraph to wrap the line.
        let paragraph = ratatui::widgets::Paragraph::new(search_line.clone())
            .block(block)
            .style(Style::default());
        frame.render_widget(paragraph, frame.area());
    }
}

/// Truncate a string to a column width, adding an ellipsis when cut.
pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out: String = chars[..width - 1].iter().collect();
    out.push('…');
    out
}
