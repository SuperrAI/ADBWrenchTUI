use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::{App, DashboardSection};
use crate::components::{render_gauge, render_keybinding_footer, render_sparkline, truncate_str};
use crate::theme::Theme;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Min(0),    // content
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[1]);
        render_footer(app, frame, chunks[2]);
        return;
    }

    let content = chunks[1];

    if app.device_manager.full_info.is_none() {
        if app.dashboard.loading {
            render_loading(frame, content);
        } else {
            super::render_disconnected(frame, content);
        }
        render_footer(app, frame, chunks[2]);
        return;
    }

    // Row 1: Device, Hardware, Software (3 columns)
    // Row 2: Battery, Storage (2 columns)
    // Row 3: CPU gauge, Memory gauge (2 columns)
    // Row 4: CPU sparkline
    // Row 5: Memory sparkline
    // Row 6: Process table (fills remaining)
    let rows = Layout::vertical([
        Constraint::Length(8), // row 1: info cards
        Constraint::Length(8), // row 2: battery + storage
        Constraint::Length(3), // row 3: CPU + Memory gauges
        Constraint::Length(3), // row 4: CPU sparkline
        Constraint::Length(3), // row 5: Memory sparkline
        Constraint::Min(0),    // row 6: process table
    ])
    .split(content);

    // Row 1: Device, Hardware, Software
    let info_cols = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(rows[0]);

    render_info_card(app, frame, info_cols[0], DashboardSection::Device);
    render_info_card(app, frame, info_cols[1], DashboardSection::Hardware);
    render_info_card(app, frame, info_cols[2], DashboardSection::Software);

    // Row 2: Battery, Storage
    let gauge_cols =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);

    render_battery_card(app, frame, gauge_cols[0]);
    render_storage_card(app, frame, gauge_cols[1]);

    // Row 3: CPU + Memory KPI gauges (no battery — already shown above)
    let kpi_cols =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[2]);

    render_kpi_card(app, frame, kpi_cols[0], "CPU");
    render_kpi_card(app, frame, kpi_cols[1], "MEMORY");

    // Row 4-5: Sparklines
    render_cpu_sparkline(app, frame, rows[3]);
    render_mem_sparkline(app, frame, rows[4]);

    // Row 6: Process table
    render_process_table(app, frame, rows[5]);

    render_footer(app, frame, chunks[2]);
}

// ── Header ────────────────────────────────────────────────────────

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled(" DASHBOARD", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("DEVICE", Theme::dim()),
    ];

    if let Some(ref info) = app.device_manager.full_info {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(&info.identity.model, Theme::text()));
    }

    spans.push(Span::styled("  ", Style::default()));

    if app.dashboard.loading {
        spans.push(Span::styled("⟳ ", Theme::warning()));
    }

    spans.push(Span::styled(
        format!("[{}]", app.dashboard.auto_refresh.label()),
        Theme::muted(),
    ));

    if let Some(last) = app.dashboard.last_refresh {
        let secs = last.elapsed().as_secs();
        spans.push(Span::styled(format!("  {secs}s ago"), Theme::muted()));
    }

    // Copied feedback
    if let Some((ref value, _)) = app.dashboard.copied_feedback {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            format!("✓ Copied: {}", truncate_str(value, 20)),
            Theme::success(),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Footer ────────────────────────────────────────────────────────

fn render_footer(_app: &App, frame: &mut Frame, area: Rect) {
    render_keybinding_footer(
        frame,
        area,
        &[
            ("r", "refresh"),
            ("a", "auto"),
            ("Tab", "section"),
            ("j/k", "item"),
            ("c", "copy"),
        ],
    );
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
        Paragraph::new(text).alignment(Alignment::Center),
        centered[1],
    );
}

// ── Info Cards with Focus ─────────────────────────────────────────

/// Items for each section: (label, getter from FullDeviceInfo)
fn section_items(section: DashboardSection) -> &'static [&'static str] {
    match section {
        DashboardSection::Device => &["Model", "Make", "Codename", "Serial"],
        DashboardSection::Hardware => &["Platform", "CPU", "RAM", "Display", "Density"],
        DashboardSection::Software => &["Android", "SDK", "Patch", "Build", "Fingerprint"],
        DashboardSection::Processes => &[],
    }
}

fn section_title(section: DashboardSection) -> &'static str {
    match section {
        DashboardSection::Device => "Device",
        DashboardSection::Hardware => "Hardware",
        DashboardSection::Software => "Software",
        DashboardSection::Processes => "Processes",
    }
}

/// Get the value string for a section+item from FullDeviceInfo.
fn get_info_value(app: &App, section: DashboardSection, item: usize) -> String {
    if section == DashboardSection::Processes {
        return String::new();
    }
    let Some(ref info) = app.device_manager.full_info else {
        return String::new();
    };
    match section {
        DashboardSection::Device => {
            let id = &info.identity;
            match item {
                0 => id.model.clone(),
                1 => id.manufacturer.clone(),
                2 => id.device.clone(),
                3 => id.serial.clone(),
                _ => String::new(),
            }
        }
        DashboardSection::Hardware => {
            let hw = &info.hardware;
            match item {
                0 => hw.hardware_platform.clone(),
                1 => hw.cpu_architecture.clone(),
                2 => hw.total_ram.clone(),
                3 => hw.display_resolution.clone(),
                4 => hw.display_density.clone(),
                _ => String::new(),
            }
        }
        DashboardSection::Software => {
            let b = &info.build;
            match item {
                0 => b.android_version.clone(),
                1 => b.sdk_level.clone(),
                2 => b.security_patch.clone(),
                3 => b.build_date.clone(),
                4 => b.build_fingerprint.clone(),
                _ => String::new(),
            }
        }
        DashboardSection::Processes => String::new(), // handled separately
    }
}

/// Render a focusable info card (Device, Hardware, or Software).
fn render_info_card(app: &App, frame: &mut Frame, area: Rect, section: DashboardSection) {
    let is_focused = app.dashboard.focus_section == section;
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
            format!(" {} ", section_title(section)),
            if is_focused {
                Theme::accent_bold()
            } else {
                Theme::title()
            },
        ))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let labels = section_items(section);
    let mut lines: Vec<Line> = Vec::with_capacity(labels.len());

    for (i, label) in labels.iter().enumerate() {
        let value = get_info_value(app, section, i);
        let is_selected = is_focused && app.dashboard.focus_item == i;

        if is_selected {
            lines.push(Line::from(vec![
                Span::styled(" ▸ ", Theme::accent()),
                Span::styled(format!("{label}: "), Theme::accent()),
                Span::styled(value, Theme::accent_bold()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(format!("{label}: "), Theme::muted()),
                Span::styled(value, Theme::dim()),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

// ── Battery & Storage Cards ───────────────────────────────────────

fn render_battery_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(" Battery ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

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
            Constraint::Min(0),
        ])
        .split(inner);

        render_gauge(
            frame,
            rows[0],
            level_pct,
            &format!(" {}%", bat.level),
            bar_color,
        );

        frame.render_widget(Paragraph::new(kv_line("Status", &bat.status)), rows[2]);
        frame.render_widget(Paragraph::new(kv_line("Health", &bat.health)), rows[3]);
        frame.render_widget(Paragraph::new(kv_line("Temp", &bat.temperature)), rows[4]);
    }
}

fn render_storage_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(" Storage ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

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
            Constraint::Min(0),
        ])
        .split(inner);

        render_gauge(
            frame,
            rows[0],
            ratio,
            &format!(" {:.0}%", st.usage_percent),
            bar_color,
        );

        frame.render_widget(Paragraph::new(kv_line("Total", &st.total)), rows[2]);
        frame.render_widget(Paragraph::new(kv_line("Used", &st.used)), rows[3]);
        frame.render_widget(Paragraph::new(kv_line("Free", &st.available)), rows[4]);
    }
}

/// Key-value line helper.
fn kv_line<'a>(key: &'a str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("   {key}: "), Theme::muted()),
        Span::styled(value, Theme::dim()),
    ])
}

// ── Performance Section ───────────────────────────────────────────

/// Single KPI gauge card.
fn render_kpi_card(app: &App, frame: &mut Frame, area: Rect, kind: &str) {
    let (percent, color) = match kind {
        "CPU" => {
            let pct = app.performance.cpu_history.last().copied().unwrap_or(0.0);
            let c = if pct > 80.0 {
                Theme::RED
            } else if pct > 50.0 {
                Theme::YELLOW
            } else {
                Theme::GREEN
            };
            (pct, c)
        }
        "MEMORY" => {
            let pct = if app.performance.mem_total_kb > 0 {
                (app.performance.mem_used_kb as f64 / app.performance.mem_total_kb as f64) * 100.0
            } else {
                0.0
            };
            let c = if pct > 80.0 {
                Theme::RED
            } else if pct > 60.0 {
                Theme::YELLOW
            } else {
                Theme::GREEN
            };
            (pct, c)
        }
        _ => (0.0, Theme::GREEN),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(format!(" {kind} "), Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let label = format!(" {:.1}%", percent);
    render_gauge(frame, inner, percent / 100.0, &label, color);
}

/// CPU history sparkline with stats.
fn render_cpu_sparkline(app: &App, frame: &mut Frame, area: Rect) {
    let data = &app.performance.cpu_history;
    let (min, avg, max) = compute_stats(data);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);

    let stats_line = Line::from(vec![
        Span::styled(" CPU", Theme::dim()),
        Span::styled(
            format!("  MIN:{min:.0}%  AVG:{avg:.0}%  MAX:{max:.0}%"),
            Theme::muted(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(stats_line).style(Style::default().bg(Theme::BG)),
        rows[0],
    );

    let padded = Layout::horizontal([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(rows[1]);
    render_sparkline(frame, padded[1], data, 100.0, Theme::ORANGE);
}

/// Memory history sparkline with stats.
fn render_mem_sparkline(app: &App, frame: &mut Frame, area: Rect) {
    let data = &app.performance.mem_history;
    let (min, avg, max) = compute_stats(data);
    let total_mb = app.performance.mem_total_kb as f64 / 1024.0;
    let used_mb = app.performance.mem_used_kb as f64 / 1024.0;

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);

    let stats_line = Line::from(vec![
        Span::styled(" MEM", Theme::dim()),
        Span::styled(
            format!("  {used_mb:.0}/{total_mb:.0}MB  MIN:{min:.0}%  AVG:{avg:.0}%  MAX:{max:.0}%"),
            Theme::muted(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(stats_line).style(Style::default().bg(Theme::BG)),
        rows[0],
    );

    let padded = Layout::horizontal([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(rows[1]);
    render_sparkline(frame, padded[1], data, 100.0, Theme::BLUE);
}

/// Process table showing all processes (scrollable, focusable).
fn render_process_table(app: &App, frame: &mut Frame, area: Rect) {
    let is_focused = app.dashboard.focus_section == DashboardSection::Processes;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_focused {
            Theme::border_active()
        } else {
            Theme::border()
        })
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" PROCESSES ({}) ", app.performance.processes.len()),
            if is_focused {
                Theme::accent_bold()
            } else {
                Theme::title()
            },
        ))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.performance.processes.is_empty() {
        return;
    }

    let visible_height = inner.height as usize;
    let available_width = inner.width as usize;
    let procs = &app.performance.processes;
    let data_rows = visible_height.saturating_sub(1); // minus header

    // Scroll offset is driven by focus_item when focused
    let scroll = if is_focused {
        let selected = app.dashboard.focus_item;
        // Keep selected row visible
        let current_scroll = app.performance.scroll_offset;
        if selected < current_scroll {
            selected
        } else if selected >= current_scroll + data_rows {
            selected.saturating_sub(data_rows.saturating_sub(1))
        } else {
            current_scroll
        }
    } else {
        app.performance
            .scroll_offset
            .min(procs.len().saturating_sub(data_rows))
    };

    // Header row: PID  USER  S  CPU%  MEM%  RES  TIME+  NAME
    // Fixed columns take ~62 chars, NAME fills the rest
    let header = Line::from(vec![
        Span::styled(format!("  {:>6}", "PID"), Theme::dim()),
        Span::styled(" ", Style::default()),
        Span::styled(format!("{:<8}", "USER"), Theme::dim()),
        Span::styled(" ", Style::default()),
        Span::styled("S", Theme::dim()),
        Span::styled(" ", Style::default()),
        Span::styled(format!("{:>5}", "CPU%"), Theme::dim()),
        Span::styled(" ", Style::default()),
        Span::styled(format!("{:>5}", "MEM%"), Theme::dim()),
        Span::styled(" ", Style::default()),
        Span::styled(format!("{:>6}", "RES"), Theme::dim()),
        Span::styled(" ", Style::default()),
        Span::styled(format!("{:>9}", "TIME+"), Theme::dim()),
        Span::styled(" ", Style::default()),
        Span::styled("NAME", Theme::dim()),
    ]);

    let mut lines: Vec<Line> = vec![header];
    let fixed_cols = 58; // approximate width of fixed columns

    for i in scroll..(scroll + data_rows).min(procs.len()) {
        let p = &procs[i];
        let is_selected = is_focused && i == app.dashboard.focus_item;
        let name_max = available_width.saturating_sub(fixed_cols);
        let name_display = truncate_str(&p.name, name_max);

        let row_style = if is_selected {
            Theme::highlight()
        } else {
            Style::default().bg(Theme::BG)
        };

        let cpu_style = if is_selected {
            Theme::accent_bold()
        } else if p.cpu_percent > 50.0 {
            Style::default().fg(Theme::RED)
        } else if p.cpu_percent > 20.0 {
            Style::default().fg(Theme::YELLOW)
        } else {
            Theme::text()
        };

        let mem_style = if is_selected {
            Theme::accent_bold()
        } else if p.mem_percent > 10.0 {
            Style::default().fg(Theme::YELLOW)
        } else {
            Theme::dim()
        };

        let state_style = if is_selected {
            Theme::accent()
        } else {
            match p.state.as_str() {
                "R" => Style::default().fg(Theme::GREEN),
                "D" => Style::default().fg(Theme::RED),
                "Z" => Style::default().fg(Theme::RED),
                _ => Theme::muted(),
            }
        };

        let name_style = if is_selected {
            Theme::accent_bold()
        } else {
            Theme::text()
        };
        let pid_style = if is_selected {
            Theme::accent()
        } else {
            Theme::muted()
        };
        let dim_style = if is_selected {
            Theme::accent()
        } else {
            Theme::dim()
        };

        lines.push(
            Line::from(vec![
                Span::styled(if is_selected { "▸ " } else { "  " }, pid_style),
                Span::styled(format!("{:>6}", p.pid), pid_style),
                Span::styled(" ", Style::default()),
                Span::styled(format!("{:<8}", truncate_str(&p.user, 8)), dim_style),
                Span::styled(" ", Style::default()),
                Span::styled(&p.state, state_style),
                Span::styled(" ", Style::default()),
                Span::styled(format!("{:>5.1}", p.cpu_percent), cpu_style),
                Span::styled(" ", Style::default()),
                Span::styled(format!("{:>5.1}", p.mem_percent), mem_style),
                Span::styled(" ", Style::default()),
                Span::styled(format!("{:>6}", p.res), dim_style),
                Span::styled(" ", Style::default()),
                Span::styled(format!("{:>9}", p.time), dim_style),
                Span::styled(" ", Style::default()),
                Span::styled(name_display, name_style),
            ])
            .style(row_style),
        );
    }

    while lines.len() < visible_height {
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        inner,
    );
}

/// Compute min, avg, max from data.
fn compute_stats(data: &[f64]) -> (f64, f64, f64) {
    if data.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let avg = data.iter().sum::<f64>() / data.len() as f64;
    (min, avg, max)
}
