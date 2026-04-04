# Codebase Structure

**Analysis Date:** 2024-05-24

## Directory Layout

```
/home/mihail/Developer/builder/vrxx/
├── data/               # Desktop entries, gschema, and icons
├── docs/               # Project documentation (Architecture, Contributing)
├── locale/             # Compiled translation files
├── po/                 # Source translation files (.po, .pot)
├── scripts/            # Shell scripts for installation/updating
└── src/                # Rust source code
    ├── domain/         # Business logic, configuration generation, and parsing
    ├── services/       # Background services (e.g., geodata updaters)
    └── ui/             # GTK User Interface components and pages
```

## Directory Purposes

**`src/domain/`:**
- Purpose: Contains the core logic for processing VPN protocols.
- Contains: Parsers for connection strings, and config builders for underlying proxy cores.
- Key files: `key_parser.rs`, `xray_config.rs`, `singbox_config.rs`

**`src/ui/`:**
- Purpose: Houses all GTK4/Libadwaita user interface code.
- Contains: UI pages, reusable widget components, and model definitions.
- Key files: `pages/vpn_page.rs`, `components/theme_switcher.rs`, `models.rs`

**`src/services/`:**
- Purpose: For background, non-UI tasks.
- Contains: Scheduled or background-triggered updates.
- Key files: `geo_updater.rs`

## Key File Locations

**Entry Points:**
- `src/main.rs`: Application entry point, logging and localization initialization.
- `src/application.rs`: `VrxxApplication` definition, setup of GTK actions, and startup sequence.

**Configuration:**
- `src/settings.rs`: Definitions for `AppSettings` and the `SettingsManager` to save/load from JSON.
- `src/protocol.rs`: Data structures for various VPN protocols (VLESS, VMess, Trojan, etc.).

**Core Logic:**
- `src/backend.rs`: Subprocess management for Xray/Sing-box and Tun2Socks.

**User Interface:**
- `src/window.rs`: The main application window (`VrxxWindow`) containing navigation and status widgets.
- `src/ui/pages/`: Specific tabs like proxy routing, VPN key lists, and settings.

## Naming Conventions

**Files:**
- snake_case: Standard Rust convention for modules (`xray_config.rs`, `vpn_page.rs`).

**Types / Structs:**
- PascalCase: For GTK objects and domain models (`VrxxWindow`, `CoreBackend`, `SettingsManager`).

**GTK Resources:**
- Extracted into XML/UI files where possible (e.g., `src/ui/components/theme_switcher.ui`).
- IDs and GTK template names usually follow camelCase or snake_case matching the widget type.

## Where to Add New Code

**New Feature (UI):**
- Primary code: `src/ui/pages/new_feature_page.rs` or as a component in `src/ui/components/`.
- UI definitions: Use inline GTK builder or create a `.ui` file and load via `#[template(resource = "...")]`.

**New VPN Core/Protocol:**
- Domain logic: Add parsers to `src/domain/key_parser.rs` and config generation to `src/domain/`.
- Data structures: Add the new protocol to `src/protocol.rs`.
- Backend management: Adjust `src/backend.rs` to support the binary parameters if different from Xray/Sing-box.

**New App Settings:**
- Configuration: Add fields to `AppSettings` in `src/settings.rs`. Update the UI in `src/ui/pages/settings_page.rs` to expose the setting.

## Special Directories

**`data/`:**
- Purpose: Contains system integration files (desktop shortcut, icons, dbus services).
- Generated: Some files are processed by `meson.build` (`.in` templates).
- Committed: Yes.

**`po/`:**
- Purpose: Gettext translation files.
- Generated: Partially (`.pot` and compiled `.mo` files), but `.po` files are manually translated.
- Committed: Yes.

---

*Structure analysis: 2024-05-24*
