use super::types::*;
use crate::app::{PackageDetails, SettingEntry};

/// Parse `/proc/meminfo` output to extract memory values.
pub fn parse_meminfo(output: &str) -> (u64, u64) {
    let mut total_kb = 0u64;
    let mut available_kb = 0u64;

    for line in output.lines() {
        if let Some(val) = line.strip_prefix("MemTotal:") {
            total_kb = parse_kb_value(val);
        } else if let Some(val) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_kb_value(val);
        }
    }

    let used_kb = total_kb.saturating_sub(available_kb);
    (total_kb, used_kb)
}

fn parse_kb_value(s: &str) -> u64 {
    s.trim().trim_end_matches("kB").trim().parse().unwrap_or(0)
}

/// Parse `dumpsys battery` output.
pub fn parse_battery(output: &str) -> BatteryInfo {
    let get = |key: &str| -> String {
        for line in output.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(key) {
                if let Some(val) = rest.strip_prefix(':') {
                    return val.trim().to_string();
                }
            }
        }
        String::new()
    };

    let level: u32 = get("level").parse().unwrap_or(0);
    let status_code: u32 = get("status").parse().unwrap_or(1);
    let health_code: u32 = get("health").parse().unwrap_or(1);
    let temp: f64 = get("temperature").parse::<f64>().unwrap_or(0.0) / 10.0;

    let status = match status_code {
        2 => "Charging",
        3 => "Discharging",
        4 => "Not Charging",
        5 => "Full",
        _ => "Unknown",
    }
    .to_string();

    let health = match health_code {
        2 => "Good",
        3 => "Overheat",
        4 => "Dead",
        5 => "Over Voltage",
        6 => "Failure",
        7 => "Cold",
        _ => "Unknown",
    }
    .to_string();

    BatteryInfo {
        level,
        status,
        health,
        temperature: format!("{temp:.1}°C"),
    }
}

/// Parse `df /data` output for storage info.
pub fn parse_storage(output: &str) -> StorageInfo {
    for line in output.lines() {
        if line.contains("/data") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                let total = parts[1].to_string();
                let used = parts[2].to_string();
                let available = parts[3].to_string();
                let percent: f64 = parts[4].trim_end_matches('%').parse().unwrap_or(0.0);

                return StorageInfo {
                    total: format_storage_size(&total),
                    used: format_storage_size(&used),
                    available: format_storage_size(&available),
                    usage_percent: percent,
                };
            }
        }
    }

    StorageInfo::default()
}

fn format_storage_size(s: &str) -> String {
    // Try to parse as a number with optional G/M/K suffix
    let s = s.trim();
    if s.ends_with('G') {
        return s.to_string();
    }
    if s.ends_with('M') {
        if let Ok(val) = s.trim_end_matches('M').parse::<f64>() {
            return format!("{:.1}G", val / 1024.0);
        }
    }
    if s.ends_with('K') {
        if let Ok(val) = s.trim_end_matches('K').parse::<f64>() {
            return format!("{:.1}G", val / 1024.0 / 1024.0);
        }
    }
    s.to_string()
}

/// Parse a single logcat line in threadtime format.
/// Format: `MM-DD HH:MM:SS.mmm  PID  TID LEVEL TAG: message`
pub fn parse_logcat_line(line: &str) -> Option<LogEntry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('-') {
        return None;
    }

    // Try threadtime format.
    // Example: "01-15 10:30:45.123  1234  5678 I ActivityManager: Start proc..."
    if let Some((date, rest)) = take_ws_token(line)
        && let Some((time, rest)) = take_ws_token(rest)
        && let Some((_pid, rest)) = take_ws_token(rest)
        && let Some((_tid, rest)) = take_ws_token(rest)
        && let Some((level_str, rest)) = take_ws_token(rest)
        && let Some(level) = level_str.chars().next().and_then(LogLevel::from_char)
    {
        let payload = rest.trim_start();
        let (tag, message) = if let Some((tag, msg)) = payload.split_once(':') {
            (tag.trim().to_string(), msg.trim().to_string())
        } else {
            (String::new(), payload.to_string())
        };

        return Some(LogEntry {
            timestamp: format!("{date} {time}"),
            level,
            tag,
            message,
        });
    }

    // Fallback: treat entire line as a message
    Some(LogEntry {
        timestamp: String::new(),
        level: LogLevel::Info,
        tag: String::new(),
        message: line.to_string(),
    })
}

fn take_ws_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    let split_at = s
        .char_indices()
        .find_map(|(i, ch)| ch.is_whitespace().then_some(i))
        .unwrap_or(s.len());
    Some((&s[..split_at], &s[split_at..]))
}

/// Parse `ls -la` output into FileEntry entries.
pub fn parse_ls_output(output: &str, parent_path: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    let parent = if parent_path.ends_with('/') {
        parent_path.to_string()
    } else {
        format!("{parent_path}/")
    };

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("total") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }

        let permissions = parts[0].to_string();
        if permissions.len() < 2 {
            continue;
        }

        let is_directory = permissions.starts_with('d');
        let is_symlink = permissions.starts_with('l');
        let (size, size_idx) = if let Some(v) = parts.get(4).and_then(|s| s.parse::<u64>().ok()) {
            (v, 4usize)
        } else if let Some(v) = parts.get(3).and_then(|s| s.parse::<u64>().ok()) {
            (v, 3usize)
        } else {
            continue;
        };

        // Android toybox often emits: "... SIZE YYYY-MM-DD HH:MM NAME"
        // Some variants emit: "... SIZE MON DD HH:MM NAME"
        let has_iso_date = parts
            .get(size_idx + 1)
            .map(|s| s.chars().all(|c| c.is_ascii_digit() || c == '-'))
            .unwrap_or(false)
            && parts
                .get(size_idx + 2)
                .map(|s| s.contains(':'))
                .unwrap_or(false);
        let name_start = if has_iso_date {
            size_idx + 3
        } else {
            size_idx + 4
        };
        if name_start >= parts.len() {
            continue;
        }
        let name_part = parts[name_start..].join(" ");

        // Handle symlinks: "name -> target"
        let name = if is_symlink {
            if let Some(arrow) = name_part.find(" -> ") {
                name_part[..arrow].trim().to_string()
            } else {
                name_part.trim().to_string()
            }
        } else {
            name_part.trim().to_string()
        };

        // Skip . and ..
        if name == "." || name == ".." {
            continue;
        }

        entries.push(FileEntry {
            path: format!("{parent}{name}"),
            name,
            is_directory,
            size,
            permissions,
            is_symlink,
        });
    }

    // Sort: directories first, then by name
    entries.sort_by(|a, b| {
        b.is_directory
            .cmp(&a.is_directory)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    entries
}

/// Parse `pm list packages -f` output into PackageInfo entries.
pub fn parse_package_list(output: &str) -> Vec<PackageInfo> {
    let mut packages = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("package:") {
            // Format: package:/path/to/apk=com.package.name
            if let Some(eq_pos) = rest.rfind('=') {
                let path = rest[..eq_pos].to_string();
                let package_name = rest[eq_pos + 1..].to_string();
                let is_system = path.starts_with("/system")
                    || path.starts_with("/product")
                    || path.starts_with("/vendor");

                packages.push(PackageInfo {
                    package_name,
                    is_system,
                    is_enabled: true,
                });
            }
        }
    }

    packages.sort_by(|a, b| a.package_name.cmp(&b.package_name));
    packages
}

/// Parse `dumpsys package <name>` output into PackageDetails.
pub fn parse_package_details(output: &str, package_name: &str) -> PackageDetails {
    let get_field = |prefix: &str| -> String {
        for line in output.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                return rest.trim_start_matches('=').trim().to_string();
            }
        }
        String::new()
    };

    let version_name = get_field("versionName=");
    let version_code = get_field("versionCode=")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let installed_path = get_field("codePath=");
    let first_install = get_field("firstInstallTime=");
    let last_update = get_field("lastUpdateTime=");

    // Parse permissions
    let mut permissions = Vec::new();
    let mut in_permissions = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("requested permissions:")
            || trimmed.starts_with("install permissions:")
        {
            in_permissions = true;
            continue;
        }
        if in_permissions {
            if trimmed.is_empty()
                || (!trimmed.starts_with("android.permission.") && !trimmed.starts_with("com."))
            {
                if !trimmed.starts_with("android.permission.") && !trimmed.starts_with("com.") {
                    in_permissions = false;
                    continue;
                }
            }
            let perm = trimmed
                .split(':')
                .next()
                .unwrap_or(trimmed)
                .trim()
                .to_string();
            if !permissions.contains(&perm) {
                permissions.push(perm);
            }
        }
    }

    PackageDetails {
        package_name: package_name.to_string(),
        version_name,
        version_code,
        installed_path,
        first_install_time: first_install,
        last_update_time: last_update,
        permissions,
    }
}

/// Parse `settings list <namespace>` output.
pub fn parse_settings_list(output: &str) -> Vec<SettingEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].to_string();
            let value = line[eq_pos + 1..].to_string();
            entries.push(SettingEntry { key, value });
        }
    }
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    entries
}

/// Parse `top -n 1 -b` output for CPU usage and process list.
pub fn parse_top_output(output: &str) -> (f64, Vec<ProcessInfo>) {
    let mut cpu_percent = 0.0;
    let mut total_cpu = 0.0_f64; // Total CPU capacity (num_cores * 100)
    let mut processes = Vec::new();
    let mut in_process_section = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // CPU line varies by Android version, look for common patterns
        // e.g., "800%cpu  52%user   7%nice  45%sys  307%idle ..."
        // The total (800%) is per-core sum, so on 8 cores idle can be > 100%.
        // We compute: usage = (total - idle) / total * 100
        if trimmed.contains("cpu") && trimmed.contains('%') && !in_process_section {
            let mut idle = 0.0_f64;
            for token in trimmed.split_whitespace() {
                if token.ends_with("%cpu") {
                    if let Ok(v) = token.trim_end_matches("%cpu").parse::<f64>() {
                        total_cpu = v;
                    }
                } else if token.ends_with("%idle") {
                    if let Ok(v) = token.trim_end_matches("%idle").parse::<f64>() {
                        idle = v;
                    }
                }
            }
            if total_cpu > 0.0 {
                cpu_percent = ((total_cpu - idle) / total_cpu * 100.0).clamp(0.0, 100.0);
            }
        }

        // Detect process header
        if trimmed.starts_with("PID") || trimmed.contains("PID") && trimmed.contains("NAME") {
            in_process_section = true;
            continue;
        }

        if in_process_section && !trimmed.is_empty() {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            // Standard Android top: PID(0) USER(1) PR(2) NI(3) VIRT(4) RES(5) SHR(6) S(7) %CPU(8) %MEM(9) TIME+(10) ARGS(11+)
            if parts.len() >= 12 {
                let pid = parts[0].to_string();
                let name = parts[11..].join(" "); // ARGS can contain spaces

                let raw_cpu = parts
                    .get(8)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                let mem_pct = parts
                    .get(9)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);

                // Normalize per-process CPU by total cores (total_cpu = num_cores * 100)
                let normalized_cpu = if total_cpu > 0.0 {
                    (raw_cpu / total_cpu * 100.0).clamp(0.0, 100.0)
                } else {
                    raw_cpu.clamp(0.0, 100.0)
                };

                processes.push(ProcessInfo {
                    pid,
                    user: parts.get(1).unwrap_or(&"").to_string(),
                    name,
                    cpu_percent: normalized_cpu,
                    mem_percent: mem_pct,
                    res: parts.get(5).unwrap_or(&"").to_string(),
                    state: parts.get(7).unwrap_or(&"").to_string(),
                    time: parts.get(10).unwrap_or(&"").to_string(),
                });
            }
        }
    }

    (cpu_percent, processes)
}
