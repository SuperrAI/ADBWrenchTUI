use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::components::{card_block, kv_line, render_gauge, render_keybinding_footer, truncate_str};
use crate::theme::Theme;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    // Page layout: header | content | footer
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Min(0),   // content
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[1]);
        render_footer(frame, chunks[2]);
        return;
    }

    let content = chunks[1];

    // If no data yet, show loading
    if app.device_manager.full_info.is_none() {
        if app.dashboard.loading {
            render_loading(frame, content);
        } else {
            super::render_disconnected(frame, content);
        }
        render_footer(frame, chunks[2]);
        return;
    }

    // Top row: 3 cards (identity, battery, storage)
    // Bottom row: 2 cards (hardware, software)
    let rows = Layout::vertical([
        Constraint::Length(9),  // top row cards
        Constraint::Length(9),  // bottom row cards
        Constraint::Min(0),    // fill
    ])
    .split(content);

    let top_cols = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(rows[0]);

    render_identity_card(app, frame, top_cols[0]);
    render_battery_card(app, frame, top_cols[1]);
    render_storage_card(app, frame, top_cols[2]);

    let bottom_cols = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(rows[1]);

    render_hardware_card(app, frame, bottom_cols[0]);
    render_software_card(app, frame, bottom_cols[1]);

    render_footer(frame, chunks[2]);
}

// ── Header ────────────────────────────────────────────────────────

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled(" DASHBOARD", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("DEVICE", Theme::dim()),
    ];

    // Device model name
    if let Some(ref info) = app.device_manager.full_info {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(&info.identity.model, Theme::text()));
    }

    // Spacer
    spans.push(Span::styled("  ", Style::default()));

    // Loading indicator
    if app.dashboard.loading {
        spans.push(Span::styled("⟳ ", Theme::warning()));
    }

    // Auto-refresh label
    spans.push(Span::styled(
        format!("[{}]", app.dashboard.auto_refresh.label()),
        Theme::muted(),
    ));

    // Last refresh time
    if let Some(last) = app.dashboard.last_refresh {
        let secs = last.elapsed().as_secs();
        spans.push(Span::styled(
            format!("  {secs}s ago"),
            Theme::muted(),
        ));
    }

    let header = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Theme::BG));
    frame.render_widget(header, area);
}

// ── Footer ────────────────────────────────────────────────────────

fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(frame, area, &[
        ("r", "refresh"),
        ("a", "auto-refresh"),
        ("Tab", "focus"),
        ("Esc", "sidebar"),
    ]);
}

// ── Loading state ─────────────────────────────────────────────────

fn render_loading(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled("⟳ ", Theme::warning()),
        Span::styled("Loading device info...", Theme::dim()),
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

// ── Card helpers (see crate::components for shared versions) ─────

// ── Cards ─────────────────────────────────────────────────────────

fn render_identity_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Device");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref info) = app.device_manager.full_info {
        let id = &info.identity;
        let lines = vec![
            kv_line("Model", &id.model),
            kv_line("Make", &id.manufacturer),
            kv_line("Codename", &id.device),
            kv_line("Serial", &id.serial),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn render_battery_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Battery");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref info) = app.device_manager.full_info {
        let bat = &info.battery;
        let level_pct = bat.level as f64 / 100.0;
        let bar_color = if bat.level >= 50 {
            Theme::GREEN
        } else if bat.level >= 20 {
            Theme::YELLOW
        } else {
            Theme::RED
        };

        let rows = Layout::vertical([
            Constraint::Length(1), // gauge
            Constraint::Length(1), // spacer
            Constraint::Length(1), // status
            Constraint::Length(1), // health
            Constraint::Length(1), // temp
            Constraint::Min(0),   // fill
        ])
        .split(inner);

        render_gauge(
            frame,
            rows[0],
            level_pct,
            &format!(" {}%", bat.level),
            bar_color,
        );

        frame.render_widget(
            Paragraph::new(kv_line("Status", &bat.status)),
            rows[2],
        );
        frame.render_widget(
            Paragraph::new(kv_line("Health", &bat.health)),
            rows[3],
        );
        frame.render_widget(
            Paragraph::new(kv_line("Temp", &bat.temperature)),
            rows[4],
        );
    }
}

fn render_storage_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Storage");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref info) = app.device_manager.full_info {
        let st = &info.storage;
        let ratio = st.usage_percent / 100.0;
        let bar_color = if st.usage_percent < 70.0 {
            Theme::ORANGE
        } else if st.usage_percent < 90.0 {
            Theme::YELLOW
        } else {
            Theme::RED
        };

        let rows = Layout::vertical([
            Constraint::Length(1), // gauge
            Constraint::Length(1), // spacer
            Constraint::Length(1), // total
            Constraint::Length(1), // used
            Constraint::Length(1), // free
            Constraint::Min(0),   // fill
        ])
        .split(inner);

        render_gauge(
            frame,
            rows[0],
            ratio,
            &format!(" {:.0}%", st.usage_percent),
            bar_color,
        );

        frame.render_widget(
            Paragraph::new(kv_line("Total", &st.total)),
            rows[2],
        );
        frame.render_widget(
            Paragraph::new(kv_line("Used", &st.used)),
            rows[3],
        );
        frame.render_widget(
            Paragraph::new(kv_line("Free", &st.available)),
            rows[4],
        );
    }
}

fn render_hardware_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Hardware");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref info) = app.device_manager.full_info {
        let hw = &info.hardware;
        let lines = vec![
            kv_line("Platform", &hw.hardware_platform),
            kv_line("CPU", &hw.cpu_architecture),
            kv_line("RAM", &hw.total_ram),
            kv_line("Display", &hw.display_resolution),
            kv_line("Density", &hw.display_density),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn render_software_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = card_block("Software");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref info) = app.device_manager.full_info {
        let b = &info.build;
        let lines = vec![
            kv_line("Android", &b.android_version),
            kv_line("SDK", &b.sdk_level),
            kv_line("Patch", &b.security_patch),
            kv_line("Build", &b.build_date),
            Line::from(vec![
                Span::styled(" FP: ", Theme::muted()),
                Span::styled(
                    truncate_str(&b.build_fingerprint, 40),
                    Theme::dim(),
                ),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

