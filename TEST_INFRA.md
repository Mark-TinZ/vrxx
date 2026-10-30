# VRXX Test Infrastructure & Methodology (`TEST_INFRA.md`)

## 1. Overview & Architecture

This document defines the automated end-to-end (E2E) testing architecture, methodology, test inventory, and execution protocol for the **VRXX** system overhaul.

The VRXX test suite follows an **opaque-box, requirement-driven, multi-tier testing strategy** covering all four core architectural requirements:
- **R1: Unix Domain Socket (UDS) & Caller Authentication (`SO_PEERCRED`)**
- **R2: Core Architecture & Daemon Configuration Generation Inversion**
- **R3: Cryptographic Storage & User-Isolated Keystore Hardening**
- **R4: Telemetry Streaming & Hardened Systemd Sandboxing**

```
+-----------------------------------------------------------------------------------+
|                              VRXX E2E TEST SUITE                                  |
+-----------------------------------------------------------------------------------+
|  Tier 1: Feature Coverage (>=5 test cases per feature covering happy paths)       |
|  Tier 2: Boundary & Corner Cases (invalid paths, auth rejection, corrupt data)   |
|  Tier 3: Cross-Feature Interactions (pairwise UDS + typed IPC + telemetry)       |
|  Tier 4: Real-World Scenarios (complete VPN lifecycle, migration, crash recovery) |
+-----------------------------------------------------------------------------------+
                                         |
     +-----------------+-----------------+-----------------+-----------------+
     |                 |                 |                 |                 |
     v                 v                 v                 v                 v
[e2e_ipc_uds]  [e2e_config_inv]  [e2e_keystore]    [e2e_telemetry]  [e2e_lifecycle]
     |                 |                 |                 |                 |
     +-----------------+-----------------+-----------------+-----------------+
                                         |
                                         v
                      [tests/common/mod.rs: Shared Test Harness]
                     - Mock Clash API Server (port 9090 / dynamic)
                     - Mock UDS Axum Server & SO_PEERCRED Middleware
                     - Isolated Tempdir / Socket Path Managers
                     - Wire Contract Encoders / Decoders
```

---

## 2. Test Tiers & Detailed Test Inventory

### Tier 1: Feature Coverage (Happy Paths)
Each feature has at least 5 independent, authoritative test cases:

#### R1: Unix Domain Socket & Authentication
1. `test_uds_socket_binding_and_file_mode`: Verifies binding Axum over `tokio::net::UnixListener`, setting socket mode to `0666` and parent dir to `0755`.
2. `test_uds_so_peercred_extraction`: Verifies kernel-level `SO_PEERCRED` credential extraction returning the caller's true UID/GID/PID.
3. `test_uds_active_user_auth_authorized`: Verifies that desktop user sessions (`UID >= 1000`) pass caller authorization.
4. `test_uds_root_user_authorized`: Verifies that `UID 0` (system root) is authoritatively permitted.
5. `test_uds_client_transport_ping_pong`: Verifies `reqwest` client configured with `.unix_socket()` successfully sends `GET /api/ping` and receives `"pong"`.
6. `test_uds_status_endpoint`: Verifies `GET /api/status` returns structured status over UDS.

#### R2: Typed IPC & Configuration Generation Inversion
1. `test_typed_start_proxy_request_serialization`: Verifies JSON roundtrip serialization of `StartProxyRequest` with `CoreType::SingBox`, `ParsedKey`, and `DaemonConfigOptions`.
2. `test_profile_validation_valid_vless_reality`: Verifies semantic profile validation accepts valid VLESS Reality keys with RFC 4122 UUID, SNI, and Reality public key.
3. `test_options_validation_valid_ports`: Verifies `DaemonConfigOptions::validate()` succeeds with valid SOCKS (`10808`) and HTTP (`10809`) ports.
4. `test_daemon_singbox_config_synthesis`: Verifies internal synthesis of complete sing-box JSON containing inbounds, outbounds, DNS, and routing rules without client involvement.
5. `test_daemon_preflight_check_with_singbox`: Executes `sing-box check -c <file>` on synthesized configuration, confirming exit code 0.
6. `test_typed_daemon_status_and_log_levels`: Verifies typed enums serialize to canonical lowercase representations (`disconnected`, `connecting`, `info`, `warning`).

#### R3: Cryptographic Storage & Keystore Hardening
1. `test_user_key_generation_and_permissions`: Generates 256-bit CSPRNG key at `~/.local/share/vrxx/.key` with strict `0600` file permissions and `0700` parent directory.
2. `test_user_key_derivation_hkdf_sha256`: Tests KDF v2 deriving a 32-byte AES key using domain separation `ru.mark.vrxx.user-keystore.v2`.
3. `test_v2_container_encryption_magic_header`: Verifies ciphertext container starts with 8-byte magic header `VRXXDAT2` followed by a 12-byte random nonce.
4. `test_v2_container_roundtrip`: Encrypts profile data into V2 container, decrypts with user key, and verifies exact profile preservation.
5. `test_user_isolation_protection`: Verifies that profiles encrypted with User A's key cannot be decrypted with User B's key (eliminating machine-id sharing).

#### R4: Clash Telemetry & Systemd Sandboxing
1. `test_clash_traffic_api_json_parsing`: Verifies parsing of Clash API `/traffic` JSON payload (`{"up": 1048576, "down": 5242880}`).
2. `test_clash_connections_api_json_parsing`: Verifies parsing of Clash API `/connections` JSON payload (`{"downloadTotal": 104857600, "uploadTotal": 52428800}`).
3. `test_daemon_event_traffic_broadcast`: Broadcasts `DaemonEvent::Traffic` over channel/SSE and confirms receiver gets exact metrics.
4. `test_telemetry_collector_polling_loop`: Simulates a mock Clash API server, runs polling loop, and verifies received traffic events.
5. `test_dynamic_bandwidth_unit_formatting`: Tests dynamic speed formatting across ranges (Bytes, KB/s, MB/s, GB/s).
6. `test_systemd_capability_bounding_set`: Verifies service unit enforces `CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_BIND_SERVICE`.
7. `test_systemd_ambient_capabilities`: Verifies service unit enforces `AmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE`.
8. `test_systemd_privilege_sandboxing`: Verifies `NoNewPrivileges=true`, `ProtectSystem=strict`, `ProtectHome=true`, `PrivateTmp=true`.
9. `test_systemd_runtime_directory`: Verifies `RuntimeDirectory=vrxx` and `RuntimeDirectoryMode=0755`.
10. `test_systemd_memory_deny_write_execute_safe`: Verifies service unit does not set `MemoryDenyWriteExecute=true` to protect Go runtime W^X compatibility.

---

### Tier 2: Boundary & Corner Cases
1. `test_boundary_invalid_uds_path_handling`: Tests error handling when binding to invalid paths or read-only filesystems.
2. `test_boundary_unauthorized_system_uid_rejected`: Verifies caller authentication rejects system accounts (`UID < 1000`, e.g., UID 999 or UID 1).
3. `test_boundary_unauthenticated_connection_rejected`: Verifies HTTP 403 Forbidden is returned when unauthorized callers access protected endpoints.
4. `test_boundary_empty_or_corrupt_config_json_rejected`: Verifies rejection of empty strings, malformed JSON, and truncated fields.
5. `test_boundary_invalid_uuid_in_parsed_key_rejected`: Verifies profile with malformed UUID (`not-a-valid-uuid`) fails validation with `KeyValidationError::InvalidUuid`.
6. `test_boundary_invalid_zero_port_in_parsed_key_rejected`: Verifies profile with port `0` fails validation with `KeyValidationError::InvalidPort`.
7. `test_boundary_conflicting_ports_rejected`: Verifies `socks_port == http_port` in `DaemonConfigOptions` fails validation.
8. `test_boundary_missing_keystore_user_key_file`: Verifies automatic creation with `0600` permissions when `.key` does not exist.
9. `test_boundary_legacy_vrxxdat1_migration`: Tests decrypting legacy `VRXXDAT1` container derived from `machine-id` and upgrading to `VRXXDAT2`.
10. `test_boundary_tampered_ciphertext_fails_aead_tag`: Verifies corrupting 1 byte in ciphertext or tag fails AEAD validation.
11. `test_boundary_clash_api_unreachable`: Verifies daemon handles Clash API connection refusal gracefully without crashing or panicking.

---

### Tier 3: Cross-Feature Interactions
1. `test_interaction_start_proxy_query_telemetry_stop_proxy_uds`: Pairwise flow over UDS socket:
   - Client sends typed `StartProxyRequest`
   - Daemon validates and transitions to `Connected`
   - Daemon streams `DaemonEvent::Traffic` from mock Clash API
   - Client consumes telemetry and issues `StopProxyRequest`
   - Daemon transitions to `Disconnected`.
2. `test_interaction_keystore_v2_to_uds_proxy_start`: Pairwise flow:
   - Encrypt profile with V2 user key
   - Decrypt profile from keystore
   - Submit decrypted `ParsedKey` in typed `StartProxyRequest` over UDS
   - Verify daemon receives and parses configuration.
3. `test_interaction_so_peercred_auth_with_typed_ipc`: Verifies authorized UID passes middleware and invokes typed IPC endpoints successfully.
4. `test_interaction_stale_socket_recovery_with_active_client`: Verifies daemon unlinks dead socket, rebinds, and allows client to connect immediately.

---

### Tier 4: Real-World Scenarios
1. `test_scenario_complete_vpn_lifecycle`:
   - End-to-end test spanning profile creation, encrypted persistence in V2 container, daemon startup over UDS, client authentication via `SO_PEERCRED`, typed proxy launch, pre-flight validation, live telemetry streaming, and graceful shutdown.
2. `test_scenario_profile_import_encrypted_persistence_and_migration`:
   - Complete migration scenario importing legacy `VRXXDAT1` files into modern `VRXXDAT2` containers with strict Linux DAC `0600` permissions.
3. `test_scenario_daemon_crash_and_stale_socket_recovery`:
   - Simulates daemon ungraceful termination (simulated crash leaving `/run/vrxx/daemon.sock`), new daemon instance detection of dead socket, safe unlinking, re-binding, and immediate client reconnection.

---

## 3. Test Isolation & Concurrency Methodology

To guarantee progressive testability, test independence, and zero cross-test interference:
1. **Dynamic Socket Paths**: Tests never bind to `/run/vrxx/daemon.sock` directly. Each test creates an isolated `tempfile::tempdir()` and uses a unique socket path (e.g., `temp_dir.path().join("test_daemon.sock")`).
2. **Ephemeral Ports for Mock Servers**: Mock Clash API servers bind to `127.0.0.1:0` (ephemeral port allocation) or temporary unix streams to avoid `EADDRINUSE` conflicts during parallel `cargo test` execution.
3. **Environment Isolation**: User key tests use temporary directories for `~/.local/share/vrxx/.key` via dedicated path parameters.
4. **Authoritative Sources of Expected Outputs**:
   - RFC 4122 UUID specifications for `ParsedKey::validate()`.
   - Linux kernel `SO_PEERCRED` getsockopt semantics for `ucred` extraction.
   - `sing-box check -c <file>` CLI oracle for pre-flight validation.
   - HKDF-SHA256 (RFC 5869) and AES-256-GCM (NIST SP 800-38D) test vectors for cryptographic operations.
   - Systemd unit syntax and specification rules for `vrxx-daemon.service`.

---

## 4. Test Execution Instructions

### Running All Tests
```bash
cargo test
```

### Running Specific E2E Test Suites
```bash
# R1: Unix Domain Socket & SO_PEERCRED Authentication
cargo test --test e2e_ipc_uds

# R2: Typed IPC & Configuration Inversion
cargo test --test e2e_config_inversion

# R3: Keystore V2 & Legacy Container Migration
cargo test --test e2e_keystore_migration

# R4: Clash API Telemetry & Bandwidth Events
cargo test --test e2e_telemetry

# R4: Hardened Systemd Sandboxing Unit Analysis
cargo test --test e2e_systemd

# Tiers 3 & 4: Cross-Feature Interactions & Real-World Scenarios
cargo test --test e2e_lifecycle
```

### Verification Pipeline
```bash
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --nocapture
```
