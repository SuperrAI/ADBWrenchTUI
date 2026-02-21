use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use ratatui_image::{Resize, StatefulImage, protocol::StatefulProtocol};

use crate::app::{App, ScreenTab};
use crate::components::{render_gauge, render_keybinding_footer, render_tab_bar, truncate_str};
use crate::theme::Theme;

/// Render the Screen page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let path_input_row = if app.screen.path_input_active { 1 } else { 0 };
    let chunks = Layout::vertical([
        Constraint::Length(2),                      // header
        Constraint::Length(path_input_row),          // path input (0 when hidden)
        Constraint::Length(1),                       // tab bar
        Constraint::Min(0),                         // content
        Constraint::Length(1),                       // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if app.screen.path_input_active {
        render_path_input(app, frame, chunks[1]);
    }

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[3]);
        render_footer(app, frame, chunks[4]);
        return;
    }

    render_tab_bar(frame, chunks[2], &[
        ("SCREENSHOT", app.screen.active_tab == ScreenTab::Screenshot),
        ("RECORD", app.screen.active_tab == ScreenTab::Record),
    ]);

    match app.screen.active_tab {
        ScreenTab::Screenshot => render_screenshot_tab(app, frame, chunks[3]),
        ScreenTab::Record => render_record_tab(app, frame, chunks[3]),
    }

    render_footer(app, frame, chunks[4]);
}

/// Header with capture count and status.
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled(" SCREEN", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("CAPTURE", Theme::dim()),
    ];

    if let Some(ref status) = app.screen.status {
        spans.push(Span::styled(format!("  ⟳ {status}"), Theme::warning()));
    } else if app.screen.is_capturing {
        spans.push(Span::styled("  ⟳ CAPTURING", Theme::warning()));
    } else if app.screen.is_recording {
        spans.push(Span::styled("  ● RECORDING", Theme::error()));
    }

    // Capture count
    if !app.screen.captures.is_empty() {
        spans.push(Span::styled(
            format!("  {} CAPTURES", app.screen.captures.len()),
            Theme::dim(),
        ));
    }

    if let Some(ref err) = app.screen.error {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(truncate_str(err, 40), Theme::error()));
    }

    let dir_label = truncate_str(&app.config.output_dir, area.width.saturating_sub(10) as usize);
    let dir_line = Line::from(vec![
        Span::styled(" DIR: ", Theme::muted()),
        Span::styled(dir_label, Theme::dim()),
    ]);

    frame.render_widget(
        Paragraph::new(vec![Line::from(spans), dir_line]).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Inline path input bar.
fn render_path_input(app: &App, frame: &mut Frame, area: Rect) {
    let spans = vec![
        Span::styled(" PATH: ", Theme::accent_bold()),
        Span::styled(&app.screen.path_input, Theme::text()),
        Span::styled("█", Theme::accent()),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG_ELEVATED)),
        area,
    );
}

// ── Screenshot Tab ────────────────────────────────────────────────

/// Screenshot tab: preview (left) + history (right), or full-width empty state.
fn render_screenshot_tab(app: &App, frame: &mut Frame, area: Rect) {
    if app.screen.captures.is_empty() {
        // Full-width empty state with capture prompt
        render_empty_screenshot_state(app, frame, area);
        return;
    }

    // Two-column layout: preview (60%) + history list (40%)
    let cols = Layout::horizontal([
        Constraint::Percentage(60),
        Constraint::Percentage(40),
    ])
    .split(area);

    render_preview(app, frame, cols[0]);
    render_screenshot_history(app, frame, cols[1]);
}

/// Empty state when no screenshots have been captured.
fn render_empty_screenshot_state(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(" SCREENSHOT ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

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

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("NO SCREENSHOTS", Theme::dim())),
        Line::from(""),
        Line::from(Span::styled(btn_label, btn_style)),
        Line::from(""),
        Line::from(Span::styled("Press c to capture", Theme::muted())),
    ];

    let centered = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(6),
        Constraint::Fill(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        centered[1],
    );
}

/// Preview panel: renders the selected screenshot image or a placeholder.
fn render_preview(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_active())
        .border_type(BorderType::Rounded)
        .title(Span::styled(" PREVIEW ", Theme::accent_bold()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Try to render the image preview
    let mut preview_state = app.screen.preview_state.borrow_mut();
    if let Some(ref mut state) = *preview_state {
        let image_widget = StatefulImage::<StatefulProtocol>::default().resize(Resize::Fit(None));
        frame.render_stateful_widget(image_widget, inner, state);
    } else {
        // No preview available — show placeholder
        let msg = if app.screen.picker.is_none() {
            "Preview not available in this terminal"
        } else {
            "Select a screenshot to preview"
        };

        let centered = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);

        frame.render_widget(
            Paragraph::new(Span::styled(msg, Theme::muted())).alignment(Alignment::Center),
            centered[1],
        );
    }
}

/// Screenshot history list with selection.
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

    let visible_height = inner.height as usize;
    let available_width = inner.width as usize;

    // Scroll to keep selection visible
    let scroll_offset = if app.screen.capture_selected >= visible_height {
        app.screen.capture_selected - visible_height + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    for (i, cap) in app.screen.captures.iter().enumerate().skip(scroll_offset).take(visible_height) {
        let is_selected = i == app.screen.capture_selected;
        let row_style = if is_selected {
            Theme::highlight()
        } else {
            Style::default().bg(Theme::BG)
        };

        // Show indicator, filename, and timestamp
        let indicator = if is_selected { " ▸ " } else { "   " };
        let name_max = available_width.saturating_sub(5);

        lines.push(
            Line::from(vec![
                Span::styled(indicator, if is_selected { Theme::accent() } else { Style::default() }),
                Span::styled(
                    truncate_str(&cap.filename, name_max),
                    if is_selected { Theme::accent_bold() } else { Theme::text() },
                ),
            ])
            .style(row_style),
        );

        // Show timestamp on the line below if space permits
        if is_selected && lines.len() < visible_height {
            lines.push(
                Line::from(vec![
                    Span::styled("   ", Style::default()),
                    Span::styled(&cap.timestamp, Theme::muted()),
                ])
                .style(row_style),
            );
        }
    }

    while lines.len() < visible_height {
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        inner,
    );
}

// ── Record Tab ────────────────────────────────────────────────────

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

// ── Footer ────────────────────────────────────────────────────────

/// Footer with keybinding hints.
fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    if app.screen.path_input_active {
        render_keybinding_footer(frame, area, &[
            ("Enter", "save"),
            ("Esc", "cancel"),
        ]);
        return;
    }
    match app.screen.active_tab {
        ScreenTab::Screenshot => {
            render_keybinding_footer(frame, area, &[
                ("1/2", "tab"),
                ("c", "capture"),
                ("j/k", "navigate"),
                ("d", "delete"),
                ("o", "open"),
                ("p", "path"),
            ]);
        }
        ScreenTab::Record => {
            render_keybinding_footer(frame, area, &[
                ("1/2", "tab"),
                ("c", "record"),
                ("d", "duration"),
                ("j/k", "navigate"),
                ("p", "path"),
            ]);
        }
    }
}
