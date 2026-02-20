use anyhow::Result;

use super::client::AdbClient;
use super::parser;
use super::types::*;

/// Manages device discovery and connection state.
pub struct DeviceManager {
    pub client: AdbClient,
    pub state: ConnectionState,
    pub current_device: Option<DeviceInfo>,
    pub devices: Vec<DeviceInfo>,
    pub full_info: Option<FullDeviceInfo>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            client: AdbClient::new(),
            state: ConnectionState::Disconnected,
            current_device: None,
            devices: Vec::new(),
            full_info: None,
        }
    }

    /// Refresh the list of connected devices via `adb devices -l`.
    pub async fn refresh_devices(&mut self) -> Result<()> {
        let output = self.client.exec(&["devices", "-l"]).await?;
        self.devices = parse_device_list(&output);

        // Auto-select single device
        if self.devices.len() == 1 && self.current_device.is_none() {
            self.connect_to(&self.devices[0].serial.clone()).await?;
        }

        Ok(())
    }

    /// Connect to a specific device by serial.
    pub async fn connect_to(&mut self, serial: &str) -> Result<()> {
        self.state = ConnectionState::Connecting;
        self.client.set_serial(Some(serial.to_string()));

        // Verify device is reachable
        let model = self.client.getprop("ro.product.model").await?;
        if model.is_empty() {
            self.state = ConnectionState::Disconnected;
            anyhow::bail!("Device not responding");
        }

        let manufacturer = self
            .client
            .getprop("ro.product.manufacturer")
            .await
            .unwrap_or_default();
        let device = self
            .client
            .getprop("ro.product.device")
            .await
            .unwrap_or_default();
        let android_version = self
            .client
            .getprop("ro.build.version.release")
            .await
            .unwrap_or_default();
        let sdk_level = self
            .client
            .getprop("ro.build.version.sdk")
            .await
            .unwrap_or_default();

        self.current_device = Some(DeviceInfo {
            serial: serial.to_string(),
            model,
            manufacturer,
            device,
            android_version,
            sdk_level,
            transport: String::new(),
            state: "device".to_string(),
        });
        self.state = ConnectionState::Connected;

        Ok(())
    }

    /// Fetch comprehensive device info for the dashboard.
    /// Runs multiple ADB commands concurrently for speed.
    pub async fn fetch_full_info(&mut self) -> Result<()> {
        if !self.is_connected() {
            anyhow::bail!("No device connected");
        }

        let serial = self
            .current_device
            .as_ref()
            .map(|d| d.serial.clone())
            .unwrap_or_default();

        // Run all data-gathering commands concurrently
        let (
            model,
            manufacturer,
            device_name,
            android_ver,
            sdk,
            fingerprint,
            build_date,
            security_patch,
            cpu_abi,
            hardware,
            meminfo_raw,
            wm_size_raw,
            wm_density_raw,
            df_raw,
            battery_raw,
        ) = tokio::join!(
            self.client.getprop("ro.product.model"),
            self.client.getprop("ro.product.manufacturer"),
            self.client.getprop("ro.product.device"),
            self.client.getprop("ro.build.version.release"),
            self.client.getprop("ro.build.version.sdk"),
            self.client.getprop("ro.build.fingerprint"),
            self.client.getprop("ro.build.date"),
            self.client.getprop("ro.build.version.security_patch"),
            self.client.getprop("ro.product.cpu.abi"),
            self.client.getprop("ro.hardware"),
            self.client.shell("cat /proc/meminfo"),
            self.client.shell("wm size"),
            self.client.shell("wm density"),
            self.client.shell("df /data"),
            self.client.shell("dumpsys battery"),
        );

        // Parse meminfo
        let meminfo_str = meminfo_raw.unwrap_or_default();
        let (total_kb, _used_kb) = parser::parse_meminfo(&meminfo_str);
        let total_ram = if total_kb > 0 {
            format!("{:.1} GB", total_kb as f64 / 1024.0 / 1024.0)
        } else {
            "Unknown".to_string()
        };

        // Parse display
        let wm_size_str = wm_size_raw.unwrap_or_default();
        let display_resolution = wm_size_str
            .lines()
            .find_map(|l| {
                l.split(':')
                    .nth(1)
                    .map(|v| v.trim().to_string())
            })
            .unwrap_or_else(|| "Unknown".to_string());

        let wm_density_str = wm_density_raw.unwrap_or_default();
        let display_density = wm_density_str
            .lines()
            .find_map(|l| {
                l.split(':')
                    .nth(1)
                    .map(|v| format!("{} dpi", v.trim()))
            })
            .unwrap_or_else(|| "Unknown".to_string());

        // Parse storage and battery
        let storage = parser::parse_storage(&df_raw.unwrap_or_default());
        let battery = parser::parse_battery(&battery_raw.unwrap_or_default());

        self.full_info = Some(FullDeviceInfo {
            identity: DeviceIdentity {
                model: model.unwrap_or_default(),
                manufacturer: manufacturer.unwrap_or_default(),
                device: device_name.unwrap_or_default(),
                serial,
            },
            build: BuildInfo {
                android_version: android_ver.unwrap_or_default(),
                sdk_level: sdk.unwrap_or_default(),
                build_fingerprint: fingerprint.unwrap_or_default(),
                build_date: build_date.unwrap_or_default(),
                security_patch: security_patch.unwrap_or_default(),
            },
            hardware: HardwareInfo {
                cpu_architecture: cpu_abi.unwrap_or_default(),
                hardware_platform: hardware.unwrap_or_default(),
                total_ram,
                display_resolution,
                display_density,
            },
            storage,
            battery,
        });

        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse `adb devices -l` output into DeviceInfo entries.
fn parse_device_list(output: &str) -> Vec<DeviceInfo> {
    let mut devices = Vec::new();

    for line in output.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let serial = parts[0].to_string();
        let state = parts[1].to_string();

        // Parse key:value pairs
        let mut model = String::new();
        let mut device = String::new();
        let mut transport = String::new();

        for part in &parts[2..] {
            if let Some(val) = part.strip_prefix("model:") {
                model = val.to_string();
            } else if let Some(val) = part.strip_prefix("device:") {
                device = val.to_string();
            } else if let Some(val) = part.strip_prefix("transport_id:") {
                transport = val.to_string();
            }
        }

        devices.push(DeviceInfo {
            serial,
            model,
            manufacturer: String::new(),
            device,
            android_version: String::new(),
            sdk_level: String::new(),
            transport,
            state,
        });
    }

    devices
}
