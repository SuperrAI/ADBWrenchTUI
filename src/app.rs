use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::adb::DeviceManager;

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
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            page: Page::Dashboard,
            sidebar_index: 0,
            focus: Focus::Sidebar,
            device_manager: DeviceManager::new(),
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

    /// Attempt initial device connection.
    pub async fn init_device(&mut self) -> Result<()> {
        self.device_manager.refresh_devices().await?;
        Ok(())
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
