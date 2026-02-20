use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    // Page layout: header | content
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Min(0),   // content
    ])
    .split(area);

    // Header
    super::render_page_header(frame, chunks[0], "DASHBOARD", "DEVICE");

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[1]);
        return;
    }

    let content = chunks[1];

    // Top row: 3 cards (identity, battery, storage)
    // Bottom row: 2 cards (hardware, software)
    let rows = Layout::vertical([
        Constraint::Length(8),  // top row cards
        Constraint::Length(1),  // spacer
        Constraint::Length(8),  // bottom row cards
        Constraint::Min(0),    // fill
    ])
    .split(content);

    // Top row: 3 columns
    let top_cols = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(rows[0]);

    render_identity_card(app, frame, top_cols[0]);
    render_battery_card(app, frame, top_cols[1]);
    render_storage_card(app, frame, top_cols[2]);

    // Bottom row: 2 columns
    let bottom_cols = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(rows[2]);

    render_hardware_card(app, frame, bottom_cols[0]);
    render_software_card(app, frame, bottom_cols[1]);
}

fn card_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(
            format!(" {title} "),
            Theme::title(),
        ))
        .style(Style::default().bg(Theme::BG))
}

fn kv_line<'a>(key: &'a str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!(" {key}: "), Theme::muted()),
        Span::styled(value, Theme::text()),
    ])
}

fn render_identity_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Device");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref dev) = app.device_manager.current_device {
        let lines = vec![
            kv_line("Model", &dev.model),
            kv_line("Manufacturer", &dev.manufacturer),
            kv_line("Codename", &dev.device),
            kv_line("Serial", &dev.serial),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn render_battery_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Battery");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref _dev) = app.device_manager.current_device {
        // Placeholder — will be filled with real data
        let lines = vec![
            Line::from(vec![
                Span::styled(" Level: ", Theme::muted()),
                Span::styled("---%", Theme::text()),
            ]),
            Line::from(vec![
                Span::styled(" Status: ", Theme::muted()),
                Span::styled("---", Theme::text()),
            ]),
            Line::from(vec![
                Span::styled(" Health: ", Theme::muted()),
                Span::styled("---", Theme::text()),
            ]),
            Line::from(vec![
                Span::styled(" Temp: ", Theme::muted()),
                Span::styled("---", Theme::text()),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn render_storage_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Storage");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref _dev) = app.device_manager.current_device {
        let lines = vec![
            Line::from(vec![
                Span::styled(" Usage: ", Theme::muted()),
                Span::styled("---", Theme::text()),
            ]),
            Line::from(vec![
                Span::styled(" Total: ", Theme::muted()),
                Span::styled("---", Theme::text()),
            ]),
            Line::from(vec![
                Span::styled(" Free: ", Theme::muted()),
                Span::styled("---", Theme::text()),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn render_hardware_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Hardware");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref _dev) = app.device_manager.current_device {
        let lines = vec![
            kv_line("CPU", "---"),
            kv_line("RAM", "---"),
            kv_line("Display", "---"),
            kv_line("Density", "---"),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn render_software_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Software");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref dev) = app.device_manager.current_device {
        let lines = vec![
            kv_line("Android", &dev.android_version),
            kv_line("SDK", &dev.sdk_level),
            kv_line("Build", "---"),
            kv_line("Patch", "---"),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
