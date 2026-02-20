use ratatui::style::{Color, Modifier, Style};

/// ADBWrench color palette — matches the web app's dark theme.
/// The web app uses a neutral dark background with orange as the primary accent.
pub struct Theme;

impl Theme {
    // ── Core palette ───────────────────────────────────────────────
    pub const BG: Color = Color::Rgb(23, 23, 23); // #171717 (neutral-900)
    pub const BG_ELEVATED: Color = Color::Rgb(38, 38, 38); // #262626 (neutral-800)
    pub const BORDER: Color = Color::Rgb(64, 64, 64); // #404040 (neutral-700)
    pub const FG: Color = Color::Rgb(245, 245, 245); // #f5f5f5 (neutral-100)
    pub const FG_DIM: Color = Color::Rgb(163, 163, 163); // #a3a3a3 (neutral-400)
    pub const FG_MUTED: Color = Color::Rgb(115, 115, 115); // #737373 (neutral-500)

    // ── Accent ─────────────────────────────────────────────────────
    pub const ORANGE: Color = Color::Rgb(249, 115, 22); // #f97316 (orange-500)

    // ── Semantic colors ────────────────────────────────────────────
    pub const GREEN: Color = Color::Rgb(34, 197, 94); // #22c55e (green-500)
    pub const RED: Color = Color::Rgb(239, 68, 68); // #ef4444 (red-500)
    pub const YELLOW: Color = Color::Rgb(234, 179, 8); // #eab308 (yellow-500)
    pub const BLUE: Color = Color::Rgb(59, 130, 246); // #3b82f6 (blue-500)

    // ── Logcat level colors ────────────────────────────────────────
    pub const LOG_VERBOSE: Color = Color::Rgb(163, 163, 163); // neutral-400
    pub const LOG_DEBUG: Color = Color::Rgb(59, 130, 246); // blue-500
    pub const LOG_INFO: Color = Color::Rgb(34, 197, 94); // green-500
    pub const LOG_WARN: Color = Color::Rgb(234, 179, 8); // yellow-500
    pub const LOG_ERROR: Color = Color::Rgb(239, 68, 68); // red-500
    pub const LOG_FATAL: Color = Color::Rgb(220, 38, 38); // red-600

    // ── Style helpers ──────────────────────────────────────────────

    /// Default text style
    pub fn text() -> Style {
        Style::default().fg(Self::FG)
    }

    /// Dimmed text (secondary information)
    pub fn dim() -> Style {
        Style::default().fg(Self::FG_DIM)
    }

    /// Muted text (hints, placeholders)
    pub fn muted() -> Style {
        Style::default().fg(Self::FG_MUTED)
    }

    /// Orange accent text
    pub fn accent() -> Style {
        Style::default().fg(Self::ORANGE)
    }

    /// Bold text
    pub fn bold() -> Style {
        Style::default().fg(Self::FG).add_modifier(Modifier::BOLD)
    }

    /// Bold accent
    pub fn accent_bold() -> Style {
        Style::default()
            .fg(Self::ORANGE)
            .add_modifier(Modifier::BOLD)
    }

    /// Success style
    pub fn success() -> Style {
        Style::default().fg(Self::GREEN)
    }

    /// Error style
    pub fn error() -> Style {
        Style::default().fg(Self::RED)
    }

    /// Warning style
    pub fn warning() -> Style {
        Style::default().fg(Self::YELLOW)
    }

    /// Border style
    pub fn border() -> Style {
        Style::default().fg(Self::BORDER)
    }

    /// Active/selected border
    pub fn border_active() -> Style {
        Style::default().fg(Self::ORANGE)
    }

    /// Highlighted row (e.g., selected item in a list)
    pub fn highlight() -> Style {
        Style::default()
            .bg(Self::BG_ELEVATED)
            .fg(Self::ORANGE)
            .add_modifier(Modifier::BOLD)
    }

    /// Block title style (uppercase label)
    pub fn title() -> Style {
        Style::default()
            .fg(Self::FG_DIM)
            .add_modifier(Modifier::BOLD)
    }
}
