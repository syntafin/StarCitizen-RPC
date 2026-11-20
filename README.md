# Star Citizen Presence

A Windows system tray application written in Rust that reads Star Citizen's `Game.log` and updates your Discord Rich Presence in real time.

## Features

- **Log-based state tracking:** Parses `Game.log` for game mode (Startup, Main Menu, Loading, Persistent Universe, Arena Commander), player name, system, shard, region, location, jurisdiction, armistice/hangar/quantum status, and current ship with manufacturer icons.
- **Multi-environment support:** Searches configured base directories for `LIVE`, `PTU`, `EPTU`, and `TECH-PREVIEW`, or locks to a specific environment via `preferred_env`.
- **Process check:** Automatically clears Discord Rich Presence when `StarCitizen.exe` is not running (configurable via `require_running_process`).
- **System tray integration:** Runs in the Windows tray with controls to pause/resume, reload `config.toml`, toggle Windows autostart, and quit.
- **Built-in auto-updater:** Checks GitHub Releases 5 seconds after startup and every 6 hours thereafter, replacing the executable in place when a newer release is available.

## Installation

Download the latest release from [GitHub Releases](https://github.com/syntafin/StarCitizen-RPC/releases):

- **`StarCitizenPresence-Setup.exe`**: Windows installer (installs to `%LOCALAPPDATA%\Programs\StarCitizenPresence` with an optional Windows autostart task).
- **`star-citizen-presence.exe`**: Standalone portable binary. Place `config.toml` in the same folder if you want custom settings.

## Configuration

Settings are read from `config.toml` next to the executable (falling back to the current working directory):

```toml
discord_app_id = "1573256731809554483"

# Base folders searched for LIVE / PTU / EPTU / TECH-PREVIEW
sc_base_dirs = [
    "D:\\Roberts Space Industries\\StarCitizen",
    "C:\\Program Files\\Roberts Space Industries\\StarCitizen",
]

# Optional: force a specific environment (e.g. "PTU" or "LIVE").
# Leave empty to auto-detect from the newest Game.log.
preferred_env = ""

# Poll interval for new log lines in milliseconds
poll_interval_ms = 1000

# Clear presence automatically when StarCitizen.exe is not running
require_running_process = true
```

## Development

### Prerequisites

- **Rust toolchain** (stable, Rust 2021 edition, `x86_64-pc-windows-msvc` target)
- **Inno Setup 6** (optional, only needed to build `StarCitizenPresence-Setup.exe` locally)

### Running and Testing

Run the application in debug mode:

```powershell
cargo run
```

Run the test suite:

```powershell
cargo test --locked
```

### Project Structure

| Path | Purpose |
| --- | --- |
| `src/main.rs` | Entry point, Windows tray icon, event loop, and worker thread |
| `src/config.rs` | `config.toml` loading and defaults |
| `src/log_watcher.rs` | `Game.log` discovery, tailing, and `StarCitizen.exe` process check |
| `src/parser.rs` | Regex-based parsing of `Game.log` lines into state updates |
| `src/state.rs` | Presence state machine and Discord Rich Presence formatting |
| `src/discord.rs` | Discord IPC client wrapper |
| `src/autostart.rs` | Windows registry (`HKCU\...\Run`) autostart management |
| `src/updater.rs` | GitHub Releases self-updater |
| `images/` | Tray icon and ship manufacturer assets |
| `installer.iss` | Inno Setup script for `StarCitizenPresence-Setup.exe` |
| `.github/workflows/release.yml` | CI workflow triggered on published GitHub releases |

## Building

### Release Binary

```powershell
cargo build --release --locked --target x86_64-pc-windows-msvc
```

The compiled binary is written to `target\x86_64-pc-windows-msvc\release\star-citizen-presence.exe`.

### Windows Installer

`installer.iss` expects the binary at `target\release\star-citizen-presence.exe`:

```powershell
New-Item -ItemType Directory -Force -Path target\release | Out-Null
Copy-Item target\x86_64-pc-windows-msvc\release\star-citizen-presence.exe target\release\star-citizen-presence.exe -Force
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" "/DAppVersion=0.1.0" installer.iss
```

The installer is output to `dist\StarCitizenPresence-Setup.exe`.

### Automated Releases

Publishing a GitHub release triggers `.github/workflows/release.yml`, which runs `cargo test --locked`, builds the release binary and Inno Setup installer, generates release notes from commit subjects since the previous tag, and attaches the executables and `config.toml` to the release.

## Commits & Pull Requests

### Commits

- Keep each commit focused on a single logical change.
- Write clear, imperative English commit messages (for example: `Add PTU log path fallback`, `Fix hangar state reset on menu return`).
- Release notes are generated directly from commit subjects (`git log --pretty=format:"- %s (%h)" --no-merges`), so avoid vague messages like `wip` or `fix`.

### Pull Requests

1. Create a branch from `main` for your change.
2. Run `cargo test --locked` and verify your changes locally before opening the PR.
3. Keep the PR scoped to the task and describe what changed, why, and how it was tested.
4. Target `main` and ensure there are no unresolved merge conflicts.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
