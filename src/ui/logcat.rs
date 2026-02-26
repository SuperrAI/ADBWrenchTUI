use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::adb::types::LogLevel;
use crate::app::{App, LogcatControl, LogcatFocus};
use crate::components::{render_keybinding_footer, truncate_str};
use crate::theme::Theme;

/// Render the Logcat page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Length(1), // control bar
        Constraint::Length(1), // search/tag display
        Constraint::Min(0),    // log area
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[3]);
        render_footer(frame, chunks[4]);
        return;
    }

    render_control_bar(app, frame, chunks[1]);
    render_filter_display(app, frame, chunks[2]);
    render_logs(app, frame, chunks[3]);
    render_footer(frame, chunks[4]);
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

        if app.logcat.dropped_lines > 0 {
            spans.push(Span::styled(
                format!("  DROP:{}", app.logcat.dropped_lines),
                Theme::warning(),
            ));
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Render the focusable control bar with all logcat controls.
fn render_control_bar(app: &App, frame: &mut Frame, area: Rect) {
    let is_bar_focused = app.logcat.focus == LogcatFocus::Controls
        && !app.logcat.search_active
        && !app.logcat.tag_active;
    let selected = app.logcat.control_index;

    let mut spans: Vec<Span> = vec![Span::raw(" ")];

    for (i, ctrl) in LogcatControl::ALL.iter().enumerate() {
        let is_sel = is_bar_focused && i == selected;
        let is_hovered = app.hover.logcat_control == Some(i);

        let (label, active) = match ctrl {
            LogcatControl::StartStop => {
                if app.logcat.is_streaming {
                    ("STOP", true)
                } else {
                    ("START", false)
                }
            }
            LogcatControl::Buffer => (app.logcat.buffer.label(), false),
            LogcatControl::Search => ("SEARCH", !app.logcat.search_query.is_empty()),
            LogcatControl::Tag => ("TAG", !app.logcat.tag_filter.is_empty()),
            LogcatControl::LevelV => ("V", app.logcat.level_filter[0]),
            LogcatControl::LevelD => ("D", app.logcat.level_filter[1]),
            LogcatControl::LevelI => ("I", app.logcat.level_filter[2]),
            LogcatControl::LevelW => ("W", app.logcat.level_filter[3]),
            LogcatControl::LevelE => ("E", app.logcat.level_filter[4]),
            LogcatControl::LevelF => ("F", app.logcat.level_filter[5]),
            LogcatControl::Timestamp => ("TIME", app.logcat.show_timestamp),
            LogcatControl::AutoScroll => ("AUTO", app.logcat.auto_scroll),
            LogcatControl::Clear => ("CLR", false),
        };

        // Color coding for level buttons
        let level_color = match ctrl {
            LogcatControl::LevelV => Some(Theme::LOG_VERBOSE),
            LogcatControl::LevelD => Some(Theme::LOG_DEBUG),
            LogcatControl::LevelI => Some(Theme::LOG_INFO),
            LogcatControl::LevelW => Some(Theme::LOG_WARN),
            LogcatControl::LevelE => Some(Theme::LOG_ERROR),
            LogcatControl::LevelF => Some(Theme::LOG_FATAL),
            _ => None,
        };

        let style = if is_sel {
            Theme::accent_bold()
        } else if is_hovered {
            Theme::accent()
        } else if let Some(lc) = level_color {
            if active {
                Style::default().fg(lc)
            } else {
                Theme::muted()
            }
        } else if active {
            Theme::accent()
        } else {
            Theme::muted()
        };

        let bracket_style = if is_sel || is_hovered {
            Theme::accent_bold()
        } else {
            Theme::muted()
        };

        if is_sel {
            spans.push(Span::styled("▸", Theme::accent()));
        } else if is_hovered {
            spans.push(Span::styled("▹", Theme::accent()));
        }
        spans.push(Span::styled("[", bracket_style));
        spans.push(Span::styled(label, style));
        spans.push(Span::styled("]", bracket_style));
        spans.push(Span::raw(" "));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Show active search/tag filter values below the control bar.
fn render_filter_display(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];

    // Search
    if app.logcat.search_active {
        spans.push(Span::styled("SEARCH: ", Theme::accent()));
        spans.push(Span::styled(
            format!("{}\u{2588}", app.logcat.search_query),
            Theme::text(),
        ));
    } else if !app.logcat.search_query.is_empty() {
        spans.push(Span::styled("SEARCH: ", Theme::dim()));
        spans.push(Span::styled(app.logcat.search_query.clone(), Theme::text()));
    }

    if !spans.is_empty() && (!app.logcat.search_query.is_empty() || app.logcat.search_active) {
        spans.push(Span::raw("  "));
    }

    // Tag
    if app.logcat.tag_active {
        spans.push(Span::styled("TAG: ", Theme::accent()));
        spans.push(Span::styled(
            format!("{}\u{2588}", app.logcat.tag_filter),
            Theme::text(),
        ));
    } else if !app.logcat.tag_filter.is_empty() {
        spans.push(Span::styled("TAG: ", Theme::dim()));
        spans.push(Span::styled(app.logcat.tag_filter.clone(), Theme::text()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Render the log entries.
fn render_logs(app: &App, frame: &mut Frame, area: Rect) {
    let visible_height = area.height as usize;
    if visible_height == 0 {
        return;
    }

    let level_enabled = app.logcat.level_filter;
    let has_tag_filter = !app.logcat.tag_filter.is_empty();
    let has_search_filter = !app.logcat.search_query.is_empty();
    let all_levels_enabled = level_enabled.iter().all(|v| *v);

    let compute_window = |total: usize| {
        let max_offset = total.saturating_sub(visible_height);
        let offset_from_bottom = if app.logcat.auto_scroll {
            0
        } else {
            app.logcat.scroll_offset.min(max_offset)
        };
        let start = total.saturating_sub(visible_height + offset_from_bottom);
        let end = (start + visible_height).min(total);
        (start, end)
    };

    let available_width = area.width as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);

    // Fast path: no active filters and all levels enabled.
    if all_levels_enabled && !has_tag_filter && !has_search_filter {
        let total = app.logcat.logs.len();
        if total == 0 {
            let hint = Paragraph::new(Span::styled(
                "Navigate to START and press Enter",
                Theme::muted(),
            ))
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

        let (start, end) = compute_window(total);
        for entry in app.logcat.logs.iter().take(end).skip(start) {
            lines.push(render_log_line(
                entry,
                app.logcat.show_timestamp,
                available_width,
            ));
        }
        while lines.len() < visible_height {
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
            area,
        );
        return;
    }

    // Filtered path.
    let filtered: Vec<_> = app
        .logcat
        .logs
        .iter()
        .filter(|entry| {
            let level_idx = level_index(entry.level);
            if !level_enabled[level_idx] {
                return false;
            }
            if has_tag_filter
                && !contains_case_insensitive_ascii(&entry.tag, &app.logcat.tag_filter)
            {
                return false;
            }
            if has_search_filter
                && !contains_case_insensitive_ascii(&entry.message, &app.logcat.search_query)
                && !contains_case_insensitive_ascii(&entry.tag, &app.logcat.search_query)
            {
                return false;
            }
            true
        })
        .collect();

    if filtered.is_empty() {
        let msg = if app.logcat.logs.is_empty() {
            "Navigate to START and press Enter"
        } else {
            "No entries match filters"
        };
        let hint = Paragraph::new(Span::styled(msg, Theme::muted())).alignment(Alignment::Center);
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
    let (start, end) = compute_window(total);
    for entry in filtered.iter().take(end).skip(start) {
        lines.push(render_log_line(
            entry,
            app.logcat.show_timestamp,
            available_width,
        ));
    }

    while lines.len() < visible_height {
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        area,
    );
}

fn level_index(level: LogLevel) -> usize {
    match level {
        LogLevel::Verbose => 0,
        LogLevel::Debug => 1,
        LogLevel::Info => 2,
        LogLevel::Warn => 3,
        LogLevel::Error => 4,
        LogLevel::Fatal => 5,
    }
}

fn contains_case_insensitive_ascii(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return false;
    }
    h.windows(n.len()).any(|window| {
        window
            .iter()
            .zip(n.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

fn render_log_line(
    entry: &crate::adb::types::LogEntry,
    show_timestamp: bool,
    available_width: usize,
) -> Line<'static> {
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

    if show_timestamp && !entry.timestamp.is_empty() {
        spans.push(Span::styled(
            truncate_str(&entry.timestamp, 18),
            Theme::muted(),
        ));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(
        entry.level.label().to_string(),
        Style::default().fg(level_color),
    ));
    spans.push(Span::raw(" "));

    let tag_max = 20.min(available_width / 4);
    if !entry.tag.is_empty() {
        spans.push(Span::styled(
            truncate_str(&entry.tag, tag_max),
            Theme::dim(),
        ));
        spans.push(Span::styled(": ", Theme::muted()));
    }

    let used_width: usize = spans.iter().map(|s| s.content.len()).sum();
    let msg_max = available_width.saturating_sub(used_width);
    spans.push(Span::styled(
        truncate_str(&entry.message, msg_max),
        Theme::text(),
    ));

    Line::from(spans)
}

/// Footer with keybinding hints.
fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(
        frame,
        area,
        &[
            ("Tab", "focus"),
            ("←/→", "control"),
            ("Enter", "activate"),
            ("j/k", "scroll"),
            ("g/G", "top/bottom"),
        ],
    );
}
