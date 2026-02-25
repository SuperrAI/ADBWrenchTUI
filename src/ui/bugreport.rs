use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::{App, BugreportStatus};
use crate::components::{render_gauge, render_keybinding_footer, truncate_str};
use crate::theme::Theme;

/// Render the Bugreport page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Length(3), // info box
        Constraint::Length(5), // progress section
        Constraint::Min(0),    // history
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[3]);
        render_footer(frame, chunks[4]);
        return;
    }

    render_info_box(frame, chunks[1]);
    render_progress(app, frame, chunks[2]);
    render_history(app, frame, chunks[3]);
    render_footer(frame, chunks[4]);
}

/// Header with status.
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled(" BUGREPORT", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("GENERATOR", Theme::dim()),
    ];

    if app.bugreport.is_generating {
        spans.push(Span::styled("  ● GENERATING", Theme::warning()));
    } else {
        spans.push(Span::styled("  ○ READY", Theme::success()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Info box about bugreports.
fn render_info_box(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(" INFO ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " Bugreports capture device state for debugging. This may take several minutes.",
            Theme::dim(),
        ))),
        inner,
    );
}

/// Progress section.
fn render_progress(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if app.bugreport.is_generating {
            Theme::border_active()
        } else {
            Theme::border()
        })
        .border_type(BorderType::Rounded)
        .title(Span::styled(" PROGRESS ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if !app.bugreport.is_generating {
        let btn = Paragraph::new(Line::from(Span::styled(
            "[ GENERATE BUGREPORT ]",
            Theme::accent_bold(),
        )))
        .alignment(Alignment::Center);

        let centered = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);
        frame.render_widget(btn, centered[1]);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1), // percentage text
        Constraint::Length(1), // gauge
        Constraint::Min(0),    // elapsed
    ])
    .split(inner);

    // Percentage display
    let pct_text = Line::from(vec![
        Span::styled(
            format!(" {}%", app.bugreport.progress),
            Theme::accent_bold(),
        ),
        Span::styled(" complete", Theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(pct_text), rows[0]);

    // Progress bar
    let gauge_area = Layout::horizontal([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(rows[1]);
    render_gauge(
        frame,
        gauge_area[1],
        app.bugreport.progress as f64 / 100.0,
        "",
        Theme::ORANGE,
    );

    // Elapsed time
    if let Some(start) = app.bugreport.start_time {
        let elapsed = start.elapsed().as_secs();
        let mins = elapsed / 60;
        let secs = elapsed % 60;
        let elapsed_line = Line::from(Span::styled(
            format!(" Elapsed: {mins}m {secs}s"),
            Theme::muted(),
        ));
        frame.render_widget(Paragraph::new(elapsed_line), rows[2]);
    }
}

/// History of generated bugreports.
fn render_history(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" HISTORY ({}) ", app.bugreport.history.len()),
            Theme::title(),
        ))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.bugreport.history.is_empty() {
        let hint = Paragraph::new(Span::styled("No bugreports generated yet", Theme::muted()))
            .alignment(Alignment::Center);
        let centered = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);
        frame.render_widget(hint, centered[1]);
        return;
    }

    let visible_height = inner.height as usize;
    let available_width = inner.width as usize;
    let selected = app
        .bugreport
        .selected_index
        .min(app.bugreport.history.len().saturating_sub(1));
    let scroll = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);

    for (i, entry) in app
        .bugreport
        .history
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
    {
        let is_selected = i == selected;
        let row_style = if is_selected {
            Theme::highlight()
        } else {
            Style::default().bg(Theme::BG)
        };

        // Status badge
        let (badge, badge_style) = match entry.status {
            BugreportStatus::Completed => ("[OK]", Theme::success()),
            BugreportStatus::Failed => ("[FAIL]", Theme::error()),
            BugreportStatus::Cancelled => ("[STOP]", Theme::warning()),
            BugreportStatus::Generating => ("[...]", Theme::warning()),
        };

        // Duration
        let duration_str = if let Some(end) = entry.end_time {
            let dur = end.duration_since(entry.start_time).as_secs();
            format!("{}m{}s", dur / 60, dur % 60)
        } else {
            let dur = entry.start_time.elapsed().as_secs();
            format!("{}m{}s", dur / 60, dur % 60)
        };

        let name_max = available_width.saturating_sub(badge.len() + duration_str.len() + 10);

        let mut spans = vec![
            Span::styled(" ", Style::default()),
            Span::styled(badge, badge_style),
            Span::styled(" ", Style::default()),
            Span::styled(
                truncate_str(&entry.filename, name_max),
                if is_selected {
                    Theme::accent_bold()
                } else {
                    Theme::text()
                },
            ),
            Span::styled("  ", Style::default()),
            Span::styled(duration_str, Theme::dim()),
        ];

        if is_selected && entry.status == BugreportStatus::Completed {
            spans.push(Span::styled("  [d:DOWNLOAD]", Theme::accent()));
        }

        lines.push(Line::from(spans).style(row_style));
    }

    while lines.len() < visible_height {
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        inner,
    );
}

/// Footer with keybinding hints.
fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(
        frame,
        area,
        &[
            ("g", "generate"),
            ("c", "cancel"),
            ("d", "download"),
            ("j/k", "navigate"),
        ],
    );
}
