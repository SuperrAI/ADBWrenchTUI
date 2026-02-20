use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .split(area);

    super::render_page_header(frame, chunks[0], "BUGREPORT", "GENERATOR");

    if !app.device_manager.is_connected() {
        super::render_disconnected(frame, chunks[1]);
        return;
    }

    // TODO: implement page content
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;
    use ratatui::layout::Alignment;
    use crate::theme::Theme;

    let placeholder = Paragraph::new(Line::from(Span::styled(
        "[ Coming soon ]",
        Theme::muted(),
    )))
    .alignment(Alignment::Center);

    let centered = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(chunks[1]);

    frame.render_widget(placeholder, centered[1]);
}
