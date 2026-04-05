# Codebase Structure

**Analysis Date:** 2025-01-24

## Directory Layout

```
/
├── data/           # D-Bus configuration, service files, and desktop entry
├── po/             # Translation files (Gettext)
├── scripts/        # Installation and management scripts
├── src/            # Source code
│   ├── daemon/     # Privileged service implementation
│   ├── domain/     # Core logic and configuration generation
│   ├── ui/         # UI implementation
│   │   ├── components/ # Reusable UI widgets
│   │   └── pages/      # Top-level UI views
│   ├── services/   # Background services (e.g., geoip updater)
│   ├── application.rs # Main application logic
│   ├── backend.rs  # UI-side core management facade
│   ├── ipc.rs      # D-Bus interface and proxy definitions
│   ├── main.rs     # Binary entry point (UI & Daemon)
│   ├── protocol.rs # Protocol-specific definitions
│   ├── settings.rs # User settings management
│   └── window.rs   # Main window definition
└── tests/          # Integration tests
```

## Directory Purposes

**src/daemon/:**
- Purpose: Contains the privileged service that runs as root.
- Contains: Logic for managing proxy core processes (Xray, Sing-box).
- Key files: `src/daemon/mod.rs` (Service implementation).

**src/ui/pages/:**
- Purpose: High-level views of the application.
- Contains: `vpn_page.rs` (Main connection view), `proxy_page.rs` (Proxy settings), `settings_page.rs` (App settings).
- Key files: `src/ui/pages/vpn_page.rs` (Manages connection logic and status display).

**src/ui/components/:**
- Purpose: Reusable UI widgets.
- Contains: `vpn_key_row.rs` (Row in the VPN list), `log_window.rs` (View for core logs).
- Key files: `src/ui/components/vpn_key_row.rs`.

**src/domain/:**
- Purpose: Business logic and data transformations.
- Contains: VPN key parsing and configuration generators for different proxy cores.
- Key files: `src/domain/key_parser.rs`, `src/domain/xray_config.rs`, `src/domain/singbox_config.rs`.

## Key File Locations

**Entry Points:**
- `src/main.rs`: Entry point for both the UI and the daemon.

**Configuration:**
- `src/settings.rs`: Handles loading/saving user settings and VPN keys.
- `data/ru.mark.vrxx.gschema.xml`: GSettings schema.

**Core Logic:**
- `src/backend.rs`: Provides an abstract `VpnCore` trait for the UI to interact with the daemon.
- `src/ipc.rs`: Defines the `ru.mark.vrxx.Daemon` D-Bus interface.

**Testing:**
- `src/ui/tests.rs`: UI-related tests.
- `test_subprocess.rs`: Subprocess management tests.

## Naming Conventions

**Files:**
- Snake case for modules: `vpn_page.rs`, `key_parser.rs`.
- `.ui` files share the name of their implementation: `vpn_page.ui` for `vpn_page.rs`.

**Directories:**
- Plural for collections: `pages`, `components`, `services`.

## Where to Add New Code

**New Feature:**
- Add a new page in `src/ui/pages/` and a corresponding `.ui` file.
- Register the page in `src/window.rs`.

**New Proxy Core:**
- Create a new config builder in `src/domain/`.
- Update `src/domain/mod.rs` and the UI logic in `vpn_page.rs` to support the new core.

**New UI Widget:**
- Add implementation in `src/ui/components/` and its `.ui` file.

**Shared Helpers:**
- Add to `src/services/` if it's a background task.
- Add to `src/domain/` if it's pure logic.

## Special Directories

**data/:**
- Purpose: Contains system integration files (D-Bus policy, systemd service).
- Committed: Yes.

**po/:**
- Purpose: Translations for the application.
- Committed: Yes.

---

*Structure analysis: 2025-01-24*
