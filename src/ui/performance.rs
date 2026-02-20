use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::components::{render_gauge, render_keybinding_footer, render_sparkline, truncate_str};
use crate::theme::Theme;

/// Render the Performance page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2),  // header
        Constraint::Length(3),  // KPI gauges
        Constraint::Length(4),  // CPU sparkline
        Constraint::Length(4),  // Memory sparkline
        Constraint::Min(0),    // process table
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(app, frame, chunks[0]);

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[4]);
        render_footer(frame, chunks[5]);
        return;
    }

    render_kpi_gauges(app, frame, chunks[1]);
    render_cpu_sparkline(app, frame, chunks[2]);
    render_mem_sparkline(app, frame, chunks[3]);
    render_process_table(app, frame, chunks[4]);
    render_footer(frame, chunks[5]);
}

/// Header with monitoring status and refresh rate.
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled(" PERFORMANCE", Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled("MONITOR", Theme::dim()),
    ];

    if app.device_manager.is_connected() {
        spans.push(Span::styled(
            format!("  RATE:{}", app.performance.refresh_rate.label()),
            Theme::muted(),
        ));

        if app.performance.is_monitoring {
            spans.push(Span::styled("  ● ACTIVE", Theme::success()));
        } else {
            spans.push(Span::styled("  ○ PAUSED", Theme::muted()));
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

/// Three KPI gauge cards: CPU, Memory, Battery.
fn render_kpi_gauges(app: &App, frame: &mut Frame, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(area);

    // CPU gauge
    let cpu_pct = app.performance.cpu_history.last().copied().unwrap_or(0.0);
    let cpu_color = if cpu_pct > 80.0 {
        Theme::RED
    } else if cpu_pct > 50.0 {
        Theme::YELLOW
    } else {
        Theme::GREEN
    };
    render_kpi_card(frame, cols[0], "CPU", cpu_pct, cpu_color);

    // Memory gauge
    let mem_pct = if app.performance.mem_total_kb > 0 {
        (app.performance.mem_used_kb as f64 / app.performance.mem_total_kb as f64) * 100.0
    } else {
        0.0
    };
    let mem_color = if mem_pct > 80.0 {
        Theme::RED
    } else if mem_pct > 60.0 {
        Theme::YELLOW
    } else {
        Theme::GREEN
    };
    render_kpi_card(frame, cols[1], "MEMORY", mem_pct, mem_color);

    // Battery gauge
    let batt_level = app
        .performance
        .battery
        .as_ref()
        .map(|b| b.level as f64)
        .unwrap_or(0.0);
    let batt_color = if batt_level < 20.0 {
        Theme::RED
    } else if batt_level < 50.0 {
        Theme::YELLOW
    } else {
        Theme::GREEN
    };
    render_kpi_card(frame, cols[2], "BATTERY", batt_level, batt_color);
}

/// Single KPI card with title, percentage, and gauge.
fn render_kpi_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    percent: f64,
    color: ratatui::style::Color,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(format!(" {title} "), Theme::title()))
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
    let rows = Layout::vertical([
        Constraint::Length(1), // label + stats
        Constraint::Length(1), // spacer
        Constraint::Min(0),   // chart
    ])
    .split(area);

    // Stats
    let data = &app.performance.cpu_history;
    let (min, avg, max) = compute_stats(data);
    let stats_line = Line::from(vec![
        Span::styled(" CPU HISTORY", Theme::dim()),
        Span::styled(
            format!("  MIN:{min:.0}%  AVG:{avg:.0}%  MAX:{max:.0}%"),
            Theme::muted(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(stats_line).style(Style::default().bg(Theme::BG)),
        rows[0],
    );

    // Sparkline
    let padded = Layout::horizontal([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(rows[2]);
    render_sparkline(frame, padded[1], data, 100.0, Theme::ORANGE);
}

/// Memory history sparkline with stats.
fn render_mem_sparkline(app: &App, frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // label + stats
        Constraint::Length(1), // spacer
        Constraint::Min(0),   // chart
    ])
    .split(area);

    let data = &app.performance.mem_history;
    let (min, avg, max) = compute_stats(data);

    let total_mb = app.performance.mem_total_kb as f64 / 1024.0;
    let used_mb = app.performance.mem_used_kb as f64 / 1024.0;

    let stats_line = Line::from(vec![
        Span::styled(" MEM HISTORY", Theme::dim()),
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
    .split(rows[2]);
    render_sparkline(frame, padded[1], data, 100.0, Theme::BLUE);
}

/// Process table showing top processes.
fn render_process_table(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" TOP PROCESSES ({}) ", app.performance.processes.len()),
            Theme::title(),
        ))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.performance.processes.is_empty() {
        let hint = Paragraph::new(Span::styled(
            "Press s to start monitoring",
            Theme::muted(),
        ))
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
    let available_width = inner.width as usize;
    let scroll = app.performance.scroll_offset;

    // Header row
    let header = Line::from(vec![
        Span::styled(format!(" {:>6}", "PID"), Theme::dim()),
        Span::styled("  ", Style::default()),
        Span::styled(format!("{:<10}", "USER"), Theme::dim()),
        Span::styled("  ", Style::default()),
        Span::styled(format!("{:>6}", "CPU%"), Theme::dim()),
        Span::styled("  ", Style::default()),
        Span::styled("NAME", Theme::dim()),
    ]);

    let mut lines: Vec<Line> = vec![header];

    let procs = &app.performance.processes;
    for i in scroll..(scroll + visible_height.saturating_sub(1)).min(procs.len()) {
        let p = &procs[i];

        let name_max = available_width.saturating_sub(28);
        let name_display = truncate_str(&p.name, name_max);

        let cpu_style = if p.cpu_percent > 50.0 {
            Style::default().fg(Theme::RED)
        } else if p.cpu_percent > 20.0 {
            Style::default().fg(Theme::YELLOW)
        } else {
            Theme::text()
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" {:>6}", p.pid), Theme::muted()),
            Span::styled("  ", Style::default()),
            Span::styled(format!("{:<10}", truncate_str(&p.user, 10)), Theme::dim()),
            Span::styled("  ", Style::default()),
            Span::styled(format!("{:>5.1}%", p.cpu_percent), cpu_style),
            Span::styled("  ", Style::default()),
            Span::styled(name_display, Theme::text()),
        ]));
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

/// Footer with keybinding hints.
fn render_footer(frame: &mut Frame, area: Rect) {
    render_keybinding_footer(frame, area, &[
        ("s", "start/stop"),
        ("r", "rate"),
        ("j/k", "scroll"),
    ]);
}
