# Architecture

**Analysis Date:** 2025-01-24

## Pattern Overview

**Overall:** Privilege Separation with D-Bus IPC

**Key Characteristics:**
- **Privilege Separation:** The application is split into an unprivileged GTK UI and a privileged system daemon.
- **D-Bus Communication:** All interactions between the UI and the daemon occur over the D-Bus System Bus.
- **Core Abstraction:** Multiple proxy cores (Xray, Sing-box) are supported through a unified configuration generation and management interface.

## Layers

**UI Layer (Unprivileged):**
- Purpose: Handles user interaction, settings management, and configuration generation.
- Location: `src/ui/`
- Contains: GTK/Adwaita components, pages, and UI-specific models.
- Depends on: `src/backend.rs`, `src/domain/`, `src/settings.rs`, `src/ipc.rs` (proxy).
- Used by: End-user.

**Daemon Layer (Privileged):**
- Purpose: Manages the lifecycle of proxy core processes (Xray, Sing-box).
- Location: `src/daemon/`
- Contains: Process management logic, D-Bus service implementation.
- Depends on: `src/ipc.rs` (interface), `tokio` for process orchestration.
- Used by: UI Layer via D-Bus.

**Domain Layer:**
- Purpose: Business logic for VPN protocol parsing and configuration generation.
- Location: `src/domain/`
- Contains: VPN key parsers (`key_parser.rs`), core-specific config builders (`xray_config.rs`, `singbox_config.rs`).
- Depends on: `serde`, `serde_json`.
- Used by: UI Layer to prepare configurations for the Daemon.

## Data Flow

**VPN Connection Flow:**

1. **Key Parsing:** UI uses `src/domain/key_parser.rs` to parse a VPN URL (VLESS/VMess).
2. **Config Generation:** UI uses `src/domain/xray_config.rs` or `src/domain/singbox_config.rs` to generate a full JSON configuration for the selected core.
3. **IPC Request:** UI calls `start_proxy(core_type, config_json)` via the `DaemonProxy` in `src/ipc.rs`.
4. **Core Execution:** Daemon receives the request, spawns the core process, and pipes the JSON configuration into its stdin.
5. **Monitoring:** Daemon monitors stdout/stderr and process exit status.
6. **Feedback Loop:** Daemon emits D-Bus signals (`log_message`, `status_changed`) which the UI listens for to update its state.

**State Management:**
- **UI State:** Managed via GLib/GObject properties and `gio::ListStore` in `src/ui/models.rs` and `src/ui/pages/`.
- **Persistent State:** Settings and VPN keys are stored in the user's configuration directory using `src/settings.rs`.
- **Daemon State:** Tracks the running child process and its status in `src/daemon/mod.rs`.

## Key Abstractions

**DaemonProxy:**
- Purpose: UI-side interface to the privileged daemon.
- Examples: `src/ipc.rs`
- Pattern: D-Bus Proxy.

**VpnCore (Trait):**
- Purpose: Abstract interface for starting/stopping VPN services from the UI perspective.
- Examples: `src/backend.rs`
- Pattern: Strategy/Facade.

## Entry Points

**Main Entry Point:**
- Location: `src/main.rs`
- Triggers: Execution of the `vrxx` binary.
- Responsibilities: Dispatches to either `daemon::run()` (if `--daemon` flag is present) or `VrxxApplication::run()`.

## Error Handling

**Strategy:** Error propagation via `anyhow::Result` in the backend/daemon and UI-level alerts for user-facing errors.

**Patterns:**
- **D-Bus Errors:** Caught by the proxy and reported to the UI, which typically displays an `adw::AlertDialog`.
- **Core Crashes:** The daemon detects unexpected core exits and signals the UI to show a "Connection error" status.

## Cross-Cutting Concerns

**Logging:** Uses `tracing` with a multi-writer setup in `src/main.rs` that logs to both `app.log` and `all.log` in the user's config directory. The daemon forwards core logs via D-Bus signals.
**Validation:** VPN keys are validated during parsing in `src/domain/key_parser.rs`.
**Authentication:** Relies on D-Bus system bus policies (defined in `data/ru.mark.vrxx.daemon.conf`) to control access to the privileged daemon.

---

*Architecture analysis: 2025-01-24*
