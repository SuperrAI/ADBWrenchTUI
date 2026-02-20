use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, ShellEntryType};
use crate::components::{render_keybinding_footer, render_text_input};
use crate::theme::Theme;

/// Preset quick commands.
const PRESETS: [(&str, &str); 8] = [
    ("1", "getprop"),
    ("2", "pm list packages"),
    ("3", "dumpsys battery"),
    ("4", "df -h"),
    ("5", "top -n 1 -b -m 5"),
    ("6", "ps -A"),
    ("7", "netstat -tlnp"),
    ("8", "ip addr"),
];

/// Render the Shell page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Length(1), // presets
        Constraint::Min(0),   // output
        Constraint::Length(1), // input
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[2]);
        render_footer(frame, chunks[4]);
        return;
    }

    render_presets(frame, chunks[1]);
    render_output(app, frame, chunks[2]);
    render_input(app, frame, chunks[3]);
    render_footer(frame, chunks[4]);
}

/// Header with title, timeout selector, and status.
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled(" SHELL", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("ADB", Theme::dim()),
    ];

    if app.device_manager.is_connected() {
        spans.push(Span::styled(
            format!("  TIMEOUT:{}", app.shell.timeout.label()),
            Theme::muted(),
        ));

        if app.shell.is_running {
            spans.push(Span::styled("  ⟳ RUNNING", Theme::warning()));
        } else if app.shell.is_streaming {
            spans.push(Span::styled("  ● STREAMING", Theme::success()));
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Quick command presets row.
fn render_presets(frame: &mut Frame, area: Rect) {
    let mut spans = vec![Span::raw(" ")];
    for (i, (key, label)) in PRESETS.iter().enumerate() {
        spans.push(Span::styled(format!("[{key}:{label}]"), Theme::muted()));
        if i < PRESETS.len() - 1 {
            spans.push(Span::raw(" "));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Scrollable output area.
fn render_output(app: &App, frame: &mut Frame, area: Rect) {
    let visible_height = area.height as usize;
    let total = app.shell.output.len();

    if total == 0 {
        let hint = Paragraph::new(Line::from(Span::styled(
            "Type a command and press Enter",
            Theme::muted(),
        )))
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

    let scroll = app.shell.scroll_offset;
    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);

    for i in scroll..(scroll + visible_height).min(total) {
        let entry = &app.shell.output[i];
        let line = match entry.entry_type {
            ShellEntryType::Command => Line::from(vec![
                Span::styled("$ ", Theme::accent()),
                Span::styled(entry.content.clone(), Theme::accent_bold()),
            ]),
            ShellEntryType::Output => Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(entry.content.clone(), Theme::text()),
            ]),
            ShellEntryType::Error => Line::from(vec![
                Span::styled("! ", Theme::error()),
                Span::styled(entry.content.clone(), Theme::error()),
            ]),
        };
        lines.push(line);
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

/// Input line at the bottom.
fn render_input(app: &App, frame: &mut Frame, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(8),
    ])
    .split(area);

    render_text_input(
        frame,
        cols[0],
        &app.shell.input,
        app.shell.cursor_pos,
        "$ ",
        true,
    );

    // Run/Stop indicator
    let indicator = if app.shell.is_running || app.shell.is_streaming {
        Span::styled(" [STOP]", Theme::error())
    } else {
        Span::styled("  [RUN]", Theme::accent())
    };
    frame.render_widget(
        Paragraph::new(Line::from(indicator)).style(Style::default().bg(Theme::BG)),
        cols[1],
    );
}

/// Footer with keybinding hints.
fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(frame, area, &[
        ("Enter", "run"),
        ("↑/↓", "history"),
        ("Ctrl+C", "stop"),
        ("c", "clear"),
        ("t", "timeout"),
        ("1-8", "preset"),
    ]);
}
