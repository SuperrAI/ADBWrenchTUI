use super::types::*;

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
    s.trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .unwrap_or(0)
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
    let volt: f64 = get("voltage").parse::<f64>().unwrap_or(0.0) / 1000.0;
    let technology = get("technology");

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
        voltage: format!("{volt:.2}V"),
        technology,
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
                let percent: f64 = parts[4]
                    .trim_end_matches('%')
                    .parse()
                    .unwrap_or(0.0);

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

    // Try threadtime format
    // Example: "01-15 10:30:45.123  1234  5678 I ActivityManager: Start proc..."
    let parts: Vec<&str> = line.splitn(6, char::is_whitespace).collect();
    if parts.len() >= 6 {
        let date_time = format!("{} {}", parts[0], parts[1]);
        let pid = parts[2].trim().to_string();
        let tid = parts[3].trim().to_string();
        let level_str = parts[4].trim();

        if let Some(level) = level_str.chars().next().and_then(LogLevel::from_char) {
            let rest = parts[5].trim();
            let (tag, message) = if let Some(colon_pos) = rest.find(':') {
                let tag = rest[..colon_pos].trim().to_string();
                let msg = rest[colon_pos + 1..].trim().to_string();
                (tag, msg)
            } else {
                (String::new(), rest.to_string())
            };

            return Some(LogEntry {
                timestamp: date_time,
                pid,
                tid,
                level,
                tag,
                message,
            });
        }
    }

    // Fallback: treat entire line as a message
    Some(LogEntry {
        timestamp: String::new(),
        pid: String::new(),
        tid: String::new(),
        level: LogLevel::Info,
        tag: String::new(),
        message: line.to_string(),
    })
}

/// Parse `top -n 1 -b` output for CPU usage and process list.
pub fn parse_top_output(output: &str) -> (f64, Vec<ProcessInfo>) {
    let mut cpu_percent = 0.0;
    let mut processes = Vec::new();
    let mut in_process_section = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // CPU line varies by Android version, look for common patterns
        if trimmed.contains("cpu") && trimmed.contains('%') && !in_process_section {
            // Try to parse combined CPU usage
            // e.g., "800%cpu  52%user   7%nice  45%sys ..."
            // or "Tasks: ...  CPU%idle: 85.3"
            if let Some(idle_str) = trimmed.split("idle").next() {
                if let Some(pct) = idle_str
                    .split_whitespace()
                    .last()
                    .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
                {
                    cpu_percent = 100.0 - pct;
                }
            }
        }

        // Detect process header
        if trimmed.starts_with("PID") || trimmed.contains("PID") && trimmed.contains("NAME") {
            in_process_section = true;
            continue;
        }

        if in_process_section && !trimmed.is_empty() {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 10 {
                // Standard top output columns vary, but PID is first and NAME is last
                let pid = parts[0].to_string();
                let name = parts.last().unwrap_or(&"").to_string();

                // CPU% and MEM% positions vary; common layout has them at indices 8 and 9
                let cpu_pct = parts
                    .iter()
                    .rev()
                    .skip(1) // skip name
                    .find_map(|s| s.trim_end_matches('%').parse::<f64>().ok())
                    .unwrap_or(0.0);

                processes.push(ProcessInfo {
                    pid,
                    user: parts.get(1).unwrap_or(&"").to_string(),
                    name,
                    cpu_percent: cpu_pct,
                    mem_percent: 0.0,
                });
            }
        }
    }

    (cpu_percent, processes)
}
