use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

fn try_send_lossy_line(
    on_line: &tokio::sync::mpsc::Sender<String>,
    dropped: &AtomicUsize,
    line: String,
) -> bool {
    match on_line.try_send(line) {
        Ok(()) => true,
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            dropped.fetch_add(1, Ordering::Relaxed);
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => false,
    }
}

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

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let reason = if !stderr.is_empty() { stderr } else { stdout };
            bail!(
                "adb shell command failed: `{command}`{}",
                if reason.is_empty() {
                    String::new()
                } else {
                    format!(" ({reason})")
                }
            );
        }

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

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let reason = if !stderr.is_empty() { stderr } else { stdout };
            let args_joined = args.join(" ");
            bail!(
                "adb command failed: `{args_joined}`{}",
                if reason.is_empty() {
                    String::new()
                } else {
                    format!(" ({reason})")
                }
            );
        }

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

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture adb shell stdout"))?;
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

    /// Stream output from an ADB shell command with bounded, lossy delivery.
    /// When the channel is full, newest lines are dropped and counted in `dropped`.
    pub async fn shell_stream_lossy(
        &self,
        command: &str,
        on_line: tokio::sync::mpsc::Sender<String>,
        dropped: Arc<AtomicUsize>,
    ) -> Result<tokio::process::Child> {
        let mut cmd = Command::new("adb");
        if let Some(ref serial) = self.serial {
            cmd.args(["-s", serial]);
        }
        cmd.arg("shell").arg(command);
        cmd.stdout(Stdio::piped()).stderr(Stdio::null());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().context("Failed to spawn adb shell")?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture adb shell stdout"))?;
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                if !try_send_lossy_line(&on_line, &dropped, line) {
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

    /// Pull a file from the device to the local filesystem.
    pub async fn pull(&self, remote: &str, local: &str) -> Result<String> {
        self.exec(&["pull", remote, local]).await
    }
}

impl Default for AdbClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn lossy_send_counts_drop_when_channel_is_full() {
        let (tx, mut rx) = mpsc::channel(2);
        let dropped = Arc::new(AtomicUsize::new(0));

        assert!(try_send_lossy_line(&tx, &dropped, "line-1".to_string()));
        assert!(try_send_lossy_line(&tx, &dropped, "line-2".to_string()));
        assert!(try_send_lossy_line(&tx, &dropped, "line-3".to_string()));

        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert_eq!(rx.try_recv().ok().as_deref(), Some("line-1"));
        assert_eq!(rx.try_recv().ok().as_deref(), Some("line-2"));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn lossy_send_stops_when_receiver_is_closed() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let dropped = Arc::new(AtomicUsize::new(0));

        assert!(!try_send_lossy_line(&tx, &dropped, "line".to_string()));
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn mocked_lossy_pipeline_reports_drops_and_keeps_delivered_order() {
        let (tx, mut rx) = mpsc::channel(4);
        let dropped = Arc::new(AtomicUsize::new(0));
        let producer_dropped = dropped.clone();

        let producer = tokio::spawn(async move {
            for i in 0..10 {
                if !try_send_lossy_line(&tx, &producer_dropped, format!("line-{i}")) {
                    break;
                }
            }
        });
        producer.await.expect("producer task should finish");

        let mut delivered = Vec::new();
        while let Ok(line) = rx.try_recv() {
            delivered.push(line);
        }

        assert_eq!(dropped.load(Ordering::Relaxed), 6);
        assert_eq!(delivered, vec!["line-0", "line-1", "line-2", "line-3"]);
    }
}
