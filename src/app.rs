use std::time::Instant;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::adb::DeviceManager;

/// Auto-refresh interval options (in seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshInterval {
    Off,
    Seconds5,
    Seconds10,
    Seconds30,
}

impl RefreshInterval {
    pub fn label(&self) -> &str {
        match self {
            Self::Off => "OFF",
            Self::Seconds5 => "5S",
            Self::Seconds10 => "10S",
            Self::Seconds30 => "30S",
        }
    }

    pub fn duration_secs(&self) -> Option<u64> {
        match self {
            Self::Off => None,
            Self::Seconds5 => Some(5),
            Self::Seconds10 => Some(10),
            Self::Seconds30 => Some(30),
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Off => Self::Seconds5,
            Self::Seconds5 => Self::Seconds10,
            Self::Seconds10 => Self::Seconds30,
            Self::Seconds30 => Self::Off,
        }
    }
}

/// Dashboard-specific state.
pub struct DashboardState {
    pub loading: bool,
    pub last_refresh: Option<Instant>,
    pub auto_refresh: RefreshInterval,
    pub error: Option<String>,
}

/// Pages available in the TUI — mirrors the web app's navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Shell,
    Logcat,
    Screen,
    Apps,
    Files,
    Controls,
    Performance,
    Bugreport,
    Settings,
}

impl Page {
    pub const ALL: &[Page] = &[
        Page::Dashboard,
        Page::Shell,
        Page::Logcat,
        Page::Screen,
        Page::Apps,
        Page::Files,
        Page::Controls,
        Page::Performance,
        Page::Bugreport,
        Page::Settings,
    ];

    pub fn label(&self) -> &str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Shell => "Shell",
            Self::Logcat => "Logcat",
            Self::Screen => "Screen",
            Self::Apps => "Apps",
            Self::Files => "Files",
            Self::Controls => "Controls",
            Self::Performance => "Perf",
            Self::Bugreport => "Bugreport",
            Self::Settings => "Settings",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Dashboard => "◉",
            Self::Shell => ">_",
            Self::Logcat => "☰",
            Self::Screen => "◻",
            Self::Apps => "▦",
            Self::Files => "🗁",
            Self::Controls => "⚙",
            Self::Performance => "▃",
            Self::Bugreport => "🐛",
            Self::Settings => "⚡",
        }
    }

    /// Section headers for the sidebar nav groups.
    pub fn section(&self) -> &str {
        match self {
            Self::Dashboard => "MAIN",
            Self::Shell | Self::Logcat | Self::Screen | Self::Apps | Self::Files => "TOOLS",
            Self::Controls | Self::Performance | Self::Bugreport | Self::Settings => "SYSTEM",
        }
    }

    /// Shortcut key (1-9, 0) for quick navigation.
    pub fn shortcut(&self) -> char {
        match self {
            Self::Dashboard => '1',
            Self::Shell => '2',
            Self::Logcat => '3',
            Self::Screen => '4',
            Self::Apps => '5',
            Self::Files => '6',
            Self::Controls => '7',
            Self::Performance => '8',
            Self::Bugreport => '9',
            Self::Settings => '0',
        }
    }
}

/// Focus region — whether the user is interacting with the sidebar or main content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Content,
}

/// Top-level application state.
pub struct App {
    pub running: bool,
    pub page: Page,
    pub sidebar_index: usize,
    pub focus: Focus,
    pub device_manager: DeviceManager,
    pub dashboard: DashboardState,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            page: Page::Dashboard,
            sidebar_index: 0,
            focus: Focus::Sidebar,
            device_manager: DeviceManager::new(),
            dashboard: DashboardState {
                loading: false,
                last_refresh: None,
                auto_refresh: RefreshInterval::Seconds10,
                error: None,
            },
        }
    }

    /// Handle a key event at the top level (global keybindings).
    /// Returns true if the event was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Ctrl+C or q always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return true;
        }
        if key.code == KeyCode::Char('q') && self.focus == Focus::Sidebar {
            self.running = false;
            return true;
        }

        // Tab toggles focus between sidebar and content
        if key.code == KeyCode::Tab {
            self.focus = match self.focus {
                Focus::Sidebar => Focus::Content,
                Focus::Content => Focus::Sidebar,
            };
            return true;
        }

        // Number shortcuts for page navigation (global)
        if let KeyCode::Char(c) = key.code {
            for (i, page) in Page::ALL.iter().enumerate() {
                if page.shortcut() == c && self.focus == Focus::Sidebar {
                    self.page = *page;
                    self.sidebar_index = i;
                    return true;
                }
            }
        }

        // Sidebar navigation
        if self.focus == Focus::Sidebar {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.sidebar_index > 0 {
                        self.sidebar_index -= 1;
                    }
                    self.page = Page::ALL[self.sidebar_index];
                    return true;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.sidebar_index < Page::ALL.len() - 1 {
                        self.sidebar_index += 1;
                    }
                    self.page = Page::ALL[self.sidebar_index];
                    return true;
                }
                KeyCode::Enter => {
                    self.focus = Focus::Content;
                    return true;
                }
                _ => {}
            }
        }

        // Escape returns focus to sidebar
        if key.code == KeyCode::Esc {
            self.focus = Focus::Sidebar;
            return true;
        }

        false
    }

    /// Handle page-specific key events. Returns true if consumed.
    pub fn handle_page_key(&mut self, key: KeyEvent) -> bool {
        if self.focus != Focus::Content {
            return false;
        }

        match self.page {
            Page::Dashboard => self.handle_dashboard_key(key),
            _ => false,
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // 'r' to manual refresh
            KeyCode::Char('r') => {
                self.dashboard.last_refresh = None; // Force refresh on next tick
                true
            }
            // 'a' to cycle auto-refresh interval
            KeyCode::Char('a') => {
                self.dashboard.auto_refresh = self.dashboard.auto_refresh.next();
                true
            }
            _ => false,
        }
    }

    /// Attempt initial device connection.
    pub async fn init_device(&mut self) -> Result<()> {
        self.device_manager.refresh_devices().await?;
        Ok(())
    }

    /// Refresh dashboard data from device.
    pub async fn refresh_dashboard(&mut self) {
        if !self.device_manager.is_connected() || self.dashboard.loading {
            return;
        }

        self.dashboard.loading = true;
        self.dashboard.error = None;

        match self.device_manager.fetch_full_info().await {
            Ok(()) => {
                self.dashboard.last_refresh = Some(Instant::now());
            }
            Err(e) => {
                self.dashboard.error = Some(e.to_string());
                tracing::error!("Dashboard refresh failed: {e}");
            }
        }

        self.dashboard.loading = false;
    }

    /// Check if dashboard auto-refresh is due.
    pub fn dashboard_needs_refresh(&self) -> bool {
        if self.page != Page::Dashboard || !self.device_manager.is_connected() {
            return false;
        }

        // First load
        if self.dashboard.last_refresh.is_none() && !self.dashboard.loading {
            return true;
        }

        // Auto-refresh timer
        if let Some(interval_secs) = self.dashboard.auto_refresh.duration_secs() {
            if let Some(last) = self.dashboard.last_refresh {
                return last.elapsed().as_secs() >= interval_secs && !self.dashboard.loading;
            }
        }

        false
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
