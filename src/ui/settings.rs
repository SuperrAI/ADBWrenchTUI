use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::{App, QUICK_TOGGLES, SettingsFocus, SettingsNamespace};
use crate::components::{render_keybinding_footer, render_text_input, toggle_span, truncate_str};
use crate::theme::Theme;

/// Render the Settings page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Length(1), // spacer
        Constraint::Length(7), // quick toggles (5 content + 2 border)
        Constraint::Length(1), // spacer
        Constraint::Length(1), // namespace tabs + search (combined row)
        Constraint::Min(0),    // settings list
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[5]);
        render_footer(frame, chunks[6]);
        return;
    }

    render_quick_toggles(app, frame, chunks[2]);
    render_tabs_and_search(app, frame, chunks[4]);
    render_settings_list(app, frame, chunks[5]);
    render_footer(frame, chunks[6]);
}

/// Header with title and count.
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let filtered = app.filtered_settings();
    let total = app.settings.settings.len();

    let mut spans = vec![
        Span::styled(" SETTINGS", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("SYSTEM", Theme::dim()),
    ];

    if app.device_manager.is_connected() {
        spans.push(Span::styled(
            format!("  {}/{total}", filtered.len()),
            Theme::muted(),
        ));
    }

    if app.settings.loading {
        spans.push(Span::styled("  \u{27f3} loading", Theme::warning()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Quick toggles grid (3x2) with descriptions.
fn render_quick_toggles(app: &App, frame: &mut Frame, area: Rect) {
    let is_focused = app.settings.focus_area == SettingsFocus::QuickToggles;
    let border_style = if is_focused {
        Theme::border_active()
    } else {
        Theme::border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(BorderType::Rounded)
        .title(Span::styled(" QUICK TOGGLES ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // 2 groups of rows: (name row, desc row, spacer) x 2 minus last spacer
    let rows = Layout::vertical([
        Constraint::Length(1), // row 1 names
        Constraint::Length(1), // row 1 descriptions
        Constraint::Length(1), // spacer
        Constraint::Length(1), // row 2 names
        Constraint::Length(1), // row 2 descriptions
    ])
    .split(inner);

    let col_layout = [
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ];

    // Row 1: toggles 0-2
    let cols_r1 = Layout::horizontal(col_layout).split(rows[0]);
    let desc_r1 = Layout::horizontal(col_layout).split(rows[1]);
    for col_idx in 0..3 {
        render_toggle_cell(
            app,
            frame,
            cols_r1[col_idx],
            desc_r1[col_idx],
            col_idx,
            is_focused,
        );
    }

    // Row 2: toggles 3-5
    let cols_r2 = Layout::horizontal(col_layout).split(rows[3]);
    let desc_r2 = Layout::horizontal(col_layout).split(rows[4]);
    for col_idx in 0..3 {
        render_toggle_cell(
            app,
            frame,
            cols_r2[col_idx],
            desc_r2[col_idx],
            3 + col_idx,
            is_focused,
        );
    }
}

/// Render a single toggle cell (name line + description line).
fn render_toggle_cell(
    app: &App,
    frame: &mut Frame,
    name_area: Rect,
    desc_area: Rect,
    toggle_idx: usize,
    section_focused: bool,
) {
    if toggle_idx >= QUICK_TOGGLES.len() {
        return;
    }

    let toggle = &QUICK_TOGGLES[toggle_idx];
    let is_on = app.settings.quick_toggle_states[toggle_idx];
    let is_selected = section_focused && app.settings.quick_toggle_focus == toggle_idx;

    let name_style = if is_selected {
        Theme::accent_bold()
    } else {
        Theme::text()
    };
    let prefix = if is_selected { "\u{25b8} " } else { "  " };

    // Name + toggle state
    let name_line = Line::from(vec![
        Span::styled(prefix, name_style),
        Span::styled(toggle.name, name_style),
        Span::raw(" "),
        toggle_span(is_on),
    ]);
    frame.render_widget(Paragraph::new(name_line), name_area);

    // Description
    let desc_line = Line::from(vec![
        Span::raw("    "),
        Span::styled(toggle.desc, Theme::muted()),
    ]);
    frame.render_widget(Paragraph::new(desc_line), desc_area);
}

/// Namespace tabs + search on a single row.
fn render_tabs_and_search(app: &App, frame: &mut Frame, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Min(0),     // tabs
        Constraint::Length(30), // search
    ])
    .split(area);

    // Tabs
    let mut tab_spans = Vec::new();
    tab_spans.push(Span::raw(" "));
    for ns in &[
        SettingsNamespace::System,
        SettingsNamespace::Secure,
        SettingsNamespace::Global,
    ] {
        if app.settings.namespace == *ns {
            tab_spans.push(Span::styled(
                format!("[{}]", ns.label()),
                Theme::accent_bold(),
            ));
        } else {
            tab_spans.push(Span::styled(format!("[{}]", ns.label()), Theme::muted()));
        }
        tab_spans.push(Span::raw(" "));
    }
    frame.render_widget(
        Paragraph::new(Line::from(tab_spans)).style(Style::default().bg(Theme::BG)),
        cols[0],
    );

    // Search
    if app.settings.search_active {
        render_text_input(
            frame,
            cols[1],
            &app.settings.search_query,
            app.settings.search_query.chars().count(),
            "/",
            true,
        );
    } else {
        let display = if app.settings.search_query.is_empty() {
            Span::styled("/ search", Theme::muted())
        } else {
            Span::styled(format!("/{}", app.settings.search_query), Theme::dim())
        };
        frame.render_widget(
            Paragraph::new(Line::from(display)).style(Style::default().bg(Theme::BG)),
            cols[1],
        );
    }
}

/// Scrollable settings list with column headers.
fn render_settings_list(app: &App, frame: &mut Frame, area: Rect) {
    let is_focused = app.settings.focus_area == SettingsFocus::List;
    let border_style = if is_focused {
        Theme::border_active()
    } else {
        Theme::border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" {} ", app.settings.namespace.label()),
            Theme::title(),
        ))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let filtered = app.filtered_settings();

    if filtered.is_empty() {
        let msg = if app.settings.loading {
            "Loading settings..."
        } else if app.settings.settings.is_empty() {
            "Press r to load settings"
        } else {
            "No settings found"
        };
        let hint = Paragraph::new(Span::styled(msg, Theme::muted())).alignment(Alignment::Center);
        let centered = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);
        frame.render_widget(hint, centered[1]);
        return;
    }

    // Split: column header + list
    let list_chunks = Layout::vertical([
        Constraint::Length(1), // column header
        Constraint::Min(0),    // list rows
    ])
    .split(inner);

    let available_width = inner.width as usize;
    let key_width = available_width / 3;
    let value_width = available_width.saturating_sub(key_width + 4);

    // Column header
    let header_line = Line::from(vec![
        Span::styled(
            format!(" {:<width$}", "KEY", width = key_width),
            Theme::muted(),
        ),
        Span::styled("   ", Theme::muted()),
        Span::styled(
            format!("{:<width$}", "VALUE", width = value_width),
            Theme::muted(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(header_line).style(Style::default().bg(Theme::BG)),
        list_chunks[0],
    );

    // List rows
    let visible_height = list_chunks[1].height as usize;
    let selected = app
        .settings
        .selected_index
        .min(filtered.len().saturating_sub(1));

    // Compute effective scroll that keeps selection visible
    let mut scroll = app.settings.scroll_offset;
    if selected < scroll {
        scroll = selected;
    } else if visible_height > 0 && selected >= scroll + visible_height {
        scroll = selected - visible_height + 1;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);

    for i in scroll..(scroll + visible_height).min(filtered.len()) {
        let entry = filtered[i];
        let is_selected = i == selected;

        let row_style = if is_selected {
            Theme::highlight()
        } else {
            Style::default().bg(Theme::BG)
        };

        let key_display = truncate_str(&entry.key, key_width);
        let val_display = truncate_str(&entry.value, value_width);

        let key_style = if is_selected {
            Theme::accent_bold()
        } else {
            Theme::text()
        };

        let mut spans = vec![
            Span::styled(
                format!(" {:<width$}", key_display, width = key_width),
                key_style,
            ),
            Span::styled(" = ", Theme::muted()),
            Span::styled(
                format!("{:<width$}", val_display, width = value_width),
                Theme::dim(),
            ),
        ];

        if is_selected {
            spans.push(Span::styled(" [e] [d]", Theme::accent()));
        }

        lines.push(Line::from(spans).style(row_style));
    }

    // Fill remaining
    while lines.len() < visible_height {
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        list_chunks[1],
    );
}

/// Footer with keybinding hints.
fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(
        frame,
        area,
        &[
            ("n", "namespace"),
            ("/", "search"),
            ("Tab", "focus"),
            ("Space", "toggle"),
            ("e", "edit"),
            ("d", "delete"),
            ("r", "refresh"),
        ],
    );
}
