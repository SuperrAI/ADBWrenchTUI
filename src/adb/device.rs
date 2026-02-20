use anyhow::Result;

use super::client::AdbClient;
use super::types::{ConnectionState, DeviceInfo};

/// Manages device discovery and connection state.
pub struct DeviceManager {
    pub client: AdbClient,
    pub state: ConnectionState,
    pub current_device: Option<DeviceInfo>,
    pub devices: Vec<DeviceInfo>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            client: AdbClient::new(),
            state: ConnectionState::Disconnected,
            current_device: None,
            devices: Vec::new(),
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

    /// Disconnect from the current device.
    pub fn disconnect(&mut self) {
        self.client.set_serial(None);
        self.current_device = None;
        self.state = ConnectionState::Disconnected;
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
