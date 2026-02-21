# >_ ADB Wrench TUI

A terminal UI for Android debugging over ADB. The TUI counterpart of [ADB Wrench](https://adbwrench.com/) by [Superr](https://superr.ai).

## Features

- **Dashboard** — Device info, battery, memory, storage, running processes at a glance
- **Shell** — Interactive ADB shell with command history
- **Logcat** — Real-time log viewer with level/tag/text filtering
- **Screen** — Screenshot capture with inline terminal preview, screen recording
- **Apps** — List, search, install, uninstall, force-stop, clear data
- **Files** — Browse device filesystem, pull files, create directories
- **Controls** — Reboot modes, volume/brightness, input text, key events
- **Bugreport** — Generate and download bugreports
- **Settings** — Browse and edit Android system/secure/global settings

## Prerequisites

- [ADB](https://developer.android.com/tools/adb) installed and on your `PATH`
- A connected Android device with USB debugging enabled

## Installation

### Download a release

Grab the latest binary for your platform from [Releases](https://github.com/SuperrAI/ADBWrenchTUI/releases).

```bash
# macOS / Linux
chmod +x adbwrenchtui-*
./adbwrenchtui-<platform>

# Verify checksum
sha256sum -c checksums-sha256.txt
```

### Build from source

Requires [Rust](https://rustup.rs/) (stable).

```bash
git clone https://github.com/SuperrAI/ADBWrenchTUI.git
cd ADBWrenchTUI
cargo build --release
./target/release/adbwrenchtui
```

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `1`-`9`, `0` | Jump to page (sidebar) |
| `j` / `k` | Navigate up/down |
| `Tab` | Toggle sidebar / content focus |
| `Enter` | Select / confirm |
| `Esc` | Back to sidebar |
| `Ctrl+C` / `q` | Quit |

### Page-specific

Each page shows its available keybindings in the footer bar.

## Configuration

Config is stored at `~/.config/adbwrenchtui/config.json`.

```json
{
  "output_dir": "/path/to/save/captures"
}
```

- **output_dir** — Where screenshots, recordings, and bugreports are saved (default: current directory)
- Change it from the Screen page by pressing `p`

## Debug Logging

Logs are written to `adbwrenchtui.log` in the working directory:

```bash
tail -f adbwrenchtui.log
```

## License

[PolyForm Noncommercial 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0/)

## Links

- [ADB Wrench (Web)](https://adbwrench.com/)
- [Superr](https://superr.ai)
