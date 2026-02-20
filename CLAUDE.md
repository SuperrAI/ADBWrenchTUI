# ADBWrenchTUI — Development Guide

## Project Overview

ADBWrenchTUI is a terminal UI version of [ADBWrench](https://adbwrench.com/), a browser-based Android debugging tool by Superr. The TUI version provides the same ADB device management capabilities in a keyboard-driven terminal interface, built with Rust and Ratatui.

**Reference web app:** `../ADBWrench/` — always consult the web app source for feature parity and visual reference.

## Architecture

```
src/
├── main.rs            # Entry point, event loop, terminal setup/teardown
├── app.rs             # App state machine, page enum, global keybindings
├── event.rs           # Crossterm event polling via tokio mpsc channel
├── tui.rs             # Terminal init/restore (alternate screen, raw mode)
├── theme.rs           # Color palette and style helpers (matches web app)
├── adb/               # ADB interaction layer
│   ├── mod.rs         # Re-exports
│   ├── client.rs      # Wraps `adb` CLI: shell, exec, stream, getprop
│   ├── device.rs      # Device discovery, connection state machine
│   ├── types.rs       # All shared data types (DeviceInfo, LogEntry, etc.)
│   └── parser.rs      # Output parsers (meminfo, battery, logcat, top, etc.)
├── ui/                # UI rendering (one file per page)
│   ├── mod.rs         # Top-level render dispatch + shared helpers
│   ├── sidebar.rs     # Left sidebar navigation
│   ├── dashboard.rs   # Device info cards
│   ├── shell.rs       # Interactive shell
│   ├── logcat.rs      # Log viewer with filters
│   ├── files.rs       # File browser
│   ├── apps.rs        # Package manager
│   ├── performance.rs # CPU/memory/battery monitoring
│   ├── controls.rs    # Device controls (reboot, volume, etc.)
│   ├── settings.rs    # Android settings editor
│   ├── bugreport.rs   # Bugreport generator
│   └── screen.rs      # Screenshot capture
└── components/        # Reusable widgets (progress bars, grids, etc.)
    └── mod.rs
```

## Tech Stack

| Layer | Crate | Purpose |
|-------|-------|---------|
| TUI framework | `ratatui` 0.29 | Widgets, layout, rendering |
| Terminal backend | `crossterm` 0.28 | Cross-platform terminal I/O |
| Async runtime | `tokio` 1.x | Async ADB commands, event handling |
| Error handling | `anyhow`, `thiserror` | Application and library errors |
| Logging | `tracing` + `tracing-appender` | Debug logging to file (never stdout) |
| Parsing | `regex` | ADB output parsing |
| Time | `chrono` | Timestamps |
| Serialization | `serde`, `serde_json` | Config persistence |

## Design Principles

### Visual Identity
- **Match the ADBWrench web app** as closely as terminal constraints allow
- **Dark theme only** — background `#171717`, border `#404040`, text `#f5f5f5`
- **Orange accent** (`#f97316`) for active elements, prompts, highlights
- **Monospace everywhere** — the terminal is inherently monospace, matching IBM Plex Mono
- **Uppercase labels** for section headers and status indicators
- **Rounded borders** (`BorderType::Rounded`) on content cards
- **Bracket-wrapped buttons** like `[ ACTION ]` for visual consistency with the web app

### Layout
- **Fixed sidebar** (26 cols) on the left, content on the right — mirrors the web app's 220px sidebar
- **Sidebar sections:** MAIN (Dashboard), TOOLS (Shell, Logcat, Screen, Apps, Files), SYSTEM (Controls, Perf, Bugreport, Settings)
- **Page structure:** Every page has a header bar, scrollable content area, and optional footer
- **Device status** always visible at the bottom of the sidebar

### Navigation & Keybindings
- **Tab** toggles focus between sidebar and content
- **j/k or ↑/↓** navigate sidebar items
- **1-9, 0** jump to pages directly
- **Enter** from sidebar enters content area
- **Esc** returns focus to sidebar
- **Ctrl+C** or **q** (in sidebar) quits
- **Page-specific keys** documented in each page's footer

### ADB Interaction
- All ADB commands go through `adb::client::AdbClient` which wraps the `adb` CLI tool
- **Never call `adb` directly** from UI code — always go through the client
- Use `shell()` for one-shot commands, `shell_stream()` for streaming (logcat, bugreport)
- Device discovery via `adb devices -l`, connection state managed by `DeviceManager`
- Parsers in `adb/parser.rs` handle all ADB output parsing — keep them testable and pure

## Conventions

### Rust Style
- `cargo clippy` must pass with no warnings before committing
- Prefer `anyhow::Result` in application code, `thiserror` for library-style errors
- All public items get a doc comment
- No `unwrap()` in production code — use `?` or `.unwrap_or_default()`
- Async functions for ADB interaction, sync for pure UI rendering

### File Organization
- One page = one file in `ui/`
- Every `ui/*.rs` file exports a single `pub fn render(app: &App, frame: &mut Frame, area: Rect)`
- Page-specific state goes into `App` struct fields (not module-level globals)
- Reusable rendering logic goes in `ui/mod.rs` or `components/`

### Rendering Rules
- All `render()` functions receive the full `App` state immutably
- Render functions must never mutate state — state changes happen in the event loop
- Use `Theme::*` constants and helpers for ALL colors — never hardcode RGB values
- Always check `app.device_manager.is_connected()` and show disconnected state if false
- Use `Layout::horizontal/vertical` with `Constraint` for all layouts — no absolute positioning

### Theming (theme.rs)
```rust
Theme::BG            // #171717 — main background
Theme::BG_ELEVATED   // #262626 — cards, elevated surfaces
Theme::BORDER        // #404040 — all borders
Theme::FG            // #f5f5f5 — primary text
Theme::FG_DIM        // #a3a3a3 — secondary text
Theme::FG_MUTED      // #737373 — hints, disabled text
Theme::ORANGE        // #f97316 — primary accent (active nav, prompts, highlights)
Theme::GREEN         // #22c55e — connected, success
Theme::RED           // #ef4444 — errors, destructive actions
Theme::YELLOW        // #eab308 — warnings
```

## Build & Run

```bash
cargo run                    # Run in debug mode
cargo run --release          # Run in release mode
cargo build --release        # Build optimized binary
cargo clippy                 # Lint
cargo test                   # Run tests
```

Debug logs are written to `./adbwrenchtui.log` — tail this file while developing:
```bash
tail -f adbwrenchtui.log
```

## Pages & Feature Mapping

| Web App Page | TUI Page | Status |
|---|---|---|
| Dashboard | `ui/dashboard.rs` | Skeleton |
| Shell | `ui/shell.rs` | Placeholder |
| Logcat | `ui/logcat.rs` | Placeholder |
| Screen (Screenshot tab) | `ui/screen.rs` | Placeholder |
| Screen (Live View) | N/A | Not feasible in TUI |
| Screen (Record) | `ui/screen.rs` | Placeholder |
| Apps | `ui/apps.rs` | Placeholder |
| Files | `ui/files.rs` | Placeholder |
| Controls | `ui/controls.rs` | Placeholder |
| Performance | `ui/performance.rs` | Placeholder |
| Bugreport | `ui/bugreport.rs` | Placeholder |
| Settings | `ui/settings.rs` | Placeholder |

## ADB Commands Reference

Commonly used ADB commands (from the web app):

```
# Device info
getprop ro.product.model|manufacturer|device
getprop ro.build.version.release|sdk|security_patch

# Shell
adb shell <command>

# Logcat
adb shell logcat -v threadtime [-b main|system|crash|events|all]

# Files
adb shell ls -la "<path>"
adb pull <remote> <local>
adb push <local> <remote>

# Apps
adb shell pm list packages [-f] [-3]
adb shell dumpsys package <name>
adb shell am start -n <activity>
adb shell am force-stop <package>
adb shell pm clear <package>
adb shell pm uninstall <package>

# Performance
adb shell top -n 1 -b -m 10
adb shell cat /proc/meminfo
adb shell dumpsys battery

# Controls
adb shell input keyevent <code>
adb shell input text "<text>"
adb shell settings put system screen_brightness <0-255>
adb reboot [recovery|bootloader]

# Settings
adb shell settings list [system|secure|global]
adb shell settings get <namespace> <key>
adb shell settings put <namespace> <key> <value>
adb shell settings delete <namespace> <key>

# Screenshot
adb shell screencap -p > screenshot.png

# Bugreport
adb shell bugreportz
```
