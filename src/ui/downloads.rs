//! The download queue.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Gauge, Paragraph, Row, Table, TableState};

use crate::app::App;
use crate::download::{DownloadKind, DownloadStatus};
use crate::ui::Theme;

/// Draw the download queue.
pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    let running = app
        .downloads
        .iter()
        .filter(|t| matches!(t.status, DownloadStatus::Running { .. }))
        .count();
    let finished = app
        .downloads
        .iter()
        .filter(|t| matches!(t.status, DownloadStatus::Finished { .. }))
        .count();
    let failed = app
        .downloads
        .iter()
        .filter(|t| matches!(t.status, DownloadStatus::Failed { .. }))
        .count();

    let header = Line::from(vec![
        Span::styled(" running ", Theme::focus()),
        Span::styled(running.to_string(), Theme::good()),
        Span::styled("   done ", Theme::muted()),
        Span::styled(finished.to_string(), Theme::good()),
        Span::styled("   failed ", Theme::muted()),
        Span::styled(failed.to_string(), Theme::bad()),
    ]);
    frame.render_widget(Paragraph::new(header), rows[0]);

    let table_header = Row::new(vec![
        Cell::from("#"),
        Cell::from("Name"),
        Cell::from("Kind"),
        Cell::from("Status"),
    ])
    .style(
        Style::default()
            .fg(Theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    );

    let rows_data: Vec<Row> = app
        .downloads
        .iter()
        .enumerate()
        .map(|(index, task)| {
            let status_style = match &task.status {
                DownloadStatus::Failed { .. } => Theme::bad(),
                DownloadStatus::Finished { .. } => Theme::good(),
                DownloadStatus::HandedOff { .. } => Theme::good(),
                _ => Theme::muted(),
            };
            Row::new(vec![
                Cell::from((index + 1).to_string()),
                Cell::from(task.name.clone()),
                Cell::from(match task.kind {
                    DownloadKind::Magnet => task.url.clone(),
                    _ => task.kind_label(),
                })
                .style(Theme::muted()),
                Cell::from(task.status.summary()).style(status_style),
            ])
        })
        .collect();

    let table = Table::new(
        rows_data,
        [
            Constraint::Length(3),
            Constraint::Min(30),
            Constraint::Length(10),
            Constraint::Min(24),
        ],
    )
    .header(table_header)
    .row_highlight_style(
        Style::default()
            .bg(Theme::ACCENT)
            .fg(ratatui::style::Color::Black)
            .add_modifier(Modifier::BOLD),
    )
    .block(Theme::block("downloads"));

    let mut state = TableState::default();
    if !app.downloads.is_empty() {
        state.select(Some(app.download_selected.min(app.downloads.len() - 1)));
    }
    frame.render_stateful_widget(table, rows[1], &mut state);

    draw_progress(frame, app, rows[2]);
}

/// The progress gauge of the active download.
fn draw_progress(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let active = app.downloads.iter().find(|t| {
        matches!(
            t.status,
            DownloadStatus::Running { .. } | DownloadStatus::Queued
        )
    });

    let Some(task) = active else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " esc back   ↑↓ select   d start a download from the release list",
                Theme::muted(),
            ))),
            area,
        );
        return;
    };

    let fraction = task.status.fraction().unwrap_or(0.0);
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Theme::ACCENT))
        .ratio(fraction.clamp(0.0, 1.0))
        .label(format!(
            " {}  {} ",
            crate::ui::truncate(&task.name, 40),
            task.status.summary()
        ));
    frame.render_widget(gauge, area);
}
