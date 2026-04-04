# External Integrations

**Analysis Date:** 2024-05-23

## APIs & External Services

**Geolocation API:**
- IP-API - Validates IP status, country, and timezone (`src/ui/pages/vpn_page.rs`)
  - Endpoint: `http://ip-api.com/json/?fields=status,country,timezone,query`
  - SDK/Client: `ureq` crate (blocking HTTP client)
  - Auth: None (public API)

**Geo Routing Rules / Download Sources:**
- GitHub Releases - Downloads latest `geosite.dat` and `geoip.dat` rules for Xray/Sing-box (`src/services/geo_updater.rs`)
  - Endpoints: Various GitHub user repos (`v2fly`, `Tech-X-Labs`, `1andrevich`)
  - Client: `reqwest` crate (async HTTP client)
  - Auth: None (public downloads)
- GitHub Raw Content - Downloads raw `.srs` rule-sets for Sing-box (`src/domain/singbox_config.rs`)
  - Endpoints: `https://raw.githubusercontent.com/SagerNet/*`
  - Client: Handled by proxy core internally

**Proxy Cores:**
- Xray & Sing-Box - Configured internally, communicates directly through the core's native formats.
- Internal API Endpoint: `http://127.0.0.1:9090/connections`
  - Client: Configured and polled by `vrxx`

## Data Storage

**Databases:**
- None detected. The application relies entirely on flat files.

**File Storage:**
- Local filesystem only.
- Configuration and VPN Keys stored as JSON: `~/.config/vrxx/settings.json` (`src/settings.rs`)
- Logging files: `~/.config/vrxx/logs/app.log` and `all.log` (`src/main.rs`)

**Caching:**
- None formal. Downloaded geo-routing files (`geosite.dat`, `geoip.dat`) act as a local cache for the proxy cores (`src/services/geo_updater.rs`).

## Authentication & Identity

**Auth Provider:**
- Custom / None. User authentication is not required to use the client. Connections depend directly on imported VPN keys (VMess, VLESS, Trojan, etc.).

## Monitoring & Observability

**Error Tracking:**
- None detected (no Sentry, Bugsnag, etc.).

**Logs:**
- Local log files (`app.log`, `all.log`) generated via `tracing` and `tracing-appender` crates.
- Overrides GLib's log writer to pipe GTK/GLib logs into the `tracing` sink (`src/main.rs`).

## CI/CD & Deployment

**Hosting:**
- Code hosted on GitHub (`https://github.com/Mark-TinZ/vrxx`).

**CI Pipeline:**
- GitHub Actions (`.github/workflows/rust.yml`, `release.yml`). Builds binaries and prepares releases.

## Environment Configuration

**Required env vars:**
- None strictly required.
- Checks `FLATPAK_ID` to modify execution paths when running inside a Flatpak container (`src/ui/pages/settings_page.rs`).

**Secrets location:**
- VPN keys (which may contain connection credentials) are stored directly in `~/.config/vrxx/settings.json` as plaintext JSON.

## Webhooks & Callbacks

**Incoming:**
- None.

**Outgoing:**
- None.

---

*Integration audit: 2024-05-23*