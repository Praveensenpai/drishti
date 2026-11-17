//! The thread browser.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Cell, Gauge, Paragraph, Row, Table, TableState, Wrap};

use crate::app::App;
use crate::lang::Lang;
use crate::model::LangSource;
use crate::quality::{Quality, analyse, human_size};
use crate::ui::Theme;

/// Draw the browser.
pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, app, rows[0]);
    draw_filter_bar(frame, app, rows[1]);
    draw_table(frame, app, rows[2]);
    draw_footer(frame, app, rows[3]);

    if app.filter_open {
        draw_filter_popup(frame, app, area);
    }
}

/// The top bar with the forum, page and counters.
fn draw_header(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let forum = app.forum();
    let visible = app.visible_threads().len();
    let total = app.threads.len();
    let key_hint = if app.gemini.is_some() {
        "AI on"
    } else {
        "AI off"
    };

    let mut spans = vec![
        Span::styled(
            " 1TamilMV ",
            Style::default()
                .fg(Theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {} ", forum.title), Theme::focus()),
        Span::styled(
            format!("[{}]  ", forum.kind.label()),
            Style::default().fg(Theme::MUTED),
        ),
    ];
    spans.push(Span::styled(
        format!("page {}/{}  ", app.page, app.total_pages),
        Style::default().fg(Theme::TEXT),
    ));
    spans.push(Span::styled(
        format!("{visible}/{total} shown  "),
        Theme::muted(),
    ));
    spans.push(Span::styled(
        format!("[{key_hint}]"),
        Style::default().fg(if app.gemini.is_some() {
            Theme::GOOD
        } else {
            Theme::MUTED
        }),
    ));
    if let Some(progress) = app.progress_text() {
        spans.push(Span::styled(
            format!("  {progress}"),
            Style::default().fg(Theme::WARN),
        ));
    }

    let block = Theme::block("").borders(ratatui::widgets::Borders::ALL);
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

/// The one-line summary of the active filters.
fn draw_filter_bar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();

    let qualities: Vec<String> = if app.filter.qualities.is_empty() {
        vec!["any".to_string()]
    } else {
        let mut names: Vec<String> = app
            .filter
            .qualities
            .iter()
            .map(|q| q.label().to_string())
            .collect();
        names.sort();
        names
    };
    spans.push(Span::styled("quality ", Theme::muted()));
    spans.push(Span::styled(qualities.join(", "), Theme::focus()));

    if !app.filter.languages.is_empty() {
        let names: Vec<String> = app
            .filter
            .languages
            .iter()
            .map(|l| l.label().to_string())
            .collect();
        spans.push(Span::styled("  orig:", Theme::muted()));
        spans.push(Span::styled(names.join(", "), Theme::focus()));
    }

    if app.filter.original_only {
        spans.push(Span::styled("  [original only]", Theme::good()));
    }
    if !app.filter.search.is_empty() {
        spans.push(Span::styled(
            format!("  search: {}", app.filter.search),
            Theme::good(),
        ));
    }
    if spans.len() == 1 {
        spans.push(Span::styled("  all qualities", Theme::muted()));
    }
    spans.push(Span::styled("   [f] filters", Theme::muted()));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The thread table.
fn draw_table(frame: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let visible = app.visible_threads();
    if visible.is_empty() && (app.loading || app.scan.active) {
        let ratio = if app.scan.active {
            app.scan.ratio()
        } else {
            0.0
        };
        let label = if app.scan.active {
            format!(
                "{} scanning page {}/{}…",
                app.spinner(),
                app.page,
                app.total_pages.max(1)
            )
        } else {
            format!("{} loading page {}…", app.spinner(), app.page)
        };
        let gauge = Gauge::default()
            .block(Theme::block("threads"))
            .gauge_style(Style::default().fg(Theme::ACCENT))
            .ratio(ratio)
            .label(label);
        frame.render_widget(gauge, area);
        return;
    }
    let header = Row::new(vec![
        Cell::from("#"),
        Cell::from("Movie"),
        Cell::from("Original"),
        Cell::from("Quality"),
        Cell::from("Src"),
        Cell::from("Res"),
        Cell::from("Releases"),
        Cell::from("Size"),
        Cell::from("Posted"),
        Cell::from("Rep"),
        Cell::from("Views"),
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
            let thread = &app.threads[*index];
            let info = analyse(&thread.title);
            let original = app.effective_original(thread);

            let (orig_text, orig_style) = match original {
                Some(o) => {
                    // Check if any release is marked dubbed
                    let has_dubbed = thread.releases.iter().any(|r| r.langs.dubbed);
                    let dubbed_label = if has_dubbed { "(Dubbed) " } else { "" };

                    // Colour the language by how confident we are in the answer:
                    // green = high, yellow = medium, red = low/misleading,
                    // muted = no answer. A verified answer is always green.
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

                    // Show the confidence percentage next to the language.
                    let pct = (conf * 100.0).round() as u8;
                    let full_text = format!("{}{} {}%", dubbed_label, o.lang.label(), pct);
                    (full_text, style)
                }
                None => ("?".to_string(), Theme::muted()),
            };

            let source_text = original
                .map(|o| match o.source {
                    LangSource::Ai => "AI",
                    LangSource::MultiAudio => "multi",
                    LangSource::TitleTag => "tag",
                    LangSource::Forum => "forum",
                    LangSource::Verified => "web",
                    LangSource::Unknown => "?",
                })
                .unwrap_or("?");

            let quality_style = if info.quality == Quality::WebDl {
                Theme::good()
            } else if info.quality == Quality::Unknown {
                Theme::muted()
            } else {
                Theme::bad()
            };

            let name = match thread.year {
                Some(y) => format!("{} ({})", thread.movie, y),
                None => thread.movie.clone(),
            };

            let size = thread
                .releases
                .iter()
                .filter_map(|r| r.info.size_bytes)
                .max()
                .map(|b| human_size(Some(b)))
                .unwrap_or_else(|| "-".to_string());

            let posted = thread
                .posted
                .as_deref()
                .map(|p| p.get(5..10).unwrap_or(p).to_string())
                .unwrap_or_else(|| "-".to_string());

            Row::new(vec![
                Cell::from((row + 1).to_string()),
                Cell::from(name),
                Cell::from(orig_text).style(orig_style),
                Cell::from(info.quality.label()).style(quality_style),
                Cell::from(source_text).style(Theme::muted()),
                Cell::from(info.resolution.label()).style(Theme::muted()),
                Cell::from(thread.releases.len().to_string()).style(Theme::muted()),
                Cell::from(size).style(Theme::muted()),
                Cell::from(posted).style(Theme::muted()),
                Cell::from(thread.replies.to_string()).style(Theme::muted()),
                Cell::from(format_views(thread.views)).style(Theme::muted()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Min(24),
            Constraint::Length(13),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::default()
            .bg(Theme::ACCENT)
            .fg(ratatui::style::Color::Black)
            .add_modifier(Modifier::BOLD),
    )
    .block(Theme::block("threads"));

    let mut state = TableState::default();
    let selected_row = app.selected_row();
    if !visible.is_empty() {
        state.select(Some(selected_row));
    }
    frame.render_stateful_widget(table, area, &mut state);
    app.selected = visible.get(selected_row).copied().unwrap_or(app.selected);
}

/// Format a view count compactly.
fn format_views(views: u32) -> String {
    if views >= 1_000_000 {
        format!("{:.1}M", views as f64 / 1_000_000.0)
    } else if views >= 1_000 {
        format!("{}k", views / 1_000)
    } else {
        views.to_string()
    }
}

/// The key hint bar.
fn draw_footer(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(" ↑↓ ", Theme::focus()),
        Span::styled("select  ", Theme::muted()),
        Span::styled("↵ ", Theme::focus()),
        Span::styled("releases  ", Theme::muted()),
        Span::styled("p ", Theme::focus()),
        Span::styled("open post  ", Theme::muted()),
        Span::styled("[ ] ", Theme::focus()),
        Span::styled("forum  ", Theme::muted()),
        Span::styled("←→ ", Theme::focus()),
        Span::styled("page  ", Theme::muted()),
        Span::styled("f ", Theme::focus()),
        Span::styled("filters  ", Theme::muted()),
        Span::styled("o ", Theme::focus()),
        Span::styled("original-only  ", Theme::muted()),
        Span::styled("? ", Theme::focus()),
        Span::styled("help  ", Theme::muted()),
        Span::styled("q ", Theme::focus()),
        Span::styled("quit", Theme::muted()),
    ];
    if let Some(error) = &app.error {
        spans.push(Span::styled(format!("  {error}"), Theme::bad()));
    } else if !app.status.is_empty() {
        spans.push(Span::styled(format!("  {}", app.status), Theme::good()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The filter popup.
fn draw_filter_popup(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let width = area.width.saturating_sub(10).clamp(40, 74);
    let height = 16.min(area.height.saturating_sub(4));
    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Quality — space toggles, a toggles all",
        Theme::muted(),
    )));
    lines.push(Line::from(""));

    let qualities = [
        Quality::WebDl,
        Quality::Bluray,
        Quality::WebRip,
        Quality::TvRip,
        Quality::DvdRip,
        Quality::HdRip,
        Quality::Theater,
        Quality::Cam,
    ];
    let mut row = Vec::new();
    for (index, quality) in qualities.iter().enumerate() {
        let on = app.filter.qualities.contains(quality);
        let style = if on {
            Style::default().fg(Theme::GOOD)
        } else {
            Style::default().fg(Theme::MUTED)
        };
        let mark = if on { "[x]" } else { "[ ]" };
        let focus = if app.filter_focus == index {
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Theme::ACCENT)
        } else {
            Style::default()
        };
        row.push(Span::styled(format!("{mark} {mark} "), style));
        row.push(Span::styled(
            format!("{:<8}", quality.label()),
            style.patch(focus),
        ));
        if index % 3 == 2 {
            lines.push(Line::from(row));
            row = Vec::new();
        }
    }
    if !row.is_empty() {
        lines.push(Line::from(row));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Original language filter — leave empty for any",
        Theme::muted(),
    )));
    let langs: Vec<Span> = Lang::ALL
        .iter()
        .take(12)
        .map(|l| {
            let on = app.filter.languages.contains(l);
            let style = if on {
                Style::default().fg(Theme::GOOD)
            } else {
                Style::default().fg(Theme::MUTED)
            };
            let mark = if on { "[x]" } else { "[ ]" };
            Span::styled(format!("{mark} {:<9}", l.label()), style)
        })
        .collect();
    lines.push(Line::from(langs));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        if app.filter.original_only {
            "[x] only releases with the original language"
        } else {
            "[ ] only releases with the original language"
        },
        if app.filter.original_only {
            Theme::good()
        } else {
            Theme::muted()
        },
    )]));

    let block = Theme::popup("filters").padding(ratatui::widgets::Padding::horizontal(1));
    frame.render_widget(ratatui::widgets::Clear, popup_area);
    frame.render_widget(Paragraph::new(lines).block(block), popup_area);
}

/// Draw the language picker.
pub fn draw_language(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let height = ((app.forums.len() + 4) as u16).min(area.height.saturating_sub(2));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(height),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(Text::from(Line::from(Span::styled(
            " choose a language to scan",
            Theme::muted(),
        ))))
        .block(Theme::block("language")),
        rows[0],
    );

    let options: Vec<Line> = app
        .forums
        .iter()
        .enumerate()
        .map(|(index, forum)| {
            let selected = index == app.forum_index;
            let style = if selected {
                Style::default()
                    .fg(Theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Theme::TEXT)
            };
            Line::from(Span::styled(
                format!(
                    "{}{}",
                    if selected { "\u{276f} " } else { "  " },
                    forum.title
                ),
                style,
            ))
        })
        .collect();

    frame.render_widget(
        Paragraph::new(Text::from(options))
            .block(Theme::block("language"))
            .wrap(Wrap { trim: true }),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " \u{2191}\u{2193} select   enter choose   esc back",
            Theme::muted(),
        ))),
        rows[2],
    );
}
