# Codebase Concerns

**Analysis Date:** 2024-12-16

## Tech Debt

**D-Bus Privilege and Security:**
- Issue: The D-Bus policy allows any user to invoke methods on the `system` bus service `ru.mark.vrxx.daemon`.
- Files: `data/ru.mark.vrxx.daemon.conf`, `src/ipc.rs`
- Impact: In a multi-user environment, any user can start, stop, or disrupt the VPN proxy of another user.
- Fix approach: Implement Polkit authorization in the daemon before performing privileged operations, or restrict the D-Bus policy to specific users/groups.

**Config Generation Complexity:**
- Issue: `build_singbox_config` uses manual JSON manipulation (`serde_json::json!`) instead of structured types.
- Files: `src/domain/singbox_config.rs`, `src/domain/xray_config.rs`
- Impact: High risk of generating invalid configurations as `sing-box` or `xray` evolve. Brittle version-specific logic is scattered throughout the function.
- Fix approach: Define Rust structs for the configuration formats and use `serde` for serialization.

**Hardcoded Rule-set URLs:**
- Issue: Remote rule-set URLs for GeoSite/GeoIP are hardcoded.
- Files: `src/domain/singbox_config.rs`
- Impact: If the upstream repositories (e.g., SagerNet/sing-geosite) change their structure or go offline, the routing feature will break.
- Fix approach: Make these URLs configurable or move them to a dedicated configuration file/resource.

## Known Bugs

**Settings Reloading:**
- Symptoms: Importing settings does not automatically update the UI or the running proxy.
- Files: `src/application.rs` (L147)
- Trigger: Use the "Import Settings" action.
- Workaround: Restart the application.

## Security Considerations

**Privileged Daemon Command Execution:**
- Risk: While the daemon limits execution to `sing-box` or `xray`, it passes a full JSON config via stdin which might contain sensitive information or complex routing rules that could be used for local network attacks if manipulated by a malicious actor on the system bus.
- Files: `src/daemon/mod.rs`
- Current mitigation: Basic `match` on `core_type`.
- Recommendations: Implement strict validation of the `config_json` in the daemon before passing it to the core binary.

## Performance Bottlenecks

**Log Bus Saturation:**
- Problem: Proxy logs are emitted as D-Bus signals for every line of output.
- Files: `src/daemon/mod.rs`, `src/ipc.rs`
- Cause: High-volume logging (e.g., at "debug" or "trace" levels) from the proxy core can saturate the system D-Bus and degrade UI performance.
- Improvement path: Implement log buffering or a shared memory/socket-based logging mechanism for high-volume data.

## Fragile Areas

**Version-Dependent Config Logic:**
- Files: `src/domain/singbox_config.rs`
- Why fragile: `sing-box` is known for frequent breaking changes in its JSON configuration format across minor versions (e.g., 1.11, 1.12).
- Safe modification: Extensive testing with multiple `sing-box` versions is required. The current `test_singbox_config_validity_permutations` helps but only tests the version installed on the build machine.
- Test coverage: Gaps in testing against multiple versions of the core binaries.

**Key URL Parsing:**
- Files: `src/domain/key_parser.rs`
- Why fragile: VPN URI schemes are not strictly standardized. VMess uses base64-encoded JSON, while others use custom URI components.
- Safe modification: Use the comprehensive test suite in `key_parser.rs` when adding support for new parameters or protocols.
- Test coverage: Missing coverage for legacy Shadowsocks formats and complex plugin parameters.

## Scaling Limits

**D-Bus Message Size:**
- Current capacity: D-Bus has a maximum message size (usually 128MB, but often much lower in practice for system bus).
- Limit: Passing very large configurations or rule-sets via D-Bus might hit limits or cause latency.
- Scaling path: Pass file descriptors or use a temporary file for large configurations instead of raw strings over D-Bus.

## Networking (TUN) Requirements

**TUN Interface Management:**
- Issue: Transitioning to TUN mode requires advanced networking privileges (`CAP_NET_ADMIN`).
- Files: `src/domain/singbox_config.rs` (L59-81), `src/daemon/mod.rs`
- Current state: `tun_mode` is present in config but lacks robust lifecycle management (IP allocation, cleanup on crash).
- Risks: Leftover TUN interfaces after a crash; conflicts with existing VPNs or local routing tables.
- Recommendations: Implement a robust cleanup mechanism in the daemon (e.g., using a supervisor process or PID files) and use `systemd-resolved` or `NetworkManager` for DNS integration.

## Missing Critical Features

**Core Binary Management:**
- Problem: The application expects `sing-box` and `xray` to be in the system PATH.
- Blocks: Portability and easy installation for non-technical users.
- Fix approach: Implement a "downloader" or "installer" service that fetches verified versions of these cores and stores them in a known location (e.g., `~/.local/share/vrxx/bin`).

## Test Coverage Gaps

**Daemon Logic:**
- What's not tested: The actual D-Bus interaction and process management in `src/daemon/mod.rs`.
- Files: `src/daemon/mod.rs`
- Risk: Deadlocks in the async event loop or race conditions during proxy start/stop could go unnoticed.
- Priority: Medium

---

*Concerns audit: 2024-12-16*
