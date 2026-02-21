use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, AppFilter, AppPanel};
use crate::components::{render_empty_state, render_keybinding_footer, render_text_input, truncate_str};
use crate::theme::Theme;

/// Render the Apps / Manager page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Length(1), // filter bar
        Constraint::Min(0),   // two-panel content
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

    // Two-panel layout: 60% list | 40% detail
    let panels = Layout::horizontal([
        Constraint::Percentage(60),
        Constraint::Percentage(40),
    ])
    .split(chunks[2]);

    render_package_list(app, frame, panels[0]);
    render_detail_panel(app, frame, panels[1]);

    render_footer(frame, chunks[3]);
}

// ── Header ────────────────────────────────────────────────────────

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let filtered = app.filtered_packages();
    let total = app.apps.packages.len();
    let filtered_count = filtered.len();

    let mut spans = vec![
        Span::styled(" APPS", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("MANAGER", Theme::dim()),
    ];

    if app.device_manager.is_connected() {
        spans.push(Span::styled(
            format!("  {filtered_count}/{total}"),
            Theme::muted(),
        ));
    }

    if app.apps.loading {
        spans.push(Span::styled("  ⟳ loading", Theme::warning()));
    }

    // Action result flash (show for a few seconds)
    if let Some((success, ref msg, ref instant)) = app.apps.action_result {
        if instant.elapsed().as_secs() < 5 {
            spans.push(Span::styled("  ", Style::default()));
            if success {
                spans.push(Span::styled(msg, Theme::success()));
            } else {
                spans.push(Span::styled(msg, Theme::error()));
            }
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Filter bar ────────────────────────────────────────────────────

fn render_filter_bar(app: &App, frame: &mut Frame, area: Rect) {
    let filter_cols = Layout::horizontal([
        Constraint::Min(0),       // search input
        Constraint::Length(22),   // filter toggles
    ])
    .split(area);

    // Search input
    if app.apps.search_active {
        render_text_input(
            frame,
            filter_cols[0],
            &app.apps.search_query,
            app.apps.search_query.len(),
            " SEARCH: ",
            true,
        );
    } else {
        let search_display = if app.apps.search_query.is_empty() {
            Span::styled(" SEARCH: (press /)", Theme::muted())
        } else {
            Span::styled(
                format!(" SEARCH: {}", app.apps.search_query),
                Theme::dim(),
            )
        };
        frame.render_widget(
            Paragraph::new(Line::from(search_display)).style(Style::default().bg(Theme::BG)),
            filter_cols[0],
        );
    }

    // Filter toggles
    let filters = [AppFilter::All, AppFilter::User, AppFilter::System];
    let mut spans = vec![Span::raw(" ")];
    for filter in &filters {
        let is_active = app.apps.filter_type == *filter;
        let label = filter.label();
        if is_active {
            spans.push(Span::styled(format!("[{label}]"), Theme::accent_bold()));
        } else {
            spans.push(Span::styled(format!("[{label}]"), Theme::muted()));
        }
        spans.push(Span::raw(" "));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        filter_cols[1],
    );
}

// ── Package list panel ────────────────────────────────────────────

fn render_package_list(app: &App, frame: &mut Frame, area: Rect) {
    let is_focused = app.apps.focus_panel == AppPanel::List;
    let border_style = if is_focused {
        Theme::border_active()
    } else {
        Theme::border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(BorderType::Rounded)
        .title(Span::styled(" PACKAGES ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let filtered = app.filtered_packages();

    if filtered.is_empty() {
        if app.apps.loading {
            render_loading(frame, inner);
        } else {
            render_empty_state(frame, inner, "📦", "No packages found", "Try changing filters or press r to refresh");
        }
        return;
    }

    let visible_height = inner.height as usize;
    let selected = app.apps.selected_index.min(filtered.len().saturating_sub(1));
    let available_width = inner.width as usize;

    // Compute effective scroll that keeps selection visible
    let mut scroll_offset = app.apps.scroll_offset;
    if selected < scroll_offset {
        scroll_offset = selected;
    } else if visible_height > 0 && selected >= scroll_offset + visible_height {
        scroll_offset = selected - visible_height + 1;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);

    for i in scroll_offset..(scroll_offset + visible_height).min(filtered.len()) {
        let pkg = filtered[i];
        let is_selected = i == selected;

        let row_style = if is_selected {
            Theme::highlight()
        } else {
            Style::default().bg(Theme::BG)
        };

        // App icon
        let icon_style = if is_selected {
            Theme::accent_bold()
        } else {
            Theme::dim()
        };

        // System badge
        let sys_badge = if pkg.is_system { " [SYS]" } else { "" };

        // Enabled status
        let status = if pkg.is_enabled { "[ON]" } else { "[OFF]" };
        let status_style = if pkg.is_enabled {
            Theme::success()
        } else {
            Theme::muted()
        };

        // Calculate max name width
        // Layout: " [A] name...  [SYS] [ON] "
        let fixed_width = 5 + sys_badge.len() + 1 + status.len() + 2;
        let name_max = if available_width > fixed_width {
            available_width - fixed_width
        } else {
            10
        };

        let display_name = truncate_str(&pkg.package_name, name_max);
        let name_padded = format!("{:<width$}", display_name, width = name_max);

        let mut spans = vec![
            Span::styled(" [A]", icon_style),
            Span::styled(" ", Style::default()),
            Span::styled(name_padded, if is_selected { Theme::accent_bold() } else { Theme::text() }),
        ];

        if pkg.is_system {
            spans.push(Span::styled(" [SYS]", Theme::dim()));
        }

        spans.push(Span::styled(" ", Style::default()));
        spans.push(Span::styled(status, status_style));
        spans.push(Span::styled(" ", Style::default()));

        lines.push(Line::from(spans).style(row_style));
    }

    // Fill remaining lines
    while lines.len() < visible_height {
        lines.push(Line::from("").style(Style::default().bg(Theme::BG)));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        inner,
    );
}

// ── Detail panel ──────────────────────────────────────────────────

fn render_detail_panel(app: &App, frame: &mut Frame, area: Rect) {
    let is_focused = app.apps.focus_panel == AppPanel::Detail;
    let border_style = if is_focused {
        Theme::border_active()
    } else {
        Theme::border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(BorderType::Rounded)
        .title(Span::styled(" DETAILS ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Show loading state for detail
    if app.apps.detail_loading {
        render_loading(frame, inner);
        return;
    }

    // Show package details if available
    let details = match app.apps.package_details {
        Some(ref d) => d,
        None => {
            render_empty_state(frame, inner, "📋", "SELECT A PACKAGE", "Choose a package from the list");
            return;
        }
    };

    let rows = Layout::vertical([
        Constraint::Length(2), // package name
        Constraint::Length(1), // version
        Constraint::Length(1), // path
        Constraint::Length(1), // installed
        Constraint::Length(1), // updated
        Constraint::Length(1), // spacer
        Constraint::Length(1), // actions row
        Constraint::Length(1), // spacer
        Constraint::Length(1), // permissions header
        Constraint::Min(0),   // permissions list
    ])
    .split(inner);

    // Package name
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(&details.package_name, Theme::accent_bold()),
        ])),
        rows[0],
    );

    // Version
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" VERSION: ", Theme::muted()),
            Span::styled(&details.version_name, Theme::text()),
            Span::styled(format!(" ({})", details.version_code), Theme::dim()),
        ])),
        rows[1],
    );

    // Path
    let path_display = truncate_str(&details.installed_path, inner.width.saturating_sub(8) as usize);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" PATH: ", Theme::muted()),
            Span::styled(path_display, Theme::dim()),
        ])),
        rows[2],
    );

    // Installed time
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" INSTALLED: ", Theme::muted()),
            Span::styled(&details.first_install_time, Theme::text()),
        ])),
        rows[3],
    );

    // Updated time
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" UPDATED: ", Theme::muted()),
            Span::styled(&details.last_update_time, Theme::text()),
        ])),
        rows[4],
    );

    // Actions row
    let actions = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled("[o:OPEN]", Theme::accent()),
        Span::styled(" ", Style::default()),
        Span::styled("[x:STOP]", Theme::warning()),
        Span::styled(" ", Style::default()),
        Span::styled("[c:CLEAR]", Theme::warning()),
        Span::styled(" ", Style::default()),
        Span::styled("[u:DELETE]", Theme::error()),
    ]);
    frame.render_widget(Paragraph::new(actions), rows[6]);

    // Permissions header
    let perm_count = details.permissions.len();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" PERMISSIONS ({perm_count})"), Theme::dim()),
        ])),
        rows[8],
    );

    // Permissions list (scrollable)
    let perm_area = rows[9];
    let visible_height = perm_area.height as usize;
    let scroll = app.apps.detail_scroll_offset;
    let max_width = perm_area.width.saturating_sub(3) as usize;

    let mut perm_lines: Vec<Line> = Vec::with_capacity(visible_height);
    for i in scroll..(scroll + visible_height).min(details.permissions.len()) {
        let perm = &details.permissions[i];
        perm_lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(truncate_str(perm, max_width), Theme::muted()),
        ]));
    }

    // Fill remaining
    while perm_lines.len() < visible_height {
        perm_lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(perm_lines).style(Style::default().bg(Theme::BG)),
        perm_area,
    );
}

// ── Loading state ─────────────────────────────────────────────────

fn render_loading(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled("⟳ ", Theme::warning()),
        Span::styled("Loading...", Theme::dim()),
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

// ── Footer ────────────────────────────────────────────────────────

fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(frame, area, &[
        ("/", "search"),
        ("f", "filter"),
        ("Tab", "panel"),
        ("o", "open"),
        ("x", "stop"),
        ("c", "clear"),
        ("u", "uninstall"),
        ("r", "refresh"),
    ]);
}
