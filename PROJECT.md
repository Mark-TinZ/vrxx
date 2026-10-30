# Project: VRXX Architecture Overhaul & Security Hardening

## Architecture
VRXX is a hybrid Linux VPN/proxy application consisting of:
1. **Privileged Daemon (`vrxx --daemon`)**:
   - Runs with minimal capabilities (`CAP_NET_ADMIN`, `CAP_NET_BIND_SERVICE`) and systemd sandboxing.
   - Binds exclusively to protected Unix Domain Socket `/run/vrxx/daemon.sock` (mode `0666`, parent dir `0755`).
   - Enforces `SO_PEERCRED` caller authentication, verifying caller identity via D-Bus `systemd-logind` active sessions (with PAM `/run/user/<uid>` fallback).
   - Authoritatively generates and pre-flight validates `sing-box` JSON configurations inside the daemon, executing with strict isolation.
   - Monitors Clash API telemetry (`127.0.0.1:9090`) and broadcasts live events over SSE.
2. **Client Layer**:
   - GTK4 / Libadwaita GUI (`vrxx`).
   - Terminal UI (`vrxx-tui`).
   - Clients interact with the privileged daemon exclusively through `DaemonClient` (`src/ipc.rs`) over the Unix Domain Socket using strongly-typed protocols (`src/protocol.rs`).
3. **Cryptographic Storage Layer**:
   - Replaces world-readable `/etc/machine-id` with user-isolated CSPRNG key at `~/.local/share/vrxx/.key` (`0600` permissions, parent dir `0700`).
   - Uses AES-256-GCM container format `VRXXDAT2` with automatic on-the-fly migration for legacy `VRXXDAT1` data.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| F1 | UDS Transport Transition | Bind daemon to `/run/vrxx/daemon.sock`, eliminate TCP `127.0.0.1:13337` listener | M2 | R1 |
| F2 | SO_PEERCRED Caller Authentication | Extract `ucred` from incoming Unix stream, enforce root/active logind session authorization | M2 | R1 |
| F3 | Client UDS Adaptation | Update `DaemonClient` in `src/ipc.rs` and all call sites to connect via UDS | M2 | R1 |
| F4 | Stale Socket Lifecycle | Stale socket probing, unlinking, directory creation (`0755`), permissions (`0666`), signal cleanup | M2 | R1 |
| F5 | Strongly-Typed IPC Protocols | Create `src/protocol.rs` with `CoreType`, `DaemonStatus`, `LogLevel`, `DaemonConfigOptions`, `StartProxyRequest` | M1 | R2 |
| F6 | Daemon Config Inversion | Move `singbox_config::build_singbox_config` inside daemon `ProxyManager::start_proxy` | M1 | R2 |
| F7 | Daemon Pre-Flight Validation | Validate `ParsedKey`, options, and execute `sing-box check -c <file>` prior to process launch | M1 | R2 |
| F8 | Client Config Generation Removal | Strip raw JSON generation from `vpn_page.rs`, `application.rs`, `backend.rs`, `tui/app.rs` | M1 | R2 |
| F9 | User-Isolated Keystore | Generate CSPRNG 256-bit key at `~/.local/share/vrxx/.key` (0600), parent dir (0700) | M3 | R3 |
| F10 | Keystore V2 Container | Implement `VRXXDAT2` container format with HKDF-SHA256 and AES-256-GCM | M3 | R3 |
| F11 | Legacy Keystore Migration | Seamless on-the-fly decryption and upgrade of legacy `VRXXDAT1` machine-id containers | M3 | R3 |
| F12 | Clash API Telemetry Collector | Daemon polls Clash API (`127.0.0.1:9090/traffic` and `/connections`), emits `DaemonEvent::Traffic` | M4 | R4 |
| F13 | TUI Real Bandwidth Telemetry | Replace synthetic modulo calculation in `vrxx-tui` with live SSE bandwidth events and dynamic units | M4 | R4 |
| F14 | Hardened Systemd Service | Create hardened `vrxx-daemon.service` with capabilities, `NoNewPrivileges`, `ProtectSystem`, `PrivateTmp` | M4 | R4 |
| F15 | Comprehensive Test & Clippy Cleanliness | Ensure `cargo check --all-targets`, `cargo test`, `cargo clippy --all-targets -- -D warnings` pass | M5 | Acceptance |
| F16 | Automated E2E Verification | Verify no TCP 13337, verify UDS socket, verify GUI and TUI connect and control daemon | M5 | Acceptance |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Core Protocol & Config Inversion | Strongly-typed `src/protocol.rs`, daemon config generation & multi-tier validation, client call site refactor | none | DONE |
| M2 | Unix Domain Socket & Caller Auth | `/run/vrxx/daemon.sock`, `SO_PEERCRED` caller auth, `DaemonClient` UDS transport, stale socket management | M1 | DONE |
| M3 | Keystore Hardening & Migration | User-isolated `~/.local/share/vrxx/.key` (`0600`), `VRXXDAT2` format, automatic `VRXXDAT1` migration | none | DONE |
| M4 | TUI Telemetry & Systemd Sandboxing | Clash API telemetry collection, `DaemonEvent::Traffic`, `vrxx-tui` live display, hardened systemd service unit | M1, M2 | DONE |
| M5 | E2E Testing & Final Verification | Full integration test suite execution (Tiers 1-4), adversarial test hardening (Tier 5), verification of acceptance criteria | M1, M2, M3, M4 | DONE |

## Interface Contracts

### Client ↔ Daemon IPC (`src/protocol.rs`)
- **Transport**: Unix Domain Socket at `/run/vrxx/daemon.sock` (configurable via `VRXX_DAEMON_SOCKET`).
- **REST Endpoints**:
  - `GET /api/ping` -> `200 OK "pong"` (or JSON)
  - `GET /api/status` -> `200 OK` JSON `DaemonStatus` (`"disconnected"`, `"connecting"`, `"connected"`, `"disconnecting"`, `"error"`)
  - `GET /api/is_running` -> `200 OK` JSON `bool`
  - `POST /api/proxy/start` -> Body JSON `StartProxyRequest`:
    ```rust
    pub struct StartProxyRequest {
        pub core_type: CoreType,
        pub profile: ParsedKey,
        pub options: DaemonConfigOptions,
    }
    ```
    Response: `200 OK` or `400 Bad Request` with structured error message.
  - `POST /api/proxy/stop` -> `200 OK` or error.
  - `GET /api/events` -> SSE stream yielding JSON-serialized `DaemonEvent`:
    - `StatusChanged(DaemonStatus)`
    - `Log { source: String, level: LogLevel, message: String }`
    - `Traffic { down_speed: u64, up_speed: u64, down_total: u64, up_total: u64 }`

### Keystore Storage Contract (`src/crypto/keystore.rs`)
- **User Key File**: `~/.local/share/vrxx/.key` (file permissions `0600`, directory `0700`).
- **V2 Magic Header**: `b"VRXXDAT2"` (8 bytes).
- **V1 Magic Header**: `b"VRXXDAT1"` (8 bytes, legacy fallback for migration).
- **Container Structure**: `[MAGIC (8 bytes)] [NONCE (12 bytes)] [CIPHERTEXT + AES-GCM TAG]`.

## Code Layout
- `src/protocol.rs`: Strongly-typed IPC enums and payload definitions (`CoreType`, `DaemonStatus`, `LogLevel`, `DaemonConfigOptions`, `StartProxyRequest`).
- `src/crypto/keystore.rs`: User-isolated key generation, KDF v2, AES-256-GCM container v2, legacy v1 migration.
- `src/daemon/mod.rs`: Daemon entry point, UDS listener binding, stale socket management, lifecycle cleanup.
- `src/daemon/auth.rs`: `CallerIdentity` extraction from `SO_PEERCRED`, systemd-logind D-Bus authorization, PAM fallback.
- `src/daemon/core.rs`: `ProxyManager`, internal `sing-box` config generation, pre-flight validation (`sing-box check`), Clash API telemetry polling.
- `src/daemon/events.rs`: `DaemonEvent` definition including `DaemonStatus`, `LogLevel`, and `Traffic`.
- `src/daemon/api.rs`: Axum router, REST handlers, authentication middleware.
- `src/ipc.rs`: `DaemonClient` configured with UDS transport, typed API calls.
- `src/tui/app.rs` & `src/tui/mod.rs`: Live telemetry consumption, sparkline updates, strongly-typed daemon communication.
- `data/vrxx-daemon.service` & `scripts/install.sh`: Sandboxed systemd service definition with capability bounding.
