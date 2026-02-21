use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::components::{render_gauge, render_keybinding_footer, render_text_input};
use crate::theme::Theme;

/// Items within each section.
const POWER_ITEMS: [&str; 3] = ["Reboot", "Recovery", "Bootloader"];
const HARDWARE_KEYS: [&str; 8] = ["HOME", "BACK", "MENU", "RECENT", "PLAY", "PREV", "NEXT", "CAM"];

/// Render the Controls page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let has_result = app.controls.result.is_some();
    let has_loading = app.controls.loading.is_some();

    let mut constraints = vec![Constraint::Length(2)]; // header
    if has_result || has_loading {
        constraints.push(Constraint::Length(1)); // status bar
    }
    constraints.push(Constraint::Length(1));  // spacer
    constraints.push(Constraint::Length(5));  // row 1: Power | Screen | Connectivity
    constraints.push(Constraint::Length(1));  // spacer
    constraints.push(Constraint::Length(5));  // row 2: Audio & Display (full width)
    constraints.push(Constraint::Length(1));  // spacer
    constraints.push(Constraint::Length(8));  // row 3: Text Input | Hardware Keys
    constraints.push(Constraint::Min(0));    // fill
    constraints.push(Constraint::Length(1)); // footer

    let chunks = Layout::vertical(constraints).split(area);

    let mut idx = 0;

    // Header
    super::render_page_header(frame, chunks[idx], "CONTROLS", "REMOTE");
    idx += 1;

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[idx]);
        render_footer(frame, *chunks.last().unwrap_or(&chunks[idx]));
        return;
    }

    // Status bar
    if has_result || has_loading {
        render_status_bar(app, frame, chunks[idx]);
        idx += 1;
    }

    idx += 1; // spacer

    // Row 1: Power | Screen | Connectivity
    let row1 = chunks[idx];
    idx += 1;
    idx += 1; // spacer

    // Row 2: Audio & Display
    let row2 = chunks[idx];
    idx += 1;
    idx += 1; // spacer

    // Row 3: Text Input | Hardware Keys
    let row3 = chunks[idx];

    // Add padding on both sides
    let padded_row1 = pad_horizontal(row1, 1);
    let padded_row2 = pad_horizontal(row2, 1);
    let padded_row3 = pad_horizontal(row3, 1);

    // Render row 1
    let row1_cols = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .split(padded_row1);

    render_power_card(app, frame, row1_cols[0]);
    render_screen_card(app, frame, row1_cols[1]);
    render_connectivity_card(app, frame, row1_cols[2]);

    // Render row 2
    render_audio_display_panel(app, frame, padded_row2);

    // Render row 3
    let row3_cols = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(padded_row3);

    render_text_input_card(app, frame, row3_cols[0]);
    render_hardware_keys_card(app, frame, row3_cols[1]);

    // Footer
    render_footer(frame, *chunks.last().unwrap_or(&chunks[idx]));
}

/// Add horizontal padding to a rect.
fn pad_horizontal(area: Rect, pad: u16) -> Rect {
    Layout::horizontal([
        Constraint::Length(pad),
        Constraint::Min(0),
        Constraint::Length(pad),
    ])
    .split(area)[1]
}

/// Render the status/result bar at the top.
fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    if let Some(ref cmd) = app.controls.loading {
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled("\u{27f3} ", Theme::warning()),
            Span::styled(format!("Running: {cmd}"), Theme::dim()),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Theme::BG)),
            area,
        );
    } else if let Some((success, ref msg)) = app.controls.result {
        let (icon, style) = if success {
            ("\u{2713} ", Theme::success())
        } else {
            ("\u{2715} ", Theme::error())
        };
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(icon, style),
            Span::styled(msg.as_str(), style),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Theme::BG)),
            area,
        );
    }
}

/// Build a card block, with orange border if this section is focused.
fn section_block(title: &str, is_focused: bool) -> Block<'_> {
    let border_style = if is_focused {
        Theme::border_active()
    } else {
        Theme::border()
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(BorderType::Rounded)
        .title(Span::styled(format!(" {title} "), Theme::title()))
        .style(Style::default().bg(Theme::BG))
}

/// Render a single selectable item line within a card.
fn item_line(label: &str, is_selected: bool) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let style = if is_selected {
        Theme::accent_bold()
    } else {
        Theme::text()
    };
    Line::from(vec![
        Span::styled(prefix.to_string(), style),
        Span::styled(format!("[ {label} ]"), style),
    ])
}

/// Render a toggle item with ON/OFF state indicator.
fn toggle_line(label: &str, is_on: bool, is_selected: bool) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let label_style = if is_selected {
        Theme::accent_bold()
    } else {
        Theme::text()
    };
    let state_style = if is_on {
        Theme::success()
    } else {
        Theme::muted()
    };
    let state_text = if is_on { " ON" } else { " OFF" };
    Line::from(vec![
        Span::styled(prefix.to_string(), label_style),
        Span::styled(format!("{label:<12}"), label_style),
        Span::styled(state_text, state_style),
    ])
}

/// Section 0: Power card.
fn render_power_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 0;
    let block = section_block("Power", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = POWER_ITEMS
        .iter()
        .enumerate()
        .map(|(i, label)| item_line(label, focused && app.controls.focus_item == i))
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Section 1: Screen card.
fn render_screen_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 1;
    let block = section_block("Screen", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        item_line("Toggle", focused && app.controls.focus_item == 0),
        item_line("Unlock", focused && app.controls.focus_item == 1),
        toggle_line("Stay Awake", app.controls.stay_awake, focused && app.controls.focus_item == 2),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Section 2: Connectivity card.
fn render_connectivity_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 2;
    let block = section_block("Connectivity", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        toggle_line("WiFi", app.controls.wifi_enabled, focused && app.controls.focus_item == 0),
        toggle_line("Bluetooth", app.controls.bluetooth_enabled, focused && app.controls.focus_item == 1),
        toggle_line("Airplane", app.controls.airplane_mode, focused && app.controls.focus_item == 2),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Section 3: Audio & Display — focusable, left/right to adjust bars.
fn render_audio_display_panel(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 3;
    let block = section_block("Audio & Display", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // volume row
        Constraint::Length(1), // spacer
        Constraint::Length(1), // brightness row
    ])
    .split(inner);

    // Volume row
    render_audio_row(
        frame,
        rows[0],
        "VOLUME",
        app.controls.volume as f64 / 15.0,
        &format!("{}/15", app.controls.volume),
        focused && app.controls.focus_item == 0,
    );

    // Brightness row
    render_audio_row(
        frame,
        rows[2],
        "BRIGHT",
        app.controls.brightness as f64 / 255.0,
        &format!("{}/255", app.controls.brightness),
        focused && app.controls.focus_item == 1,
    );
}

/// Render a label + gauge bar in a single row area.
fn render_audio_row(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    ratio: f64,
    value: &str,
    is_selected: bool,
) {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let label_style = if is_selected { Theme::accent_bold() } else { Theme::muted() };

    let cols = Layout::horizontal([
        Constraint::Length(2),                  // prefix
        Constraint::Length(8),                  // label
        Constraint::Min(0),                     // gauge
        Constraint::Length(1),                  // pad
        Constraint::Length(value.len() as u16), // value
        Constraint::Length(1),                  // pad
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Span::styled(prefix, label_style)),
        cols[0],
    );

    frame.render_widget(
        Paragraph::new(Span::styled(label, label_style)),
        cols[1],
    );

    render_gauge(frame, cols[2], ratio, "", Theme::ORANGE);

    frame.render_widget(
        Paragraph::new(Span::styled(value, Theme::dim())),
        cols[4],
    );
}

/// Section 4: Text Input card.
fn render_text_input_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 4;
    let block = section_block("Text Input", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // hint
        Constraint::Length(1), // spacer
        Constraint::Length(1), // text input
        Constraint::Length(1), // spacer
        Constraint::Length(1), // send button
        Constraint::Min(0),   // fill
    ])
    .split(inner);

    // Hint
    let hint_style = if app.controls.text_input_active {
        Theme::accent()
    } else {
        Theme::muted()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            " Type text to send to device",
            hint_style,
        )),
        rows[0],
    );

    // Text input field
    let input_area = Layout::horizontal([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(rows[2]);
    render_text_input(
        frame,
        input_area[1],
        &app.controls.text_input,
        app.controls.text_cursor_pos,
        "> ",
        app.controls.text_input_active,
    );

    // Send button
    let send_style = if focused && !app.controls.text_input_active {
        Theme::accent_bold()
    } else {
        Theme::dim()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("  [ SEND ]", send_style))),
        rows[4],
    );
}

/// Section 5: Hardware Keys card.
fn render_hardware_keys_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 5;
    let block = section_block("Hardware Keys", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Lay out keys in 2 columns x 4 rows
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    for row_idx in 0..4 {
        let cols = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(rows[row_idx]);

        for col_idx in 0..2 {
            let item_idx = row_idx * 2 + col_idx;
            if item_idx < HARDWARE_KEYS.len() {
                let selected = focused && app.controls.focus_item == item_idx;
                let line = item_line(HARDWARE_KEYS[item_idx], selected);
                frame.render_widget(Paragraph::new(line), cols[col_idx]);
            }
        }
    }
}

/// Render the footer with keybinding hints.
fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(frame, area, &[
        ("Tab", "section"),
        ("j/k", "item"),
        ("h/l", "adjust"),
        ("Enter", "activate"),
        ("m", "mute"),
        ("i", "text"),
    ]);
}
