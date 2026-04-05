# Phase 2: Core Proxy Integration - Research

**Researched:** 2026-04-05
**Domain:** Async process management in Rust, D-Bus state synchronization, Zero-Disk configuration.
**Confidence:** HIGH

## Summary

Phase 2 focuses on moving the proxy management logic (Xray/Sing-box) into the privileged daemon established in Phase 1. The goal is to ensure the daemon "reliably manages" these processes using asynchronous patterns that keep the UI responsive.

Key transitions:
1.  Move `backend.rs` logic (process spawning, stdin piping) into the daemon's D-Bus interface implementation.
2.  Switch from `std::process` to `tokio::process` for better async integration with `zbus`.
3.  Use D-Bus signals for real-time status and log streaming to the unprivileged UI.

## Core Management with Tokio

Using `tokio::process::Command` allows the daemon to manage the core process without blocking the D-Bus message loop.

### Pattern: Robust Stdin Piping
To ensure Zero-Disk configuration, we pipe the JSON directly to the child's `stdin`.

```rust
use tokio::process::Command;
use tokio::io::AsyncWriteExt;
use std::process::Stdio;

async fn start_core(config_json: String) -> Result<()> {
    let mut child = Command::new("xray")
        .arg("run")
        .arg("-config")
        .arg("stdin:")
        .stdin(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(config_json.as_bytes()).await?;
        stdin.flush().await?;
    }
    // ... manage child
}
```

### Pattern: Signal-Based Shutdown
Using `nix` or `tokio` signals to send `SIGTERM` to the core for graceful shutdown, falling back to `SIGKILL` if it hangs.

## D-Bus Communication Patterns

### Status Property vs Signals
- **Property:** Use `#[zbus(property)]` for the current state (e.g., `Connected`, `Disconnected`). This allows new UI instances to immediately see the current state upon connection.
- **Signals:** Use `#[zbus(signal)]` for event-driven updates like log lines.

### Log Streaming
Streaming logs over D-Bus via signals is efficient enough for a VPN client.
`signal LogMessage(level: s, message: s)`

## Reliability Features

### Crash Detection
The daemon should monitor the child process. If it exits unexpectedly, the daemon should update its status and potentially notify the UI with an error message.

```rust
tokio::spawn(async move {
    let status = child.wait().await;
    // Update D-Bus property and emit signal on exit
});
```

## Environment & Permissions

Since the daemon runs as root:
- It can claim `cap_net_admin` for TUN mode without `pkexec` (it's already root).
- It should manage its own logs in `/var/log/vrxx/` or similar, but the UI might not have read access. D-Bus streaming solves this.

## Open Questions

1. **Config Validation:** Should the UI or the Daemon validate the config?
   - Recommendation: UI validates schema, Daemon does a "dry run" or handles core start failure gracefully.
2. **Multiple Clients:** What if two UI instances connect?
   - Recommendation: The daemon is a single-instance system service. All UI instances should see the same state.

## Implementation Waves

- **Wave 1:** Core management logic transition to `tokio` inside the daemon.
- **Wave 2:** D-Bus property and signal implementation for status and logs.
- **Wave 3:** UI integration and async handling.
