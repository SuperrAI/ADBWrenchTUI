use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, SettingsFocus, SettingsNamespace, QUICK_TOGGLES};
use crate::components::{
    render_keybinding_footer, render_tab_bar, render_text_input, toggle_span, truncate_str,
};
use crate::theme::Theme;

/// Render the Settings page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Length(5), // quick toggles
        Constraint::Length(1), // namespace tabs
        Constraint::Length(1), // search bar
        Constraint::Min(0),   // settings list
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[4]);
        render_footer(frame, chunks[5]);
        return;
    }

    render_quick_toggles(app, frame, chunks[1]);
    render_namespace_tabs(app, frame, chunks[2]);
    render_search_bar(app, frame, chunks[3]);
    render_settings_list(app, frame, chunks[4]);
    render_footer(frame, chunks[5]);
}

/// Header with title and loading state.
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
        spans.push(Span::styled("  ⟳ loading", Theme::warning()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Quick toggles grid (3x2).
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

    // 3 columns x 2 rows
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    for row_idx in 0..2 {
        let cols = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[row_idx]);

        for col_idx in 0..3 {
            let toggle_idx = row_idx * 3 + col_idx;
            if toggle_idx < QUICK_TOGGLES.len() {
                let toggle = &QUICK_TOGGLES[toggle_idx];
                let is_on = app.settings.quick_toggle_states[toggle_idx];
                let is_selected = is_focused && app.settings.quick_toggle_focus == toggle_idx;

                let name_style = if is_selected {
                    Theme::accent_bold()
                } else {
                    Theme::text()
                };
                let prefix = if is_selected { "▸ " } else { "  " };

                let line = Line::from(vec![
                    Span::styled(prefix, name_style),
                    Span::styled(toggle.name, name_style),
                    Span::styled(" ", Style::default()),
                    toggle_span(is_on),
                ]);
                frame.render_widget(Paragraph::new(line), cols[col_idx]);
            }
        }
    }
}

/// Namespace tab bar.
fn render_namespace_tabs(app: &App, frame: &mut Frame, area: Rect) {
    render_tab_bar(frame, area, &[
        (
            SettingsNamespace::System.label(),
            app.settings.namespace == SettingsNamespace::System,
        ),
        (
            SettingsNamespace::Secure.label(),
            app.settings.namespace == SettingsNamespace::Secure,
        ),
        (
            SettingsNamespace::Global.label(),
            app.settings.namespace == SettingsNamespace::Global,
        ),
    ]);
}

/// Search bar.
fn render_search_bar(app: &App, frame: &mut Frame, area: Rect) {
    if app.settings.search_active {
        render_text_input(
            frame,
            area,
            &app.settings.search_query,
            app.settings.search_query.len(),
            " SEARCH: ",
            true,
        );
    } else {
        let display = if app.settings.search_query.is_empty() {
            Span::styled(" SEARCH: (press /)", Theme::muted())
        } else {
            Span::styled(
                format!(" SEARCH: {}", app.settings.search_query),
                Theme::dim(),
            )
        };
        frame.render_widget(
            Paragraph::new(Line::from(display)).style(Style::default().bg(Theme::BG)),
            area,
        );
    }
}

/// Scrollable settings list.
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
        } else {
            "No settings found"
        };
        let hint = Paragraph::new(Span::styled(msg, Theme::muted()))
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
    let selected = app.settings.selected_index;
    let scroll = app.settings.scroll_offset;
    let available_width = inner.width as usize;

    // Key-value layout: key = value [EDIT] [DEL]
    let action_width = 12; // " [EDIT] [DEL]"
    let key_width = available_width / 3;
    let value_width = available_width.saturating_sub(key_width + action_width + 4);

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
            Span::styled(format!(" {:<width$}", key_display, width = key_width), key_style),
            Span::styled(" = ", Theme::muted()),
            Span::styled(format!("{:<width$}", val_display, width = value_width), Theme::dim()),
        ];

        if is_selected {
            spans.push(Span::styled(" [e:EDIT]", Theme::accent()));
            spans.push(Span::styled(" [d:DEL]", Theme::error()));
        }

        lines.push(Line::from(spans).style(row_style));
    }

    // Fill remaining
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
    render_keybinding_footer(frame, area, &[
        ("n", "namespace"),
        ("/", "search"),
        ("Tab", "focus"),
        ("Space", "toggle"),
        ("e", "edit"),
        ("d", "delete"),
        ("r", "refresh"),
    ]);
}
