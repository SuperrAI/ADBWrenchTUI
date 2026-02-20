use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap};
use ratatui::Frame;

use crate::theme::Theme;

// ── Progress / gauge ─────────────────────────────────────────────

/// Render a horizontal progress bar in a single-line area.
pub fn render_gauge(frame: &mut Frame, area: Rect, ratio: f64, label: &str, color: Color) {
    let clamped = ratio.clamp(0.0, 1.0);
    let gauge = Gauge::default()
        .ratio(clamped)
        .label(Span::styled(
            label,
            Style::default().fg(Theme::FG).add_modifier(Modifier::BOLD),
        ))
        .gauge_style(Style::default().fg(color).bg(Theme::BG_ELEVATED));
    frame.render_widget(gauge, area);
}

// ── Keybinding footer ────────────────────────────────────────────

/// Render a footer row with keybinding hints: `key label  key label  ...`
pub fn render_keybinding_footer(frame: &mut Frame, area: Rect, bindings: &[(&str, &str)]) {
    let mut spans = Vec::with_capacity(bindings.len() * 3);
    spans.push(Span::raw(" "));
    for (i, (key, label)) in bindings.iter().enumerate() {
        spans.push(Span::styled(*key, Theme::accent()));
        spans.push(Span::styled(format!(" {label}"), Theme::muted()));
        if i < bindings.len() - 1 {
            spans.push(Span::styled("  ", Style::default()));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Tab bar ──────────────────────────────────────────────────────

/// Render a row of toggleable tabs: `[TAB1] [TAB2] [TAB3]`.
/// Each entry is (label, is_active).
pub fn render_tab_bar(frame: &mut Frame, area: Rect, tabs: &[(&str, bool)]) {
    let mut spans = Vec::with_capacity(tabs.len() * 2 + 1);
    spans.push(Span::raw(" "));
    for (label, active) in tabs {
        if *active {
            spans.push(Span::styled(format!("[{label}]"), Theme::accent_bold()));
        } else {
            spans.push(Span::styled(format!("[{label}]"), Theme::muted()));
        }
        spans.push(Span::raw(" "));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Toggle button ────────────────────────────────────────────────

/// Produce a styled span for an [ON]/[OFF] toggle.
pub fn toggle_span(active: bool) -> Span<'static> {
    if active {
        Span::styled("[ON]", Theme::success())
    } else {
        Span::styled("[OFF]", Theme::muted())
    }
}

// ── Text input ───────────────────────────────────────────────────

/// Render a single-line text input with cursor.
pub fn render_text_input(
    frame: &mut Frame,
    area: Rect,
    value: &str,
    cursor_pos: usize,
    prompt: &str,
    is_focused: bool,
) {
    let cursor_char = if is_focused { "\u{2588}" } else { "" };

    // Calculate visible window of text
    let prompt_len = prompt.len();
    let available_width = area.width.saturating_sub(prompt_len as u16 + 1) as usize;

    let (visible_text, cursor_in_view) = if cursor_pos > available_width {
        let start = cursor_pos - available_width;
        (&value[start..], available_width)
    } else {
        (value, cursor_pos)
    };

    let visible_text = if visible_text.len() > available_width {
        &visible_text[..available_width]
    } else {
        visible_text
    };

    // Build text with cursor
    let before_cursor = &visible_text[..cursor_in_view.min(visible_text.len())];
    let after_cursor = if cursor_in_view < visible_text.len() {
        &visible_text[cursor_in_view..]
    } else {
        ""
    };

    let spans = vec![
        Span::styled(prompt, Theme::accent()),
        Span::styled(before_cursor, Theme::text()),
        Span::styled(cursor_char, Style::default().fg(Theme::ORANGE)),
        Span::styled(after_cursor, Theme::text()),
    ];

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Sparkline ────────────────────────────────────────────────────

/// Unicode block characters for sparkline rendering (8 levels + baseline).
const SPARKLINE_CHARS: [char; 9] = [
    ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
    '\u{2588}',
];

/// Render an ASCII sparkline chart in a single-line area.
pub fn render_sparkline(frame: &mut Frame, area: Rect, data: &[f64], max: f64, color: Color) {
    if data.is_empty() || max <= 0.0 {
        return;
    }

    let width = area.width as usize;
    let start = if data.len() > width {
        data.len() - width
    } else {
        0
    };
    let visible = &data[start..];

    let chars: String = visible
        .iter()
        .map(|&v| {
            let normalized = (v / max).clamp(0.0, 1.0);
            let idx = (normalized * 8.0).round() as usize;
            SPARKLINE_CHARS[idx.min(8)]
        })
        .collect();

    // Pad with spaces if data is shorter than width
    let padded = if chars.len() < width {
        format!("{}{}", " ".repeat(width - chars.len()), chars)
    } else {
        chars
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(padded, Style::default().fg(color))))
            .style(Style::default().bg(Theme::BG)),
        area,
    );
}

// ── Empty state ──────────────────────────────────────────────────

/// Render a centered empty state with icon, message, and hint.
pub fn render_empty_state(frame: &mut Frame, area: Rect, icon: &str, message: &str, hint: &str) {
    let chunks = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(icon, Theme::muted()),
            Span::styled(format!(" {message}"), Theme::dim()),
        ]))
        .alignment(Alignment::Center),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Theme::muted())).alignment(Alignment::Center),
        chunks[2],
    );
}

// ── Modal helpers ────────────────────────────────────────────────

/// Return a centered rect within `r` using percentage dimensions.
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// Render a confirmation modal overlay.
pub fn render_confirm_modal(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    message: &str,
    confirm_focused: bool,
) {
    let popup = centered_rect(50, 30, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_active())
        .border_type(BorderType::Rounded)
        .title(Span::styled(format!(" {title} "), Theme::accent_bold()))
        .style(Style::default().bg(Theme::BG_ELEVATED));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    // Message
    frame.render_widget(
        Paragraph::new(Span::styled(message, Theme::text()))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        chunks[1],
    );

    // Buttons
    let cancel_style = if !confirm_focused {
        Theme::accent_bold()
    } else {
        Theme::muted()
    };
    let confirm_style = if confirm_focused {
        Style::default()
            .fg(Theme::RED)
            .add_modifier(Modifier::BOLD)
    } else {
        Theme::muted()
    };

    let buttons = Line::from(vec![
        Span::styled("[ CANCEL ]", cancel_style),
        Span::raw("   "),
        Span::styled("[ CONFIRM ]", confirm_style),
    ]);
    frame.render_widget(
        Paragraph::new(buttons).alignment(Alignment::Center),
        chunks[3],
    );
}

/// Render a text input modal overlay.
pub fn render_input_modal(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    prompt: &str,
    value: &str,
    cursor_pos: usize,
) {
    let popup = centered_rect(60, 30, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_active())
        .border_type(BorderType::Rounded)
        .title(Span::styled(format!(" {title} "), Theme::accent_bold()))
        .style(Style::default().bg(Theme::BG_ELEVATED));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    // Label
    frame.render_widget(
        Paragraph::new(Span::styled(prompt, Theme::dim())).alignment(Alignment::Center),
        chunks[1],
    );

    // Input field
    let input_area = Layout::horizontal([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .split(chunks[2]);
    render_text_input(frame, input_area[1], value, cursor_pos, "> ", true);

    // Hint
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Enter to confirm  Esc to cancel",
            Theme::muted(),
        ))
        .alignment(Alignment::Center),
        chunks[3],
    );
}

// ── Utility ──────────────────────────────────────────────────────

/// Truncate a string with ellipsis if it exceeds max length.
pub fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}
