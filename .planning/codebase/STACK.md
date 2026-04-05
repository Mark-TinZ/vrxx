# Technology Stack

**Analysis Date:** 2025-02-13

## Languages

**Primary:**
- Rust 2021 - Core application logic, daemon, and UI bindings.

**Secondary:**
- Meson - Build system configuration.
- XML - GTK4 UI definitions (`.ui`), GResource manifests (`.xml`), and desktop/D-Bus metadata.
- Bash/Shell - Installation and uninstallation scripts.

## Runtime

**Environment:**
- Linux - Primary target platform.
- GTK4 4.16+ - UI framework.
- D-Bus - IPC between client and privileged daemon.
- systemd - Service management and D-Bus activation.

**Package Manager:**
- Cargo (Rust) - Dependency management and compilation.
- Lockfile: `Cargo.lock` present.

## Frameworks

**Core:**
- GTK4 (`gtk4` crate) - Primary UI toolkit.
- Libadwaita (`libadwaita` crate) - Modern GNOME-style UI components.
- Tokio 1.0 - Async runtime for the daemon and background tasks.

**Testing:**
- Native Rust tests - Unit tests for configuration generation and parsing.

**Build/Dev:**
- Meson - Top-level build system.
- `build.rs` - Rust build script for resource compilation (GResource, Gettext) and Meson integration.

## Key Dependencies

**Critical:**
- `zbus` 5.0 - High-level D-Bus implementation for IPC.
- `zbus_polkit` - Polkit integration for daemon authorization.
- `serde` / `serde_json` - JSON serialization/deserialization for settings and proxy configs.
- `nix` - Unix-specific system calls (signals, process management).

**Infrastructure:**
- `reqwest` - HTTP client for downloading GeoIP/Geosite databases.
- `tracing` / `tracing-subscriber` - Logging and observability.
- `dirs` - Cross-platform directory path resolution.
- `anyhow` / `thiserror` - Error handling patterns.

## Configuration

**Environment:**
- `XDG_CONFIG_HOME/vrxx/` - Configuration directory (defaults to `~/.config/vrxx/`).
- `XDG_DATA_HOME/` - Desktop entries and icon storage.

**Build:**
- `meson.build` - Main build configuration.
- `Cargo.toml` - Rust project metadata.
- `src/config.rs.in` - Template for build-time constants (version, paths).

## Platform Requirements

**Development:**
- Rust toolchain (cargo, rustc).
- GTK4 and Libadwaita development headers.
- D-Bus development headers.
- `glib-compile-resources` and `gettext` tools.

**Production:**
- `xray` or `sing-box` binaries (must be in PATH).
- D-Bus System Bus.
- Polkit (for privileged daemon).

---

*Stack analysis: 2025-02-13*
