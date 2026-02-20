use serde::{Deserialize, Serialize};

/// Connection state machine — mirrors the web app's states.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
}

/// Basic device identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub model: String,
    pub manufacturer: String,
    pub device: String,
    pub android_version: String,
    pub sdk_level: String,
    pub transport: String, // "usb" or "tcp"
    pub state: String,     // "device", "offline", "unauthorized"
}

/// Comprehensive device information for the dashboard.
#[derive(Debug, Clone, Default)]
pub struct FullDeviceInfo {
    pub identity: DeviceIdentity,
    pub build: BuildInfo,
    pub hardware: HardwareInfo,
    pub storage: StorageInfo,
    pub battery: BatteryInfo,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceIdentity {
    pub model: String,
    pub manufacturer: String,
    pub device: String,
    pub serial: String,
}

#[derive(Debug, Clone, Default)]
pub struct BuildInfo {
    pub android_version: String,
    pub sdk_level: String,
    pub build_fingerprint: String,
    pub build_date: String,
    pub security_patch: String,
}

#[derive(Debug, Clone, Default)]
pub struct HardwareInfo {
    pub cpu_architecture: String,
    pub hardware_platform: String,
    pub total_ram: String,
    pub display_resolution: String,
    pub display_density: String,
}

#[derive(Debug, Clone, Default)]
pub struct StorageInfo {
    pub total: String,
    pub used: String,
    pub available: String,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Default)]
pub struct BatteryInfo {
    pub level: u32,
    pub status: String,
    pub health: String,
    pub temperature: String,
}

/// Package information.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub package_name: String,
    pub is_system: bool,
    pub is_enabled: bool,
}

/// File entry from device filesystem.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
    pub permissions: String,
    pub is_symlink: bool,
}

/// Logcat entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn label(&self) -> &str {
        match self {
            Self::Verbose => "V",
            Self::Debug => "D",
            Self::Info => "I",
            Self::Warn => "W",
            Self::Error => "E",
            Self::Fatal => "F",
        }
    }

    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'V' => Some(Self::Verbose),
            'D' => Some(Self::Debug),
            'I' => Some(Self::Info),
            'W' => Some(Self::Warn),
            'E' => Some(Self::Error),
            'F' => Some(Self::Fatal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: String,
    pub user: String,
    pub name: String,
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub res: String,
    pub state: String,
    pub time: String,
}
