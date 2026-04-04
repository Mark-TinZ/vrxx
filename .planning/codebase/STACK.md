# Technology Stack

**Analysis Date:** 2024-05-23

## Languages

**Primary:**
- Rust (Edition 2021) - Core application logic, proxy configuration, and UI bindings (`src/main.rs`, `src/**/*.rs`)

**Secondary:**
- Meson - Build system configuration (`meson.build`)
- XML/UI - GTK4 UI layouts (`src/ui/**/*.ui`)
- Gettext (PO) - Application translations (`po/*.po`)

## Runtime

**Environment:**
- Linux Desktop (Native execution or Flatpak)

**Package Manager:**
- Cargo (Rust package manager)
- Lockfile: present (`Cargo.lock`)

## Frameworks

**Core:**
- GTK4 (`gtk4-rs` v0.10, features: `v4_16`) - Core GUI framework for Linux
- Libadwaita (`libadwaita-rs` v0.8, features: `v1_7`) - GNOME HIG compliant UI widgets
- Tokio (v1) - Asynchronous runtime for networking and proxy management

**Testing:**
- Rust built-in `#[test]` framework - Unit testing (e.g., `src/ui/tests.rs`, `test_sub.rs`)

**Build/Dev:**
- Meson & Ninja - Primary build system integrating Rust with GNOME ecosystem
- Cargo - Rust dependency management

## Key Dependencies

**Critical:**
- `reqwest` & `ureq` - HTTP clients used for IP checks and downloading routing rules
- `serde` & `serde_json` - Core serialization/deserialization for proxy configurations (Xray/Sing-box) and app settings
- `nix` - System calls and signal management for managing background proxy processes

**Infrastructure:**
- `tracing`, `tracing-subscriber`, `tracing-appender` - Structured logging framework writing to local files
- `dirs` - Resolving standard user directories for configuration storage (`~/.config/vrxx/`)
- `gettext-rs` - Application internationalization (i18n) bindings

## Configuration

**Environment:**
- Configured via local JSON files.
- Checks `FLATPAK_ID` environment variable to adapt behavior if running under Flatpak (`src/ui/pages/settings_page.rs`).
- Modifies `LANGUAGE`, `LC_ALL`, `LANG`, `LC_MESSAGES` internally for forcing language settings (`src/main.rs`).

**Build:**
- `Cargo.toml` - Rust dependencies
- `meson.build` - Full project build pipeline, desktop file installation, and GLib schema compilation

## Platform Requirements

**Development:**
- Rust toolchain
- GNOME development libraries (GTK4, libadwaita)
- Gettext
- Meson and Ninja

**Production:**
- Linux desktop environment (GNOME, KDE, etc.) with GTK4 and Libadwaita available
- Proxy cores (Xray, Sing-box) typically bundled or expected to be available

---

*Stack analysis: 2024-05-23*