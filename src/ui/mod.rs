mod sidebar;
mod dashboard;
mod shell;
mod logcat;
mod files;
mod apps;
mod controls;
mod settings;
mod bugreport;
mod screen;
mod about;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::{App, ModalState, Page};
use crate::components::{render_confirm_modal, render_input_modal};
use crate::theme::Theme;

/// Sidebar width in columns.
const SIDEBAR_WIDTH: u16 = 26;

/// Render the full application UI.
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Set background
    let bg_block = ratatui::widgets::Block::default()
        .style(ratatui::style::Style::default().bg(Theme::BG));
    frame.render_widget(bg_block, area);

    // Two-column layout: sidebar | content
    let chunks = Layout::horizontal([
        Constraint::Length(SIDEBAR_WIDTH),
        Constraint::Min(0),
    ])
    .split(area);

    // Render sidebar
    sidebar::render(app, frame, chunks[0]);

    // Render active page
    let content_area = chunks[1];
    match app.page {
        Page::Dashboard => dashboard::render(app, frame, content_area),
        Page::Shell => shell::render(app, frame, content_area),
        Page::Logcat => logcat::render(app, frame, content_area),
        Page::Screen => screen::render(app, frame, content_area),
        Page::Apps => apps::render(app, frame, content_area),
        Page::Files => files::render(app, frame, content_area),
        Page::Controls => controls::render(app, frame, content_area),
        Page::Bugreport => bugreport::render(app, frame, content_area),
        Page::Settings => settings::render(app, frame, content_area),
        Page::About => about::render(app, frame, content_area),
    }

    // Render modal overlay if active
    match &app.modal {
        ModalState::None => {}
        ModalState::Confirm { title, message, confirm_focused, .. } => {
            render_confirm_modal(frame, area, title, message, *confirm_focused);
        }
        ModalState::TextInput { title, prompt, value, cursor_pos, .. } => {
            render_input_modal(frame, area, title, prompt, value, *cursor_pos);
        }
    }
}

/// Helper: render a page header bar like "SHELL // ADB".
fn render_page_header(frame: &mut Frame, area: Rect, title: &str, subtitle: &str) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let line = Line::from(vec![
        Span::styled(title, Theme::accent_bold()),
        Span::styled(" // ", Theme::muted()),
        Span::styled(subtitle, Theme::dim()),
    ]);

    let header = Paragraph::new(line)
        .style(ratatui::style::Style::default().bg(Theme::BG));

    frame.render_widget(header, area);
}

/// Helper: render a centered "DISCONNECTED" message.
fn render_disconnected(frame: &mut Frame, area: Rect) {
    use ratatui::layout::Alignment;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let text = Line::from(vec![
        Span::styled("✕ ", Theme::error()),
        Span::styled("DISCONNECTED", Theme::muted()),
    ]);

    let msg = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(ratatui::style::Style::default().bg(Theme::BG));

    // Center vertically
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);

    frame.render_widget(msg, vertical[1]);
}
