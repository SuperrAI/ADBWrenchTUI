use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Wrapper around the `adb` command-line tool.
#[derive(Debug, Clone)]
pub struct AdbClient {
    /// Serial of the target device (for -s flag). None = use default device.
    serial: Option<String>,
}

impl AdbClient {
    pub fn new() -> Self {
        Self { serial: None }
    }

    pub fn with_serial(serial: impl Into<String>) -> Self {
        Self {
            serial: Some(serial.into()),
        }
    }

    pub fn set_serial(&mut self, serial: Option<String>) {
        self.serial = serial;
    }

    /// Execute an ADB shell command and return stdout as a string.
    pub async fn shell(&self, command: &str) -> Result<String> {
        let mut cmd = Command::new("adb");
        if let Some(ref serial) = self.serial {
            cmd.args(["-s", serial]);
        }
        cmd.arg("shell").arg(command);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .context("Failed to execute adb command")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Execute a raw ADB command (not shell).
    pub async fn exec(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new("adb");
        if let Some(ref serial) = self.serial {
            cmd.args(["-s", serial]);
        }
        cmd.args(args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to execute adb")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Stream output from an ADB shell command line-by-line.
    /// Returns a handle that can be used to kill the process.
    pub async fn shell_stream(
        &self,
        command: &str,
        on_line: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> Result<tokio::process::Child> {
        let mut cmd = Command::new("adb");
        if let Some(ref serial) = self.serial {
            cmd.args(["-s", serial]);
        }
        cmd.arg("shell").arg(command);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().context("Failed to spawn adb shell")?;

        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                if on_line.send(line).is_err() {
                    break;
                }
            }
        });

        Ok(child)
    }

    /// Get a system property via `getprop`.
    pub async fn getprop(&self, prop: &str) -> Result<String> {
        let output = self.shell(&format!("getprop {prop}")).await?;
        Ok(output.trim().to_string())
    }
}

impl Default for AdbClient {
    fn default() -> Self {
        Self::new()
    }
}
