use std::collections::HashSet;
use std::time::Instant;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::adb::types::*;
use crate::adb::DeviceManager;
use crate::adb::parser;

// ── Enums for page-specific options ──────────────────────────────

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellTimeout {
    Sec10,
    Sec30,
    Sec60,
}

impl ShellTimeout {
    pub fn label(&self) -> &str {
        match self {
            Self::Sec10 => "10S",
            Self::Sec30 => "30S",
            Self::Sec60 => "60S",
        }
    }

    pub fn secs(&self) -> u64 {
        match self {
            Self::Sec10 => 10,
            Self::Sec30 => 30,
            Self::Sec60 => 60,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Sec10 => Self::Sec30,
            Self::Sec30 => Self::Sec60,
            Self::Sec60 => Self::Sec10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogcatBuffer {
    Main,
    System,
    Crash,
    Events,
    All,
}

impl LogcatBuffer {
    pub fn label(&self) -> &str {
        match self {
            Self::Main => "MAIN",
            Self::System => "SYSTEM",
            Self::Crash => "CRASH",
            Self::Events => "EVENTS",
            Self::All => "ALL",
        }
    }

    pub fn arg(&self) -> &str {
        match self {
            Self::Main => "main",
            Self::System => "system",
            Self::Crash => "crash",
            Self::Events => "events",
            Self::All => "all",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Main => Self::System,
            Self::System => Self::Crash,
            Self::Crash => Self::Events,
            Self::Events => Self::All,
            Self::All => Self::Main,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerfRefreshRate {
    Sec1,
    Sec2,
    Sec5,
}

impl PerfRefreshRate {
    pub fn label(&self) -> &str {
        match self {
            Self::Sec1 => "1S",
            Self::Sec2 => "2S",
            Self::Sec5 => "5S",
        }
    }

    pub fn secs(&self) -> u64 {
        match self {
            Self::Sec1 => 1,
            Self::Sec2 => 2,
            Self::Sec5 => 5,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Sec1 => Self::Sec2,
            Self::Sec2 => Self::Sec5,
            Self::Sec5 => Self::Sec1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordDuration {
    Sec30,
    Min1,
    Min2,
    Min3,
}

impl RecordDuration {
    pub fn label(&self) -> &str {
        match self {
            Self::Sec30 => "30S",
            Self::Min1 => "1M",
            Self::Min2 => "2M",
            Self::Min3 => "3M",
        }
    }

    pub fn secs(&self) -> u32 {
        match self {
            Self::Sec30 => 30,
            Self::Min1 => 60,
            Self::Min2 => 120,
            Self::Min3 => 180,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Sec30 => Self::Min1,
            Self::Min1 => Self::Min2,
            Self::Min2 => Self::Min3,
            Self::Min3 => Self::Sec30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFilter {
    All,
    User,
    System,
}

impl AppFilter {
    pub fn label(&self) -> &str {
        match self {
            Self::All => "ALL",
            Self::User => "USER",
            Self::System => "SYSTEM",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::All => Self::User,
            Self::User => Self::System,
            Self::System => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsNamespace {
    System,
    Secure,
    Global,
}

impl SettingsNamespace {
    pub fn label(&self) -> &str {
        match self {
            Self::System => "SYSTEM",
            Self::Secure => "SECURE",
            Self::Global => "GLOBAL",
        }
    }

    pub fn arg(&self) -> &str {
        match self {
            Self::System => "system",
            Self::Secure => "secure",
            Self::Global => "global",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::System => Self::Secure,
            Self::Secure => Self::Global,
            Self::Global => Self::System,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BugreportStatus {
    Generating,
    Completed,
    Failed,
    Cancelled,
}

// ── Page state structs ───────────────────────────────────────────

/// Dashboard-specific state.
pub struct DashboardState {
    pub loading: bool,
    pub last_refresh: Option<Instant>,
    pub auto_refresh: RefreshInterval,
    pub error: Option<String>,
}

/// Shell output entry type.
#[derive(Debug, Clone)]
pub enum ShellEntryType {
    Command,
    Output,
    Error,
}

/// A single shell output entry.
#[derive(Debug, Clone)]
pub struct ShellOutputEntry {
    pub entry_type: ShellEntryType,
    pub content: String,
}

/// Shell page state.
pub struct ShellState {
    pub input: String,
    pub cursor_pos: usize,
    pub output: Vec<ShellOutputEntry>,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub is_running: bool,
    pub is_streaming: bool,
    pub scroll_offset: usize,
    pub timeout: ShellTimeout,
    pub stream_rx: Option<mpsc::UnboundedReceiver<String>>,
}

/// Logcat page state.
pub struct LogcatState {
    pub logs: Vec<LogEntry>,
    pub is_streaming: bool,
    pub auto_scroll: bool,
    pub show_timestamp: bool,
    pub buffer: LogcatBuffer,
    pub level_filter: [bool; 6], // V, D, I, W, E, F
    pub search_query: String,
    pub tag_filter: String,
    pub scroll_offset: usize,
    pub search_active: bool,
    pub tag_active: bool,
    pub stream_rx: Option<mpsc::UnboundedReceiver<String>>,
}

/// Controls page state.
pub struct ControlsState {
    pub focus_section: usize,
    pub focus_item: usize,
    pub loading: Option<String>,
    pub result: Option<(bool, String)>,
    pub result_timer: Option<Instant>,
    pub brightness: u16,
    pub volume: u8,
    pub text_input: String,
    pub text_input_active: bool,
    pub text_cursor_pos: usize,
}

/// Performance page state.
pub struct PerfState {
    pub is_monitoring: bool,
    pub refresh_rate: PerfRefreshRate,
    pub cpu_history: Vec<f64>,
    pub mem_history: Vec<f64>,
    pub mem_total_kb: u64,
    pub mem_used_kb: u64,
    pub battery: Option<BatteryInfo>,
    pub processes: Vec<ProcessInfo>,
    pub last_collect: Option<Instant>,
    pub scroll_offset: usize,
}

/// Files page state.
pub struct FilesState {
    pub current_path: String,
    pub entries: Vec<FileEntry>,
    pub selected_index: usize,
    pub selected_files: HashSet<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub mkdir_input: String,
    pub mkdir_cursor_pos: usize,
}

/// Apps page panel focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPanel {
    List,
    Detail,
}

/// Apps page state.
pub struct AppsState {
    pub packages: Vec<PackageInfo>,
    pub loading: bool,
    pub search_query: String,
    pub search_active: bool,
    pub filter_type: AppFilter,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub detail_package: Option<String>,
    pub detail_loading: bool,
    pub package_details: Option<PackageDetails>,
    pub detail_scroll_offset: usize,
    pub focus_panel: AppPanel,
    pub action_result: Option<(bool, String, Instant)>,
}

/// Package details (extended info from dumpsys package).
#[derive(Debug, Clone)]
pub struct PackageDetails {
    pub package_name: String,
    pub version_name: String,
    pub version_code: String,
    pub installed_path: String,
    pub first_install_time: String,
    pub last_update_time: String,
    pub is_system: bool,
    pub is_enabled: bool,
    pub permissions: Vec<String>,
}

/// Settings page focus area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFocus {
    QuickToggles,
    List,
}

/// A single setting entry.
#[derive(Debug, Clone)]
pub struct SettingEntry {
    pub key: String,
    pub value: String,
}

/// Settings page state.
pub struct SettingsState {
    pub namespace: SettingsNamespace,
    pub settings: Vec<SettingEntry>,
    pub loading: bool,
    pub search_query: String,
    pub search_active: bool,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub quick_toggle_focus: usize,
    pub quick_toggle_states: [bool; 6],
    pub focus_area: SettingsFocus,
}

/// Quick toggle definition.
pub struct QuickToggle {
    pub name: &'static str,
    pub description: &'static str,
    pub namespace: &'static str,
    pub key: &'static str,
    pub enable_value: &'static str,
    pub disable_value: &'static str,
}

pub const QUICK_TOGGLES: &[QuickToggle] = &[
    QuickToggle { name: "WIRELESS ADB", description: "ADB over WiFi", namespace: "global", key: "adb_wifi_enabled", enable_value: "1", disable_value: "0" },
    QuickToggle { name: "SHOW TOUCHES", description: "Show touch points", namespace: "system", key: "show_touches", enable_value: "1", disable_value: "0" },
    QuickToggle { name: "POINTER LOC", description: "Pointer location", namespace: "system", key: "pointer_location", enable_value: "1", disable_value: "0" },
    QuickToggle { name: "STAY AWAKE", description: "Stay on while plugged", namespace: "global", key: "stay_on_while_plugged_in", enable_value: "3", disable_value: "0" },
    QuickToggle { name: "GPU DEBUG", description: "GPU debug layers", namespace: "global", key: "enable_gpu_debug_layers", enable_value: "1", disable_value: "0" },
    QuickToggle { name: "ANIM SCALE", description: "Animator scale", namespace: "global", key: "animator_duration_scale", enable_value: "1.0", disable_value: "0" },
];

/// Bugreport history entry.
#[derive(Debug, Clone)]
pub struct BugreportEntry {
    pub filename: String,
    pub status: BugreportStatus,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub device_path: Option<String>,
    pub local_path: Option<String>,
    pub error: Option<String>,
}

/// Bugreport page state.
pub struct BugreportState {
    pub is_generating: bool,
    pub progress: u8,
    pub start_time: Option<Instant>,
    pub history: Vec<BugreportEntry>,
    pub selected_index: usize,
    pub stream_rx: Option<mpsc::UnboundedReceiver<String>>,
    pub raw_output: String,
}

/// Screen capture entry.
#[derive(Debug, Clone)]
pub struct ScreenCapture {
    pub filename: String,
    pub local_path: String,
    pub timestamp: String,
}

/// Screen recording entry.
#[derive(Debug, Clone)]
pub struct RecordingEntry {
    pub filename: String,
    pub local_path: String,
    pub duration_secs: u32,
    pub timestamp: String,
}

/// Screen tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenTab {
    Screenshot,
    Record,
}

/// Screen page state.
pub struct ScreenState {
    pub active_tab: ScreenTab,
    pub is_capturing: bool,
    pub captures: Vec<ScreenCapture>,
    pub capture_selected: usize,
    pub is_recording: bool,
    pub record_duration: RecordDuration,
    pub record_elapsed: u32,
    pub record_start: Option<Instant>,
    pub recordings: Vec<RecordingEntry>,
    pub recording_selected: usize,
    pub stream_rx: Option<mpsc::UnboundedReceiver<String>>,
    pub error: Option<String>,
}

// ── Modal state ──────────────────────────────────────────────────

/// Modal dialog state.
#[derive(Debug, Clone)]
pub enum ModalState {
    None,
    Confirm {
        title: String,
        message: String,
        action: AppAction,
        confirm_focused: bool,
    },
    TextInput {
        title: String,
        prompt: String,
        value: String,
        cursor_pos: usize,
        action_tag: String,
    },
}

// ── App action ───────────────────────────────────────────────────

/// Actions that page key handlers request; dispatched asynchronously by the event loop.
#[derive(Debug, Clone)]
pub enum AppAction {
    None,
    // Shell
    ShellExecute(String),
    ShellExecuteStream(String),
    ShellStop,
    // Logcat
    LogcatStart,
    LogcatStop,
    // Performance
    PerfCollect,
    // Files
    FilesNavigate(String),
    FilesRefresh,
    FilesDelete(Vec<String>),
    FilesPull(Vec<String>),
    FilesMkdir(String),
    // Apps
    AppsRefresh,
    AppsLoadDetails(String),
    AppsOpen(String),
    AppsStop(String),
    AppsClear(String),
    AppsUninstall(String),
    // Controls
    ControlsExec(String),
    // Settings
    SettingsLoad,
    SettingsPut(String, String, String),
    SettingsDelete(String, String),
    SettingsToggle(usize),
    // Bugreport
    BugreportStart,
    BugreportCancel,
    BugreportDownload(usize),
    // Screen
    ScreenCapture,
    ScreenRecordStart,
    ScreenRecordStop,
}

// ── Pages ────────────────────────────────────────────────────────

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

// ── App struct ───────────────────────────────────────────────────

/// Top-level application state.
pub struct App {
    pub running: bool,
    pub page: Page,
    pub sidebar_index: usize,
    pub focus: Focus,
    pub device_manager: DeviceManager,
    pub modal: ModalState,
    // Page states
    pub dashboard: DashboardState,
    pub shell: ShellState,
    pub logcat: LogcatState,
    pub controls: ControlsState,
    pub performance: PerfState,
    pub files: FilesState,
    pub apps: AppsState,
    pub settings: SettingsState,
    pub bugreport: BugreportState,
    pub screen: ScreenState,
    // Stream child processes (for cleanup)
    pub stream_children: Vec<tokio::process::Child>,
    // Pending action from modal confirmations
    pub pending_action: Option<AppAction>,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            page: Page::Dashboard,
            sidebar_index: 0,
            focus: Focus::Sidebar,
            device_manager: DeviceManager::new(),
            modal: ModalState::None,
            dashboard: DashboardState {
                loading: false,
                last_refresh: None,
                auto_refresh: RefreshInterval::Seconds10,
                error: None,
            },
            shell: ShellState {
                input: String::new(),
                cursor_pos: 0,
                output: Vec::new(),
                history: Vec::new(),
                history_index: None,
                is_running: false,
                is_streaming: false,
                scroll_offset: 0,
                timeout: ShellTimeout::Sec10,
                stream_rx: None,
            },
            logcat: LogcatState {
                logs: Vec::new(),
                is_streaming: false,
                auto_scroll: true,
                show_timestamp: true,
                buffer: LogcatBuffer::Main,
                level_filter: [true; 6],
                search_query: String::new(),
                tag_filter: String::new(),
                scroll_offset: 0,
                search_active: false,
                tag_active: false,
                stream_rx: None,
            },
            controls: ControlsState {
                focus_section: 0,
                focus_item: 0,
                loading: None,
                result: None,
                result_timer: None,
                brightness: 128,
                volume: 7,
                text_input: String::new(),
                text_input_active: false,
                text_cursor_pos: 0,
            },
            performance: PerfState {
                is_monitoring: false,
                refresh_rate: PerfRefreshRate::Sec2,
                cpu_history: Vec::new(),
                mem_history: Vec::new(),
                mem_total_kb: 0,
                mem_used_kb: 0,
                battery: None,
                processes: Vec::new(),
                last_collect: None,
                scroll_offset: 0,
            },
            files: FilesState {
                current_path: "/sdcard".to_string(),
                entries: Vec::new(),
                selected_index: 0,
                selected_files: HashSet::new(),
                loading: false,
                error: None,
                mkdir_input: String::new(),
                mkdir_cursor_pos: 0,
            },
            apps: AppsState {
                packages: Vec::new(),
                loading: false,
                search_query: String::new(),
                search_active: false,
                filter_type: AppFilter::All,
                selected_index: 0,
                scroll_offset: 0,
                detail_package: None,
                detail_loading: false,
                package_details: None,
                detail_scroll_offset: 0,
                focus_panel: AppPanel::List,
                action_result: None,
            },
            settings: SettingsState {
                namespace: SettingsNamespace::System,
                settings: Vec::new(),
                loading: false,
                search_query: String::new(),
                search_active: false,
                selected_index: 0,
                scroll_offset: 0,
                quick_toggle_focus: 0,
                quick_toggle_states: [false; 6],
                focus_area: SettingsFocus::QuickToggles,
            },
            bugreport: BugreportState {
                is_generating: false,
                progress: 0,
                start_time: None,
                history: Vec::new(),
                selected_index: 0,
                stream_rx: None,
                raw_output: String::new(),
            },
            screen: ScreenState {
                active_tab: ScreenTab::Screenshot,
                is_capturing: false,
                captures: Vec::new(),
                capture_selected: 0,
                is_recording: false,
                record_duration: RecordDuration::Sec30,
                record_elapsed: 0,
                record_start: None,
                recordings: Vec::new(),
                recording_selected: 0,
                stream_rx: None,
                error: None,
            },
            stream_children: Vec::new(),
            pending_action: None,
        }
    }

    // ── Global key handling ──────────────────────────────────────

    /// Handle a key event at the top level (global keybindings).
    /// Returns true if the event was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Modal captures all keys when active
        if !matches!(self.modal, ModalState::None) {
            return self.handle_modal_key(key);
        }

        // Ctrl+C always quits (unless in text input mode on content pages)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            // If streaming, stop the stream instead of quitting
            if self.focus == Focus::Content {
                match self.page {
                    Page::Shell if self.shell.is_streaming => {
                        return true; // handled by page handler
                    }
                    Page::Logcat if self.logcat.is_streaming => {
                        return true; // handled by page handler
                    }
                    _ => {}
                }
            }
            self.running = false;
            return true;
        }

        // q quits from sidebar
        if key.code == KeyCode::Char('q') && self.focus == Focus::Sidebar {
            self.running = false;
            return true;
        }

        // Tab / Shift+Tab navigation
        if (key.code == KeyCode::Tab || key.code == KeyCode::BackTab) && !self.is_text_input_active() {
            if self.focus == Focus::Sidebar {
                self.focus = Focus::Content;
                return true;
            }
            // Pages with internal sections handle Tab/BackTab themselves (fall through).
            // Other pages: Tab/BackTab returns to sidebar.
            match self.page {
                Page::Controls | Page::Apps | Page::Settings => {
                    // Fall through to page handler for internal Tab cycling
                }
                _ => {
                    self.focus = Focus::Sidebar;
                    return true;
                }
            }
        }

        // Number shortcuts for page navigation (sidebar only)
        if self.focus == Focus::Sidebar {
            if let KeyCode::Char(c) = key.code {
                for (i, page) in Page::ALL.iter().enumerate() {
                    if page.shortcut() == c {
                        let old_page = self.page;
                        self.page = *page;
                        self.sidebar_index = i;
                        if old_page != *page {
                            self.on_page_leave(old_page);
                        }
                        return true;
                    }
                }
            }
        }

        // Sidebar navigation
        if self.focus == Focus::Sidebar {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.sidebar_index > 0 {
                        let old_page = self.page;
                        self.sidebar_index -= 1;
                        self.page = Page::ALL[self.sidebar_index];
                        if old_page != self.page {
                            self.on_page_leave(old_page);
                        }
                    }
                    return true;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.sidebar_index < Page::ALL.len() - 1 {
                        let old_page = self.page;
                        self.sidebar_index += 1;
                        self.page = Page::ALL[self.sidebar_index];
                        if old_page != self.page {
                            self.on_page_leave(old_page);
                        }
                    }
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
        if key.code == KeyCode::Esc && !self.is_text_input_active() {
            self.focus = Focus::Sidebar;
            return true;
        }

        false
    }

    /// Check if any text input field is currently active (to prevent key stealing).
    fn is_text_input_active(&self) -> bool {
        if self.focus != Focus::Content {
            return false;
        }
        match self.page {
            Page::Shell => true, // Shell always captures input
            Page::Logcat => self.logcat.search_active || self.logcat.tag_active,
            Page::Controls => self.controls.text_input_active,
            Page::Apps => self.apps.search_active,
            Page::Settings => self.settings.search_active,
            _ => false,
        }
    }

    // ── Modal key handling ───────────────────────────────────────

    fn handle_modal_key(&mut self, key: KeyEvent) -> bool {
        match &mut self.modal {
            ModalState::None => false,
            ModalState::Confirm {
                confirm_focused,
                action,
                ..
            } => match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    *confirm_focused = !*confirm_focused;
                    true
                }
                KeyCode::Enter => {
                    if *confirm_focused {
                        let action = action.clone();
                        self.modal = ModalState::None;
                        self.pending_action = Some(action);
                    } else {
                        self.modal = ModalState::None;
                    }
                    true
                }
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                    true
                }
                _ => true,
            },
            ModalState::TextInput {
                value,
                cursor_pos,
                action_tag,
                ..
            } => match key.code {
                KeyCode::Char(c) => {
                    value.insert(*cursor_pos, c);
                    *cursor_pos += 1;
                    true
                }
                KeyCode::Backspace => {
                    if *cursor_pos > 0 {
                        *cursor_pos -= 1;
                        value.remove(*cursor_pos);
                    }
                    true
                }
                KeyCode::Left => {
                    if *cursor_pos > 0 {
                        *cursor_pos -= 1;
                    }
                    true
                }
                KeyCode::Right => {
                    if *cursor_pos < value.len() {
                        *cursor_pos += 1;
                    }
                    true
                }
                KeyCode::Enter => {
                    let val = value.clone();
                    let tag = action_tag.clone();
                    self.modal = ModalState::None;
                    self.handle_modal_submit(&tag, &val);
                    true
                }
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                    true
                }
                _ => true,
            },
        }
    }

    /// Handle submission from text input modals.
    fn handle_modal_submit(&mut self, tag: &str, value: &str) {
        match tag {
            "mkdir" => {
                if !value.is_empty() {
                    let path = format!("{}/{}", self.files.current_path, value);
                    self.pending_action = Some(AppAction::FilesMkdir(path));
                }
            }
            "settings_edit" => {
                // value format: "namespace:key:new_value"
                let parts: Vec<&str> = value.splitn(3, ':').collect();
                if parts.len() == 3 {
                    self.pending_action = Some(AppAction::SettingsPut(
                        parts[0].to_string(),
                        parts[1].to_string(),
                        parts[2].to_string(),
                    ));
                }
            }
            _ => {}
        }
    }

    // ── Page key handling ────────────────────────────────────────

    /// Handle page-specific key events. Returns an action to dispatch.
    pub fn handle_page_key(&mut self, key: KeyEvent) -> AppAction {
        if self.focus != Focus::Content {
            return AppAction::None;
        }

        match self.page {
            Page::Dashboard => {
                self.handle_dashboard_key(key);
                AppAction::None
            }
            Page::Shell => self.handle_shell_key(key),
            Page::Logcat => self.handle_logcat_key(key),
            Page::Controls => self.handle_controls_key(key),
            Page::Performance => self.handle_performance_key(key),
            Page::Files => self.handle_files_key(key),
            Page::Apps => self.handle_apps_key(key),
            Page::Settings => self.handle_settings_key(key),
            Page::Bugreport => self.handle_bugreport_key(key),
            Page::Screen => self.handle_screen_key(key),
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('r') => {
                self.dashboard.last_refresh = None;
                true
            }
            KeyCode::Char('a') => {
                self.dashboard.auto_refresh = self.dashboard.auto_refresh.next();
                true
            }
            _ => false,
        }
    }

    fn handle_shell_key(&mut self, key: KeyEvent) -> AppAction {
        // Ctrl+C stops streaming
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.shell.is_streaming {
                return AppAction::ShellStop;
            }
            return AppAction::None;
        }

        match key.code {
            KeyCode::Enter => {
                if self.shell.is_running {
                    return AppAction::None;
                }
                let cmd = self.shell.input.trim().to_string();
                if cmd.is_empty() {
                    return AppAction::None;
                }
                // Add to history
                if self.shell.history.last().map_or(true, |h| h != &cmd) {
                    self.shell.history.push(cmd.clone());
                    if self.shell.history.len() > 100 {
                        self.shell.history.remove(0);
                    }
                }
                self.shell.history_index = None;
                self.shell.input.clear();
                self.shell.cursor_pos = 0;
                // Add command to output
                self.shell.output.push(ShellOutputEntry {
                    entry_type: ShellEntryType::Command,
                    content: cmd.clone(),
                });
                AppAction::ShellExecute(cmd)
            }
            KeyCode::Up => {
                if self.shell.history.is_empty() {
                    return AppAction::None;
                }
                let idx = match self.shell.history_index {
                    None => self.shell.history.len() - 1,
                    Some(i) => i.saturating_sub(1),
                };
                self.shell.history_index = Some(idx);
                self.shell.input = self.shell.history[idx].clone();
                self.shell.cursor_pos = self.shell.input.len();
                AppAction::None
            }
            KeyCode::Down => {
                if let Some(idx) = self.shell.history_index {
                    if idx + 1 < self.shell.history.len() {
                        self.shell.history_index = Some(idx + 1);
                        self.shell.input = self.shell.history[idx + 1].clone();
                    } else {
                        self.shell.history_index = None;
                        self.shell.input.clear();
                    }
                    self.shell.cursor_pos = self.shell.input.len();
                }
                AppAction::None
            }
            KeyCode::Char(c) => {
                // Quick commands: digit keys when input is empty
                if self.shell.input.is_empty() {
                    let presets = [
                        "getprop", "pm list packages", "dumpsys battery",
                        "df -h", "top -n 1 -b -m 10", "ps -A",
                        "netstat -tlnp", "ip addr",
                    ];
                    if let Some(idx) = c.to_digit(10) {
                        let idx = idx as usize;
                        if idx >= 1 && idx <= 8 {
                            self.shell.input = presets[idx - 1].to_string();
                            self.shell.cursor_pos = self.shell.input.len();
                            return AppAction::None;
                        }
                    }
                    if c == 'c' {
                        self.shell.output.clear();
                        self.shell.scroll_offset = 0;
                        return AppAction::None;
                    }
                    if c == 't' {
                        self.shell.timeout = self.shell.timeout.next();
                        return AppAction::None;
                    }
                }
                self.shell.input.insert(self.shell.cursor_pos, c);
                self.shell.cursor_pos += 1;
                AppAction::None
            }
            KeyCode::Backspace => {
                if self.shell.cursor_pos > 0 {
                    self.shell.cursor_pos -= 1;
                    self.shell.input.remove(self.shell.cursor_pos);
                }
                AppAction::None
            }
            KeyCode::Left => {
                self.shell.cursor_pos = self.shell.cursor_pos.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Right => {
                if self.shell.cursor_pos < self.shell.input.len() {
                    self.shell.cursor_pos += 1;
                }
                AppAction::None
            }
            KeyCode::PageUp => {
                self.shell.scroll_offset = self.shell.scroll_offset.saturating_add(10);
                AppAction::None
            }
            KeyCode::PageDown => {
                self.shell.scroll_offset = self.shell.scroll_offset.saturating_sub(10);
                AppAction::None
            }
            KeyCode::Esc => {
                if !self.shell.input.is_empty() {
                    self.shell.input.clear();
                    self.shell.cursor_pos = 0;
                } else {
                    self.focus = Focus::Sidebar;
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_logcat_key(&mut self, key: KeyEvent) -> AppAction {
        // Handle text input modes
        if self.logcat.search_active {
            return self.handle_logcat_search_input(key);
        }
        if self.logcat.tag_active {
            return self.handle_logcat_tag_input(key);
        }

        // Ctrl+C stops streaming
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.logcat.is_streaming {
                return AppAction::LogcatStop;
            }
            return AppAction::None;
        }

        match key.code {
            KeyCode::Char('s') => {
                if self.logcat.is_streaming {
                    AppAction::LogcatStop
                } else {
                    AppAction::LogcatStart
                }
            }
            KeyCode::Char('b') => {
                if !self.logcat.is_streaming {
                    self.logcat.buffer = self.logcat.buffer.next();
                }
                AppAction::None
            }
            KeyCode::Char('v') => { self.logcat.level_filter[0] = !self.logcat.level_filter[0]; AppAction::None }
            KeyCode::Char('d') => { self.logcat.level_filter[1] = !self.logcat.level_filter[1]; AppAction::None }
            KeyCode::Char('i') => { self.logcat.level_filter[2] = !self.logcat.level_filter[2]; AppAction::None }
            KeyCode::Char('w') => { self.logcat.level_filter[3] = !self.logcat.level_filter[3]; AppAction::None }
            KeyCode::Char('e') => { self.logcat.level_filter[4] = !self.logcat.level_filter[4]; AppAction::None }
            KeyCode::Char('f') => { self.logcat.level_filter[5] = !self.logcat.level_filter[5]; AppAction::None }
            KeyCode::Char('/') => { self.logcat.search_active = true; AppAction::None }
            KeyCode::Char('T') => { self.logcat.tag_active = true; AppAction::None }
            KeyCode::Char('t') => { self.logcat.show_timestamp = !self.logcat.show_timestamp; AppAction::None }
            KeyCode::Char('a') => { self.logcat.auto_scroll = !self.logcat.auto_scroll; AppAction::None }
            KeyCode::Char('c') => { self.logcat.logs.clear(); self.logcat.scroll_offset = 0; AppAction::None }
            KeyCode::Char('G') => { self.logcat.scroll_offset = 0; self.logcat.auto_scroll = true; AppAction::None }
            KeyCode::Char('g') => {
                self.logcat.auto_scroll = false;
                self.logcat.scroll_offset = self.logcat.logs.len().saturating_sub(1);
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.logcat.auto_scroll = false;
                self.logcat.scroll_offset = self.logcat.scroll_offset.saturating_add(1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.logcat.scroll_offset = self.logcat.scroll_offset.saturating_sub(1);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_logcat_search_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char(c) => { self.logcat.search_query.push(c); AppAction::None }
            KeyCode::Backspace => { self.logcat.search_query.pop(); AppAction::None }
            KeyCode::Enter | KeyCode::Esc => { self.logcat.search_active = false; AppAction::None }
            _ => AppAction::None,
        }
    }

    fn handle_logcat_tag_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char(c) => { self.logcat.tag_filter.push(c); AppAction::None }
            KeyCode::Backspace => { self.logcat.tag_filter.pop(); AppAction::None }
            KeyCode::Enter | KeyCode::Esc => { self.logcat.tag_active = false; AppAction::None }
            _ => AppAction::None,
        }
    }

    fn handle_controls_key(&mut self, key: KeyEvent) -> AppAction {
        // Text input mode
        if self.controls.text_input_active {
            return self.handle_controls_text_input(key);
        }

        match key.code {
            KeyCode::Tab => {
                self.controls.focus_section = (self.controls.focus_section + 1) % 6;
                self.controls.focus_item = 0;
                AppAction::None
            }
            KeyCode::BackTab => {
                self.controls.focus_section = if self.controls.focus_section == 0 { 5 } else { self.controls.focus_section - 1 };
                self.controls.focus_item = 0;
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.controls.focus_item = self.controls.focus_item.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.controls.focus_item += 1;
                AppAction::None
            }
            KeyCode::Enter => self.activate_control(),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                AppAction::ControlsExec("input keyevent 24".to_string())
            }
            KeyCode::Char('-') => {
                AppAction::ControlsExec("input keyevent 25".to_string())
            }
            KeyCode::Char(']') => {
                self.controls.brightness = (self.controls.brightness + 25).min(255);
                AppAction::ControlsExec(format!("settings put system screen_brightness {}", self.controls.brightness))
            }
            KeyCode::Char('[') => {
                self.controls.brightness = self.controls.brightness.saturating_sub(25);
                AppAction::ControlsExec(format!("settings put system screen_brightness {}", self.controls.brightness))
            }
            KeyCode::Char('i') => {
                self.controls.text_input_active = true;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_controls_text_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char(c) => {
                self.controls.text_input.insert(self.controls.text_cursor_pos, c);
                self.controls.text_cursor_pos += 1;
                AppAction::None
            }
            KeyCode::Backspace => {
                if self.controls.text_cursor_pos > 0 {
                    self.controls.text_cursor_pos -= 1;
                    self.controls.text_input.remove(self.controls.text_cursor_pos);
                }
                AppAction::None
            }
            KeyCode::Enter => {
                if !self.controls.text_input.is_empty() {
                    let escaped = self.controls.text_input.replace(' ', "%s").replace('\'', "\\'");
                    let cmd = format!("input text '{escaped}'");
                    self.controls.text_input.clear();
                    self.controls.text_cursor_pos = 0;
                    self.controls.text_input_active = false;
                    AppAction::ControlsExec(cmd)
                } else {
                    AppAction::None
                }
            }
            KeyCode::Esc => {
                self.controls.text_input_active = false;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn activate_control(&mut self) -> AppAction {
        // Map section + item to a command
        match self.controls.focus_section {
            0 => { // Power
                let (cmd, label) = match self.controls.focus_item {
                    0 => ("reboot", "Reboot"),
                    1 => ("reboot recovery", "Reboot Recovery"),
                    2 => ("reboot bootloader", "Reboot Bootloader"),
                    _ => return AppAction::None,
                };
                self.modal = ModalState::Confirm {
                    title: label.to_string(),
                    message: format!("Are you sure you want to {label}?"),
                    action: AppAction::ControlsExec(cmd.to_string()),
                    confirm_focused: false,
                };
                AppAction::None
            }
            1 => { // Screen
                match self.controls.focus_item {
                    0 => AppAction::ControlsExec("input keyevent 26".to_string()),
                    1 => AppAction::ControlsExec("input swipe 540 1800 540 800".to_string()),
                    2 => AppAction::ControlsExec("svc power stayon true".to_string()),
                    3 => AppAction::ControlsExec("svc power stayon false".to_string()),
                    _ => AppAction::None,
                }
            }
            2 => { // Connectivity
                match self.controls.focus_item {
                    0 => AppAction::ControlsExec("svc wifi enable".to_string()),
                    1 => AppAction::ControlsExec("svc wifi disable".to_string()),
                    2 => AppAction::ControlsExec("settings put global airplane_mode_on 1 && am broadcast -a android.intent.action.AIRPLANE_MODE".to_string()),
                    3 => AppAction::ControlsExec("settings put global airplane_mode_on 0 && am broadcast -a android.intent.action.AIRPLANE_MODE".to_string()),
                    _ => AppAction::None,
                }
            }
            3 => { // Audio & Display
                match self.controls.focus_item {
                    0 => AppAction::ControlsExec("input keyevent 24".to_string()),
                    1 => AppAction::ControlsExec("input keyevent 25".to_string()),
                    2 => AppAction::ControlsExec("input keyevent 164".to_string()),
                    _ => AppAction::None,
                }
            }
            4 => { // Text input
                self.controls.text_input_active = true;
                AppAction::None
            }
            5 => { // Hardware keys
                let keycode = match self.controls.focus_item {
                    0 => 3,   // HOME
                    1 => 4,   // BACK
                    2 => 82,  // MENU
                    3 => 187, // RECENT
                    4 => 85,  // PLAY
                    5 => 88,  // PREV
                    6 => 87,  // NEXT
                    7 => 27,  // CAM
                    _ => return AppAction::None,
                };
                AppAction::ControlsExec(format!("input keyevent {keycode}"))
            }
            _ => AppAction::None,
        }
    }

    fn handle_performance_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char('s') => {
                self.performance.is_monitoring = !self.performance.is_monitoring;
                if self.performance.is_monitoring {
                    self.performance.last_collect = None; // Force immediate collect
                    AppAction::PerfCollect
                } else {
                    AppAction::None
                }
            }
            KeyCode::Char('r') => {
                self.performance.refresh_rate = self.performance.refresh_rate.next();
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.performance.scroll_offset = self.performance.scroll_offset.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.performance.scroll_offset += 1;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_files_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.files.selected_index = self.files.selected_index.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.files.entries.is_empty() {
                    self.files.selected_index = (self.files.selected_index + 1).min(self.files.entries.len() - 1);
                }
                AppAction::None
            }
            KeyCode::Enter => {
                if let Some(entry) = self.files.entries.get(self.files.selected_index) {
                    if entry.is_directory {
                        let path = entry.path.clone();
                        return AppAction::FilesNavigate(path);
                    }
                }
                AppAction::None
            }
            KeyCode::Char(' ') => {
                if let Some(entry) = self.files.entries.get(self.files.selected_index) {
                    let path = entry.path.clone();
                    if self.files.selected_files.contains(&path) {
                        self.files.selected_files.remove(&path);
                    } else {
                        self.files.selected_files.insert(path);
                    }
                }
                AppAction::None
            }
            KeyCode::Backspace | KeyCode::Char('h') => {
                let parent = if self.files.current_path == "/" {
                    "/".to_string()
                } else {
                    let parts: Vec<&str> = self.files.current_path.rsplitn(2, '/').collect();
                    if parts.len() > 1 && !parts[1].is_empty() {
                        parts[1].to_string()
                    } else {
                        "/".to_string()
                    }
                };
                AppAction::FilesNavigate(parent)
            }
            KeyCode::Char('r') => AppAction::FilesRefresh,
            KeyCode::Char('d') => {
                let selected: Vec<String> = self.files.selected_files.iter().cloned().collect();
                if selected.is_empty() {
                    if let Some(entry) = self.files.entries.get(self.files.selected_index) {
                        let path = entry.path.clone();
                        self.modal = ModalState::Confirm {
                            title: "Delete".to_string(),
                            message: format!("Delete {}?", entry.name),
                            action: AppAction::FilesDelete(vec![path]),
                            confirm_focused: false,
                        };
                    }
                } else {
                    self.modal = ModalState::Confirm {
                        title: "Delete".to_string(),
                        message: format!("Delete {} selected items?", selected.len()),
                        action: AppAction::FilesDelete(selected),
                        confirm_focused: false,
                    };
                }
                AppAction::None
            }
            KeyCode::Char('p') => {
                let selected: Vec<String> = self.files.selected_files.iter().cloned().collect();
                if selected.is_empty() {
                    if let Some(entry) = self.files.entries.get(self.files.selected_index) {
                        return AppAction::FilesPull(vec![entry.path.clone()]);
                    }
                } else {
                    return AppAction::FilesPull(selected);
                }
                AppAction::None
            }
            KeyCode::Char('m') => {
                self.modal = ModalState::TextInput {
                    title: "New Folder".to_string(),
                    prompt: "Folder name:".to_string(),
                    value: String::new(),
                    cursor_pos: 0,
                    action_tag: "mkdir".to_string(),
                };
                AppAction::None
            }
            KeyCode::Char('a') => {
                if self.files.selected_files.len() == self.files.entries.len() {
                    self.files.selected_files.clear();
                } else {
                    for entry in &self.files.entries {
                        self.files.selected_files.insert(entry.path.clone());
                    }
                }
                AppAction::None
            }
            KeyCode::Char('1') => AppAction::FilesNavigate("/sdcard".to_string()),
            KeyCode::Char('2') => AppAction::FilesNavigate("/sdcard/Download".to_string()),
            KeyCode::Char('3') => AppAction::FilesNavigate("/sdcard/DCIM".to_string()),
            KeyCode::Char('4') => AppAction::FilesNavigate("/data/local/tmp".to_string()),
            _ => AppAction::None,
        }
    }

    fn handle_apps_key(&mut self, key: KeyEvent) -> AppAction {
        if self.apps.search_active {
            return self.handle_apps_search_input(key);
        }

        match key.code {
            KeyCode::Char('/') => { self.apps.search_active = true; AppAction::None }
            KeyCode::Char('f') => { self.apps.filter_type = self.apps.filter_type.next(); AppAction::None }
            KeyCode::Char('r') => AppAction::AppsRefresh,
            KeyCode::Tab | KeyCode::BackTab => {
                self.apps.focus_panel = match self.apps.focus_panel {
                    AppPanel::List => AppPanel::Detail,
                    AppPanel::Detail => AppPanel::List,
                };
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.apps.focus_panel {
                    AppPanel::List => {
                        self.apps.selected_index = self.apps.selected_index.saturating_sub(1);
                    }
                    AppPanel::Detail => {
                        self.apps.detail_scroll_offset = self.apps.detail_scroll_offset.saturating_sub(1);
                    }
                }
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.apps.focus_panel {
                    AppPanel::List => {
                        self.apps.selected_index += 1; // Clamped at render time
                    }
                    AppPanel::Detail => {
                        self.apps.detail_scroll_offset += 1;
                    }
                }
                AppAction::None
            }
            KeyCode::Enter => {
                if self.apps.focus_panel == AppPanel::List {
                    // Load details for selected package
                    let filtered = self.filtered_packages();
                    if let Some(pkg) = filtered.get(self.apps.selected_index) {
                        let name = pkg.package_name.clone();
                        self.apps.detail_package = Some(name.clone());
                        return AppAction::AppsLoadDetails(name);
                    }
                }
                AppAction::None
            }
            KeyCode::Char('o') => {
                if let Some(ref name) = self.apps.detail_package {
                    return AppAction::AppsOpen(name.clone());
                }
                AppAction::None
            }
            KeyCode::Char('x') => {
                if let Some(ref name) = self.apps.detail_package {
                    return AppAction::AppsStop(name.clone());
                }
                AppAction::None
            }
            KeyCode::Char('c') => {
                if let Some(ref name) = self.apps.detail_package {
                    return AppAction::AppsClear(name.clone());
                }
                AppAction::None
            }
            KeyCode::Char('u') => {
                if let Some(ref name) = self.apps.detail_package {
                    let name = name.clone();
                    self.modal = ModalState::Confirm {
                        title: "Uninstall".to_string(),
                        message: format!("Uninstall {name}?"),
                        action: AppAction::AppsUninstall(name),
                        confirm_focused: false,
                    };
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_apps_search_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char(c) => { self.apps.search_query.push(c); AppAction::None }
            KeyCode::Backspace => { self.apps.search_query.pop(); AppAction::None }
            KeyCode::Enter | KeyCode::Esc => { self.apps.search_active = false; AppAction::None }
            _ => AppAction::None,
        }
    }

    /// Get filtered package list based on search and filter.
    pub fn filtered_packages(&self) -> Vec<&PackageInfo> {
        self.apps.packages.iter().filter(|p| {
            let type_match = match self.apps.filter_type {
                AppFilter::All => true,
                AppFilter::User => !p.is_system,
                AppFilter::System => p.is_system,
            };
            let search_match = self.apps.search_query.is_empty()
                || p.package_name.to_lowercase().contains(&self.apps.search_query.to_lowercase());
            type_match && search_match
        }).collect()
    }

    fn handle_settings_key(&mut self, key: KeyEvent) -> AppAction {
        if self.settings.search_active {
            return self.handle_settings_search_input(key);
        }

        match key.code {
            KeyCode::Char('n') => {
                self.settings.namespace = self.settings.namespace.next();
                self.settings.selected_index = 0;
                self.settings.scroll_offset = 0;
                AppAction::SettingsLoad
            }
            KeyCode::Char('/') => { self.settings.search_active = true; AppAction::None }
            KeyCode::Char('r') => AppAction::SettingsLoad,
            KeyCode::Tab | KeyCode::BackTab => {
                self.settings.focus_area = match self.settings.focus_area {
                    SettingsFocus::QuickToggles => SettingsFocus::List,
                    SettingsFocus::List => SettingsFocus::QuickToggles,
                };
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.settings.focus_area {
                    SettingsFocus::QuickToggles => {
                        self.settings.quick_toggle_focus = self.settings.quick_toggle_focus.saturating_sub(1);
                    }
                    SettingsFocus::List => {
                        self.settings.selected_index = self.settings.selected_index.saturating_sub(1);
                    }
                }
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.settings.focus_area {
                    SettingsFocus::QuickToggles => {
                        self.settings.quick_toggle_focus = (self.settings.quick_toggle_focus + 1).min(QUICK_TOGGLES.len() - 1);
                    }
                    SettingsFocus::List => {
                        self.settings.selected_index += 1; // Clamped at render time
                    }
                }
                AppAction::None
            }
            KeyCode::Char(' ') => {
                if self.settings.focus_area == SettingsFocus::QuickToggles {
                    return AppAction::SettingsToggle(self.settings.quick_toggle_focus);
                }
                AppAction::None
            }
            KeyCode::Char('e') | KeyCode::Enter => {
                if self.settings.focus_area == SettingsFocus::List {
                    let filtered = self.filtered_settings();
                    if let Some(entry) = filtered.get(self.settings.selected_index) {
                        let ns = self.settings.namespace.arg().to_string();
                        let key = entry.key.clone();
                        let value = entry.value.clone();
                        self.modal = ModalState::TextInput {
                            title: "Edit Setting".to_string(),
                            prompt: format!("{ns}/{key}"),
                            value: format!("{ns}:{key}:{value}"),
                            cursor_pos: ns.len() + key.len() + value.len() + 2,
                            action_tag: "settings_edit".to_string(),
                        };
                    }
                }
                AppAction::None
            }
            KeyCode::Char('d') => {
                if self.settings.focus_area == SettingsFocus::List {
                    let filtered = self.filtered_settings();
                    if let Some(entry) = filtered.get(self.settings.selected_index) {
                        let ns = self.settings.namespace.arg().to_string();
                        let key = entry.key.clone();
                        self.modal = ModalState::Confirm {
                            title: "Delete Setting".to_string(),
                            message: format!("Delete {ns}/{key}?"),
                            action: AppAction::SettingsDelete(ns, key),
                            confirm_focused: false,
                        };
                    }
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_settings_search_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char(c) => { self.settings.search_query.push(c); AppAction::None }
            KeyCode::Backspace => { self.settings.search_query.pop(); AppAction::None }
            KeyCode::Enter | KeyCode::Esc => { self.settings.search_active = false; AppAction::None }
            _ => AppAction::None,
        }
    }

    /// Get filtered settings list based on search.
    pub fn filtered_settings(&self) -> Vec<&SettingEntry> {
        if self.settings.search_query.is_empty() {
            self.settings.settings.iter().collect()
        } else {
            let q = self.settings.search_query.to_lowercase();
            self.settings.settings.iter().filter(|s| {
                s.key.to_lowercase().contains(&q) || s.value.to_lowercase().contains(&q)
            }).collect()
        }
    }

    fn handle_bugreport_key(&mut self, key: KeyEvent) -> AppAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.bugreport.is_generating {
                return AppAction::BugreportCancel;
            }
            return AppAction::None;
        }

        match key.code {
            KeyCode::Char('g') | KeyCode::Enter => {
                if !self.bugreport.is_generating {
                    AppAction::BugreportStart
                } else {
                    AppAction::None
                }
            }
            KeyCode::Char('c') => {
                if self.bugreport.is_generating {
                    AppAction::BugreportCancel
                } else {
                    AppAction::None
                }
            }
            KeyCode::Char('d') => {
                if !self.bugreport.history.is_empty() {
                    AppAction::BugreportDownload(self.bugreport.selected_index)
                } else {
                    AppAction::None
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.bugreport.selected_index = self.bugreport.selected_index.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.bugreport.history.is_empty() {
                    self.bugreport.selected_index = (self.bugreport.selected_index + 1).min(self.bugreport.history.len() - 1);
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_screen_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char('1') => { self.screen.active_tab = ScreenTab::Screenshot; AppAction::None }
            KeyCode::Char('2') => { self.screen.active_tab = ScreenTab::Record; AppAction::None }
            KeyCode::Char('c') | KeyCode::Enter => {
                match self.screen.active_tab {
                    ScreenTab::Screenshot => {
                        if !self.screen.is_capturing {
                            AppAction::ScreenCapture
                        } else {
                            AppAction::None
                        }
                    }
                    ScreenTab::Record => {
                        if self.screen.is_recording {
                            AppAction::ScreenRecordStop
                        } else {
                            AppAction::ScreenRecordStart
                        }
                    }
                }
            }
            KeyCode::Char('d') => {
                if self.screen.active_tab == ScreenTab::Record {
                    self.screen.record_duration = self.screen.record_duration.next();
                }
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.screen.active_tab {
                    ScreenTab::Screenshot => {
                        self.screen.capture_selected = self.screen.capture_selected.saturating_sub(1);
                    }
                    ScreenTab::Record => {
                        self.screen.recording_selected = self.screen.recording_selected.saturating_sub(1);
                    }
                }
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.screen.active_tab {
                    ScreenTab::Screenshot => {
                        if !self.screen.captures.is_empty() {
                            self.screen.capture_selected = (self.screen.capture_selected + 1).min(self.screen.captures.len() - 1);
                        }
                    }
                    ScreenTab::Record => {
                        if !self.screen.recordings.is_empty() {
                            self.screen.recording_selected = (self.screen.recording_selected + 1).min(self.screen.recordings.len() - 1);
                        }
                    }
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    // ── Page lifecycle ───────────────────────────────────────────

    /// Called when leaving a page — stop streams that shouldn't run in background.
    pub fn on_page_leave(&mut self, old_page: Page) {
        match old_page {
            Page::Shell => {
                // Stop shell streaming
                self.shell.is_streaming = false;
                self.shell.is_running = false;
                self.shell.stream_rx = None;
            }
            Page::Logcat => {
                // Stop logcat streaming
                self.logcat.is_streaming = false;
                self.logcat.stream_rx = None;
            }
            Page::Performance => {
                self.performance.is_monitoring = false;
            }
            // Bugreport continues in background
            _ => {}
        }
        // Kill any lingering child processes
        for child in self.stream_children.drain(..) {
            drop(child); // kill_on_drop=true
        }
    }

    // ── Stream draining ──────────────────────────────────────────

    /// Drain shell output from the streaming channel.
    pub fn drain_shell_output(&mut self) {
        if let Some(ref mut rx) = self.shell.stream_rx {
            let mut count = 0;
            while let Ok(line) = rx.try_recv() {
                self.shell.output.push(ShellOutputEntry {
                    entry_type: ShellEntryType::Output,
                    content: line,
                });
                count += 1;
                if count >= 100 { break; } // Batch limit per tick
            }
            // Trim output
            if self.shell.output.len() > 5000 {
                let drain = self.shell.output.len() - 5000;
                self.shell.output.drain(..drain);
            }
        }
    }

    /// Drain logcat lines from the streaming channel.
    pub fn drain_logcat_lines(&mut self) {
        if let Some(ref mut rx) = self.logcat.stream_rx {
            let mut count = 0;
            while let Ok(line) = rx.try_recv() {
                if let Some(entry) = parser::parse_logcat_line(&line) {
                    self.logcat.logs.push(entry);
                }
                count += 1;
                if count >= 200 { break; }
            }
            // Trim
            if self.logcat.logs.len() > 5000 {
                let drain = self.logcat.logs.len() - 5000;
                self.logcat.logs.drain(..drain);
                if self.logcat.scroll_offset > drain {
                    self.logcat.scroll_offset -= drain;
                } else {
                    self.logcat.scroll_offset = 0;
                }
            }
        }
    }

    /// Drain bugreport progress from the streaming channel.
    pub fn drain_bugreport_progress(&mut self) {
        // Collect lines first to avoid borrow conflict
        let lines: Vec<String> = if let Some(ref mut rx) = self.bugreport.stream_rx {
            let mut collected = Vec::new();
            while let Ok(line) = rx.try_recv() {
                collected.push(line);
            }
            collected
        } else {
            return;
        };

        let mut should_close = false;
        for line in lines {
            self.bugreport.raw_output.push_str(&line);
            self.bugreport.raw_output.push('\n');

            // Parse progress: look for N/M pattern
            if let Some(slash) = line.find('/') {
                let before = &line[..slash];
                let after = &line[slash + 1..];
                if let (Some(n), Some(m)) = (
                    before.split_whitespace().last().and_then(|s| s.parse::<u32>().ok()),
                    after.split_whitespace().next().and_then(|s| s.parse::<u32>().ok()),
                ) {
                    if m > 0 {
                        self.bugreport.progress = ((n as f64 / m as f64) * 100.0) as u8;
                    }
                }
            }

            // Check for completion
            if line.contains("OK:") {
                if let Some(path) = line.split("OK:").nth(1) {
                    let path = path.trim().to_string();
                    self.bugreport.is_generating = false;
                    self.bugreport.progress = 100;
                    if let Some(entry) = self.bugreport.history.last_mut() {
                        entry.status = BugreportStatus::Completed;
                        entry.end_time = Some(Instant::now());
                        entry.device_path = Some(path);
                    }
                    should_close = true;
                }
            } else if line.contains("FAIL") {
                self.bugreport.is_generating = false;
                if let Some(entry) = self.bugreport.history.last_mut() {
                    entry.status = BugreportStatus::Failed;
                    entry.end_time = Some(Instant::now());
                    entry.error = Some(line.clone());
                }
                should_close = true;
            }
        }

        if should_close {
            self.bugreport.stream_rx = None;
        }
    }

    /// Update screen recording elapsed time.
    pub fn update_screen_recording(&mut self) {
        if self.screen.is_recording {
            if let Some(start) = self.screen.record_start {
                self.screen.record_elapsed = start.elapsed().as_secs() as u32;
                if self.screen.record_elapsed >= self.screen.record_duration.secs() {
                    self.screen.is_recording = false;
                    self.screen.stream_rx = None;
                }
            }
        }
    }

    // ── Async operations ─────────────────────────────────────────

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
        if self.dashboard.last_refresh.is_none() && !self.dashboard.loading {
            return true;
        }
        if let Some(interval_secs) = self.dashboard.auto_refresh.duration_secs() {
            if let Some(last) = self.dashboard.last_refresh {
                return last.elapsed().as_secs() >= interval_secs && !self.dashboard.loading;
            }
        }
        false
    }

    /// Check if performance data collection is due.
    pub fn perf_needs_collect(&self) -> bool {
        if self.page != Page::Performance || !self.performance.is_monitoring || !self.device_manager.is_connected() {
            return false;
        }
        match self.performance.last_collect {
            None => true,
            Some(last) => last.elapsed().as_secs() >= self.performance.refresh_rate.secs(),
        }
    }

    /// Collect performance data from device.
    pub async fn collect_perf_data(&mut self) {
        if !self.device_manager.is_connected() {
            return;
        }

        let (top_raw, mem_raw, bat_raw) = tokio::join!(
            self.device_manager.client.shell("top -n 1 -b -m 10"),
            self.device_manager.client.shell("cat /proc/meminfo"),
            self.device_manager.client.shell("dumpsys battery"),
        );

        // CPU + processes
        if let Ok(top_out) = top_raw {
            let (cpu_pct, procs) = parser::parse_top_output(&top_out);
            self.performance.cpu_history.push(cpu_pct);
            if self.performance.cpu_history.len() > 60 {
                self.performance.cpu_history.remove(0);
            }
            self.performance.processes = procs;
        }

        // Memory
        if let Ok(mem_out) = mem_raw {
            let (total_kb, used_kb) = parser::parse_meminfo(&mem_out);
            self.performance.mem_total_kb = total_kb;
            self.performance.mem_used_kb = used_kb;
            let pct = if total_kb > 0 { used_kb as f64 / total_kb as f64 * 100.0 } else { 0.0 };
            self.performance.mem_history.push(pct);
            if self.performance.mem_history.len() > 60 {
                self.performance.mem_history.remove(0);
            }
        }

        // Battery
        if let Ok(bat_out) = bat_raw {
            self.performance.battery = Some(parser::parse_battery(&bat_out));
        }

        self.performance.last_collect = Some(Instant::now());
    }

    /// Dispatch an AppAction asynchronously.
    /// Navigate to a directory in the files page.
    async fn navigate_files(&mut self, path: &str) {
        self.files.loading = true;
        self.files.error = None;
        self.files.selected_files.clear();
        self.files.selected_index = 0;
        match self.device_manager.client.shell(&format!("ls -la \"{path}\"")).await {
            Ok(output) => {
                self.files.entries = parser::parse_ls_output(&output, path);
                self.files.current_path = path.to_string();
            }
            Err(e) => {
                self.files.error = Some(e.to_string());
            }
        }
        self.files.loading = false;
    }

    /// Refresh the package list for the apps page.
    async fn refresh_packages(&mut self) {
        self.apps.loading = true;
        match self.device_manager.client.shell("pm list packages -f").await {
            Ok(output) => {
                self.apps.packages = parser::parse_package_list(&output);
            }
            Err(e) => {
                tracing::error!("Failed to list packages: {e}");
            }
        }
        self.apps.loading = false;
    }

    /// Load settings list and quick toggle states.
    async fn load_settings(&mut self) {
        self.settings.loading = true;
        let ns = self.settings.namespace.arg();
        match self.device_manager.client.shell(&format!("settings list {ns}")).await {
            Ok(output) => {
                self.settings.settings = parser::parse_settings_list(&output);
            }
            Err(e) => {
                tracing::error!("Failed to load settings: {e}");
            }
        }
        for (i, toggle) in QUICK_TOGGLES.iter().enumerate() {
            if let Ok(val) = self.device_manager.client.shell(&format!("settings get {} {}", toggle.namespace, toggle.key)).await {
                self.settings.quick_toggle_states[i] = val.trim() == toggle.enable_value;
            }
        }
        self.settings.loading = false;
    }

    pub async fn dispatch_action(&mut self, action: AppAction) {
        if !self.device_manager.is_connected() {
            return;
        }

        match action {
            AppAction::None => {}

            // Shell
            AppAction::ShellExecute(cmd) => {
                self.shell.is_running = true;
                let timeout = std::time::Duration::from_secs(self.shell.timeout.secs());
                match tokio::time::timeout(timeout, self.device_manager.client.shell(&cmd)).await {
                    Ok(Ok(output)) => {
                        for line in output.lines() {
                            self.shell.output.push(ShellOutputEntry {
                                entry_type: ShellEntryType::Output,
                                content: line.to_string(),
                            });
                        }
                    }
                    Ok(Err(e)) => {
                        self.shell.output.push(ShellOutputEntry {
                            entry_type: ShellEntryType::Error,
                            content: e.to_string(),
                        });
                    }
                    Err(_) => {
                        self.shell.output.push(ShellOutputEntry {
                            entry_type: ShellEntryType::Error,
                            content: "Command timed out".to_string(),
                        });
                    }
                }
                self.shell.is_running = false;
            }
            AppAction::ShellExecuteStream(cmd) => {
                let (tx, rx) = mpsc::unbounded_channel();
                match self.device_manager.client.shell_stream(&cmd, tx).await {
                    Ok(child) => {
                        self.shell.stream_rx = Some(rx);
                        self.shell.is_streaming = true;
                        self.shell.is_running = true;
                        self.stream_children.push(child);
                    }
                    Err(e) => {
                        self.shell.output.push(ShellOutputEntry {
                            entry_type: ShellEntryType::Error,
                            content: e.to_string(),
                        });
                    }
                }
            }
            AppAction::ShellStop => {
                self.shell.is_streaming = false;
                self.shell.is_running = false;
                self.shell.stream_rx = None;
                for child in self.stream_children.drain(..) {
                    drop(child);
                }
            }

            // Logcat
            AppAction::LogcatStart => {
                let buf_arg = self.logcat.buffer.arg();
                let cmd = format!("logcat -v threadtime -b {buf_arg}");
                let (tx, rx) = mpsc::unbounded_channel();
                match self.device_manager.client.shell_stream(&cmd, tx).await {
                    Ok(child) => {
                        self.logcat.stream_rx = Some(rx);
                        self.logcat.is_streaming = true;
                        self.stream_children.push(child);
                    }
                    Err(e) => {
                        tracing::error!("Failed to start logcat: {e}");
                    }
                }
            }
            AppAction::LogcatStop => {
                self.logcat.is_streaming = false;
                self.logcat.stream_rx = None;
                for child in self.stream_children.drain(..) {
                    drop(child);
                }
            }

            // Performance
            AppAction::PerfCollect => {
                self.collect_perf_data().await;
            }

            // Files
            AppAction::FilesNavigate(path) => {
                self.navigate_files(&path).await;
            }
            AppAction::FilesRefresh => {
                let path = self.files.current_path.clone();
                self.navigate_files(&path).await;
            }
            AppAction::FilesDelete(paths) => {
                for path in &paths {
                    let _ = self.device_manager.client.shell(&format!("rm -rf \"{path}\"")).await;
                }
                self.files.selected_files.clear();
                let path = self.files.current_path.clone();
                self.navigate_files(&path).await;
            }
            AppAction::FilesPull(paths) => {
                for path in &paths {
                    let filename = path.rsplit('/').next().unwrap_or("file");
                    let local = format!("./{filename}");
                    match self.device_manager.client.pull(path, &local).await {
                        Ok(_) => {
                            self.controls.result = Some((true, format!("Pulled {filename}")));
                            self.controls.result_timer = Some(Instant::now());
                        }
                        Err(e) => {
                            tracing::error!("Pull failed: {e}");
                        }
                    }
                }
            }
            AppAction::FilesMkdir(path) => {
                let _ = self.device_manager.client.shell(&format!("mkdir -p \"{path}\"")).await;
                let parent = self.files.current_path.clone();
                self.navigate_files(&parent).await;
            }

            // Apps
            AppAction::AppsRefresh => {
                self.refresh_packages().await;
            }
            AppAction::AppsLoadDetails(name) => {
                self.apps.detail_loading = true;
                match self.device_manager.client.shell(&format!("dumpsys package {name}")).await {
                    Ok(output) => {
                        self.apps.package_details = Some(parser::parse_package_details(&output, &name));
                    }
                    Err(e) => {
                        tracing::error!("Failed to get package details: {e}");
                    }
                }
                self.apps.detail_loading = false;
            }
            AppAction::AppsOpen(name) => {
                let _ = self.device_manager.client.shell(&format!("monkey -p {name} -c android.intent.category.LAUNCHER 1")).await;
                self.apps.action_result = Some((true, format!("Opened {name}"), Instant::now()));
            }
            AppAction::AppsStop(name) => {
                let _ = self.device_manager.client.shell(&format!("am force-stop {name}")).await;
                self.apps.action_result = Some((true, format!("Stopped {name}"), Instant::now()));
            }
            AppAction::AppsClear(name) => {
                let _ = self.device_manager.client.shell(&format!("pm clear {name}")).await;
                self.apps.action_result = Some((true, format!("Cleared {name}"), Instant::now()));
            }
            AppAction::AppsUninstall(name) => {
                match self.device_manager.client.shell(&format!("pm uninstall {name}")).await {
                    Ok(output) => {
                        let success = output.contains("Success");
                        self.apps.action_result = Some((success, format!("Uninstall: {}", output.trim()), Instant::now()));
                        if success {
                            self.refresh_packages().await;
                        }
                    }
                    Err(e) => {
                        self.apps.action_result = Some((false, e.to_string(), Instant::now()));
                    }
                }
            }

            // Controls
            AppAction::ControlsExec(cmd) => {
                self.controls.loading = Some(cmd.clone());
                match self.device_manager.client.shell(&cmd).await {
                    Ok(output) => {
                        let msg = if output.trim().is_empty() { "OK".to_string() } else { output.trim().to_string() };
                        self.controls.result = Some((true, msg));
                    }
                    Err(e) => {
                        self.controls.result = Some((false, e.to_string()));
                    }
                }
                self.controls.result_timer = Some(Instant::now());
                self.controls.loading = None;
            }

            // Settings
            AppAction::SettingsLoad => {
                self.load_settings().await;
            }
            AppAction::SettingsPut(ns, key, value) => {
                let _ = self.device_manager.client.shell(&format!("settings put {ns} {key} {value}")).await;
                self.load_settings().await;
            }
            AppAction::SettingsDelete(ns, key) => {
                let _ = self.device_manager.client.shell(&format!("settings delete {ns} {key}")).await;
                self.load_settings().await;
            }
            AppAction::SettingsToggle(idx) => {
                if let Some(toggle) = QUICK_TOGGLES.get(idx) {
                    let current = self.settings.quick_toggle_states[idx];
                    let new_val = if current { toggle.disable_value } else { toggle.enable_value };
                    let _ = self.device_manager.client.shell(&format!("settings put {} {} {new_val}", toggle.namespace, toggle.key)).await;
                    self.settings.quick_toggle_states[idx] = !current;
                }
            }

            // Bugreport
            AppAction::BugreportStart => {
                let (tx, rx) = mpsc::unbounded_channel();
                match self.device_manager.client.shell_stream("bugreportz", tx).await {
                    Ok(child) => {
                        self.bugreport.stream_rx = Some(rx);
                        self.bugreport.is_generating = true;
                        self.bugreport.progress = 0;
                        self.bugreport.start_time = Some(Instant::now());
                        self.bugreport.raw_output.clear();
                        self.bugreport.history.push(BugreportEntry {
                            filename: format!("bugreport-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")),
                            status: BugreportStatus::Generating,
                            start_time: Instant::now(),
                            end_time: None,
                            device_path: None,
                            local_path: None,
                            error: None,
                        });
                        self.stream_children.push(child);
                    }
                    Err(e) => {
                        tracing::error!("Failed to start bugreport: {e}");
                    }
                }
            }
            AppAction::BugreportCancel => {
                self.bugreport.is_generating = false;
                self.bugreport.stream_rx = None;
                if let Some(entry) = self.bugreport.history.last_mut() {
                    entry.status = BugreportStatus::Cancelled;
                    entry.end_time = Some(Instant::now());
                }
                for child in self.stream_children.drain(..) {
                    drop(child);
                }
            }
            AppAction::BugreportDownload(idx) => {
                if let Some(entry) = self.bugreport.history.get_mut(idx) {
                    if let Some(ref device_path) = entry.device_path {
                        let local = format!("./{}.zip", entry.filename);
                        match self.device_manager.client.pull(device_path, &local).await {
                            Ok(_) => {
                                entry.local_path = Some(local);
                            }
                            Err(e) => {
                                tracing::error!("Bugreport download failed: {e}");
                            }
                        }
                    }
                }
            }

            // Screen
            AppAction::ScreenCapture => {
                self.screen.is_capturing = true;
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
                let device_path = "/sdcard/adbwrench_screenshot.png";
                let local_path = format!("./screenshot-{timestamp}.png");

                let result = async {
                    self.device_manager.client.shell(&format!("screencap -p {device_path}")).await?;
                    self.device_manager.client.pull(device_path, &local_path).await?;
                    self.device_manager.client.shell(&format!("rm {device_path}")).await?;
                    Ok::<(), anyhow::Error>(())
                }.await;

                match result {
                    Ok(()) => {
                        self.screen.captures.push(ScreenCapture {
                            filename: format!("screenshot-{timestamp}.png"),
                            local_path,
                            timestamp,
                        });
                    }
                    Err(e) => {
                        self.screen.error = Some(e.to_string());
                    }
                }
                self.screen.is_capturing = false;
            }
            AppAction::ScreenRecordStart => {
                let duration = self.screen.record_duration.secs();
                let cmd = format!("screenrecord --time-limit {duration} /sdcard/adbwrench_recording.mp4");
                let (tx, rx) = mpsc::unbounded_channel();
                match self.device_manager.client.shell_stream(&cmd, tx).await {
                    Ok(child) => {
                        self.screen.stream_rx = Some(rx);
                        self.screen.is_recording = true;
                        self.screen.record_start = Some(Instant::now());
                        self.screen.record_elapsed = 0;
                        self.stream_children.push(child);
                    }
                    Err(e) => {
                        self.screen.error = Some(e.to_string());
                    }
                }
            }
            AppAction::ScreenRecordStop => {
                self.screen.is_recording = false;
                self.screen.stream_rx = None;
                for child in self.stream_children.drain(..) {
                    drop(child);
                }
                // Pull the recording
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
                let local_path = format!("./recording-{timestamp}.mp4");
                if let Ok(_) = self.device_manager.client.pull("/sdcard/adbwrench_recording.mp4", &local_path).await {
                    let _ = self.device_manager.client.shell("rm /sdcard/adbwrench_recording.mp4").await;
                    self.screen.recordings.push(RecordingEntry {
                        filename: format!("recording-{timestamp}.mp4"),
                        local_path,
                        duration_secs: self.screen.record_elapsed,
                        timestamp,
                    });
                }
            }
        }
    }

    /// Clear auto-clearing result messages.
    pub fn clear_stale_results(&mut self) {
        // Controls result
        if let Some(timer) = self.controls.result_timer {
            if timer.elapsed().as_secs() >= 3 {
                self.controls.result = None;
                self.controls.result_timer = None;
            }
        }
        // Apps action result
        if let Some((_, _, time)) = &self.apps.action_result {
            if time.elapsed().as_secs() >= 3 {
                self.apps.action_result = None;
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
