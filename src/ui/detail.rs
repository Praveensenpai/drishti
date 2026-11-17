//! The release detail pane.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};

use crate::app::App;
use crate::model::LangSource;
use crate::quality::{Quality, human_size};
use crate::ui::{Theme, truncate};

/// Draw the detail pane.
pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, app, rows[0]);
    draw_releases(frame, app, rows[1]);
    draw_footer(frame, app, rows[2]);
}

/// The thread header with the resolved original language.
fn draw_header(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let Some(thread) = app.detail_thread() else {
        frame.render_widget(
            Paragraph::new("no thread selected").block(Theme::block("releases")),
            area,
        );
        return;
    };

    let title = match thread.year {
        Some(y) => format!("{} ({})", thread.movie, y),
        None => thread.movie.clone(),
    };

    let original = app.effective_original(thread);
    // Check if any release in the thread is marked dubbed
    let has_dubbed = thread.releases.iter().any(|r| r.langs.dubbed);
    let (dubbed_label, _dub_style) = if has_dubbed {
        let muted = Style::default().fg(Theme::MUTED);
        ("(Dubbed)".to_string(), muted)
    } else {
        (String::new(), Style::default())
    };
    let (orig_text, orig_style) = match original {
        Some(o) => {
            let conf = o.confidence;
            let style = if o.source == LangSource::Verified {
                Style::default().fg(Theme::GOOD)
            } else if o.source == LangSource::Unknown || conf < 0.5 {
                Style::default().fg(Theme::BAD)
            } else if conf < 0.8 {
                Style::default().fg(Theme::WARN)
            } else {
                Style::default().fg(Theme::GOOD)
            };
            let src = match o.source {
                LangSource::Ai => "AI",
                LangSource::MultiAudio => "multi-audio tag",
                LangSource::TitleTag => "title tag",
                LangSource::Forum => "forum",
                LangSource::Verified => "verified",
                LangSource::Unknown => "unknown",
            };
            // Prepend dubbed badge if present
            let full_orig_text = if !dubbed_label.is_empty() {
                format!("{} {}", dubbed_label, o.lang.label())
            } else {
                o.lang.label().to_string()
            };
            (
                format!(
                    "{}  ({} · {}% confidence · {})",
                    full_orig_text,
                    src,
                    (o.confidence * 100.0) as u8,
                    o.lang.code()
                ),
                style,
            )
        }
        None => (
            "unknown — open the thread to resolve".to_string(),
            Theme::muted(),
        ),
    };

    let third = if app.status.is_empty() {
        Line::from(vec![
            Span::styled(thread.forum.title, Theme::muted()),
            Span::styled("  ·  ", Theme::muted()),
            Span::styled(format!("by {}", thread.author), Theme::muted()),
            Span::styled("  ·  ", Theme::muted()),
            Span::styled(format!("{} replies", thread.replies), Theme::muted()),
            Span::styled("  ·  ", Theme::muted()),
            Span::styled(format!("{} views", thread.views), Theme::muted()),
        ])
    } else {
        Line::from(Span::styled(app.status.clone(), Theme::good()))
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(title, Theme::focus()),
            Span::styled("   original: ", Theme::muted()),
            Span::styled(orig_text, orig_style),
        ]),
        Line::from(Span::styled(
            truncate(&thread.title, area.width.saturating_sub(4) as usize),
            Theme::muted(),
        )),
        third,
    ];

    frame.render_widget(Paragraph::new(lines).block(Theme::block("releases")), area);
}

/// The release table.
fn draw_releases(frame: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let Some(thread) = app.detail_thread() else {
        return;
    };
    let visible = app.visible_releases();
    let original = app.effective_original(thread).map(|o| o.lang);

    let header = Row::new(vec![
        Cell::from("#"),
        Cell::from("Release"),
        Cell::from("Quality"),
        Cell::from("Source"),
        Cell::from("Res"),
        Cell::from("Audio"),
        Cell::from("Size"),
        Cell::from("Orig"),
    ])
    .style(
        Style::default()
            .fg(Theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = visible
        .iter()
        .enumerate()
        .map(|(row, index)| {
            let release = &thread.releases[*index];
            let quality_style = if release.info.quality == Quality::WebDl {
                Theme::good()
            } else {
                Theme::bad()
            };

            let (orig_mark, orig_style) = match original {
                Some(orig) if release.has_original(orig) => ("yes", Theme::good()),
                Some(_) => ("dub", Theme::bad()),
                None => ("?", Theme::muted()),
            };

            Row::new(vec![
                Cell::from((row + 1).to_string()),
                Cell::from(release.name.clone()),
                Cell::from(release.info.quality.label()).style(quality_style),
                Cell::from(release.info.source.label()).style(Theme::muted()),
                Cell::from(release.info.resolution.label()).style(Theme::muted()),
                Cell::from(release.langs.compact()).style(Theme::muted()),
                Cell::from(human_size(release.info.size_bytes)).style(Theme::muted()),
                Cell::from(orig_mark).style(orig_style),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Min(30),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(22),
            Constraint::Length(8),
            Constraint::Length(5),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::default()
            .bg(Theme::ACCENT)
            .fg(ratatui::style::Color::Black)
            .add_modifier(Modifier::BOLD),
    )
    .block(Theme::block("releases"));

    let mut state = TableState::default();
    if !visible.is_empty() {
        let row = app.detail_selected.min(visible.len() - 1);
        state.select(Some(row));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

/// The key hint bar.
fn draw_footer(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let only = if app.detail_only_original {
        "showing original-language releases"
    } else {
        "showing every release"
    };
    let spans = vec![
        Span::styled(" ↵/p ", Theme::focus()),
        Span::styled("open post  ", Theme::muted()),
        Span::styled("m ", Theme::focus()),
        Span::styled("copy magnet  ", Theme::muted()),
        Span::styled("o ", Theme::focus()),
        Span::styled("toggle original-only  ", Theme::muted()),
        Span::styled("esc ", Theme::focus()),
        Span::styled("back  ", Theme::muted()),
        Span::styled(only, Theme::good()),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
