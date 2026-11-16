//! The key binding reference.

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::App;
use crate::ui::Theme;

/// Draw the help screen.
pub fn draw(frame: &mut ratatui::Frame, _app: &mut App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(1)])
        .split(area);

    let bindings: Vec<(&str, &str)> = vec![
        ("↑ ↓ / j k", "move the selection"),
        ("← → / h l", "previous / next page"),
        ("[ ]", "switch forum"),
        ("g / G", "jump to the first / last row"),
        ("↵", "open the selected thread"),
        ("esc", "go back"),
        ("f", "open the filter panel"),
        ("space", "toggle the highlighted filter"),
        ("a", "toggle every quality at once"),
        ("o", "only show releases in the original language"),
        ("c", "clear the language filter"),
        ("s", "re-scan the page and refresh the language column"),
        ("r", "reload the current page"),
        ("↵ / p (detail)", "open the thread page in your browser"),
        ("m (detail)", "copy the selected release's magnet link"),
        ("t (detail)", "copy the selected release's torrent link"),
        ("?", "toggle this help"),
        ("q", "quit"),
    ];

    let mut lines: Vec<Line> = Vec::new();
    for (key, description) in bindings {
        lines.push(Line::from(vec![
            Span::styled(format!(" {key:<12}"), Theme::focus()),
            Span::styled(description, Style::default().fg(Theme::TEXT)),
        ]));
    }

    let notes = vec![
        Line::from(""),
        Line::from(Span::styled(
            " The Original column shows the language the film was made in.",
            Theme::muted(),
        )),
        Line::from(Span::styled(
            " AI marks an answer from Gemini, multi a multi-audio release name, tag a",
            Theme::muted(),
        )),
        Line::from(Span::styled(
            " language word in the title, forum the forum it was posted in.",
            Theme::muted(),
        )),
    ];

    let all = [lines, notes].concat();
    frame.render_widget(
        Paragraph::new(all)
            .block(Theme::block("keys"))
            .wrap(Wrap { trim: true }),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " esc back   q quit",
            Theme::muted(),
        ))),
        rows[1],
    );
}
