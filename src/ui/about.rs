use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

/// Render the About page.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Min(0),   // content
    ])
    .split(area);

    super::render_page_header(frame, chunks[0], " ABOUT", "INFO");

    let content_area = chunks[1];
    let cols = Layout::horizontal([
        Constraint::Length(2),  // left pad
        Constraint::Min(0),    // content
        Constraint::Length(2),  // right pad
    ])
    .split(content_area);

    let rows = Layout::vertical([
        Constraint::Length(1),  // spacer
        Constraint::Length(9),  // app info card
        Constraint::Length(1),  // spacer
        Constraint::Length(7),  // build info card
        Constraint::Length(1),  // spacer
        Constraint::Length(5),  // config card
        Constraint::Min(0),    // fill
    ])
    .split(cols[1]);

    render_app_card(frame, rows[1]);
    render_build_card(frame, rows[3]);
    render_config_card(app, frame, rows[5]);
}

/// Application info card.
fn render_app_card(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(" APPLICATION ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("  >_", Theme::accent_bold()),
            Span::raw(" "),
            Span::styled("ADB Wrench TUI", Theme::bold()),
        ]),
        Line::from(""),
        info_line("NAME", env!("CARGO_PKG_NAME")),
        info_line("VERSION", env!("CARGO_PKG_VERSION")),
        info_line("LICENSE", env!("CARGO_PKG_LICENSE")),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("superr.ai", Theme::accent()),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        inner,
    );
}

/// Build info card.
fn render_build_card(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(" BUILD ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let git_hash = env!("BUILD_GIT_HASH");
    let build_time = env!("BUILD_TIMESTAMP");

    let lines = vec![
        info_line("RUST", env!("CARGO_PKG_RUST_VERSION", "stable")),
        info_line("TARGET", std::env::consts::ARCH),
        info_line("OS", std::env::consts::OS),
        info_line("COMMIT", if git_hash.is_empty() { "unknown" } else { git_hash }),
        info_line("BUILT", build_time),
    ];

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        inner,
    );
}

/// Config info card.
fn render_config_card(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .border_type(BorderType::Rounded)
        .title(Span::styled(" CONFIGURATION ", Theme::title()))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        info_line("OUTPUT DIR", &app.config.output_dir),
        info_line("CONFIG", "~/.config/adbwrenchtui/config.json"),
        info_line("LOG FILE", "adbwrenchtui.log"),
    ];

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Theme::BG)),
        inner,
    );
}

/// Helper: renders a "  KEY  value" info line.
fn info_line<'a>(key: &'a str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {key:<12}"), Theme::muted()),
        Span::styled(value, Theme::text()),
    ])
}
