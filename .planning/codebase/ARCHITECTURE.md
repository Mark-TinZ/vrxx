# Architecture

**Analysis Date:** 2024-05-24

## Pattern Overview

**Overall:** Event-Driven Desktop Application with MVC-like Separation

**Key Characteristics:**
- **GTK/GLib Event Loop:** UI runs on the main GTK event loop. Heavy operations (like core execution and log tailing) are offloaded to background threads.
- **Subprocess Management:** Manages external VPN core binaries (`xray`, `sing-box`, `tun2socks`) via child processes.
- **Polling:** UI components poll state (e.g., active connection statistics) using GLib timeout intervals rather than reactive bindings for background state.

## Layers

**UI Layer (`src/ui/`, `src/window.rs`):**
- Purpose: Renders the GTK4/Libadwaita interface and responds to user input.
- Location: `src/ui/`, `src/window.rs`
- Contains: Pages, reusable components, and models for lists.
- Depends on: `gtk`, `adw`, `gio`, SettingsManager.
- Used by: Application entry point.

**Backend/Process Layer (`src/backend.rs`):**
- Purpose: Controls the lifecycle of VPN core processes.
- Location: `src/backend.rs`
- Contains: `CoreBackend` implementation, `VpnCore` trait, stdout/stderr logging threads.
- Depends on: `std::process::Command`, `nix` (for process signaling).
- Used by: UI callbacks when toggling connections.

**Domain/Data Layer (`src/domain/`, `src/protocol.rs`):**
- Purpose: Business logic for parsing VPN keys and generating core configurations.
- Location: `src/domain/`, `src/protocol.rs`
- Contains: `key_parser.rs`, JSON config generators (`xray_config.rs`, `singbox_config.rs`), and protocol definitions.
- Depends on: `serde`, `serde_json`, `regex`, `base64`.
- Used by: UI and Backend to parse URLs into configs.

**Settings Management (`src/settings.rs`):**
- Purpose: Persists user configuration and saved VPN keys to disk.
- Location: `src/settings.rs`
- Contains: `SettingsManager`, `AppSettings` struct.
- Depends on: `serde_json`, `dirs`.
- Used by: Almost all layers to read/write state.

## Data Flow

**Connecting to a VPN:**
1. User clicks a connection in the UI (`src/ui/pages/vpn_page.rs`).
2. Protocol settings are passed to config generators in `src/domain/` to produce a JSON string.
3. `CoreBackend::start()` is called with the generated JSON config (`src/backend.rs`).
4. Backend stops any existing processes, validates permissions (e.g., `pkexec` for `cap_net_admin` in TUN mode), and spawns the core process.
5. `SettingsManager` is updated to mark the key as active.
6. `VrxxWindow`'s polling loop (`window.rs`) picks up the active state and updates the UI timer/traffic.

**State Management:**
- State is predominantly persisted to disk via `SettingsManager` (saving JSON to `~/.config/vrxx/settings.json`).
- In-memory state (like active processes) is held in `Arc<Mutex<Option<Child>>>` within the `CoreBackend`.
- GLib timeout loops (`glib::timeout_add_local`) are used to periodically read state and update the UI.

## Key Abstractions

**`VpnCore`:**
- Purpose: Trait defining standard operations for VPN backends (start, stop, is_running).
- Examples: `src/backend.rs` (`CoreBackend`)
- Pattern: Facade for subprocess management.

**`ProtocolSettings`:**
- Purpose: Represents different supported proxy protocols.
- Examples: `src/protocol.rs` (Enum with `VlessSettings`, `VmessSettings`, etc.)
- Pattern: Tagged Enum for polymorphic configuration handling.

## Entry Points

**Main Application:**
- Location: `src/main.rs`
- Triggers: User launches application.
- Responsibilities: Initializes logger (`tracing`), configures gettext, registers GTK resources, and starts the `VrxxApplication`.

**Application Activation:**
- Location: `src/application.rs`
- Triggers: GTK application `activate` signal.
- Responsibilities: Creates or presents the main `VrxxWindow`, sets up global GTK actions (e.g., about dialog, import/export config).

## Error Handling

**Strategy:** Result-based with `anyhow` for flexibility. UI alerts are shown for critical process failures.

**Patterns:**
- `anyhow::Result` is used heavily in `backend.rs` and config parsing.
- Process crashes are caught by background stdout/stderr threads and logged to `error.log`.
- `tracing` is used to capture GLib/GTK warnings alongside application logs.

## Cross-Cutting Concerns

**Logging:** Managed via `tracing-subscriber` with a custom `MultiWriter` to write to both application logs and general system logs. Logs are rotated manually if they exceed 5MB (`rotate_log_if_needed`).
**Configuration:** `SettingsManager` provides a synchronized way to read and write application preferences to JSON.

---

*Architecture analysis: 2024-05-24*
