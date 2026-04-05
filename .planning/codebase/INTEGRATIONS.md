# External Integrations

**Analysis Date:** 2025-02-13

## APIs & External Services

**Proxy Engines:**
- `xray` - Primary proxy engine (VLESS, VMESS, Reality, Trojan, Fragment).
  - Client: `src/domain/xray_config.rs` (config generation).
  - Integration: `src/daemon/mod.rs` (process management).
- `sing-box` - Secondary proxy engine (modern features, better TUN mode).
  - Client: `src/domain/singbox_config.rs` (config generation).
  - Integration: `src/daemon/mod.rs` (process management).

**Geo Databases:**
- GitHub (v2fly, Tech-X-Labs, SagerNet) - Sources for GeoIP/Geosite databases.
  - Client: `src/services/geo_updater.rs`.
  - Auth: Public URLs.

## Data Storage

**Databases:**
- Local Filesystem - Native JSON storage.
  - Path: `~/.config/vrxx/settings.json`.
  - Client: `src/settings.rs` (`SettingsManager`).

**File Storage:**
- Local filesystem for logs (`~/.config/vrxx/logs/`).
- Local filesystem for Geo assets (`~/.config/vrxx/*.dat`).

**Caching:**
- None (beyond local file caching for Geo databases).

## Authentication & Identity

**Auth Provider:**
- Polkit - Authorization for privileged daemon access.
  - Implementation: `data/ru.mark.vrxx.policy`.
  - Client: `zbus_polkit` used in daemon logic.

## Monitoring & Observability

**Error Tracking:**
- None (local logging only).

**Logs:**
- `tracing` - Application-level structured logging.
- `src/main.rs` - Multi-writer setup for `app.log` and `all.log`.
- `xray` logs - Captured from process stdout/stderr and sent via D-Bus signals.

## CI/CD & Deployment

**Hosting:**
- GitHub - Source control and releases.

**CI Pipeline:**
- GitHub Actions - Automated builds for Rust and releases.
  - Config: `.github/workflows/rust.yml` and `.github/workflows/release.yml`.

## Environment Configuration

**Required env vars:**
- `VRXX_CONFIG_RS_PATH` - Used during build to pass Meson config to Cargo.
- `XDG_CONFIG_HOME` - Base directory for settings.
- `LANGUAGE`, `LC_ALL`, `LANG` - Overridden by the application for UI localization.

**Secrets location:**
- Not applicable (no secret credentials used).

## Webhooks & Callbacks

**Incoming:**
- D-Bus Signals - `log_message` and `status_changed` from the daemon.
  - Client: `src/ipc.rs` and `src/backend.rs`.

**Outgoing:**
- D-Bus Methods - `start_proxy`, `stop_proxy`, `ping` sent to the daemon.
  - Client: `src/ipc.rs` and `src/backend.rs`.

---

*Integration audit: 2025-02-13*
