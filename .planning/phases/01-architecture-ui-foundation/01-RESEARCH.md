# Phase 1: Architecture & UI Foundation - Research

**Researched:** 2026-04-04
**Domain:** Native Linux desktop application architecture (Rust, GTK4, D-Bus, PolicyKit)
**Confidence:** HIGH

## Summary

The current application requires transitioning to a strict privilege-separated architecture. The existing setup uses `gtk4` and `libadwaita` for the UI, but relies on `pkexec` directly within the process for privileged operations. The new architecture will separate the application into two distinct binaries or operating modes: an unprivileged GTK4 UI and a background D-Bus system daemon (`vrxx-daemon`) running as root.

**Primary recommendation:** Use `zbus` for seamless Rust-native D-Bus communication and `zbus_polkit` within the system daemon to validate caller privileges before executing sensitive network or proxy operations.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CORE-03 | System uses privilege-separated client-core architecture. | `zbus` over system bus with a `.conf` policy and Polkit `.policy` file |
| UI-01 | Interface is built with native GTK4/Libadwaita adhering to GNOME HIG. | Continue using `gtk4` (0.10) and `adw` (0.8) crates natively. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `zbus` | 5.14.0 | D-Bus client and server | Official standard Rust implementation of D-Bus, macro-driven and ergonomic |
| `zbus_polkit` | 5.0.0 | Polkit authorization | Native Polkit API checks via D-Bus within a zbus daemon |
| `gtk4` | 0.10+ | UI framework | Official GTK4 Rust bindings |
| `adw` | 0.8+ | GNOME HIG components | Official Libadwaita bindings |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | 1.x | Async runtime | Managing zbus connections and concurrent tasks in the background |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `zbus` | `dbus-rs` | `dbus-rs` relies on C-bindings (`libdbus`) whereas `zbus` is pure Rust, safer, and native async. |
| Polkit via DBus | `pkexec` wrapper | Spawning `pkexec` on demand breaks asynchronous UI workflows and asks for passwords redundantly. |

**Installation:**
```bash
cargo add zbus --features "tokio"
cargo add zbus_polkit
```

## Architecture Patterns

### Recommended Project Structure
```text
src/
├── bin/
│   ├── ui.rs         # The unprivileged GUI entrypoint
│   └── daemon.rs     # The privileged D-Bus daemon entrypoint
├── ui/               # GTK components
├── daemon/           # Daemon logic, Polkit checks, zbus handlers
└── protocol.rs       # Shared D-Bus trait definitions
data/
├── dbus-1/
│   └── system.d/
│       └── ru.mark.vrxx.conf     # System bus policy allowing name ownership
├── polkit-1/
│   └── actions/
│       └── ru.mark.vrxx.policy   # Polkit rules for the daemon
```
*Note: The project might currently have a single `main.rs`, which can be split or branched via CLI flags (e.g. `vrxx --daemon`).*

### Pattern 1: Async D-Bus Client in GTK4
**What:** Calling a system bus daemon from an unprivileged GTK UI without blocking the main thread.
**When to use:** Whenever the UI needs to tell the daemon to start/stop the proxy.
**Example:**
```rust
// Source: zbus official docs & gtk-rs async patterns
use zbus::Connection;
use glib::clone;

glib::MainContext::default().spawn_local(clone!(@weak window => async move {
    let connection = Connection::system().await.unwrap();
    let proxy = DaemonProxy::new(&connection).await.unwrap();
    // Non-blocking call
    if let Err(e) = proxy.start_proxy().await {
        eprintln!("Failed to start proxy: {}", e);
    }
}));
```

### Anti-Patterns to Avoid
- **Blocking the Main Thread:** Do not use `futures::executor::block_on` inside GTK event handlers when awaiting a `zbus` call. It will freeze the UI.
- **Root UI:** Running the entire GTK application via `sudo` or `pkexec`. It breaks Wayland support and poses significant security risks.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| D-Bus Serializers | Custom IPC parsing | `zbus` macros | Macro-driven IPC generation handles Rust types (like `Result`, `String`) automatically and efficiently over the wire. |
| Unix Socket IPC | Raw `std::os::unix::net` sockets | `zbus` | D-Bus handles system-wide bus registration and integrates natively with Polkit's caller-identifying features via bus names. |

## Common Pitfalls

### Pitfall 1: Missing D-Bus System Configuration
**What goes wrong:** The daemon fails to start with "Connection refused" or "Access denied" on the system bus.
**Why it happens:** The system bus enforces strict rules; by default, no user or root can claim a well-known name (e.g., `ru.mark.vrxx.daemon`) without an explicit allow rule in `/etc/dbus-1/system.d/` or `/usr/share/dbus-1/system.d/`.
**How to avoid:** Ensure a proper XML `.conf` file is installed to the system bus directory granting root ownership of the name and allowing users to invoke methods on it.

### Pitfall 2: Polkit Async Contexts
**What goes wrong:** Polkit verification fails unexpectedly or freezes.
**Why it happens:** When calling Polkit, the daemon must accurately pass the caller's D-Bus unique name (`:1.45`) and optionally handle the async challenge if interactive authentication is required.
**How to avoid:** Use `zbus_polkit`'s built-in `check_authorization` method and make sure `allow_interactive` is correctly propagated based on UI capabilities.

## Code Examples

### D-Bus Server Definition
```rust
// Source: zbus documentation
use zbus::{interface, ConnectionBuilder};

struct VrxxDaemon;

#[interface(name = "ru.mark.vrxx.Daemon")]
impl VrxxDaemon {
    async fn start_proxy(&self, #[zbus(header)] hdr: zbus::MessageHeader<'_>) -> Result<String, zbus::fdo::Error> {
        // Here you would verify polkit credentials using hdr.sender()
        Ok("Proxy started securely".to_string())
    }
}
```

## Open Questions

1. **System Service Installation**
   - What we know: The daemon needs to be installed as a systemd service and DBus system config must be placed in `/usr/share/dbus-1/system.d/`.
   - What's unclear: Does the project build system (Meson) currently handle `sudo make install` correctly for these elevated paths, or will we need to update the `meson.build` files?
   - Recommendation: The plan should include creating these `.conf` and `.policy` files and wiring them into `data/meson.build`.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `dbus-daemon` | System Bus | ✓ | 1.16.2 | — |
| `pkexec` / Polkit | Privilege checks | ✓ | 127 | — |
| `pkg-config` | Build system | ✓ | 2.5.1 | — |
| `cargo` / `rustc` | Rust build | ✓ | 1.88.0 | — |

**Missing dependencies with no fallback:**
- None

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` |
| Config file | `Cargo.toml` |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CORE-03 | Privilege-separated daemon | unit/integ | `cargo test --lib daemon` | ❌ Wave 0 |
| UI-01 | GTK4 window renders unprivileged | manual | `cargo run` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo check`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/test_daemon.rs` — covers CORE-03 IPC mocking
- [ ] `zbus` test connections via session bus for CI tests

## Sources

### Primary (HIGH confidence)
- Official `zbus` docs - Validated version 5.14.0 and API structures
- Official GTK4-rs async guide - Verified non-blocking spawn local pattern
- Current `Cargo.toml` - Verified current `gtk4` and `adw` dependencies

### Secondary (MEDIUM confidence)
- D-Bus system bus specification for `.conf` xml policies.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - `zbus` is the undisputed standard in Rust.
- Architecture: HIGH - Privilege separated architecture matches standard Linux application designs (e.g., systemd, NetworkManager).
- Pitfalls: HIGH - Common documented issues when building D-Bus system daemons.

**Research date:** 2026-04-04
**Valid until:** Stable
