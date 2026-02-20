use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, ScreenTab};
use crate::components::{render_gauge, render_keybinding_footer, render_tab_bar, truncate_str};
use crate::theme::Theme;

/// Render the Screen page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Length(1), // tab bar
        Constraint::Min(0),   // content
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[2]);
        render_footer(app, frame, chunks[3]);
        return;
    }

    render_tab_bar(frame, chunks[1], &[
        ("SCREENSHOT", app.screen.active_tab == ScreenTab::Screenshot),
        ("RECORD", app.screen.active_tab == ScreenTab::Record),
    ]);

    match app.screen.active_tab {
        ScreenTab::Screenshot => render_screenshot_tab(app, frame, chunks[2]),
        ScreenTab::Record => render_record_tab(app, frame, chunks[2]),
    }

    render_footer(app, frame, chunks[3]);
}

/// Header.
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled(" SCREEN", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("CAPTURE", Theme::dim()),
    ];

    if app.screen.is_capturing {
        spans.push(Span::styled("  ⟳ CAPTURING", Theme::warning()));
    }
    if app.screen.is_recording {
        spans.push(Span::styled("  ● RECORDING", Theme::error()));
    }
    if let Some(ref err) = app.screen.error {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(truncate_str(err, 40), Theme::error()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Screenshot tab content.
fn render_screenshot_tab(app: &App, frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(3), // action area
        Constraint::Min(0),   // history
    ])
    .split(area);

    // Capture button
    let btn_style = if app.screen.is_capturing {
        Theme::muted()
    } else {
        Theme::accent_bold()
    };
    let btn_label = if app.screen.is_capturing {
        "[ CAPTURING... ]"
    } else {
        "[ CAPTURE SCREENSHOT ]"
    };

    let btn = Paragraph::new(Line::from(Span::styled(btn_label, btn_style)))
        .alignment(Alignment::Center);
    let btn_centered = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(rows[0]);
    frame.render_widget(btn, btn_centered[1]);

    // History list
    render_screenshot_history(app, frame, rows[1]);
}

/// Screenshot history list.
fn render_screenshot_history(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" CAPTURES ({}) ", app.screen.captures.len()),
            Theme::title(),
        ))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.screen.captures.is_empty() {
        let hint = Paragraph::new(Span::styled(
            "Press c to capture a screenshot",
            Theme::muted(),
        ))
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

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    for (i, cap) in app.screen.captures.iter().enumerate().take(visible_height) {
        let is_selected = i == app.screen.capture_selected;
        let row_style = if is_selected {
            Theme::highlight()
        } else {
            Style::default().bg(Theme::BG)
        };

        let name_max = available_width.saturating_sub(22);
        lines.push(
            Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    truncate_str(&cap.filename, name_max),
                    if is_selected { Theme::accent_bold() } else { Theme::text() },
                ),
                Span::styled("  ", Style::default()),
                Span::styled(cap.timestamp.clone(), Theme::muted()),
            ])
            .style(row_style),
        );
    }

    while lines.len() < visible_height {
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        inner,
    );
}

/// Record tab content.
fn render_record_tab(app: &App, frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // duration selector
        Constraint::Length(1), // spacer
        Constraint::Length(3), // action button + progress
        Constraint::Min(0),   // history
    ])
    .split(area);

    // Duration selector
    let durations = [
        crate::app::RecordDuration::Sec30,
        crate::app::RecordDuration::Min1,
        crate::app::RecordDuration::Min2,
        crate::app::RecordDuration::Min3,
    ];
    let mut dur_spans = vec![Span::styled(" DURATION: ", Theme::dim())];
    for d in &durations {
        let is_active = app.screen.record_duration == *d;
        if is_active {
            dur_spans.push(Span::styled(format!("[{}]", d.label()), Theme::accent_bold()));
        } else {
            dur_spans.push(Span::styled(format!("[{}]", d.label()), Theme::muted()));
        }
        dur_spans.push(Span::raw(" "));
    }
    frame.render_widget(
        Paragraph::new(Line::from(dur_spans)).style(Style::default().bg(Theme::BG)),
        rows[0],
    );

    // Record button / progress
    let btn_area = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(rows[2]);

    if app.screen.is_recording {
        // Show progress
        let total = app.screen.record_duration.secs();
        let elapsed = app.screen.record_elapsed;
        let ratio = elapsed as f64 / total as f64;
        let label = format!(" RECORDING {elapsed}s / {total}s ");

        let gauge_area = Layout::horizontal([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(4),
        ])
        .split(btn_area[1]);
        render_gauge(frame, gauge_area[1], ratio, &label, Theme::RED);
    } else {
        let btn = Paragraph::new(Line::from(Span::styled(
            "[ START RECORDING ]",
            Theme::accent_bold(),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(btn, btn_area[1]);
    }

    // Recording history
    render_recording_history(app, frame, rows[3]);
}

/// Recording history list.
fn render_recording_history(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" RECORDINGS ({}) ", app.screen.recordings.len()),
            Theme::title(),
        ))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.screen.recordings.is_empty() {
        let hint = Paragraph::new(Span::styled(
            "No recordings yet",
            Theme::muted(),
        ))
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

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    for (i, rec) in app.screen.recordings.iter().enumerate().take(visible_height) {
        let is_selected = i == app.screen.recording_selected;
        let row_style = if is_selected {
            Theme::highlight()
        } else {
            Style::default().bg(Theme::BG)
        };

        let name_max = available_width.saturating_sub(30);
        lines.push(
            Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    truncate_str(&rec.filename, name_max),
                    if is_selected { Theme::accent_bold() } else { Theme::text() },
                ),
                Span::styled(format!("  {}s", rec.duration_secs), Theme::dim()),
                Span::styled("  ", Style::default()),
                Span::styled(rec.timestamp.clone(), Theme::muted()),
            ])
            .style(row_style),
        );
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
fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    match app.screen.active_tab {
        ScreenTab::Screenshot => {
            render_keybinding_footer(frame, area, &[
                ("1/2", "tab"),
                ("c", "capture"),
                ("j/k", "navigate"),
            ]);
        }
        ScreenTab::Record => {
            render_keybinding_footer(frame, area, &[
                ("1/2", "tab"),
                ("c", "record"),
                ("d", "duration"),
                ("j/k", "navigate"),
            ]);
        }
    }
}
