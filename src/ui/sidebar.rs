use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::adb::ConnectionState;
use crate::app::{App, Focus, Page};
use crate::theme::Theme;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    // Sidebar border — highlighted when focused
    let border_style = if app.focus == Focus::Sidebar {
        Theme::border_active()
    } else {
        Theme::border()
    };

    let sidebar_block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(border_style)
        .style(Style::default().bg(Theme::BG));

    let inner = sidebar_block.inner(area);
    frame.render_widget(sidebar_block, area);

    // Split sidebar: header | nav | device status | footer
    let chunks = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Min(0),    // nav items
        Constraint::Length(4), // device status
        Constraint::Length(1), // footer
    ])
    .split(inner);

    render_header(frame, chunks[0]);
    render_nav(app, frame, chunks[1]);
    render_device_status(app, frame, chunks[2]);
    render_footer(frame, chunks[3]);
}

fn render_header(frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(">_", Theme::accent_bold()),
            Span::raw(" "),
            Span::styled("ADB Wrench", Theme::bold()),
        ]),
        Line::from(vec![Span::styled("   TUI Edition", Theme::muted())]),
    ])
    .style(Style::default().bg(Theme::BG));

    frame.render_widget(header, area);
}

fn render_nav(app: &App, frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    let mut current_section = "";

    for (i, page) in Page::ALL.iter().enumerate() {
        let section = page.section();
        if section != current_section {
            if !current_section.is_empty() {
                lines.push(Line::raw("")); // spacer
            }
            lines.push(Line::from(Span::styled(
                format!(" {section}"),
                Theme::muted(),
            )));
            current_section = section;
        }

        let is_active = i == app.sidebar_index;
        let shortcut = page.shortcut();
        let label = page.label();

        let line = if is_active {
            Line::from(vec![
                Span::styled(" ▸ ", Theme::accent()),
                Span::styled(format!("{shortcut}"), Theme::accent()),
                Span::styled(format!(" {label}"), Theme::accent_bold()),
            ])
        } else {
            Line::from(vec![
                Span::styled("   ", Theme::dim()),
                Span::styled(format!("{shortcut}"), Theme::muted()),
                Span::styled(format!(" {label}"), Theme::dim()),
            ])
        };

        lines.push(line);
    }

    let nav = Paragraph::new(lines).style(Style::default().bg(Theme::BG));
    frame.render_widget(nav, area);
}

fn render_device_status(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Theme::border())
        .style(Style::default().bg(Theme::BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = match &app.device_manager.state {
        ConnectionState::Connected => {
            if let Some(ref device) = app.device_manager.current_device {
                vec![
                    Line::from(vec![
                        Span::styled(" ● ", Theme::success()),
                        Span::styled(&device.model, Theme::text()),
                    ]),
                    Line::from(vec![
                        Span::styled("   ", Style::default()),
                        Span::styled(&device.serial, Theme::muted()),
                    ]),
                ]
            } else {
                vec![Line::from(Span::styled(" ● Connected", Theme::success()))]
            }
        }
        ConnectionState::Connecting => {
            vec![Line::from(vec![
                Span::styled(" ◌ ", Theme::warning()),
                Span::styled("Connecting...", Theme::dim()),
            ])]
        }
        ConnectionState::Disconnected => {
            vec![Line::from(vec![
                Span::styled(" ○ ", Theme::muted()),
                Span::styled("No device", Theme::muted()),
            ])]
        }
    };

    let status = Paragraph::new(lines).style(Style::default().bg(Theme::BG));
    frame.render_widget(status, inner);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![Span::styled(
        format!(" v{} ", env!("CARGO_PKG_VERSION")),
        Theme::muted(),
    )]))
    .style(Style::default().bg(Theme::BG));

    frame.render_widget(footer, area);
}
