use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::components::{render_gauge, render_keybinding_footer, render_text_input};
use crate::theme::Theme;

/// Section titles for the 3x2 grid of control cards.
const SECTION_TITLES: [&str; 6] = [
    "Power",
    "Screen",
    "Connectivity",
    "Audio & Display",
    "Text Input",
    "Hardware Keys",
];

/// Items within each section.
const POWER_ITEMS: [&str; 3] = ["Reboot", "Recovery", "Bootloader"];
const SCREEN_ITEMS: [&str; 4] = ["Toggle", "Unlock", "Stay Awake ON", "Stay Awake OFF"];
const CONNECTIVITY_ITEMS: [&str; 4] = ["WiFi ON", "WiFi OFF", "Airplane ON", "Airplane OFF"];
const HARDWARE_KEYS: [&str; 8] = ["HOME", "BACK", "MENU", "RECENT", "PLAY", "PREV", "NEXT", "CAM"];

/// Render the Controls page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    // Determine if we need a result bar
    let has_result = app.controls.result.is_some();
    let has_loading = app.controls.loading.is_some();

    let mut constraints = vec![Constraint::Length(2)]; // header
    if has_result || has_loading {
        constraints.push(Constraint::Length(1)); // result/loading bar
    }
    constraints.push(Constraint::Min(0)); // grid
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

    // Result / loading bar
    if has_result || has_loading {
        render_status_bar(app, frame, chunks[idx]);
        idx += 1;
    }

    // Grid area
    let grid_area = chunks[idx];
    render_grid(app, frame, grid_area);

    // Footer
    render_footer(frame, *chunks.last().unwrap_or(&chunks[idx]));
}

/// Render the status/result bar at the top.
fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    if let Some(ref cmd) = app.controls.loading {
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled("⟳ ", Theme::warning()),
            Span::styled(format!("Running: {cmd}"), Theme::dim()),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Theme::BG)),
            area,
        );
    } else if let Some((success, ref msg)) = app.controls.result {
        let (icon, style) = if success {
            ("✓ ", Theme::success())
        } else {
            ("✕ ", Theme::error())
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

/// Render the 3x2 grid of control cards.
fn render_grid(app: &App, frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(area);

    let top_cols = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(rows[0]);

    let bottom_cols = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(rows[1]);

    // Row 1
    render_power_card(app, frame, top_cols[0]);
    render_screen_card(app, frame, top_cols[1]);
    render_connectivity_card(app, frame, top_cols[2]);

    // Row 2
    render_audio_display_card(app, frame, bottom_cols[0]);
    render_text_input_card(app, frame, bottom_cols[1]);
    render_hardware_keys_card(app, frame, bottom_cols[2]);
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
    let prefix = if is_selected { "▸ " } else { "  " };
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

/// Section 0: Power card.
fn render_power_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 0;
    let block = section_block(SECTION_TITLES[0], focused);
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
    let block = section_block(SECTION_TITLES[1], focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = SCREEN_ITEMS
        .iter()
        .enumerate()
        .map(|(i, label)| item_line(label, focused && app.controls.focus_item == i))
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Section 2: Connectivity card.
fn render_connectivity_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 2;
    let block = section_block(SECTION_TITLES[2], focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = CONNECTIVITY_ITEMS
        .iter()
        .enumerate()
        .map(|(i, label)| item_line(label, focused && app.controls.focus_item == i))
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Section 3: Audio & Display card (volume, brightness).
fn render_audio_display_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 3;
    let block = section_block(SECTION_TITLES[3], focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Items: Volume +, Volume -, Mute  (focus_item 0,1,2)
    // Plus a brightness gauge
    let audio_items = ["Volume +", "Volume -", "Mute"];

    let rows = Layout::vertical([
        Constraint::Length(1), // volume label
        Constraint::Length(3), // volume items
        Constraint::Length(1), // spacer
        Constraint::Length(1), // brightness label
        Constraint::Length(1), // brightness bar
        Constraint::Min(0),   // fill
    ])
    .split(inner);

    // Volume header with current level
    let vol_line = Line::from(vec![
        Span::styled(" Volume: ", Theme::muted()),
        Span::styled(format!("{}/15", app.controls.volume), Theme::text()),
    ]);
    frame.render_widget(Paragraph::new(vol_line), rows[0]);

    // Volume action items
    let lines: Vec<Line> = audio_items
        .iter()
        .enumerate()
        .map(|(i, label)| item_line(label, focused && app.controls.focus_item == i))
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[1]);

    // Brightness label
    let bright_line = Line::from(vec![
        Span::styled(" Brightness: ", Theme::muted()),
        Span::styled(format!("{}/255", app.controls.brightness), Theme::text()),
    ]);
    frame.render_widget(Paragraph::new(bright_line), rows[3]);

    // Brightness gauge
    let bright_ratio = app.controls.brightness as f64 / 255.0;
    let padded = Layout::horizontal([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(rows[4]);
    render_gauge(
        frame,
        padded[1],
        bright_ratio,
        &format!(" {}", app.controls.brightness),
        Theme::ORANGE,
    );
}

/// Section 4: Text Input card.
fn render_text_input_card(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.controls.focus_section == 4;
    let block = section_block(SECTION_TITLES[4], focused);
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
    let block = section_block(SECTION_TITLES[5], focused);
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
        ("Enter", "activate"),
        ("+/-", "vol"),
        ("[/]", "bright"),
        ("i", "text"),
    ]);
}
