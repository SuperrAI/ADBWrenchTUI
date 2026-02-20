use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::adb::types::LogLevel;
use crate::components::{render_keybinding_footer, truncate_str};
use crate::theme::Theme;

/// Render the Logcat page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Length(1), // filter bar
        Constraint::Min(0),   // log area
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[2]);
        render_footer(frame, chunks[3]);
        return;
    }

    render_filter_bar(app, frame, chunks[1]);
    render_logs(app, frame, chunks[2]);
    render_footer(frame, chunks[3]);
}

/// Header with event count, buffer name, and LIVE indicator.
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled(" LOGCAT", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("VIEWER", Theme::dim()),
    ];

    if app.device_manager.is_connected() {
        spans.push(Span::styled(
            format!("  {} events", app.logcat.logs.len()),
            Theme::muted(),
        ));
        spans.push(Span::styled(
            format!("  [{}]", app.logcat.buffer.label()),
            Theme::dim(),
        ));

        if app.logcat.is_streaming {
            spans.push(Span::styled("  ● LIVE", Theme::success()));
        } else {
            spans.push(Span::styled("  ○ PAUSED", Theme::muted()));
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Filter bar with search, tag, level toggles, time/auto toggles.
fn render_filter_bar(app: &App, frame: &mut Frame, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Min(0),       // search/tag area
        Constraint::Length(30),   // level + toggles
    ])
    .split(area);

    // Search and tag display
    let search_tag_area = cols[0];
    let mut filter_spans = vec![Span::raw(" ")];

    // Search
    if app.logcat.search_active {
        filter_spans.push(Span::styled("SEARCH:", Theme::accent()));
        filter_spans.push(Span::styled(
            format!("{}\u{2588}", app.logcat.search_query),
            Theme::text(),
        ));
    } else if !app.logcat.search_query.is_empty() {
        filter_spans.push(Span::styled("SEARCH:", Theme::dim()));
        filter_spans.push(Span::styled(app.logcat.search_query.clone(), Theme::text()));
    } else {
        filter_spans.push(Span::styled("/search", Theme::muted()));
    }

    filter_spans.push(Span::raw("  "));

    // Tag
    if app.logcat.tag_active {
        filter_spans.push(Span::styled("TAG:", Theme::accent()));
        filter_spans.push(Span::styled(
            format!("{}\u{2588}", app.logcat.tag_filter),
            Theme::text(),
        ));
    } else if !app.logcat.tag_filter.is_empty() {
        filter_spans.push(Span::styled("TAG:", Theme::dim()));
        filter_spans.push(Span::styled(app.logcat.tag_filter.clone(), Theme::text()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(filter_spans)).style(Style::default().bg(Theme::BG)),
        search_tag_area,
    );

    // Level toggles + TIME + AUTO
    let level_labels = ["V", "D", "I", "W", "E", "F"];
    let level_colors = [
        Theme::LOG_VERBOSE,
        Theme::LOG_DEBUG,
        Theme::LOG_INFO,
        Theme::LOG_WARN,
        Theme::LOG_ERROR,
        Theme::LOG_FATAL,
    ];

    let mut toggle_spans = vec![Span::raw(" ")];
    for (i, (label, color)) in level_labels.iter().zip(level_colors.iter()).enumerate() {
        let style = if app.logcat.level_filter[i] {
            Style::default().fg(*color)
        } else {
            Theme::muted()
        };
        toggle_spans.push(Span::styled(format!("[{label}]"), style));
    }

    toggle_spans.push(Span::raw(" "));

    // TIME toggle
    if app.logcat.show_timestamp {
        toggle_spans.push(Span::styled("[TIME]", Theme::accent()));
    } else {
        toggle_spans.push(Span::styled("[TIME]", Theme::muted()));
    }

    // AUTO toggle
    toggle_spans.push(Span::raw(" "));
    if app.logcat.auto_scroll {
        toggle_spans.push(Span::styled("[AUTO]", Theme::accent()));
    } else {
        toggle_spans.push(Span::styled("[AUTO]", Theme::muted()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(toggle_spans)).style(Style::default().bg(Theme::BG)),
        cols[1],
    );
}

/// Render the log entries.
fn render_logs(app: &App, frame: &mut Frame, area: Rect) {
    let visible_height = area.height as usize;

    // Apply filters
    let filtered: Vec<_> = app.logcat.logs.iter().filter(|entry| {
        // Level filter
        let level_idx = match entry.level {
            LogLevel::Verbose => 0,
            LogLevel::Debug => 1,
            LogLevel::Info => 2,
            LogLevel::Warn => 3,
            LogLevel::Error => 4,
            LogLevel::Fatal => 5,
        };
        if !app.logcat.level_filter[level_idx] {
            return false;
        }
        // Tag filter
        if !app.logcat.tag_filter.is_empty()
            && !entry.tag.to_lowercase().contains(&app.logcat.tag_filter.to_lowercase())
        {
            return false;
        }
        // Search filter
        if !app.logcat.search_query.is_empty() {
            let query_lower = app.logcat.search_query.to_lowercase();
            if !entry.message.to_lowercase().contains(&query_lower)
                && !entry.tag.to_lowercase().contains(&query_lower)
            {
                return false;
            }
        }
        true
    }).collect();

    if filtered.is_empty() {
        let msg = if app.logcat.logs.is_empty() {
            "Press s to start logcat"
        } else {
            "No entries match filters"
        };
        let hint = Paragraph::new(Span::styled(msg, Theme::muted()))
            .alignment(Alignment::Center);
        let centered = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(area);
        frame.render_widget(hint, centered[1]);
        return;
    }

    let total = filtered.len();
    let scroll = app.logcat.scroll_offset.min(total.saturating_sub(visible_height));
    let available_width = area.width as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);

    for i in scroll..(scroll + visible_height).min(total) {
        let entry = filtered[i];

        let level_color = match entry.level {
            LogLevel::Verbose => Theme::LOG_VERBOSE,
            LogLevel::Debug => Theme::LOG_DEBUG,
            LogLevel::Info => Theme::LOG_INFO,
            LogLevel::Warn => Theme::LOG_WARN,
            LogLevel::Error => Theme::LOG_ERROR,
            LogLevel::Fatal => Theme::LOG_FATAL,
        };

        let mut spans = Vec::new();
        spans.push(Span::raw(" "));

        // Optional timestamp
        if app.logcat.show_timestamp && !entry.timestamp.is_empty() {
            spans.push(Span::styled(
                truncate_str(&entry.timestamp, 18),
                Theme::muted(),
            ));
            spans.push(Span::raw(" "));
        }

        // Level letter
        spans.push(Span::styled(
            entry.level.label(),
            Style::default().fg(level_color),
        ));
        spans.push(Span::raw(" "));

        // Tag (truncated)
        let tag_max = 20.min(available_width / 4);
        if !entry.tag.is_empty() {
            spans.push(Span::styled(
                truncate_str(&entry.tag, tag_max),
                Theme::dim(),
            ));
            spans.push(Span::styled(": ", Theme::muted()));
        }

        // Message (fill remaining width)
        let used_width: usize = spans.iter().map(|s| s.content.len()).sum();
        let msg_max = available_width.saturating_sub(used_width);
        spans.push(Span::styled(
            truncate_str(&entry.message, msg_max),
            Theme::text(),
        ));

        lines.push(Line::from(spans));
    }

    // Fill remaining
    while lines.len() < visible_height {
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Footer with keybinding hints.
fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(frame, area, &[
        ("s", "start/stop"),
        ("b", "buffer"),
        ("v/d/i/w/e/f", "levels"),
        ("/", "search"),
        ("t", "time"),
        ("a", "auto"),
        ("c", "clear"),
    ]);
}
