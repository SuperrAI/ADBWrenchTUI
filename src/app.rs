use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use tokio::sync::mpsc;

use crate::adb::DeviceManager;
use crate::adb::parser;
use crate::adb::types::*;
use crate::components::list_viewport;
use crate::config::AppConfig;

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

/// Performance data refresh interval in seconds.
const PERF_REFRESH_SECS: u64 = 2;
/// Device scan interval in seconds (for reconnect/hot-plug detection).
const DEVICE_SCAN_SECS: u64 = 3;
/// Max number of logcat lines kept in memory.
const LOGCAT_MAX_LINES: usize = 2000;
/// Max number of newest logcat lines parsed per drain cycle.
const LOGCAT_PARSE_BATCH: usize = 1000;
/// Bounded channel capacity for incoming logcat lines.
const LOGCAT_CHANNEL_CAPACITY: usize = 4096;
/// Max raw logcat lines to receive from channel per background cycle.
const LOGCAT_RECV_LIMIT: usize = 4096;
/// Sidebar width used by UI layout (must match `ui::SIDEBAR_WIDTH`).
const UI_SIDEBAR_WIDTH: u16 = 26;

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

/// Dashboard focus section for Tab cycling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardSection {
    Device,
    Hardware,
    Software,
    Processes,
}

impl DashboardSection {
    pub fn next(self) -> Self {
        match self {
            Self::Device => Self::Hardware,
            Self::Hardware => Self::Software,
            Self::Software => Self::Processes,
            Self::Processes => Self::Device,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Device => Self::Processes,
            Self::Hardware => Self::Device,
            Self::Software => Self::Hardware,
            Self::Processes => Self::Software,
        }
    }
}

/// Dashboard-specific state.
pub struct DashboardState {
    pub loading: bool,
    pub last_refresh: Option<Instant>,
    pub auto_refresh: RefreshInterval,
    pub error: Option<String>,
    pub focus_section: DashboardSection,
    pub focus_item: usize,
    pub copied_feedback: Option<(String, Instant)>,
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
    pub stream_child: Option<tokio::process::Child>,
}

/// Logcat focus area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogcatFocus {
    Controls,
    Logs,
}

/// Logcat control bar items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogcatControl {
    StartStop,
    Buffer,
    Search,
    Tag,
    LevelV,
    LevelD,
    LevelI,
    LevelW,
    LevelE,
    LevelF,
    Timestamp,
    AutoScroll,
    Clear,
}

impl LogcatControl {
    pub const ALL: &[LogcatControl] = &[
        Self::StartStop,
        Self::Buffer,
        Self::Search,
        Self::Tag,
        Self::LevelV,
        Self::LevelD,
        Self::LevelI,
        Self::LevelW,
        Self::LevelE,
        Self::LevelF,
        Self::Timestamp,
        Self::AutoScroll,
        Self::Clear,
    ];

    pub fn next_idx(idx: usize) -> usize {
        (idx + 1) % Self::ALL.len()
    }

    pub fn prev_idx(idx: usize) -> usize {
        if idx == 0 {
            Self::ALL.len() - 1
        } else {
            idx - 1
        }
    }
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
    pub stream_rx: Option<mpsc::Receiver<String>>,
    pub stream_child: Option<tokio::process::Child>,
    pub dropped_counter: Option<Arc<AtomicUsize>>,
    pub dropped_lines: usize,
    pub focus: LogcatFocus,
    pub control_index: usize,
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
    // Toggle states
    pub stay_awake: bool,
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub airplane_mode: bool,
}

/// Performance page state.
pub struct PerfState {
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
    pub result: Option<(bool, String, Instant)>,
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
    pub desc: &'static str,
    pub namespace: &'static str,
    pub key: &'static str,
    pub enable_value: &'static str,
    pub disable_value: &'static str,
}

pub const QUICK_TOGGLES: &[QuickToggle] = &[
    QuickToggle {
        name: "WIRELESS ADB",
        desc: "Debug over network",
        namespace: "global",
        key: "adb_wifi_enabled",
        enable_value: "1",
        disable_value: "0",
    },
    QuickToggle {
        name: "SHOW TOUCHES",
        desc: "Visual feedback",
        namespace: "system",
        key: "show_touches",
        enable_value: "1",
        disable_value: "0",
    },
    QuickToggle {
        name: "POINTER LOC",
        desc: "Coordinate overlay",
        namespace: "system",
        key: "pointer_location",
        enable_value: "1",
        disable_value: "0",
    },
    QuickToggle {
        name: "STAY AWAKE",
        desc: "Screen on while charging",
        namespace: "global",
        key: "stay_on_while_plugged_in",
        enable_value: "3",
        disable_value: "0",
    },
    QuickToggle {
        name: "GPU DEBUG",
        desc: "Force GPU debug layers",
        namespace: "global",
        key: "enable_gpu_debug_layers",
        enable_value: "1",
        disable_value: "0",
    },
    QuickToggle {
        name: "ANIM SCALE",
        desc: "Window animation scale",
        namespace: "global",
        key: "animator_duration_scale",
        enable_value: "1.0",
        disable_value: "0",
    },
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
    pub stream_child: Option<tokio::process::Child>,
    pub raw_output: String,
}

/// Screen capture entry.
#[derive(Debug, Clone)]
pub struct ScreenCapture {
    pub filename: String,
    pub timestamp: String,
}

/// Screen recording entry.
#[derive(Debug, Clone)]
pub struct RecordingEntry {
    pub filename: String,
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
    pub stream_child: Option<tokio::process::Child>,
    pub error: Option<String>,
    /// Image protocol picker for terminal image rendering.
    pub picker: Option<Picker>,
    /// Current preview image state (needs interior mutability for StatefulImage rendering).
    pub preview_state: RefCell<Option<StatefulProtocol>>,
    /// Filename of the currently loaded preview (to avoid redundant reloads).
    pub preview_filename: Option<String>,
    /// Whether the output path input is being edited.
    pub path_input_active: bool,
    /// Current path input buffer.
    pub path_input: String,
    /// Transient status message shown in header (e.g. "SAVING RECORDING...").
    pub status: Option<String>,
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
    ShellStop,
    // Logcat
    LogcatStart,
    LogcatStop,
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
    Bugreport,
    Settings,
    About,
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
        Page::Bugreport,
        Page::Settings,
        Page::About,
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
            Self::Bugreport => "Bugreport",
            Self::Settings => "Settings",
            Self::About => "About",
        }
    }

    /// Section headers for the sidebar nav groups.
    pub fn section(&self) -> &str {
        match self {
            Self::Dashboard => "MAIN",
            Self::Shell | Self::Logcat | Self::Screen | Self::Apps | Self::Files => "TOOLS",
            Self::Controls | Self::Bugreport | Self::Settings => "SYSTEM",
            Self::About => "INFO",
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
            Self::Bugreport => '8',
            Self::Settings => '9',
            Self::About => '0',
        }
    }
}

/// Focus region — whether the user is interacting with the sidebar or main content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Content,
}

/// Hover metadata for mouse-over visual affordances and contextual hint text.
#[derive(Debug, Clone, Default)]
pub struct HoverState {
    pub sidebar_page: Option<usize>,
    pub logcat_control: Option<usize>,
    pub screen_tab: Option<ScreenTab>,
    pub apps_filter: Option<AppFilter>,
    pub settings_namespace: Option<SettingsNamespace>,
    pub settings_quick_toggle: Option<usize>,
    pub hint: Option<String>,
}

// ── App struct ───────────────────────────────────────────────────

/// Top-level application state.
pub struct App {
    pub running: bool,
    pub page: Page,
    pub sidebar_index: usize,
    pub focus: Focus,
    pub hover: HoverState,
    pub device_manager: DeviceManager,
    pub modal: ModalState,
    pub config: AppConfig,
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
    // Pending action from modal confirmations
    pub pending_action: Option<AppAction>,
    // Last time we scanned adb devices for hot-plug/reconnect handling
    pub last_device_scan: Option<Instant>,
}

fn char_count(s: &str) -> usize {
    s.chars().count()
}

fn byte_idx_from_char_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or_else(|| s.len())
}

fn insert_char_at(s: &mut String, char_idx: usize, c: char) {
    let byte_idx = byte_idx_from_char_idx(s, char_idx);
    s.insert(byte_idx, c);
}

fn remove_char_before(s: &mut String, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    let start = byte_idx_from_char_idx(s, char_idx - 1);
    let end = byte_idx_from_char_idx(s, char_idx);
    s.replace_range(start..end, "");
    char_idx - 1
}

impl App {
    pub fn new(picker: Option<Picker>) -> Self {
        let config = AppConfig::load();
        Self {
            running: true,
            page: Page::Dashboard,
            sidebar_index: 0,
            config,
            focus: Focus::Sidebar,
            hover: HoverState::default(),
            device_manager: DeviceManager::new(),
            modal: ModalState::None,
            dashboard: DashboardState {
                loading: false,
                last_refresh: None,
                auto_refresh: RefreshInterval::Seconds10,
                error: None,
                focus_section: DashboardSection::Device,
                focus_item: 0,
                copied_feedback: None,
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
                stream_child: None,
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
                stream_child: None,
                dropped_counter: None,
                dropped_lines: 0,
                focus: LogcatFocus::Controls,
                control_index: 0,
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
                stay_awake: false,
                wifi_enabled: true,
                bluetooth_enabled: true,
                airplane_mode: false,
            },
            performance: PerfState {
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
                result: None,
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
                stream_child: None,
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
                stream_child: None,
                error: None,
                picker,
                preview_state: RefCell::new(None),
                preview_filename: None,
                path_input_active: false,
                path_input: String::new(),
                status: None,
            },
            pending_action: None,
            last_device_scan: None,
        }
    }

    // ── Global key handling ──────────────────────────────────────

    /// Handle a key event at the top level (global keybindings).
    /// Returns true if the event was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Keyboard activity cancels transient hover affordances.
        self.hover = HoverState::default();

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
                    Page::Bugreport if self.bugreport.is_generating => {
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
        if (key.code == KeyCode::Tab || key.code == KeyCode::BackTab)
            && !self.is_text_input_active()
        {
            if self.focus == Focus::Sidebar {
                self.focus = Focus::Content;
                return true;
            }
            // Pages with internal sections handle Tab/BackTab themselves (fall through).
            // Other pages: Tab/BackTab returns to sidebar.
            match self.page {
                Page::Dashboard | Page::Logcat | Page::Controls | Page::Apps | Page::Settings => {
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
            Page::Screen => self.screen.path_input_active,
            _ => false,
        }
    }

    // ── Mouse handling ────────────────────────────────────────────

    /// Handle mouse events (scroll + left-click navigation/actions).
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> AppAction {
        let (width, height) = crossterm::terminal::size().unwrap_or((0, 0));
        let area = Rect::new(0, 0, width, height);
        self.handle_mouse_in_area(mouse, area)
    }

    fn handle_mouse_in_area(&mut self, mouse: MouseEvent, area: Rect) -> AppAction {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.handle_scroll(-3);
                AppAction::None
            }
            MouseEventKind::ScrollDown => {
                self.handle_scroll(3);
                AppAction::None
            }
            MouseEventKind::Moved => {
                self.update_hover(mouse.column, mouse.row, area);
                AppAction::None
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let action = self.handle_left_click(mouse.column, mouse.row, area);
                self.update_hover(mouse.column, mouse.row, area);
                action
            }
            _ => AppAction::None,
        }
    }

    fn handle_left_click(&mut self, column: u16, row: u16, area: Rect) -> AppAction {
        // Modal dialogs are keyboard-driven for now.
        if !matches!(self.modal, ModalState::None) {
            return AppAction::None;
        }
        if area.width == 0 || area.height == 0 {
            return AppAction::None;
        }

        let chunks = Layout::horizontal([Constraint::Length(UI_SIDEBAR_WIDTH), Constraint::Min(0)])
            .split(area);
        let sidebar = chunks[0];
        let content = chunks[1];

        if Self::rect_contains(sidebar, column, row) {
            self.focus = Focus::Sidebar;
            self.handle_sidebar_click(sidebar, row);
            return AppAction::None;
        }
        if Self::rect_contains(content, column, row) {
            self.focus = Focus::Content;
            if self.is_text_input_active() {
                return AppAction::None;
            }
            return self.handle_content_click(content, column, row);
        }

        AppAction::None
    }

    fn handle_sidebar_click(&mut self, sidebar_area: Rect, row: u16) {
        let Some(page_index) = Self::sidebar_page_index_at_row(sidebar_area, row) else {
            return;
        };
        let old_page = self.page;
        self.sidebar_index = page_index;
        self.page = Page::ALL[page_index];
        if old_page != self.page {
            self.on_page_leave(old_page);
        }
    }

    fn handle_content_click(&mut self, content_area: Rect, column: u16, row: u16) -> AppAction {
        match self.page {
            Page::Dashboard => self.handle_dashboard_click(content_area, column, row),
            Page::Logcat => self.handle_logcat_click(content_area, column, row),
            Page::Screen => self.handle_screen_click_mouse(content_area, column, row),
            Page::Apps => self.handle_apps_click_mouse(content_area, column, row),
            Page::Settings => self.handle_settings_click_mouse(content_area, column, row),
            _ => AppAction::None,
        }
    }

    fn update_hover(&mut self, column: u16, row: u16, area: Rect) {
        self.hover = HoverState::default();
        if !matches!(self.modal, ModalState::None) || area.width == 0 || area.height == 0 {
            return;
        }

        let chunks = Layout::horizontal([Constraint::Length(UI_SIDEBAR_WIDTH), Constraint::Min(0)])
            .split(area);
        let sidebar = chunks[0];
        let content = chunks[1];

        if Self::rect_contains(sidebar, column, row) {
            if let Some(page_idx) = Self::sidebar_page_index_at_row(sidebar, row) {
                self.hover.sidebar_page = Some(page_idx);
                self.hover.hint = Some(format!("Click to open {}", Page::ALL[page_idx].label()));
            }
            return;
        }
        if !Self::rect_contains(content, column, row) {
            return;
        }

        match self.page {
            Page::Logcat => self.update_logcat_hover(content, column, row),
            Page::Screen => self.update_screen_hover(content, column, row),
            Page::Apps => self.update_apps_hover(content, column, row),
            Page::Settings => self.update_settings_hover(content, column, row),
            Page::Dashboard => {
                self.hover.hint =
                    Some("Click sections to focus, then scroll to navigate".to_string());
            }
            _ => {}
        }
    }

    fn update_logcat_hover(&mut self, content_area: Rect, column: u16, row: u16) {
        let chunks = Layout::vertical([
            Constraint::Length(2), // header
            Constraint::Length(1), // control bar
            Constraint::Length(1), // search/tag display
            Constraint::Min(0),    // logs
            Constraint::Length(1), // footer
        ])
        .split(content_area);

        if Self::rect_contains(chunks[1], column, row) {
            let local_x = usize::from(column.saturating_sub(chunks[1].x));
            if let Some(idx) = self.logcat_control_at_x(local_x) {
                self.hover.logcat_control = Some(idx);
                self.hover.hint = Some(self.logcat_control_hint(LogcatControl::ALL[idx]));
            }
            return;
        }
        if Self::rect_contains(chunks[3], column, row) {
            self.hover.hint = Some("Scroll to navigate logs; click controls to filter".to_string());
        }
    }

    fn update_screen_hover(&mut self, content_area: Rect, column: u16, row: u16) {
        let path_input_row = if self.screen.path_input_active { 1 } else { 0 };
        let chunks = Layout::vertical([
            Constraint::Length(2),              // header
            Constraint::Length(path_input_row), // path input
            Constraint::Length(1),              // tab bar
            Constraint::Min(0),                 // content
            Constraint::Length(1),              // footer
        ])
        .split(content_area);

        if Self::rect_contains(chunks[2], column, row) {
            let local_x = usize::from(column.saturating_sub(chunks[2].x));
            if let Some(tab_idx) = Self::bracketed_item_at_x(local_x, &["SCREENSHOT", "RECORD"]) {
                let tab = if tab_idx == 0 {
                    ScreenTab::Screenshot
                } else {
                    ScreenTab::Record
                };
                self.hover.screen_tab = Some(tab);
                self.hover.hint = Some(format!(
                    "Click to switch to {} tab",
                    if matches!(tab, ScreenTab::Screenshot) {
                        "Screenshot"
                    } else {
                        "Record"
                    }
                ));
            }
            return;
        }

        if !Self::rect_contains(chunks[3], column, row) {
            return;
        }

        match self.screen.active_tab {
            ScreenTab::Screenshot => {
                if self.screen.captures.is_empty() {
                    if !self.screen.is_capturing {
                        self.hover.hint = Some("Click to capture screenshot".to_string());
                    }
                    return;
                }
                let cols =
                    Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
                        .split(chunks[3]);
                if !Self::rect_contains(cols[1], column, row) {
                    return;
                }
                let inner = Self::inner_rect(cols[1]);
                if !Self::rect_contains(inner, column, row) || inner.height == 0 {
                    return;
                }
                let visible_height = inner.height as usize;
                if visible_height == 0 {
                    return;
                }
                let viewport = list_viewport(
                    self.screen.captures.len(),
                    visible_height,
                    self.screen.capture_selected,
                    0,
                );
                let rel_row = usize::from(row.saturating_sub(inner.y));
                let Some(clicked) = viewport.index_at_row(rel_row) else {
                    return;
                };
                if clicked < self.screen.captures.len() {
                    self.hover.hint = Some("Click to preview selected screenshot".to_string());
                }
            }
            ScreenTab::Record => {
                let rows = Layout::vertical([
                    Constraint::Length(1), // duration selector
                    Constraint::Length(1), // spacer
                    Constraint::Length(3), // action button + progress
                    Constraint::Min(0),    // history
                ])
                .split(chunks[3]);

                if Self::rect_contains(rows[0], column, row) {
                    let local_x = usize::from(column.saturating_sub(rows[0].x));
                    if Self::bracketed_item_at_x(local_x, &["30S", "1M", "2M", "3M"]).is_some() {
                        self.hover.hint = Some("Click to set recording duration".to_string());
                    }
                    return;
                }
                if Self::rect_contains(rows[2], column, row) {
                    self.hover.hint = Some(if self.screen.is_recording {
                        "Click to stop recording".to_string()
                    } else {
                        "Click to start recording".to_string()
                    });
                    return;
                }
                if !Self::rect_contains(rows[3], column, row) || self.screen.recordings.is_empty() {
                    return;
                }
                let inner = Self::inner_rect(rows[3]);
                if !Self::rect_contains(inner, column, row) || inner.height == 0 {
                    return;
                }
                let visible_height = inner.height as usize;
                if visible_height == 0 {
                    return;
                }
                let viewport = list_viewport(
                    self.screen.recordings.len(),
                    visible_height,
                    self.screen.recording_selected,
                    0,
                );
                let rel_row = usize::from(row.saturating_sub(inner.y));
                if viewport.index_at_row(rel_row).is_some() {
                    self.hover.hint = Some("Click to select recording".to_string());
                }
            }
        }
    }

    fn update_apps_hover(&mut self, content_area: Rect, column: u16, row: u16) {
        let chunks = Layout::vertical([
            Constraint::Length(2), // header
            Constraint::Length(1), // filter bar
            Constraint::Min(0),    // two-panel content
            Constraint::Length(1), // footer
        ])
        .split(content_area);

        if Self::rect_contains(chunks[1], column, row) {
            let cols = Layout::horizontal([
                Constraint::Min(0),     // search
                Constraint::Length(22), // filter toggles
            ])
            .split(chunks[1]);
            if Self::rect_contains(cols[1], column, row) {
                let local_x = usize::from(column.saturating_sub(cols[1].x));
                if let Some(idx) = Self::bracketed_item_at_x(local_x, &["ALL", "USER", "SYSTEM"]) {
                    let filter = match idx {
                        0 => AppFilter::All,
                        1 => AppFilter::User,
                        _ => AppFilter::System,
                    };
                    self.hover.apps_filter = Some(filter);
                    self.hover.hint = Some(format!(
                        "Click to filter {} apps",
                        match filter {
                            AppFilter::All => "all",
                            AppFilter::User => "user",
                            AppFilter::System => "system",
                        }
                    ));
                }
            }
            return;
        }

        if !Self::rect_contains(chunks[2], column, row) {
            return;
        }
        let panels = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(chunks[2]);
        if !Self::rect_contains(panels[0], column, row) {
            return;
        }
        let inner = Self::inner_rect(panels[0]);
        if !Self::rect_contains(inner, column, row) || inner.height == 0 {
            return;
        }
        let filtered = self.filtered_packages();
        if filtered.is_empty() {
            return;
        }
        let visible_height = inner.height as usize;
        if visible_height == 0 {
            return;
        }
        let viewport = list_viewport(
            filtered.len(),
            visible_height,
            self.apps.selected_index,
            self.apps.scroll_offset,
        );
        let rel_row = usize::from(row.saturating_sub(inner.y));
        if viewport.index_at_row(rel_row).is_some() {
            self.hover.hint = Some("Click to load package details".to_string());
        }
    }

    fn update_settings_hover(&mut self, content_area: Rect, column: u16, row: u16) {
        let chunks = Layout::vertical([
            Constraint::Length(2), // header
            Constraint::Length(1), // spacer
            Constraint::Length(7), // quick toggles
            Constraint::Length(1), // spacer
            Constraint::Length(1), // namespace tabs + search
            Constraint::Min(0),    // settings list
            Constraint::Length(1), // footer
        ])
        .split(content_area);

        if Self::rect_contains(chunks[2], column, row) {
            if let Some(idx) = Self::settings_toggle_at(chunks[2], column, row) {
                self.hover.settings_quick_toggle = Some(idx);
                self.hover.hint = Some(format!(
                    "Click to toggle {}",
                    QUICK_TOGGLES[idx].name.to_lowercase()
                ));
            }
            return;
        }

        if Self::rect_contains(chunks[4], column, row) {
            let cols = Layout::horizontal([
                Constraint::Min(0),     // tabs
                Constraint::Length(30), // search
            ])
            .split(chunks[4]);

            if Self::rect_contains(cols[0], column, row) {
                let local_x = usize::from(column.saturating_sub(cols[0].x));
                if let Some(idx) =
                    Self::bracketed_item_at_x(local_x, &["SYSTEM", "SECURE", "GLOBAL"])
                {
                    let ns = match idx {
                        0 => SettingsNamespace::System,
                        1 => SettingsNamespace::Secure,
                        _ => SettingsNamespace::Global,
                    };
                    self.hover.settings_namespace = Some(ns);
                    self.hover.hint = Some(format!(
                        "Click to switch to {} namespace",
                        ns.label().to_lowercase()
                    ));
                }
            }
            return;
        }

        if !Self::rect_contains(chunks[5], column, row) {
            return;
        }
        let inner = Self::inner_rect(chunks[5]);
        if !Self::rect_contains(inner, column, row) || inner.height == 0 {
            return;
        }
        let list_chunks = Layout::vertical([
            Constraint::Length(1), // column header
            Constraint::Min(0),    // list rows
        ])
        .split(inner);
        if !Self::rect_contains(list_chunks[1], column, row) {
            return;
        }
        let filtered = self.filtered_settings();
        if filtered.is_empty() {
            return;
        }
        let visible_height = list_chunks[1].height as usize;
        if visible_height == 0 {
            return;
        }
        let viewport = list_viewport(
            filtered.len(),
            visible_height,
            self.settings.selected_index,
            self.settings.scroll_offset,
        );
        let rel_row = usize::from(row.saturating_sub(list_chunks[1].y));
        if viewport.index_at_row(rel_row).is_some() {
            self.hover.hint = Some("Click to select setting".to_string());
        }
    }

    fn handle_dashboard_click(&mut self, content_area: Rect, column: u16, row: u16) -> AppAction {
        let chunks = Layout::vertical([
            Constraint::Length(2), // header
            Constraint::Min(0),    // content
            Constraint::Length(1), // footer
        ])
        .split(content_area);

        if !Self::rect_contains(chunks[1], column, row)
            || !self.device_manager.is_connected()
            || self.device_manager.full_info.is_none()
        {
            return AppAction::None;
        }

        let rows = Layout::vertical([
            Constraint::Length(8), // row 1: info cards
            Constraint::Length(8), // row 2: battery + storage
            Constraint::Length(3), // row 3: CPU + Memory gauges
            Constraint::Length(3), // row 4: CPU sparkline
            Constraint::Length(3), // row 5: Memory sparkline
            Constraint::Min(0),    // row 6: process table
        ])
        .split(chunks[1]);

        if Self::rect_contains(rows[0], column, row) {
            let info_cols = Layout::horizontal([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .split(rows[0]);

            let section = if Self::rect_contains(info_cols[0], column, row) {
                Some(DashboardSection::Device)
            } else if Self::rect_contains(info_cols[1], column, row) {
                Some(DashboardSection::Hardware)
            } else if Self::rect_contains(info_cols[2], column, row) {
                Some(DashboardSection::Software)
            } else {
                None
            };

            if let Some(section) = section {
                self.dashboard.focus_section = section;
                self.dashboard.focus_item = 0;
                self.performance.scroll_offset = 0;
            }
            return AppAction::None;
        }

        if !Self::rect_contains(rows[5], column, row) {
            return AppAction::None;
        }

        let was_focused = self.dashboard.focus_section == DashboardSection::Processes;
        self.dashboard.focus_section = DashboardSection::Processes;

        if self.performance.processes.is_empty() {
            self.dashboard.focus_item = 0;
            return AppAction::None;
        }

        let inner = Self::inner_rect(rows[5]);
        if !Self::rect_contains(inner, column, row) || inner.height == 0 {
            return AppAction::None;
        }

        let rel_row = usize::from(row.saturating_sub(inner.y));
        if rel_row == 0 {
            return AppAction::None;
        }

        let data_rows = inner.height.saturating_sub(1) as usize;
        if data_rows == 0 {
            return AppAction::None;
        }

        let selected_for_window = if was_focused {
            self.dashboard.focus_item
        } else {
            self.performance.scroll_offset
        };
        let viewport = list_viewport(
            self.performance.processes.len(),
            data_rows,
            selected_for_window,
            self.performance.scroll_offset,
        );

        let Some(clicked_index) = viewport.index_at_row(rel_row - 1) else {
            return AppAction::None;
        };
        if clicked_index < self.performance.processes.len() {
            self.dashboard.focus_item = clicked_index;
            self.performance.scroll_offset = viewport.scroll;
        }

        AppAction::None
    }

    fn handle_logcat_click(&mut self, content_area: Rect, column: u16, row: u16) -> AppAction {
        let chunks = Layout::vertical([
            Constraint::Length(2), // header
            Constraint::Length(1), // control bar
            Constraint::Length(1), // search/tag display
            Constraint::Min(0),    // logs
            Constraint::Length(1), // footer
        ])
        .split(content_area);

        if Self::rect_contains(chunks[1], column, row) {
            let local_x = usize::from(column.saturating_sub(chunks[1].x));
            if let Some(idx) = self.logcat_control_at_x(local_x) {
                self.logcat.focus = LogcatFocus::Controls;
                self.logcat.control_index = idx;
                return self.activate_logcat_control();
            }
            return AppAction::None;
        }

        if Self::rect_contains(chunks[3], column, row) {
            self.logcat.focus = LogcatFocus::Logs;
        }

        AppAction::None
    }

    fn handle_screen_click_mouse(
        &mut self,
        content_area: Rect,
        column: u16,
        row: u16,
    ) -> AppAction {
        let path_input_row = if self.screen.path_input_active { 1 } else { 0 };
        let chunks = Layout::vertical([
            Constraint::Length(2),              // header
            Constraint::Length(path_input_row), // path input
            Constraint::Length(1),              // tab bar
            Constraint::Min(0),                 // content
            Constraint::Length(1),              // footer
        ])
        .split(content_area);

        if Self::rect_contains(chunks[2], column, row) {
            let local_x = usize::from(column.saturating_sub(chunks[2].x));
            if let Some(tab_idx) = Self::bracketed_item_at_x(local_x, &["SCREENSHOT", "RECORD"]) {
                self.screen.active_tab = if tab_idx == 0 {
                    ScreenTab::Screenshot
                } else {
                    ScreenTab::Record
                };
            }
            return AppAction::None;
        }

        if !Self::rect_contains(chunks[3], column, row) {
            return AppAction::None;
        }

        match self.screen.active_tab {
            ScreenTab::Screenshot => self.handle_screenshot_tab_click(chunks[3], column, row),
            ScreenTab::Record => self.handle_record_tab_click(chunks[3], column, row),
        }
    }

    fn handle_screenshot_tab_click(&mut self, area: Rect, column: u16, row: u16) -> AppAction {
        if self.screen.captures.is_empty() {
            if !self.screen.is_capturing {
                return AppAction::ScreenCapture;
            }
            return AppAction::None;
        }

        let cols = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        if !Self::rect_contains(cols[1], column, row) {
            return AppAction::None;
        }

        let inner = Self::inner_rect(cols[1]);
        if !Self::rect_contains(inner, column, row) || inner.height == 0 {
            return AppAction::None;
        }

        let visible_height = inner.height as usize;
        if visible_height == 0 {
            return AppAction::None;
        }

        let viewport = list_viewport(
            self.screen.captures.len(),
            visible_height,
            self.screen.capture_selected,
            0,
        );

        let mut line_to_index: Vec<Option<usize>> = Vec::with_capacity(visible_height);
        for i in viewport.iter_range() {
            line_to_index.push(Some(i));
            if i == viewport.selected && line_to_index.len() < visible_height {
                line_to_index.push(Some(i));
            }
        }
        while line_to_index.len() < visible_height {
            line_to_index.push(None);
        }

        let rel_row = usize::from(row.saturating_sub(inner.y));
        if let Some(Some(idx)) = line_to_index.get(rel_row) {
            self.screen.capture_selected = *idx;
            self.load_screenshot_preview();
        }

        AppAction::None
    }

    fn handle_record_tab_click(&mut self, area: Rect, column: u16, row: u16) -> AppAction {
        let rows = Layout::vertical([
            Constraint::Length(1), // duration selector
            Constraint::Length(1), // spacer
            Constraint::Length(3), // action button + progress
            Constraint::Min(0),    // history
        ])
        .split(area);

        if Self::rect_contains(rows[0], column, row) {
            let local_x = usize::from(column.saturating_sub(rows[0].x));
            if let Some(idx) = Self::bracketed_item_at_x(local_x, &["30S", "1M", "2M", "3M"]) {
                self.screen.record_duration = match idx {
                    0 => RecordDuration::Sec30,
                    1 => RecordDuration::Min1,
                    2 => RecordDuration::Min2,
                    _ => RecordDuration::Min3,
                };
            }
            return AppAction::None;
        }

        if Self::rect_contains(rows[2], column, row) {
            return if self.screen.is_recording {
                AppAction::ScreenRecordStop
            } else {
                AppAction::ScreenRecordStart
            };
        }

        if !Self::rect_contains(rows[3], column, row) || self.screen.recordings.is_empty() {
            return AppAction::None;
        }

        let inner = Self::inner_rect(rows[3]);
        if !Self::rect_contains(inner, column, row) || inner.height == 0 {
            return AppAction::None;
        }

        let visible_height = inner.height as usize;
        if visible_height == 0 {
            return AppAction::None;
        }
        let viewport = list_viewport(
            self.screen.recordings.len(),
            visible_height,
            self.screen.recording_selected,
            0,
        );
        let rel_row = usize::from(row.saturating_sub(inner.y));
        if let Some(clicked) = viewport.index_at_row(rel_row) {
            self.screen.recording_selected = clicked;
        }

        AppAction::None
    }

    fn handle_apps_click_mouse(&mut self, content_area: Rect, column: u16, row: u16) -> AppAction {
        let chunks = Layout::vertical([
            Constraint::Length(2), // header
            Constraint::Length(1), // filter bar
            Constraint::Min(0),    // two-panel content
            Constraint::Length(1), // footer
        ])
        .split(content_area);

        if Self::rect_contains(chunks[1], column, row) {
            let filter_cols = Layout::horizontal([
                Constraint::Min(0),     // search
                Constraint::Length(22), // filter toggles
            ])
            .split(chunks[1]);

            if Self::rect_contains(filter_cols[1], column, row) {
                let local_x = usize::from(column.saturating_sub(filter_cols[1].x));
                if let Some(idx) = Self::bracketed_item_at_x(local_x, &["ALL", "USER", "SYSTEM"]) {
                    self.apps.filter_type = match idx {
                        0 => AppFilter::All,
                        1 => AppFilter::User,
                        _ => AppFilter::System,
                    };
                    self.apps.selected_index = 0;
                    self.apps.scroll_offset = 0;
                }
            }
            return AppAction::None;
        }

        if !Self::rect_contains(chunks[2], column, row) {
            return AppAction::None;
        }

        let panels = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(chunks[2]);

        if Self::rect_contains(panels[0], column, row) {
            self.apps.focus_panel = AppPanel::List;
            return self.handle_apps_list_click(panels[0], column, row);
        }

        if Self::rect_contains(panels[1], column, row) {
            self.apps.focus_panel = AppPanel::Detail;
        }

        AppAction::None
    }

    fn handle_apps_list_click(&mut self, area: Rect, column: u16, row: u16) -> AppAction {
        let inner = Self::inner_rect(area);
        if !Self::rect_contains(inner, column, row) || inner.height == 0 {
            return AppAction::None;
        }

        let filtered = self.filtered_packages();
        if filtered.is_empty() {
            return AppAction::None;
        }

        let visible_height = inner.height as usize;
        if visible_height == 0 {
            return AppAction::None;
        }

        let viewport = list_viewport(
            filtered.len(),
            visible_height,
            self.apps.selected_index,
            self.apps.scroll_offset,
        );
        let rel_row = usize::from(row.saturating_sub(inner.y));
        let Some(clicked) = viewport.index_at_row(rel_row) else {
            return AppAction::None;
        };
        let package_name = filtered.get(clicked).map(|p| p.package_name.clone());
        let Some(name) = package_name else {
            return AppAction::None;
        };

        self.apps.selected_index = clicked;
        self.apps.detail_package = Some(name.clone());
        AppAction::AppsLoadDetails(name)
    }

    fn handle_settings_click_mouse(
        &mut self,
        content_area: Rect,
        column: u16,
        row: u16,
    ) -> AppAction {
        let chunks = Layout::vertical([
            Constraint::Length(2), // header
            Constraint::Length(1), // spacer
            Constraint::Length(7), // quick toggles
            Constraint::Length(1), // spacer
            Constraint::Length(1), // namespace tabs + search
            Constraint::Min(0),    // settings list
            Constraint::Length(1), // footer
        ])
        .split(content_area);

        if Self::rect_contains(chunks[2], column, row) {
            if let Some(toggle_idx) = Self::settings_toggle_at(chunks[2], column, row) {
                self.settings.focus_area = SettingsFocus::QuickToggles;
                self.settings.quick_toggle_focus = toggle_idx;
                return AppAction::SettingsToggle(toggle_idx);
            }
            return AppAction::None;
        }

        if Self::rect_contains(chunks[4], column, row) {
            let cols = Layout::horizontal([
                Constraint::Min(0),     // tabs
                Constraint::Length(30), // search
            ])
            .split(chunks[4]);

            if Self::rect_contains(cols[0], column, row) {
                let local_x = usize::from(column.saturating_sub(cols[0].x));
                if let Some(idx) =
                    Self::bracketed_item_at_x(local_x, &["SYSTEM", "SECURE", "GLOBAL"])
                {
                    let next = match idx {
                        0 => SettingsNamespace::System,
                        1 => SettingsNamespace::Secure,
                        _ => SettingsNamespace::Global,
                    };
                    if self.settings.namespace != next {
                        self.settings.namespace = next;
                        self.settings.selected_index = 0;
                        self.settings.scroll_offset = 0;
                        return AppAction::SettingsLoad;
                    }
                }
            }
            return AppAction::None;
        }

        if !Self::rect_contains(chunks[5], column, row) {
            return AppAction::None;
        }

        self.settings.focus_area = SettingsFocus::List;
        let inner = Self::inner_rect(chunks[5]);
        if !Self::rect_contains(inner, column, row) || inner.height == 0 {
            return AppAction::None;
        }

        let list_chunks = Layout::vertical([
            Constraint::Length(1), // column header
            Constraint::Min(0),    // list rows
        ])
        .split(inner);
        if !Self::rect_contains(list_chunks[1], column, row) {
            return AppAction::None;
        }

        let filtered = self.filtered_settings();
        if filtered.is_empty() {
            return AppAction::None;
        }

        let visible_height = list_chunks[1].height as usize;
        if visible_height == 0 {
            return AppAction::None;
        }

        let viewport = list_viewport(
            filtered.len(),
            visible_height,
            self.settings.selected_index,
            self.settings.scroll_offset,
        );
        let rel_row = usize::from(row.saturating_sub(list_chunks[1].y));
        if let Some(clicked) = viewport.index_at_row(rel_row) {
            self.settings.selected_index = clicked;
        }

        AppAction::None
    }

    fn logcat_control_at_x(&self, x: usize) -> Option<usize> {
        let bar_focused = self.logcat.focus == LogcatFocus::Controls
            && !self.logcat.search_active
            && !self.logcat.tag_active;
        let mut cursor = 1usize; // leading space

        for (i, ctrl) in LogcatControl::ALL.iter().enumerate() {
            let selected = bar_focused && i == self.logcat.control_index;
            if selected {
                // Include the selection marker hit-box.
                if x == cursor {
                    return Some(i);
                }
                cursor += 1;
            }

            let label = self.logcat_control_label(*ctrl);
            let token_width = label.chars().count() + 2; // [label]
            let end = cursor + token_width;
            if x >= cursor && x < end {
                return Some(i);
            }
            cursor = end + 1; // trailing space
        }

        None
    }

    fn logcat_control_label(&self, ctrl: LogcatControl) -> String {
        match ctrl {
            LogcatControl::StartStop => {
                if self.logcat.is_streaming {
                    "STOP".to_string()
                } else {
                    "START".to_string()
                }
            }
            LogcatControl::Buffer => self.logcat.buffer.label().to_string(),
            LogcatControl::Search => "SEARCH".to_string(),
            LogcatControl::Tag => "TAG".to_string(),
            LogcatControl::LevelV => "V".to_string(),
            LogcatControl::LevelD => "D".to_string(),
            LogcatControl::LevelI => "I".to_string(),
            LogcatControl::LevelW => "W".to_string(),
            LogcatControl::LevelE => "E".to_string(),
            LogcatControl::LevelF => "F".to_string(),
            LogcatControl::Timestamp => "TIME".to_string(),
            LogcatControl::AutoScroll => "AUTO".to_string(),
            LogcatControl::Clear => "CLR".to_string(),
        }
    }

    fn logcat_control_hint(&self, ctrl: LogcatControl) -> String {
        match ctrl {
            LogcatControl::StartStop => {
                if self.logcat.is_streaming {
                    "Click to stop log stream".to_string()
                } else {
                    "Click to start log stream".to_string()
                }
            }
            LogcatControl::Buffer => "Click to cycle log buffer".to_string(),
            LogcatControl::Search => "Click to edit search filter".to_string(),
            LogcatControl::Tag => "Click to edit tag filter".to_string(),
            LogcatControl::LevelV
            | LogcatControl::LevelD
            | LogcatControl::LevelI
            | LogcatControl::LevelW
            | LogcatControl::LevelE
            | LogcatControl::LevelF => "Click to toggle log level".to_string(),
            LogcatControl::Timestamp => "Click to toggle timestamps".to_string(),
            LogcatControl::AutoScroll => "Click to toggle auto-scroll".to_string(),
            LogcatControl::Clear => "Click to clear logs".to_string(),
        }
    }

    fn settings_toggle_at(area: Rect, column: u16, row: u16) -> Option<usize> {
        let inner = Self::inner_rect(area);
        if !Self::rect_contains(inner, column, row) {
            return None;
        }

        let rows = Layout::vertical([
            Constraint::Length(1), // row 1 names
            Constraint::Length(1), // row 1 desc
            Constraint::Length(1), // spacer
            Constraint::Length(1), // row 2 names
            Constraint::Length(1), // row 2 desc
        ])
        .split(inner);

        let (base_idx, row_area) = if Self::rect_contains(rows[0], column, row)
            || Self::rect_contains(rows[1], column, row)
        {
            (0usize, rows[0])
        } else if Self::rect_contains(rows[3], column, row)
            || Self::rect_contains(rows[4], column, row)
        {
            (3usize, rows[3])
        } else {
            return None;
        };

        let cols = Layout::horizontal([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(row_area);

        for (col_idx, col) in cols.iter().enumerate() {
            if Self::rect_contains(*col, column, row) {
                let idx = base_idx + col_idx;
                if idx < QUICK_TOGGLES.len() {
                    return Some(idx);
                }
            }
        }

        None
    }

    fn sidebar_page_index_at_row(sidebar_area: Rect, row: u16) -> Option<usize> {
        let inner = Rect {
            x: sidebar_area.x,
            y: sidebar_area.y,
            width: sidebar_area.width.saturating_sub(1), // right border only
            height: sidebar_area.height,
        };
        let chunks = Layout::vertical([
            Constraint::Length(3), // header
            Constraint::Min(0),    // nav
            Constraint::Length(4), // device status
            Constraint::Length(1), // footer
        ])
        .split(inner);
        let nav = chunks[1];

        if row < nav.y || row >= nav.y.saturating_add(nav.height) {
            return None;
        }
        let rel_y = usize::from(row.saturating_sub(nav.y));
        Self::sidebar_nav_line_map().get(rel_y).copied().flatten()
    }

    fn sidebar_nav_line_map() -> Vec<Option<usize>> {
        let mut lines: Vec<Option<usize>> = Vec::new();
        let mut current_section = "";

        for (i, page) in Page::ALL.iter().enumerate() {
            let section = page.section();
            if section != current_section {
                if !current_section.is_empty() {
                    lines.push(None); // spacer
                }
                lines.push(None); // section header
                current_section = section;
            }
            lines.push(Some(i));
        }

        lines
    }

    #[cfg(test)]
    fn sidebar_line_for_page_index(page_index: usize) -> Option<usize> {
        Self::sidebar_nav_line_map()
            .iter()
            .position(|entry| *entry == Some(page_index))
    }

    fn bracketed_item_at_x(x: usize, labels: &[&str]) -> Option<usize> {
        let mut cursor = 1usize; // leading space
        for (idx, label) in labels.iter().enumerate() {
            let width = label.chars().count() + 2; // [label]
            let end = cursor + width;
            if x >= cursor && x < end {
                return Some(idx);
            }
            cursor = end + 1; // trailing space
        }
        None
    }

    fn rect_contains(area: Rect, column: u16, row: u16) -> bool {
        let right = area.x.saturating_add(area.width);
        let bottom = area.y.saturating_add(area.height);
        column >= area.x && column < right && row >= area.y && row < bottom
    }

    fn inner_rect(area: Rect) -> Rect {
        Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        }
    }

    /// Apply a scroll delta to the current page's scrollable area.
    fn handle_scroll(&mut self, delta: i32) {
        match self.page {
            Page::Dashboard => {
                if self.dashboard.focus_section == DashboardSection::Processes {
                    let max = self.performance.processes.len().saturating_sub(1);
                    if delta < 0 {
                        self.dashboard.focus_item = self
                            .dashboard
                            .focus_item
                            .saturating_sub(delta.unsigned_abs() as usize);
                    } else {
                        self.dashboard.focus_item =
                            (self.dashboard.focus_item + delta as usize).min(max);
                    }
                }
            }
            Page::Logcat => {
                if delta < 0 {
                    self.logcat.scroll_offset = self
                        .logcat
                        .scroll_offset
                        .saturating_add(delta.unsigned_abs() as usize);
                    self.logcat.auto_scroll = false;
                } else {
                    self.logcat.scroll_offset =
                        self.logcat.scroll_offset.saturating_sub(delta as usize);
                    if self.logcat.scroll_offset == 0 {
                        self.logcat.auto_scroll = true;
                    }
                }
            }
            Page::Shell => {
                if delta < 0 {
                    self.shell.scroll_offset = self
                        .shell
                        .scroll_offset
                        .saturating_add(delta.unsigned_abs() as usize);
                } else {
                    self.shell.scroll_offset =
                        self.shell.scroll_offset.saturating_sub(delta as usize);
                }
            }
            Page::Files => {
                if delta < 0 {
                    self.files.selected_index = self
                        .files
                        .selected_index
                        .saturating_sub(delta.unsigned_abs() as usize);
                } else {
                    let max = self.files.entries.len().saturating_sub(1);
                    self.files.selected_index = self
                        .files
                        .selected_index
                        .saturating_add(delta as usize)
                        .min(max);
                }
            }
            Page::Apps => {
                if delta < 0 {
                    self.apps.selected_index = self
                        .apps
                        .selected_index
                        .saturating_sub(delta.unsigned_abs() as usize);
                    self.apps.scroll_offset = self
                        .apps
                        .scroll_offset
                        .saturating_sub(delta.unsigned_abs() as usize);
                } else {
                    let max = self.filtered_packages().len().saturating_sub(1);
                    self.apps.selected_index = self
                        .apps
                        .selected_index
                        .saturating_add(delta as usize)
                        .min(max);
                    self.apps.scroll_offset =
                        self.apps.scroll_offset.saturating_add(delta as usize);
                }
            }
            Page::Settings => {
                if delta < 0 {
                    self.settings.selected_index = self
                        .settings
                        .selected_index
                        .saturating_sub(delta.unsigned_abs() as usize);
                    self.settings.scroll_offset = self
                        .settings
                        .scroll_offset
                        .saturating_sub(delta.unsigned_abs() as usize);
                } else {
                    let max = self.filtered_settings().len().saturating_sub(1);
                    self.settings.selected_index = self
                        .settings
                        .selected_index
                        .saturating_add(delta as usize)
                        .min(max);
                    self.settings.scroll_offset =
                        self.settings.scroll_offset.saturating_add(delta as usize);
                }
            }
            Page::Bugreport => {
                if delta < 0 {
                    self.bugreport.selected_index = self
                        .bugreport
                        .selected_index
                        .saturating_sub(delta.unsigned_abs() as usize);
                } else {
                    let max = self.bugreport.history.len().saturating_sub(1);
                    self.bugreport.selected_index = self
                        .bugreport
                        .selected_index
                        .saturating_add(delta as usize)
                        .min(max);
                }
            }
            _ => {}
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
                    insert_char_at(value, *cursor_pos, c);
                    *cursor_pos += 1;
                    true
                }
                KeyCode::Backspace => {
                    *cursor_pos = remove_char_before(value, *cursor_pos);
                    true
                }
                KeyCode::Left => {
                    if *cursor_pos > 0 {
                        *cursor_pos -= 1;
                    }
                    true
                }
                KeyCode::Right => {
                    if *cursor_pos < char_count(value) {
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
            Page::Dashboard => self.handle_dashboard_key(key),
            Page::Shell => self.handle_shell_key(key),
            Page::Logcat => self.handle_logcat_key(key),
            Page::Controls => self.handle_controls_key(key),
            Page::Files => self.handle_files_key(key),
            Page::Apps => self.handle_apps_key(key),
            Page::Settings => self.handle_settings_key(key),
            Page::Bugreport => self.handle_bugreport_key(key),
            Page::Screen => self.handle_screen_key(key),
            Page::About => AppAction::None,
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            // Section cycling
            KeyCode::Tab => {
                self.dashboard.focus_section = self.dashboard.focus_section.next();
                self.dashboard.focus_item = 0;
                self.performance.scroll_offset = 0;
                AppAction::None
            }
            KeyCode::BackTab => {
                self.dashboard.focus_section = self.dashboard.focus_section.prev();
                self.dashboard.focus_item = 0;
                self.performance.scroll_offset = 0;
                AppAction::None
            }
            // Item navigation / process scrolling
            KeyCode::Up | KeyCode::Char('k') => {
                if self.dashboard.focus_section == DashboardSection::Processes {
                    self.dashboard.focus_item = self.dashboard.focus_item.saturating_sub(1);
                    // Scroll to keep selection visible
                    if self.dashboard.focus_item < self.performance.scroll_offset {
                        self.performance.scroll_offset = self.dashboard.focus_item;
                    }
                } else {
                    self.dashboard.focus_item = self.dashboard.focus_item.saturating_sub(1);
                }
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.dashboard.focus_section == DashboardSection::Processes {
                    let max = self.performance.processes.len().saturating_sub(1);
                    self.dashboard.focus_item = (self.dashboard.focus_item + 1).min(max);
                } else {
                    let max = match self.dashboard.focus_section {
                        DashboardSection::Device => 3,
                        DashboardSection::Hardware | DashboardSection::Software => 4,
                        DashboardSection::Processes => unreachable!(),
                    };
                    self.dashboard.focus_item = (self.dashboard.focus_item + 1).min(max);
                }
                AppAction::None
            }
            // Home / End for process list
            KeyCode::Home | KeyCode::Char('g') => {
                if self.dashboard.focus_section == DashboardSection::Processes {
                    self.dashboard.focus_item = 0;
                    self.performance.scroll_offset = 0;
                }
                AppAction::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                if self.dashboard.focus_section == DashboardSection::Processes {
                    self.dashboard.focus_item = self.performance.processes.len().saturating_sub(1);
                }
                AppAction::None
            }
            // Copy focused value to clipboard
            KeyCode::Char('c') => {
                if let Some(value) = self.get_dashboard_focused_value() {
                    self.copy_to_clipboard(&value);
                    self.dashboard.copied_feedback = Some((value, Instant::now()));
                }
                AppAction::None
            }
            KeyCode::Char('r') => {
                self.dashboard.last_refresh = None;
                AppAction::None
            }
            KeyCode::Char('a') => {
                self.dashboard.auto_refresh = self.dashboard.auto_refresh.next();
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    /// Get the value of the currently focused dashboard item.
    fn get_dashboard_focused_value(&self) -> Option<String> {
        match self.dashboard.focus_section {
            DashboardSection::Processes => {
                let p = self.performance.processes.get(self.dashboard.focus_item)?;
                Some(p.name.clone())
            }
            _ => {
                let info = self.device_manager.full_info.as_ref()?;
                match self.dashboard.focus_section {
                    DashboardSection::Device => {
                        let id = &info.identity;
                        match self.dashboard.focus_item {
                            0 => Some(id.model.clone()),
                            1 => Some(id.manufacturer.clone()),
                            2 => Some(id.device.clone()),
                            3 => Some(id.serial.clone()),
                            _ => None,
                        }
                    }
                    DashboardSection::Hardware => {
                        let hw = &info.hardware;
                        match self.dashboard.focus_item {
                            0 => Some(hw.hardware_platform.clone()),
                            1 => Some(hw.cpu_architecture.clone()),
                            2 => Some(hw.total_ram.clone()),
                            3 => Some(hw.display_resolution.clone()),
                            4 => Some(hw.display_density.clone()),
                            _ => None,
                        }
                    }
                    DashboardSection::Software => {
                        let b = &info.build;
                        match self.dashboard.focus_item {
                            0 => Some(b.android_version.clone()),
                            1 => Some(b.sdk_level.clone()),
                            2 => Some(b.security_patch.clone()),
                            3 => Some(b.build_date.clone()),
                            4 => Some(b.build_fingerprint.clone()),
                            _ => None,
                        }
                    }
                    DashboardSection::Processes => unreachable!(),
                }
            }
        }
    }

    /// Copy text to the system clipboard.
    fn copy_to_clipboard(&self, text: &str) {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let result = if cfg!(target_os = "macos") {
            Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                })
        } else {
            Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                })
        };

        if let Err(e) = result {
            tracing::warn!("Failed to copy to clipboard: {e}");
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
                self.shell.cursor_pos = char_count(&self.shell.input);
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
                    self.shell.cursor_pos = char_count(&self.shell.input);
                }
                AppAction::None
            }
            KeyCode::Char(c) => {
                // Quick commands: digit keys when input is empty
                if self.shell.input.is_empty() {
                    let presets = [
                        "getprop",
                        "pm list packages",
                        "dumpsys battery",
                        "df -h",
                        "top -n 1 -b -m 10",
                        "ps -A",
                        "netstat -tlnp",
                        "ip addr",
                    ];
                    if let Some(idx) = c.to_digit(10) {
                        let idx = idx as usize;
                        if idx >= 1 && idx <= 8 {
                            self.shell.input = presets[idx - 1].to_string();
                            self.shell.cursor_pos = char_count(&self.shell.input);
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
                insert_char_at(&mut self.shell.input, self.shell.cursor_pos, c);
                self.shell.cursor_pos += 1;
                AppAction::None
            }
            KeyCode::Backspace => {
                self.shell.cursor_pos =
                    remove_char_before(&mut self.shell.input, self.shell.cursor_pos);
                AppAction::None
            }
            KeyCode::Left => {
                self.shell.cursor_pos = self.shell.cursor_pos.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Right => {
                if self.shell.cursor_pos < char_count(&self.shell.input) {
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
        // Text input modes capture all keys
        if self.logcat.search_active {
            return self.handle_logcat_text_input(key, true);
        }
        if self.logcat.tag_active {
            return self.handle_logcat_text_input(key, false);
        }

        // Ctrl+C stops streaming
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.logcat.is_streaming {
                return AppAction::LogcatStop;
            }
            return AppAction::None;
        }

        match key.code {
            // Tab/BackTab cycles between Controls bar and Logs area
            KeyCode::Tab => {
                self.logcat.focus = match self.logcat.focus {
                    LogcatFocus::Controls => LogcatFocus::Logs,
                    LogcatFocus::Logs => LogcatFocus::Controls,
                };
                AppAction::None
            }
            KeyCode::BackTab => {
                self.logcat.focus = match self.logcat.focus {
                    LogcatFocus::Controls => LogcatFocus::Logs,
                    LogcatFocus::Logs => LogcatFocus::Controls,
                };
                AppAction::None
            }
            _ => match self.logcat.focus {
                LogcatFocus::Controls => self.handle_logcat_controls_key(key),
                LogcatFocus::Logs => self.handle_logcat_logs_key(key),
            },
        }
    }

    /// Handle keys when the control bar is focused.
    fn handle_logcat_controls_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                self.logcat.control_index = LogcatControl::prev_idx(self.logcat.control_index);
                AppAction::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.logcat.control_index = LogcatControl::next_idx(self.logcat.control_index);
                AppAction::None
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_logcat_control(),
            _ => AppAction::None,
        }
    }

    /// Activate the currently highlighted logcat control button.
    fn activate_logcat_control(&mut self) -> AppAction {
        match LogcatControl::ALL[self.logcat.control_index] {
            LogcatControl::StartStop => {
                if self.logcat.is_streaming {
                    AppAction::LogcatStop
                } else {
                    AppAction::LogcatStart
                }
            }
            LogcatControl::Buffer => {
                if !self.logcat.is_streaming {
                    self.logcat.buffer = self.logcat.buffer.next();
                }
                AppAction::None
            }
            LogcatControl::Search => {
                self.logcat.search_active = true;
                AppAction::None
            }
            LogcatControl::Tag => {
                self.logcat.tag_active = true;
                AppAction::None
            }
            LogcatControl::LevelV => {
                self.logcat.level_filter[0] = !self.logcat.level_filter[0];
                AppAction::None
            }
            LogcatControl::LevelD => {
                self.logcat.level_filter[1] = !self.logcat.level_filter[1];
                AppAction::None
            }
            LogcatControl::LevelI => {
                self.logcat.level_filter[2] = !self.logcat.level_filter[2];
                AppAction::None
            }
            LogcatControl::LevelW => {
                self.logcat.level_filter[3] = !self.logcat.level_filter[3];
                AppAction::None
            }
            LogcatControl::LevelE => {
                self.logcat.level_filter[4] = !self.logcat.level_filter[4];
                AppAction::None
            }
            LogcatControl::LevelF => {
                self.logcat.level_filter[5] = !self.logcat.level_filter[5];
                AppAction::None
            }
            LogcatControl::Timestamp => {
                self.logcat.show_timestamp = !self.logcat.show_timestamp;
                AppAction::None
            }
            LogcatControl::AutoScroll => {
                self.logcat.auto_scroll = !self.logcat.auto_scroll;
                if self.logcat.auto_scroll {
                    self.logcat.scroll_offset = 0;
                }
                AppAction::None
            }
            LogcatControl::Clear => {
                self.logcat.logs.clear();
                self.logcat.scroll_offset = 0;
                self.logcat.dropped_lines = 0;
                if let Some(counter) = &self.logcat.dropped_counter {
                    counter.store(0, Ordering::Relaxed);
                }
                AppAction::None
            }
        }
    }

    /// Handle keys when the log output area is focused (scrolling).
    fn handle_logcat_logs_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.logcat.auto_scroll = false;
                self.logcat.scroll_offset = self.logcat.scroll_offset.saturating_add(1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.logcat.scroll_offset = self.logcat.scroll_offset.saturating_sub(1);
                if self.logcat.scroll_offset == 0 {
                    self.logcat.auto_scroll = true;
                }
                AppAction::None
            }
            KeyCode::Char('G') => {
                self.logcat.scroll_offset = 0;
                self.logcat.auto_scroll = true;
                AppAction::None
            }
            KeyCode::Char('g') => {
                self.logcat.auto_scroll = false;
                self.logcat.scroll_offset = self.logcat.logs.len();
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    /// Handle text input for search or tag filter.
    fn handle_logcat_text_input(&mut self, key: KeyEvent, is_search: bool) -> AppAction {
        let (query, active) = if is_search {
            (
                &mut self.logcat.search_query,
                &mut self.logcat.search_active,
            )
        } else {
            (&mut self.logcat.tag_filter, &mut self.logcat.tag_active)
        };
        match key.code {
            KeyCode::Char(c) => {
                query.push(c);
            }
            KeyCode::Backspace => {
                query.pop();
            }
            KeyCode::Enter | KeyCode::Esc => {
                *active = false;
            }
            _ => {}
        }
        AppAction::None
    }

    fn handle_controls_key(&mut self, key: KeyEvent) -> AppAction {
        // Text input mode
        if self.controls.text_input_active {
            return self.handle_controls_text_input(key);
        }

        // 6 sections: Power(0), Screen(1), Connectivity(2), Audio(3), TextInput(4), HardwareKeys(5)
        match key.code {
            KeyCode::Tab => {
                self.controls.focus_section = (self.controls.focus_section + 1) % 6;
                self.controls.focus_item = 0;
                AppAction::None
            }
            KeyCode::BackTab => {
                self.controls.focus_section = if self.controls.focus_section == 0 {
                    5
                } else {
                    self.controls.focus_section - 1
                };
                self.controls.focus_item = 0;
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.controls.focus_item = self.controls.focus_item.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max_item = match self.controls.focus_section {
                    0 => 2, // Power: 3 items
                    1 => 2, // Screen: 3 items
                    2 => 2, // Connectivity: 3 items
                    3 => 1, // Audio: 2 items (Volume, Brightness)
                    4 => 0, // Text Input: activates on focus
                    5 => 7, // Hardware Keys: 8 items
                    _ => 0,
                };
                self.controls.focus_item = (self.controls.focus_item + 1).min(max_item);
                AppAction::None
            }
            // Left/Right: adjust audio bars when in Audio section
            KeyCode::Left | KeyCode::Char('h') if self.controls.focus_section == 3 => {
                if self.controls.focus_item == 0 {
                    self.controls.volume = self.controls.volume.saturating_sub(1);
                    AppAction::ControlsExec("input keyevent 25".to_string())
                } else {
                    self.controls.brightness = self.controls.brightness.saturating_sub(25);
                    AppAction::ControlsExec(format!(
                        "settings put system screen_brightness {}",
                        self.controls.brightness
                    ))
                }
            }
            KeyCode::Right | KeyCode::Char('l') if self.controls.focus_section == 3 => {
                if self.controls.focus_item == 0 {
                    self.controls.volume = (self.controls.volume + 1).min(15);
                    AppAction::ControlsExec("input keyevent 24".to_string())
                } else {
                    self.controls.brightness = (self.controls.brightness + 25).min(255);
                    AppAction::ControlsExec(format!(
                        "settings put system screen_brightness {}",
                        self.controls.brightness
                    ))
                }
            }
            KeyCode::Enter => self.activate_control(),
            // Audio hotkeys (work from any section)
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.controls.volume = (self.controls.volume + 1).min(15);
                AppAction::ControlsExec("input keyevent 24".to_string())
            }
            KeyCode::Char('-') => {
                self.controls.volume = self.controls.volume.saturating_sub(1);
                AppAction::ControlsExec("input keyevent 25".to_string())
            }
            KeyCode::Char('m') => {
                self.controls.volume = 0;
                AppAction::ControlsExec("input keyevent 164".to_string())
            }
            KeyCode::Char(']') => {
                self.controls.brightness = (self.controls.brightness + 25).min(255);
                AppAction::ControlsExec(format!(
                    "settings put system screen_brightness {}",
                    self.controls.brightness
                ))
            }
            KeyCode::Char('[') => {
                self.controls.brightness = self.controls.brightness.saturating_sub(25);
                AppAction::ControlsExec(format!(
                    "settings put system screen_brightness {}",
                    self.controls.brightness
                ))
            }
            KeyCode::Char('i') => {
                self.controls.focus_section = 4;
                self.controls.text_input_active = true;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_controls_text_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char(c) => {
                insert_char_at(
                    &mut self.controls.text_input,
                    self.controls.text_cursor_pos,
                    c,
                );
                self.controls.text_cursor_pos += 1;
                AppAction::None
            }
            KeyCode::Backspace => {
                self.controls.text_cursor_pos = remove_char_before(
                    &mut self.controls.text_input,
                    self.controls.text_cursor_pos,
                );
                AppAction::None
            }
            KeyCode::Enter => {
                if !self.controls.text_input.is_empty() {
                    let escaped = self
                        .controls
                        .text_input
                        .replace(' ', "%s")
                        .replace('\'', "\\'");
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
        match self.controls.focus_section {
            0 => {
                // Power
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
            1 => {
                // Screen
                match self.controls.focus_item {
                    0 => AppAction::ControlsExec("input keyevent 26".to_string()),
                    1 => AppAction::ControlsExec("input swipe 540 1800 540 800".to_string()),
                    2 => {
                        self.controls.stay_awake = !self.controls.stay_awake;
                        if self.controls.stay_awake {
                            AppAction::ControlsExec("svc power stayon true".to_string())
                        } else {
                            AppAction::ControlsExec("svc power stayon false".to_string())
                        }
                    }
                    _ => AppAction::None,
                }
            }
            2 => {
                // Connectivity
                match self.controls.focus_item {
                    0 => {
                        self.controls.wifi_enabled = !self.controls.wifi_enabled;
                        if self.controls.wifi_enabled {
                            AppAction::ControlsExec("svc wifi enable".to_string())
                        } else {
                            AppAction::ControlsExec("svc wifi disable".to_string())
                        }
                    }
                    1 => {
                        self.controls.bluetooth_enabled = !self.controls.bluetooth_enabled;
                        if self.controls.bluetooth_enabled {
                            AppAction::ControlsExec("svc bluetooth enable".to_string())
                        } else {
                            AppAction::ControlsExec("svc bluetooth disable".to_string())
                        }
                    }
                    2 => {
                        self.controls.airplane_mode = !self.controls.airplane_mode;
                        if self.controls.airplane_mode {
                            AppAction::ControlsExec("settings put global airplane_mode_on 1 && am broadcast -a android.intent.action.AIRPLANE_MODE".to_string())
                        } else {
                            AppAction::ControlsExec("settings put global airplane_mode_on 0 && am broadcast -a android.intent.action.AIRPLANE_MODE".to_string())
                        }
                    }
                    _ => AppAction::None,
                }
            }
            3 => {
                // Audio & Display — Enter on volume mutes
                if self.controls.focus_item == 0 {
                    self.controls.volume = 0;
                    AppAction::ControlsExec("input keyevent 164".to_string())
                } else {
                    AppAction::None
                }
            }
            4 => {
                // Text input
                self.controls.text_input_active = true;
                AppAction::None
            }
            5 => {
                // Hardware keys
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

    fn handle_files_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.files.selected_index = self.files.selected_index.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.files.entries.is_empty() {
                    self.files.selected_index = self
                        .files
                        .selected_index
                        .saturating_add(1)
                        .min(self.files.entries.len() - 1);
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
            KeyCode::Char('/') => {
                self.apps.search_active = true;
                AppAction::None
            }
            KeyCode::Char('f') => {
                self.apps.filter_type = self.apps.filter_type.next();
                self.apps.selected_index = 0;
                self.apps.scroll_offset = 0;
                AppAction::None
            }
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
                        let max = self.filtered_packages().len().saturating_sub(1);
                        let current = self.apps.selected_index.min(max);
                        self.apps.selected_index = current.saturating_sub(1);
                    }
                    AppPanel::Detail => {
                        self.apps.detail_scroll_offset =
                            self.apps.detail_scroll_offset.saturating_sub(1);
                    }
                }
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.apps.focus_panel {
                    AppPanel::List => {
                        let max = self.filtered_packages().len().saturating_sub(1);
                        let current = self.apps.selected_index.min(max);
                        self.apps.selected_index = current.saturating_add(1).min(max);
                    }
                    AppPanel::Detail => {
                        self.apps.detail_scroll_offset =
                            self.apps.detail_scroll_offset.saturating_add(1);
                    }
                }
                AppAction::None
            }
            KeyCode::Enter => {
                if self.apps.focus_panel == AppPanel::List {
                    // Load details for selected package
                    let filtered = self.filtered_packages();
                    let selected = self
                        .apps
                        .selected_index
                        .min(filtered.len().saturating_sub(1));
                    if let Some(pkg) = filtered.get(selected) {
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
            KeyCode::Char(c) => {
                self.apps.search_query.push(c);
                self.apps.selected_index = 0;
                self.apps.scroll_offset = 0;
                AppAction::None
            }
            KeyCode::Backspace => {
                self.apps.search_query.pop();
                self.apps.selected_index = 0;
                self.apps.scroll_offset = 0;
                AppAction::None
            }
            KeyCode::Enter | KeyCode::Esc => {
                self.apps.search_active = false;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    /// Get filtered package list based on search and filter.
    pub fn filtered_packages(&self) -> Vec<&PackageInfo> {
        self.apps
            .packages
            .iter()
            .filter(|p| {
                let type_match = match self.apps.filter_type {
                    AppFilter::All => true,
                    AppFilter::User => !p.is_system,
                    AppFilter::System => p.is_system,
                };
                let search_match = self.apps.search_query.is_empty()
                    || p.package_name
                        .to_lowercase()
                        .contains(&self.apps.search_query.to_lowercase());
                type_match && search_match
            })
            .collect()
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
            KeyCode::Char('/') => {
                self.settings.search_active = true;
                AppAction::None
            }
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
                        self.settings.quick_toggle_focus =
                            self.settings.quick_toggle_focus.saturating_sub(1);
                    }
                    SettingsFocus::List => {
                        let max = self.filtered_settings().len().saturating_sub(1);
                        let current = self.settings.selected_index.min(max);
                        self.settings.selected_index = current.saturating_sub(1);
                    }
                }
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.settings.focus_area {
                    SettingsFocus::QuickToggles => {
                        self.settings.quick_toggle_focus =
                            (self.settings.quick_toggle_focus + 1).min(QUICK_TOGGLES.len() - 1);
                    }
                    SettingsFocus::List => {
                        let max = self.filtered_settings().len().saturating_sub(1);
                        let current = self.settings.selected_index.min(max);
                        self.settings.selected_index = current.saturating_add(1).min(max);
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
                    let selected = self
                        .settings
                        .selected_index
                        .min(filtered.len().saturating_sub(1));
                    if let Some(entry) = filtered.get(selected) {
                        let ns = self.settings.namespace.arg().to_string();
                        let key = entry.key.clone();
                        let value = entry.value.clone();
                        let modal_value = format!("{ns}:{key}:{value}");
                        self.modal = ModalState::TextInput {
                            title: "Edit Setting".to_string(),
                            prompt: format!("{ns}/{key}"),
                            cursor_pos: char_count(&modal_value),
                            value: modal_value,
                            action_tag: "settings_edit".to_string(),
                        };
                    }
                }
                AppAction::None
            }
            KeyCode::Char('d') => {
                if self.settings.focus_area == SettingsFocus::List {
                    let filtered = self.filtered_settings();
                    let selected = self
                        .settings
                        .selected_index
                        .min(filtered.len().saturating_sub(1));
                    if let Some(entry) = filtered.get(selected) {
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
            KeyCode::Char(c) => {
                self.settings.search_query.push(c);
                self.settings.selected_index = 0;
                self.settings.scroll_offset = 0;
                AppAction::None
            }
            KeyCode::Backspace => {
                self.settings.search_query.pop();
                self.settings.selected_index = 0;
                self.settings.scroll_offset = 0;
                AppAction::None
            }
            KeyCode::Enter | KeyCode::Esc => {
                self.settings.search_active = false;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    /// Get filtered settings list based on search.
    pub fn filtered_settings(&self) -> Vec<&SettingEntry> {
        if self.settings.search_query.is_empty() {
            self.settings.settings.iter().collect()
        } else {
            let q = self.settings.search_query.to_lowercase();
            self.settings
                .settings
                .iter()
                .filter(|s| {
                    s.key.to_lowercase().contains(&q) || s.value.to_lowercase().contains(&q)
                })
                .collect()
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
                    self.bugreport.selected_index = self
                        .bugreport
                        .selected_index
                        .saturating_add(1)
                        .min(self.bugreport.history.len() - 1);
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_screen_key(&mut self, key: KeyEvent) -> AppAction {
        // Path input mode — capture all keys
        if self.screen.path_input_active {
            return self.handle_screen_path_input(key);
        }
        match key.code {
            KeyCode::Char('p') => {
                self.screen.path_input_active = true;
                self.screen.path_input = self.config.output_dir.clone();
                AppAction::None
            }
            KeyCode::Char('1') => {
                self.screen.active_tab = ScreenTab::Screenshot;
                AppAction::None
            }
            KeyCode::Char('2') => {
                self.screen.active_tab = ScreenTab::Record;
                AppAction::None
            }
            KeyCode::Char('c') | KeyCode::Enter => match self.screen.active_tab {
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
            },
            KeyCode::Char('d') => {
                match self.screen.active_tab {
                    ScreenTab::Screenshot => {
                        self.delete_selected_screenshot();
                    }
                    ScreenTab::Record => {
                        self.screen.record_duration = self.screen.record_duration.next();
                    }
                }
                AppAction::None
            }
            KeyCode::Char('o') => {
                if self.screen.active_tab == ScreenTab::Screenshot {
                    self.open_selected_screenshot();
                }
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.screen.active_tab {
                    ScreenTab::Screenshot => {
                        self.screen.capture_selected =
                            self.screen.capture_selected.saturating_sub(1);
                        self.load_screenshot_preview();
                    }
                    ScreenTab::Record => {
                        self.screen.recording_selected =
                            self.screen.recording_selected.saturating_sub(1);
                    }
                }
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.screen.active_tab {
                    ScreenTab::Screenshot => {
                        if !self.screen.captures.is_empty() {
                            self.screen.capture_selected = self
                                .screen
                                .capture_selected
                                .saturating_add(1)
                                .min(self.screen.captures.len() - 1);
                            self.load_screenshot_preview();
                        }
                    }
                    ScreenTab::Record => {
                        if !self.screen.recordings.is_empty() {
                            self.screen.recording_selected = self
                                .screen
                                .recording_selected
                                .saturating_add(1)
                                .min(self.screen.recordings.len() - 1);
                        }
                    }
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    /// Handle key input while editing the output directory path.
    fn handle_screen_path_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Enter => {
                let path = self.screen.path_input.trim().to_string();
                if path.is_empty() {
                    self.screen.path_input_active = false;
                    return AppAction::None;
                }
                let expanded = if let Some(rest) = path.strip_prefix('~') {
                    if let Some(home) =
                        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
                    {
                        home.to_string_lossy().to_string() + rest
                    } else {
                        path.clone()
                    }
                } else {
                    path.clone()
                };
                let p = std::path::Path::new(&expanded);
                if p.is_dir() {
                    self.config.output_dir = expanded;
                    self.config.save();
                    self.screen.error = None;
                } else {
                    self.screen.error = Some(format!("Not a directory: {path}"));
                }
                self.screen.path_input_active = false;
                AppAction::None
            }
            KeyCode::Esc => {
                self.screen.path_input_active = false;
                AppAction::None
            }
            KeyCode::Backspace => {
                self.screen.path_input.pop();
                AppAction::None
            }
            KeyCode::Char(c) => {
                self.screen.path_input.push(c);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    /// Load the selected screenshot into the preview state.
    pub fn load_screenshot_preview(&mut self) {
        let Some(cap) = self.screen.captures.get(self.screen.capture_selected) else {
            *self.screen.preview_state.borrow_mut() = None;
            self.screen.preview_filename = None;
            return;
        };

        // Skip if already loaded
        if self.screen.preview_filename.as_deref() == Some(&cap.filename) {
            return;
        }

        let filename = cap.filename.clone();
        let path = self.config.output_path(&filename);

        let Some(ref mut picker) = self.screen.picker else {
            return;
        };

        match image::open(&path) {
            Ok(img) => {
                let protocol = picker.new_resize_protocol(img);
                *self.screen.preview_state.borrow_mut() = Some(protocol);
                self.screen.preview_filename = Some(filename);
            }
            Err(e) => {
                tracing::warn!("Failed to load screenshot {path}: {e}");
                *self.screen.preview_state.borrow_mut() = None;
                self.screen.preview_filename = None;
            }
        }
    }

    /// Delete the currently selected screenshot file and remove from list.
    fn delete_selected_screenshot(&mut self) {
        if self.screen.captures.is_empty() {
            return;
        }
        let idx = self.screen.capture_selected;
        let cap = &self.screen.captures[idx];

        // Delete the file from disk
        let path = self.config.output_path(&cap.filename);
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("Failed to delete screenshot {path}: {e}");
        }

        self.screen.captures.remove(idx);
        if self.screen.captures.is_empty() {
            self.screen.capture_selected = 0;
        } else {
            self.screen.capture_selected = idx.min(self.screen.captures.len() - 1);
        }

        // Clear preview and reload
        *self.screen.preview_state.borrow_mut() = None;
        self.screen.preview_filename = None;
        self.load_screenshot_preview();
    }

    /// Open the selected screenshot in the system default viewer.
    fn open_selected_screenshot(&self) {
        let Some(cap) = self.screen.captures.get(self.screen.capture_selected) else {
            return;
        };
        let path = self.config.output_path(&cap.filename);
        let cmd = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        if let Err(e) = std::process::Command::new(cmd).arg(&path).spawn() {
            tracing::warn!("Failed to open screenshot: {e}");
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
                if let Some(mut child) = self.shell.stream_child.take() {
                    let _ = child.start_kill();
                }
            }
            Page::Logcat => {
                // Stop logcat streaming
                self.logcat.is_streaming = false;
                self.logcat.stream_rx = None;
                self.logcat.dropped_counter = None;
                if let Some(mut child) = self.logcat.stream_child.take() {
                    let _ = child.start_kill();
                }
            }
            // Bugreport continues in background
            _ => {}
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
                if count >= 100 {
                    break;
                } // Batch limit per tick
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
        if let Some(counter) = &self.logcat.dropped_counter {
            let producer_dropped = counter.swap(0, Ordering::Relaxed);
            if producer_dropped > 0 {
                self.logcat.dropped_lines =
                    self.logcat.dropped_lines.saturating_add(producer_dropped);
            }
        }

        if let Some(ref mut rx) = self.logcat.stream_rx {
            // Drain a bounded raw batch per cycle, then parse only the newest N lines.
            let mut newest = VecDeque::with_capacity(LOGCAT_PARSE_BATCH);
            let mut dropped_in_cycle = 0usize;
            let mut received = 0usize;
            while received < LOGCAT_RECV_LIMIT {
                let Ok(line) = rx.try_recv() else {
                    break;
                };
                received += 1;
                if newest.len() == LOGCAT_PARSE_BATCH {
                    let _ = newest.pop_front();
                    dropped_in_cycle += 1;
                }
                newest.push_back(line);
            }

            let mut added = 0usize;
            for line in newest {
                if let Some(entry) = parser::parse_logcat_line(&line) {
                    self.logcat.logs.push(entry);
                    added += 1;
                }
            }

            if dropped_in_cycle > 0 {
                self.logcat.dropped_lines =
                    self.logcat.dropped_lines.saturating_add(dropped_in_cycle);
            }

            if self.logcat.auto_scroll {
                self.logcat.scroll_offset = 0;
            } else if added > 0 {
                // Keep the viewport anchored when new lines arrive while paused.
                self.logcat.scroll_offset = self.logcat.scroll_offset.saturating_add(added);
            }
            // Trim in-memory buffer.
            if self.logcat.logs.len() > LOGCAT_MAX_LINES {
                let drain = self.logcat.logs.len() - LOGCAT_MAX_LINES;
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
                    before
                        .split_whitespace()
                        .last()
                        .and_then(|s| s.parse::<u32>().ok()),
                    after
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.parse::<u32>().ok()),
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
            self.bugreport.stream_child = None;
        }
    }

    /// Update screen recording elapsed time.
    pub fn update_screen_recording(&mut self) {
        if self.screen.is_recording {
            if let Some(start) = self.screen.record_start {
                self.screen.record_elapsed = start.elapsed().as_secs() as u32;
                if self.screen.record_elapsed >= self.screen.record_duration.secs() {
                    // Queue the stop action so the recording gets pulled and saved
                    self.pending_action = Some(AppAction::ScreenRecordStop);
                }
            }
        }
    }

    // ── Async operations ─────────────────────────────────────────

    /// Attempt initial device connection.
    pub async fn init_device(&mut self) -> Result<()> {
        self.device_manager.refresh_devices().await?;
        self.last_device_scan = Some(Instant::now());
        Ok(())
    }

    /// Whether it's time to refresh adb device discovery.
    pub fn device_scan_due(&self) -> bool {
        match self.last_device_scan {
            None => true,
            Some(last) => last.elapsed().as_secs() >= DEVICE_SCAN_SECS,
        }
    }

    /// Refresh device list and connection state without failing the event loop.
    pub async fn refresh_device_connection(&mut self) {
        self.last_device_scan = Some(Instant::now());
        if let Err(e) = self.device_manager.refresh_devices().await {
            tracing::warn!("Device refresh failed: {e}");
        }
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
        if self.page != Page::Dashboard || !self.device_manager.is_connected() {
            return false;
        }
        match self.performance.last_collect {
            None => true,
            Some(last) => last.elapsed().as_secs() >= PERF_REFRESH_SECS,
        }
    }

    /// Collect performance data from device.
    pub async fn collect_perf_data(&mut self) {
        if !self.device_manager.is_connected() {
            return;
        }

        let (top_raw, mem_raw, bat_raw) = tokio::join!(
            self.device_manager.client.shell("top -n 1 -b"),
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
            let pct = if total_kb > 0 {
                used_kb as f64 / total_kb as f64 * 100.0
            } else {
                0.0
            };
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
        match self
            .device_manager
            .client
            .shell(&format!("ls -la \"{path}\""))
            .await
        {
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
        match self
            .device_manager
            .client
            .shell("pm list packages -f")
            .await
        {
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
        match self
            .device_manager
            .client
            .shell(&format!("settings list {ns}"))
            .await
        {
            Ok(output) => {
                self.settings.settings = parser::parse_settings_list(&output);
            }
            Err(e) => {
                tracing::error!("Failed to load settings: {e}");
            }
        }
        for (i, toggle) in QUICK_TOGGLES.iter().enumerate() {
            if let Ok(val) = self
                .device_manager
                .client
                .shell(&format!("settings get {} {}", toggle.namespace, toggle.key))
                .await
            {
                self.settings.quick_toggle_states[i] = val.trim() == toggle.enable_value;
            }
        }
        self.settings.loading = false;
    }

    pub async fn dispatch_action(&mut self, action: AppAction) {
        let local_only_action = matches!(
            &action,
            AppAction::None
                | AppAction::ShellStop
                | AppAction::LogcatStop
                | AppAction::BugreportCancel
                | AppAction::ScreenRecordStop
        );
        if !local_only_action && !self.device_manager.is_connected() {
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
            AppAction::ShellStop => {
                self.shell.is_streaming = false;
                self.shell.is_running = false;
                self.shell.stream_rx = None;
                if let Some(mut child) = self.shell.stream_child.take() {
                    let _ = child.start_kill();
                }
            }

            // Logcat
            AppAction::LogcatStart => {
                if let Some(mut child) = self.logcat.stream_child.take() {
                    let _ = child.start_kill();
                }
                self.logcat.stream_rx = None;
                self.logcat.dropped_counter = None;
                self.logcat.is_streaming = false;
                let buf_arg = self.logcat.buffer.arg();
                let cmd = format!("logcat -v threadtime -b {buf_arg}");
                let (tx, rx) = mpsc::channel(LOGCAT_CHANNEL_CAPACITY);
                let dropped_counter = Arc::new(AtomicUsize::new(0));
                match self
                    .device_manager
                    .client
                    .shell_stream_lossy(&cmd, tx, dropped_counter.clone())
                    .await
                {
                    Ok(child) => {
                        self.logcat.dropped_lines = 0;
                        self.logcat.stream_rx = Some(rx);
                        self.logcat.is_streaming = true;
                        self.logcat.stream_child = Some(child);
                        self.logcat.dropped_counter = Some(dropped_counter);
                    }
                    Err(e) => {
                        tracing::error!("Failed to start logcat: {e}");
                        self.logcat.dropped_counter = None;
                    }
                }
            }
            AppAction::LogcatStop => {
                self.logcat.is_streaming = false;
                self.logcat.stream_rx = None;
                self.logcat.dropped_counter = None;
                if let Some(mut child) = self.logcat.stream_child.take() {
                    let _ = child.start_kill();
                }
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
                let mut errors = Vec::new();
                for path in &paths {
                    if let Err(e) = self
                        .device_manager
                        .client
                        .shell(&format!("rm -rf \"{path}\""))
                        .await
                    {
                        errors.push(format!("{path}: {e}"));
                    }
                }
                if errors.is_empty() {
                    self.files.result = Some((
                        true,
                        format!("Deleted {} item(s)", paths.len()),
                        Instant::now(),
                    ));
                } else {
                    self.files.result = Some((false, errors.join(" | "), Instant::now()));
                }
                self.files.selected_files.clear();
                let path = self.files.current_path.clone();
                self.navigate_files(&path).await;
            }
            AppAction::FilesPull(paths) => {
                for path in &paths {
                    let filename = path.rsplit('/').next().unwrap_or("file");
                    let local = self.config.output_path(filename);
                    match self.device_manager.client.pull(path, &local).await {
                        Ok(_) => {
                            self.files.result =
                                Some((true, format!("Pulled {filename}"), Instant::now()));
                        }
                        Err(e) => {
                            let msg = format!("Pull failed for {filename}: {e}");
                            tracing::error!("{msg}");
                            self.files.result = Some((false, msg, Instant::now()));
                        }
                    }
                }
            }
            AppAction::FilesMkdir(path) => {
                match self
                    .device_manager
                    .client
                    .shell(&format!("mkdir -p \"{path}\""))
                    .await
                {
                    Ok(_) => {
                        self.files.result =
                            Some((true, "Folder created".to_string(), Instant::now()));
                    }
                    Err(e) => {
                        self.files.result =
                            Some((false, format!("mkdir failed: {e}"), Instant::now()));
                    }
                }
                let parent = self.files.current_path.clone();
                self.navigate_files(&parent).await;
            }

            // Apps
            AppAction::AppsRefresh => {
                self.refresh_packages().await;
            }
            AppAction::AppsLoadDetails(name) => {
                self.apps.detail_loading = true;
                match self
                    .device_manager
                    .client
                    .shell(&format!("dumpsys package {name}"))
                    .await
                {
                    Ok(output) => {
                        self.apps.package_details =
                            Some(parser::parse_package_details(&output, &name));
                    }
                    Err(e) => {
                        tracing::error!("Failed to get package details: {e}");
                    }
                }
                self.apps.detail_loading = false;
            }
            AppAction::AppsOpen(name) => {
                match self
                    .device_manager
                    .client
                    .shell(&format!(
                        "monkey -p {name} -c android.intent.category.LAUNCHER 1"
                    ))
                    .await
                {
                    Ok(_) => {
                        self.apps.action_result =
                            Some((true, format!("Opened {name}"), Instant::now()));
                    }
                    Err(e) => {
                        self.apps.action_result =
                            Some((false, format!("Open failed: {e}"), Instant::now()));
                    }
                }
            }
            AppAction::AppsStop(name) => {
                match self
                    .device_manager
                    .client
                    .shell(&format!("am force-stop {name}"))
                    .await
                {
                    Ok(_) => {
                        self.apps.action_result =
                            Some((true, format!("Stopped {name}"), Instant::now()));
                    }
                    Err(e) => {
                        self.apps.action_result =
                            Some((false, format!("Stop failed: {e}"), Instant::now()));
                    }
                }
            }
            AppAction::AppsClear(name) => {
                match self
                    .device_manager
                    .client
                    .shell(&format!("pm clear {name}"))
                    .await
                {
                    Ok(_) => {
                        self.apps.action_result =
                            Some((true, format!("Cleared {name}"), Instant::now()));
                    }
                    Err(e) => {
                        self.apps.action_result =
                            Some((false, format!("Clear failed: {e}"), Instant::now()));
                    }
                }
            }
            AppAction::AppsUninstall(name) => {
                match self
                    .device_manager
                    .client
                    .shell(&format!("pm uninstall {name}"))
                    .await
                {
                    Ok(output) => {
                        let success = output.contains("Success");
                        self.apps.action_result = Some((
                            success,
                            format!("Uninstall: {}", output.trim()),
                            Instant::now(),
                        ));
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
                        let msg = if output.trim().is_empty() {
                            "OK".to_string()
                        } else {
                            output.trim().to_string()
                        };
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
                if let Err(e) = self
                    .device_manager
                    .client
                    .shell(&format!("settings put {ns} {key} {value}"))
                    .await
                {
                    tracing::error!("Failed to set setting {ns}/{key}: {e}");
                }
                self.load_settings().await;
            }
            AppAction::SettingsDelete(ns, key) => {
                if let Err(e) = self
                    .device_manager
                    .client
                    .shell(&format!("settings delete {ns} {key}"))
                    .await
                {
                    tracing::error!("Failed to delete setting {ns}/{key}: {e}");
                }
                self.load_settings().await;
            }
            AppAction::SettingsToggle(idx) => {
                if let Some(toggle) = QUICK_TOGGLES.get(idx) {
                    let current = self.settings.quick_toggle_states[idx];
                    let new_val = if current {
                        toggle.disable_value
                    } else {
                        toggle.enable_value
                    };
                    match self
                        .device_manager
                        .client
                        .shell(&format!(
                            "settings put {} {} {new_val}",
                            toggle.namespace, toggle.key
                        ))
                        .await
                    {
                        Ok(_) => {
                            self.settings.quick_toggle_states[idx] = !current;
                        }
                        Err(e) => {
                            tracing::error!("Failed to toggle setting {}: {e}", toggle.key);
                        }
                    }
                }
            }

            // Bugreport
            AppAction::BugreportStart => {
                if let Some(mut child) = self.bugreport.stream_child.take() {
                    let _ = child.start_kill();
                }
                let (tx, rx) = mpsc::unbounded_channel();
                match self
                    .device_manager
                    .client
                    .shell_stream("bugreportz", tx)
                    .await
                {
                    Ok(child) => {
                        self.bugreport.stream_rx = Some(rx);
                        self.bugreport.stream_child = Some(child);
                        self.bugreport.is_generating = true;
                        self.bugreport.progress = 0;
                        self.bugreport.start_time = Some(Instant::now());
                        self.bugreport.raw_output.clear();
                        self.bugreport.history.push(BugreportEntry {
                            filename: format!(
                                "bugreport-{}",
                                chrono::Local::now().format("%Y%m%d-%H%M%S")
                            ),
                            status: BugreportStatus::Generating,
                            start_time: Instant::now(),
                            end_time: None,
                            device_path: None,
                            local_path: None,
                            error: None,
                        });
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
                if let Some(mut child) = self.bugreport.stream_child.take() {
                    let _ = child.start_kill();
                }
            }
            AppAction::BugreportDownload(idx) => {
                if let Some(entry) = self.bugreport.history.get_mut(idx) {
                    if let Some(ref device_path) = entry.device_path {
                        let local = self.config.output_path(&format!("{}.zip", entry.filename));
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
                self.screen.status = Some("CAPTURING SCREENSHOT...".into());
                self.screen.error = None;
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
                let device_path = "/sdcard/adbwrench_screenshot.png";
                let local_path = self
                    .config
                    .output_path(&format!("screenshot-{timestamp}.png"));

                let result = async {
                    self.device_manager
                        .client
                        .shell(&format!("screencap -p {device_path}"))
                        .await?;
                    self.device_manager
                        .client
                        .pull(device_path, &local_path)
                        .await?;
                    let _ = self
                        .device_manager
                        .client
                        .shell(&format!("rm {device_path}"))
                        .await;
                    Ok::<(), anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => {
                        self.screen.captures.insert(
                            0,
                            ScreenCapture {
                                filename: format!("screenshot-{timestamp}.png"),
                                timestamp,
                            },
                        );
                        self.screen.capture_selected = 0;
                        self.load_screenshot_preview();
                    }
                    Err(e) => {
                        self.screen.error = Some(e.to_string());
                    }
                }
                self.screen.is_capturing = false;
                self.screen.status = None;
            }
            AppAction::ScreenRecordStart => {
                if let Some(mut child) = self.screen.stream_child.take() {
                    let _ = child.start_kill();
                }
                self.screen.status = Some("STARTING RECORDING...".into());
                self.screen.error = None;
                let duration = self.screen.record_duration.secs();
                let cmd =
                    format!("screenrecord --time-limit {duration} /sdcard/adbwrench_recording.mp4");
                let (tx, rx) = mpsc::unbounded_channel();
                match self.device_manager.client.shell_stream(&cmd, tx).await {
                    Ok(child) => {
                        self.screen.stream_rx = Some(rx);
                        self.screen.stream_child = Some(child);
                        self.screen.is_recording = true;
                        self.screen.record_start = Some(Instant::now());
                        self.screen.record_elapsed = 0;
                    }
                    Err(e) => {
                        self.screen.error = Some(e.to_string());
                    }
                }
                self.screen.status = None;
            }
            AppAction::ScreenRecordStop => {
                self.screen.is_recording = false;
                self.screen.stream_rx = None;
                self.screen.status = Some("SAVING RECORDING...".into());
                self.screen.error = None;

                // Kill the adb process to stop screenrecord on the device
                if let Some(mut child) = self.screen.stream_child.take() {
                    let _ = child.kill().await;
                }

                // Give screenrecord time to finalize the mp4 container
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                // Pull the recording
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
                let local_path = self
                    .config
                    .output_path(&format!("recording-{timestamp}.mp4"));
                match self
                    .device_manager
                    .client
                    .pull("/sdcard/adbwrench_recording.mp4", &local_path)
                    .await
                {
                    Ok(_) => {
                        let _ = self
                            .device_manager
                            .client
                            .shell("rm /sdcard/adbwrench_recording.mp4")
                            .await;
                        self.screen.recordings.push(RecordingEntry {
                            filename: format!("recording-{timestamp}.mp4"),
                            duration_secs: self.screen.record_elapsed,
                            timestamp,
                        });
                    }
                    Err(e) => {
                        tracing::error!("Failed to pull recording: {e}");
                        self.screen.error = Some(format!("Failed to save recording: {e}"));
                    }
                }
                self.screen.status = None;
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
        // Files action result
        if let Some((_, _, time)) = &self.files.result {
            if time.elapsed().as_secs() >= 3 {
                self.files.result = None;
            }
        }
        // Dashboard copied feedback
        if let Some((_, time)) = &self.dashboard.copied_feedback {
            if time.elapsed().as_secs() >= 2 {
                self.dashboard.copied_feedback = None;
            }
        }
    }

    /// Run periodic/background maintenance regardless of foreground input activity.
    pub async fn process_background(&mut self) {
        if self.device_scan_due() {
            self.refresh_device_connection().await;
        }

        if self.dashboard_needs_refresh() {
            self.refresh_dashboard().await;
        }

        if self.perf_needs_collect() {
            self.collect_perf_data().await;
        }

        self.drain_shell_output();
        self.drain_logcat_lines();
        self.drain_bugreport_progress();
        self.update_screen_recording();
        self.clear_stale_results();

        if let Some(action) = self.pending_action.take() {
            self.dispatch_action(action).await;
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse_down(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn mouse_move(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn threadtime_line(i: usize) -> String {
        let secs = i % 60;
        format!("01-15 10:30:{secs:02}.123  1234  5678 I TestTag: msg-{i}")
    }

    fn clear_control_index() -> usize {
        LogcatControl::ALL
            .iter()
            .position(|c| matches!(c, LogcatControl::Clear))
            .expect("Logcat clear control must exist")
    }

    #[test]
    fn mouse_sidebar_click_switches_page() {
        let mut app = App::default();
        app.focus = Focus::Content;
        let area = Rect::new(0, 0, 120, 40);
        let screen_idx = Page::ALL
            .iter()
            .position(|p| *p == Page::Screen)
            .expect("screen page exists");
        let line = App::sidebar_line_for_page_index(screen_idx).expect("line for screen page");
        let row = 3 + line as u16; // Sidebar header is 3 rows.

        let action = app.handle_mouse_in_area(mouse_down(2, row), area);

        assert!(matches!(action, AppAction::None));
        assert_eq!(app.focus, Focus::Sidebar);
        assert_eq!(app.page, Page::Screen);
        assert_eq!(app.sidebar_index, screen_idx);
    }

    #[test]
    fn mouse_settings_namespace_click_returns_load_action() {
        let mut app = App::default();
        app.page = Page::Settings;
        app.sidebar_index = Page::ALL
            .iter()
            .position(|p| *p == Page::Settings)
            .expect("settings page exists");
        app.settings.namespace = SettingsNamespace::System;
        let area = Rect::new(0, 0, 120, 40);

        // Settings tabs row y = 2(header) + 1(spacer) + 7(quick toggles) + 1(spacer) = 11.
        let action = app.handle_mouse_in_area(mouse_down(UI_SIDEBAR_WIDTH + 12, 11), area);

        assert!(matches!(action, AppAction::SettingsLoad));
        assert_eq!(app.focus, Focus::Content);
        assert_eq!(app.settings.namespace, SettingsNamespace::Secure);
    }

    #[test]
    fn mouse_screen_tab_click_switches_active_tab() {
        let mut app = App::default();
        app.page = Page::Screen;
        app.sidebar_index = Page::ALL
            .iter()
            .position(|p| *p == Page::Screen)
            .expect("screen page exists");
        app.screen.active_tab = ScreenTab::Screenshot;
        let area = Rect::new(0, 0, 120, 40);

        // Screen tab row starts at y=2 when path input is hidden.
        let action = app.handle_mouse_in_area(mouse_down(UI_SIDEBAR_WIDTH + 15, 2), area);

        assert!(matches!(action, AppAction::None));
        assert_eq!(app.focus, Focus::Content);
        assert_eq!(app.screen.active_tab, ScreenTab::Record);
    }

    #[test]
    fn mouse_logcat_control_click_dispatches_start() {
        let mut app = App::default();
        app.page = Page::Logcat;
        app.sidebar_index = Page::ALL
            .iter()
            .position(|p| *p == Page::Logcat)
            .expect("logcat page exists");
        app.logcat.is_streaming = false;
        let area = Rect::new(0, 0, 120, 40);

        // Logcat control bar is y=2 and START is the first control.
        let action = app.handle_mouse_in_area(mouse_down(UI_SIDEBAR_WIDTH + 2, 2), area);

        assert!(matches!(action, AppAction::LogcatStart));
        assert_eq!(app.focus, Focus::Content);
        assert_eq!(app.logcat.focus, LogcatFocus::Controls);
        assert_eq!(app.logcat.control_index, 0);
    }

    #[test]
    fn mouse_sidebar_hover_updates_highlight_and_hint() {
        let mut app = App::default();
        let area = Rect::new(0, 0, 120, 40);
        let settings_idx = Page::ALL
            .iter()
            .position(|p| *p == Page::Settings)
            .expect("settings page exists");
        let line = App::sidebar_line_for_page_index(settings_idx).expect("line for settings page");
        let row = 3 + line as u16;

        let action = app.handle_mouse_in_area(mouse_move(2, row), area);

        assert!(matches!(action, AppAction::None));
        assert_eq!(app.hover.sidebar_page, Some(settings_idx));
        assert_eq!(app.hover.hint.as_deref(), Some("Click to open Settings"));
    }

    #[test]
    fn mouse_settings_hover_tracks_namespace_tab_and_hint() {
        let mut app = App::default();
        app.page = Page::Settings;
        app.sidebar_index = Page::ALL
            .iter()
            .position(|p| *p == Page::Settings)
            .expect("settings page exists");
        let area = Rect::new(0, 0, 120, 40);

        // Settings tabs row y = 11; x selects [SECURE].
        let action = app.handle_mouse_in_area(mouse_move(UI_SIDEBAR_WIDTH + 12, 11), area);

        assert!(matches!(action, AppAction::None));
        assert_eq!(
            app.hover.settings_namespace,
            Some(SettingsNamespace::Secure)
        );
        assert_eq!(
            app.hover.hint.as_deref(),
            Some("Click to switch to secure namespace")
        );
    }

    #[test]
    fn logcat_drain_keeps_newest_batch_and_tracks_drops() {
        let mut app = App::default();
        let total = LOGCAT_PARSE_BATCH + 25;
        let (tx, rx) = mpsc::channel(total + 1);
        app.logcat.stream_rx = Some(rx);
        app.logcat.auto_scroll = false;
        app.logcat.scroll_offset = 5;

        for i in 0..total {
            tx.try_send(threadtime_line(i))
                .expect("channel should accept test line");
        }
        drop(tx);

        app.drain_logcat_lines();

        assert_eq!(app.logcat.logs.len(), LOGCAT_PARSE_BATCH);
        assert_eq!(app.logcat.dropped_lines, 25);
        assert_eq!(app.logcat.scroll_offset, 5 + LOGCAT_PARSE_BATCH);
        assert_eq!(
            app.logcat.logs.first().map(|e| e.message.as_str()),
            Some("msg-25")
        );
        assert_eq!(
            app.logcat.logs.last().map(|e| e.message.as_str()),
            Some(format!("msg-{}", total - 1).as_str())
        );
    }

    #[test]
    fn logcat_drain_counts_producer_drops_even_without_receiver_data() {
        let mut app = App::default();
        let counter = Arc::new(AtomicUsize::new(17));
        app.logcat.dropped_counter = Some(counter.clone());

        app.drain_logcat_lines();

        assert_eq!(app.logcat.dropped_lines, 17);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn logcat_clear_resets_drop_state_and_visible_buffer() {
        let mut app = App::default();
        let counter = Arc::new(AtomicUsize::new(9));
        app.logcat.dropped_counter = Some(counter.clone());
        app.logcat.dropped_lines = 12;
        app.logcat.scroll_offset = 3;
        app.logcat.logs.push(LogEntry {
            timestamp: "01-15 10:30:45.123".to_string(),
            level: LogLevel::Info,
            tag: "Tag".to_string(),
            message: "Message".to_string(),
        });
        app.logcat.control_index = clear_control_index();

        let action = app.activate_logcat_control();

        assert!(matches!(action, AppAction::None));
        assert!(app.logcat.logs.is_empty());
        assert_eq!(app.logcat.scroll_offset, 0);
        assert_eq!(app.logcat.dropped_lines, 0);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn logcat_drain_respects_receive_limit_per_cycle() {
        let mut app = App::default();
        let total = LOGCAT_RECV_LIMIT + 200;
        let (tx, rx) = mpsc::channel(total + 1);
        app.logcat.stream_rx = Some(rx);

        for i in 0..total {
            tx.try_send(threadtime_line(i))
                .expect("channel should accept test line");
        }
        drop(tx);

        app.drain_logcat_lines();

        let mut remaining = 0usize;
        if let Some(rx) = app.logcat.stream_rx.as_mut() {
            while rx.try_recv().is_ok() {
                remaining += 1;
            }
        }

        assert_eq!(remaining, 200);
        assert_eq!(app.logcat.logs.len(), LOGCAT_PARSE_BATCH);
        assert_eq!(
            app.logcat.dropped_lines,
            LOGCAT_RECV_LIMIT - LOGCAT_PARSE_BATCH
        );
    }
}
