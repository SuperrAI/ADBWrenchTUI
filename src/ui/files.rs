use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::components::{render_empty_state, render_keybinding_footer, truncate_str};
use crate::theme::Theme;

/// Render the Files / Browser page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let has_selected = !app.files.selected_files.is_empty();

    // Build dynamic constraints based on whether selection bar is visible.
    let mut constraints = vec![
        Constraint::Length(2), // header
        Constraint::Length(1), // breadcrumbs
        Constraint::Length(1), // shortcuts
    ];
    if has_selected {
        constraints.push(Constraint::Length(1)); // selection bar
    }
    constraints.push(Constraint::Min(0)); // file list
    constraints.push(Constraint::Length(1)); // footer

    let chunks = Layout::vertical(constraints).split(area);

    // Track chunk indices dynamically.
    let mut idx = 0;

    // ── Header ───────────────────────────────────────────────────
    render_header(app, frame, chunks[idx]);
    idx += 1;

    // ── Disconnected guard ───────────────────────────────────────
    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[idx]);
        render_footer(frame, chunks[chunks.len() - 1]);
        return;
    }

    // ── Breadcrumbs ──────────────────────────────────────────────
    render_breadcrumbs(app, frame, chunks[idx]);
    idx += 1;

    // ── Quick shortcuts ──────────────────────────────────────────
    render_shortcuts(frame, chunks[idx]);
    idx += 1;

    // ── Selection bar (conditional) ──────────────────────────────
    if has_selected {
        render_selection_bar(app, frame, chunks[idx]);
        idx += 1;
    }

    // ── File list ────────────────────────────────────────────────
    let list_area = chunks[idx];
    idx += 1;

    if app.files.loading {
        render_loading(frame, list_area);
    } else if app.files.entries.is_empty() {
        if let Some(ref err) = app.files.error {
            render_error(frame, list_area, err);
        } else {
            render_empty_state(frame, list_area, "📂", "Empty directory", "Press h to go up");
        }
    } else {
        render_file_list(app, frame, list_area);
    }

    // ── Footer ───────────────────────────────────────────────────
    render_footer(frame, chunks[idx]);
}

// ── Header ────────────────────────────────────────────────────────

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let entry_count = app.files.entries.len();
    let mut spans = vec![
        Span::styled(" FILES", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("BROWSER", Theme::dim()),
    ];

    if app.device_manager.is_connected() {
        spans.push(Span::styled(
            format!("  {entry_count} items"),
            Theme::muted(),
        ));
    }

    if app.files.loading {
        spans.push(Span::styled("  ⟳ loading", Theme::warning()));
    }

    if let Some(ref err) = app.files.error {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            truncate_str(err, 40),
            Theme::error(),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Breadcrumbs ───────────────────────────────────────────────────

fn render_breadcrumbs(app: &App, frame: &mut Frame, area: Rect) {
    let path = &app.files.current_path;
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let mut spans = vec![Span::styled(" /", Theme::accent())];
    for seg in &segments {
        spans.push(Span::styled(" > ", Theme::muted()));
        spans.push(Span::styled(*seg, Theme::dim()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Quick shortcuts ───────────────────────────────────────────────

fn render_shortcuts(frame: &mut Frame, area: Rect) {
    let shortcuts = [
        ("1", "SDCARD"),
        ("2", "DOWNLOAD"),
        ("3", "DCIM"),
        ("4", "TMP"),
    ];

    let mut spans = vec![Span::raw(" ")];
    for (i, (key, label)) in shortcuts.iter().enumerate() {
        spans.push(Span::styled(format!("[{key}:{label}]"), Theme::muted()));
        if i < shortcuts.len() - 1 {
            spans.push(Span::raw(" "));
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Selection bar ─────────────────────────────────────────────────

fn render_selection_bar(app: &App, frame: &mut Frame, area: Rect) {
    let count = app.files.selected_files.len();
    let spans = vec![
        Span::styled(format!(" {count} SELECTED"), Theme::accent_bold()),
        Span::styled("  ", Style::default()),
        Span::styled("[d:DELETE]", Theme::accent()),
        Span::styled(" ", Style::default()),
        Span::styled("[p:PULL]", Theme::accent()),
    ];

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── File list ─────────────────────────────────────────────────────

fn render_file_list(app: &App, frame: &mut Frame, area: Rect) {
    let entries = &app.files.entries;
    let visible_height = area.height as usize;
    let selected = app.files.selected_index;

    // Calculate scroll offset to keep selected item visible.
    let scroll_offset = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };

    let available_width = area.width as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);

    for i in scroll_offset..(scroll_offset + visible_height).min(entries.len()) {
        let entry = &entries[i];
        let is_selected = i == selected;
        let is_checked = app.files.selected_files.contains(&entry.path);

        let row_style = if is_selected {
            Theme::highlight()
        } else {
            Style::default().bg(Theme::BG)
        };

        // Type icon
        let (icon, icon_style) = if entry.is_symlink {
            ("[L]", Theme::dim())
        } else if entry.is_directory {
            ("[D]", Style::default().fg(Theme::BLUE))
        } else {
            ("[F]", Theme::accent())
        };

        // Size formatting
        let size_str = if entry.is_directory {
            "--".to_string()
        } else {
            format_size(entry.size)
        };

        // Checkbox
        let checkbox = if is_checked { "[X]" } else { "[ ]" };

        // Calculate space for filename
        // Layout: " [T] name...  size  perms  [X] "
        let fixed_width = 3 + 1 + 2 + size_str.len() + 2 + entry.permissions.len() + 2 + 3 + 1;
        let name_max = if available_width > fixed_width {
            available_width - fixed_width
        } else {
            10
        };

        let display_name = truncate_str(&entry.name, name_max);
        // Pad name to fill available space
        let name_padded = format!("{:<width$}", display_name, width = name_max);

        let spans = vec![
            Span::styled(format!(" {icon}"), icon_style),
            Span::styled(" ", Style::default()),
            Span::styled(name_padded, if is_selected { Theme::accent_bold() } else { Theme::text() }),
            Span::styled("  ", Style::default()),
            Span::styled(size_str, Theme::dim()),
            Span::styled("  ", Style::default()),
            Span::styled(entry.permissions.clone(), Theme::muted()),
            Span::styled("  ", Style::default()),
            Span::styled(checkbox, if is_checked { Theme::accent() } else { Theme::muted() }),
            Span::styled(" ", Style::default()),
        ];

        lines.push(Line::from(spans).style(row_style));
    }

    // Fill remaining lines with empty space
    while lines.len() < visible_height {
        lines.push(Line::from("").style(Style::default().bg(Theme::BG)));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Loading state ─────────────────────────────────────────────────

fn render_loading(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled("⟳ ", Theme::warning()),
        Span::styled("Loading directory...", Theme::dim()),
    ]);
    let centered = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(text).alignment(ratatui::layout::Alignment::Center),
        centered[1],
    );
}

// ── Error state ───────────────────────────────────────────────────

fn render_error(frame: &mut Frame, area: Rect, error: &str) {
    render_empty_state(frame, area, "✕", error, "Press r to retry or h to go up");
}

// ── Footer ────────────────────────────────────────────────────────

fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(frame, area, &[
        ("Enter", "open"),
        ("Space", "select"),
        ("h", "up"),
        ("d", "delete"),
        ("p", "pull"),
        ("m", "mkdir"),
        ("r", "refresh"),
        ("a", "all"),
    ]);
}

// ── Helpers ───────────────────────────────────────────────────────

/// Format a file size into a human-readable string.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
