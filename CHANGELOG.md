# Журнал изменений (Changelog)

Все изменения проекта документируются в соответствии с [Conventional Commits](https://www.conventionalcommits.org/).

## [26.1.0-beta.3] - 2026-11-11

### 🚀 Новые возможности

- *(security)* **Overhaul Daemon IPC to UDS & Hardening**: Migrated daemon IPC to Unix Domain Sockets (UDS) with strict peer credential authentication (`SO_PEERCRED`), systemd service sandboxing (`CapabilityBoundingSet`, `ProtectSystem=strict`, `MemoryDenyWriteExecute`), and stale socket recovery
- *(security)* **Keystore v2 (`VRXXDAT2`)**: Upgraded cryptographic profile storage to container format v2 using HKDF-SHA256 key derivation and AES-256-GCM authenticated encryption with automatic zero-downtime migration from legacy format
- *(tui)* **TUI Redesign & Bilingual Localization**: Complete overhaul of terminal UI (`vrxx-tui`) adhering to unslop design standards, active profile rendering, real-time traffic sparklines, and 100% full English and Russian localization
- *(domain)* **Subscription Management & Key Grouping**: Added subscription URL parser, auto-updating, Base64 profile imports, custom grouping ("Move to Group"), and live ping in the VPN manager
- *(ui)* **Smart Location Resolver**: Integrated IP-based country flag and geolocation detection in key row displaying endpoint country and flag icon
- *(protocols)* **sing-box 1.13.18 Integration**: Added full sing-box 1.13.18 protocol matrix support, TLS fragmentation, TCP ping defaults, and Antizapret geodata rule-sets
- *(logging)* **Enterprise Tracing Standard**: Production-grade structured logging with `tracing`, hot-reload of log level without restarts (`tracing_subscriber::reload::Handle`), automatic PII/token masking (`mask_secret`), 10MB file rotation, and 7-day retention cleanup
- *(ui)* **Declarative Dialogs**: Moved and converted all modal dialogs to declarative Libadwaita `.ui` templates under `src/ui/dialogs/` (`VrxxKeyDialog`, `VrxxRuleDialog`, `VrxxQrDialog`, `VrxxExportDialog`, `VrxxImportDialog`)
- *(ui)* **Mobile Adaptivity**: Adaptive presentation mode (`bottom-sheet`) and responsive `AdwClamp` sizing for narrow screen geometries (< 500px)
- *(ui)* **Dynamic Protocol Fields**: Adaptive form field toggling in `VrxxKeyDialog` supporting VLESS, VMess, Trojan, Shadowsocks, Hysteria2, TUIC, WireGuard, SOCKS5, and HTTP
- *(ui)* **Batch Ping & Cooldown**: Added "Ping All Keys" header action (`<Primary><Shift>P`), startup auto-ping option, and configurable background cooldown timer (5m, 10m, 15m, 30m, 1h)
- *(shortcuts)* **Complete Hotkey Coverage**: Implemented and registered standard GTK4 accelerators (`<Primary>D` disconnect, `<Primary>P` ping selected, `<Primary><Shift>P` ping all, `<Primary>comma` settings, zoom and navigation)

### 🐛 Исправления ошибок

- *(ci)* Sanitize pre-release version strings for RPM packaging (`~beta.3`)
- *(ci)* Fix cargo-deny schema and grant checks permissions for cargo-audit
- *(ci)* Install sing-box in CI for integration tests and guard missing binary in tests
- *(tui)* Fix status, error localization and active profile state rendering
- *(ui)* Translate default profile name and server labels, sync docs with keystore v2 and settings
- *(proxy)* **GNOME System Proxy Fix**: Guaranteed proxy reset to `'none'` on VPN disconnection and in TUN mode, preventing network loss; added startup self-healing
- *(ui)* **VPN Key Row Subtitle**: Removed IP address from subtitle, displaying protocol only for cleaner HIG appearance
- *(ui)* **QR Dialog Privacy**: Converted to native `AdwDialog` with hardware-accelerated CSS opacity blur shroud, eliminating GTK CSS errors

### 🔧 Обслуживание и CI

- *(security)* Integrate DevSecOps linters: Gitleaks secrets detection, Semgrep SAST, Trivy CVE scanner, and Cargo Audit
- *(workflow)* Dynamic prerelease detection in GitHub release workflow based on semver tags
- *(packaging)* Update Arch Linux PKGBUILD to 26.1.0_beta.3

## [26.1.0-beta.2] - 2026-08-30

### 🚀 Новые возможности

- *(scripts)* Modernize install and uninstall scripts with automatic privilege elevation (`sudo -v` keepalive loop), unprivileged user build, and systemd service management
- *(ui)* Split menu actions into "Close Window" (`_Close Window`, GUI only) and "Quit" (`_Quit`, stops VPN tunnel, terminates daemon, and closes app with `AdwAlertDialog` confirmation)

### 🐛 Исправления ошибок

- *(daemon)* Fix TUN startup conflict `TUNSETIFF: device or resource busy` by delegating interface lifecycle and routing to sing-box native `auto_route` driver with orphan link cleanup
- *(logging)* Standardize application logs format (`YYYY-MM-DD HH:MM:SS LEVEL [target]: message`) and translate all internal tracing logs across the codebase to English
- *(ui)* Fix log level coloring in `VrxxLogWindow` so application and system logs are correctly tagged by level (Error, Warning, Debug, Info)

### 📚 Документация

- *(docs)* Update daemon architecture documentation to reflect sing-box native TUN driver and fallback TunManager

## [26.1.0-beta.1] - 2026-08-28

### 🚀 Новые возможности

- Complete UI/UX overhaul and core stability improvements
- Implements fully async connection verification, HIG alerts on failure, and non-blocking geodata resolution
- Upgrade to 0.1.6, refactor architecture, enhance logging and config
- Overhaul vpn core architecture, custom routing and ci/cd setup
- Stabilize vpn cores and enhance UI/UX for production readiness
- Complete Phase 3 (Networking & Routing)
- *(ui)* Add explicit buttons for importing VPN keys from clipboard and URL
- *(ui)* Add tooltips for technical terms and translations
- *(ui)* Add Russian comments, update geo URLs, add Update Geo button
- *(ui)* Add empty state placeholder for VPN list and fix compilation errors
- *(tui)* Implement Ratatui interactive TUI interface and documentation
- *(domain)* Sing-box 1.13+ config generation, versioning adaptation, and multi-protocol support
- *(ping)* Implement 4-algorithm Ping Engine with parallel execution and UI integration
- Implement network self-healing, daily log rotation, and AdwAlertDialog error handling (TZ 07)
- *(ui)* Implement GNOME HIG compliance and panic-free cross-desktop fallback
- *(release)* Update version to 26.1.0-beta.1 and configure release workflow

### 🐛 Исправления ошибок

- Resolve LogWindow template crash on startup
- Resolve core startup bind errors and improve UI/UX
- Resolve core issues and improve architecture
- Update sing-box config for version 1.11+ compatibility and TUN mode fixes
- Resolve runtime crash in settings page and address all compiler warnings
- *(geo)* Update geo-data URLs and implement manual update button

### ⚡ Производительность

- *(ipc)* Cache D-Bus connection via tokio OnceCell

### 🚜 Рефакторинг

- Apply production grade fixes and resolve warnings
- *(ui)* Remove duplicate TUN settings from settings page
- Переход на Sing-box, интеграция REST API и полная локализация

### 📚 Документация

- Map existing codebase
- Initialize project
- Complete project research
- *(1)* Research phase domain
- *(phase1)* Create phase 1 plans
- Refresh codebase map
- *(phase-3)* Research networking and routing domain
- *(03)* Create phase 3 plans
- Plan phase 3 (networking & routing)
- Mark Phase 2 as complete and Phase 3 as planned
- *(planning)* Add todos for core fixes, UI improvements and tech debt
- *(04-01)* Finish plan 04-01, add summary and update state
- *(phase-04)* Complete phase execution
- *(phase-04)* Evolve PROJECT.md after phase completion
- *(05)* Create phase 05 plans for UI cleanup and core fixes
- *(06-stability-cleanup)* Create phase plans
- *(06)* Plan Phase 06 Core Stability & Sing-box Refactor
- *(phase-06)* Complete phase execution
- *(phase-06)* Evolve PROJECT.md after phase completion
- Добавить скриншоты приложения в README.md
- Update daemon, privilege separation and IPC specification (Spec 01)

### 🧪 Тестирование

- Ignore GTK UI test due to thread safety issues
- *(phase-04)* Add unit and E2E tests from add-tests command

### 🔧 Обслуживание и CI

- Commit pending changes before history cleanup
- Add output.md to .gitignore to prevent accidental commits
- Add project config
- Fix syntax error in settings_page.rs
- Update roadmap and state for plan 04-01 completion

### Refactor

- Reorganize project structure, migrate domain logic to src/domain, and update UI for routing and settings

<!-- Generated by git-cliff -->
