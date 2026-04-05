# Phase 3: Networking & Routing - Research

**Researched:** 2025-03-27
**Domain:** Linux Networking, TUN devices, System Proxy, DNS Leak Prevention
**Confidence:** HIGH

## Summary

Phase 3 focuses on implementing transparent proxying via TUN mode, managing system-wide proxy settings in GNOME, and ensuring DNS leak prevention. Sing-box provides a highly integrated TUN implementation with automatic routing, while Xray requires more manual orchestration from the privileged daemon. GNOME system proxy configuration is handled via GSettings, and DNS integrity is maintained by integrating with `systemd-resolved` via D-Bus.

**Primary recommendation:** Use `sing-box` as the preferred backend for TUN mode due to its native `auto_route` and `strict_route` features. For Xray, the privileged daemon must manage routing tables and IP rules using the `rtnetlink` crate.

## User Constraints

### Locked Decisions (from Phase Focus)
- Support TUN mode for both Xray and Sing-box on Linux.
- Use a privileged Rust daemon for device management.
- Configure GNOME system proxy (HTTP/SOCKS5) via GSettings or D-Bus.
- Implement routing rules (bypass LAN, ad-blocking).
- Integrate with `systemd-resolved` for DNS leak prevention.

### the agent's Discretion
- Choice of Rust crates for TUN and Netlink (`tun-rs`, `rtnetlink` recommended).
- Specific implementation of DNS hijacking (FakeDNS vs Direct).
- Logic for automatic routing table management for Xray.

### Deferred Ideas (OUT OF SCOPE)
- Support for non-Linux platforms.
- Complex policy-based routing beyond basic LAN bypass and ad-blocking.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tun-rs` | 1.0+ | TUN device creation | Modern, high-performance, async-friendly. |
| `rtnetlink` | 0.14+ | Network configuration | Type-safe Linux Netlink implementation for IPs/routes. |
| `zbus` | 5.0 | D-Bus communication | Standard Rust D-Bus crate, used for systemd-resolved. |
| `gio` | 0.20+ | GSettings access | Native GNOME/GLib library for system settings. |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `nix` | 0.31 | Signal handling & iface index | Low-level syscalls and interface name-to-index. |
| `serde_json` | 1.0 | Config generation | Generating core JSON configurations in-memory. |

**Installation:**
```bash
# Add new networking dependencies
cargo add tun-rs rtnetlink zbus
```

## Architecture Patterns

### Recommended Project Structure
```
src/
├── daemon/
│   ├── mod.rs          # Existing: IPC and core management
│   ├── network.rs      # New: TUN device and routing logic (rtnetlink)
│   └── dns.rs          # New: systemd-resolved integration (zbus)
├── domain/
│   ├── singbox_config.rs # Update: Enhanced TUN and routing rules
│   └── xray_config.rs    # Update: Add TUN inbound and FakeDNS
└── ui/
    └── backend.rs      # Update: Proxy setting toggle logic (GSettings)
```

### Pattern 1: Sing-box TUN Mode (Native)
**What:** Leveraging Sing-box's built-in Linux networking stack.
**When to use:** Default for TUN mode on Sing-box.
**Example:**
```json
// Source: https://sing-box.sagernet.org/configuration/inbound/tun/
{
  "type": "tun",
  "interface_name": "tun0",
  "address": ["172.19.0.1/30"],
  "auto_route": true,
  "strict_route": true,
  "stack": "gvisor", // Userspace stack for better compatibility/safety
  "sniff": true
}
```

### Pattern 2: Xray TUN Mode (Manual Orchestration)
**What:** Xray 1.8.0+ TUN inbound combined with daemon-managed routing.
**When to use:** Required for Xray backend.
**Example (Daemon Logic):**
```rust
// Use rtnetlink to bring interface up and set default route
// Equivalent to: ip link set dev xray0 up && ip route add default dev xray0 table 100
let mut links = handle.link().get().match_name("xray0".into()).execute();
if let Some(link) = links.try_next().await? {
    handle.link().set(link.header.index).up().execute().await?;
    handle.route().add().v4().destination_prefix(Ipv4Addr::UNSPECIFIED, 0)
        .output_interface(link.header.index).table(100).execute().await?;
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| DNS Settings | `/etc/resolv.conf` | `systemd-resolved` (D-Bus) | Avoid race conditions and system-breaking edits. |
| Routing Table | `Command::new("ip")` | `rtnetlink` | Faster, type-safe, and avoids dependency on external binaries. |
| Proxy Config | Manual `.env` / Exports | `GSettings` | Standard GNOME way; applications respect it instantly. |
| Device Creation | Manual `ioctl` | `tun-rs` | Handles edge cases, platform differences, and async safety. |

## Common Pitfalls

### Pitfall 1: Routing Loops
**What goes wrong:** Proxy's outbound traffic (to VPS) is routed back into the TUN interface, causing a crash or infinite loop.
**How to avoid:** Use `sockopt.mark` (Fwmark) or `sockopt.interface` in outbound configuration to ensure proxy traffic bypasses the TUN.

### Pitfall 2: DNS Leaks
**What goes wrong:** System continues using ISP DNS instead of the proxy DNS, revealing browsing activity.
**How to avoid:** Configure `systemd-resolved` via `SetLinkDNS` and set `SetLinkDomains` to `~.` for the TUN interface to capture all queries.

### Pitfall 3: IPv6 Leaks
**What goes wrong:** Traffic leaks via IPv6 if the TUN only handles IPv4.
**How to avoid:** Either disable IPv6 globally during connection or ensure TUN handles `::/0` and provides an IPv6 address.

## Code Examples

### GSettings Proxy Configuration (Unprivileged Client)
```rust
// Source: GIO Settings API
use gio::prelude::*;
use gio::Settings;

fn set_gnome_proxy(host: &str, port: u16) {
    let settings = Settings::new("org.gnome.system.proxy");
    settings.set_string("mode", "manual").unwrap();
    
    let http_settings = Settings::new("org.gnome.system.proxy.http");
    http_settings.set_string("host", host).unwrap();
    http_settings.set_int("port", port as i32).unwrap();
    
    let socks_settings = Settings::new("org.gnome.system.proxy.socks");
    socks_settings.set_string("host", host).unwrap();
    socks_settings.set_int("port", port as i32).unwrap();
}
```

### systemd-resolved Integration (Privileged Daemon)
```rust
// Source: systemd-resolved D-Bus API
async fn protect_dns(iface_index: i32, dns_server: &str) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let msg = (iface_index, vec![(2, dns_server.parse::<Ipv4Addr>().unwrap().octets().to_vec())]);
    conn.call_method(
        Some("org.freedesktop.resolve1"),
        "/org/freedesktop/resolve1",
        Some("org.freedesktop.resolve1.Manager"),
        "SetLinkDNS",
        &msg,
    ).await?;
    
    // Set search domain to "~." to capture all traffic
    conn.call_method(
        Some("org.freedesktop.resolve1"),
        "/org/freedesktop/resolve1",
        Some("org.freedesktop.resolve1.Manager"),
        "SetLinkDomains",
        &(iface_index, vec![("~.", true)]),
    ).await?;
    Ok(())
}
```

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `sing-box` | Backend Core | ✓ | 1.13.0 | Use Xray |
| `xray` | Backend Core | ✓ | 26.2.6 | Use Sing-box |
| `systemd-resolved` | DNS Protection | ✓ | active | Manual `/etc/resolv.conf` (Risky) |
| `gsettings` | System Proxy | ✓ | 2.86.5 | Environment variables (Fragile) |
| `iproute2` | Networking | ✓ | 6.19.0 | — |

**Missing dependencies with no fallback:**
- None.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Cargo (Rust) |
| Config file | `Cargo.toml` |
| Quick run command | `cargo test` |
| Full suite command | `cargo test --all-features` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| NET-01 | TUN device creation | Integration | `cargo test test_tun_creation` | ❌ Wave 0 |
| NET-02 | GSettings proxy toggle | Integration | `cargo test test_proxy_settings` | ❌ Wave 0 |
| NET-03 | Routing rule generation | Unit | `cargo test test_routing_rules` | ❌ Wave 0 |

### Wave 0 Gaps
- [ ] `src/daemon/tests.rs` — covers TUN device creation and DNS integration.
- [ ] `src/ui/proxy_tests.rs` — covers GSettings interaction.

## Sources

### Primary (HIGH confidence)
- Official Sing-box Docs: TUN Inbound configuration.
- Official Xray Docs: TUN Inbound and FakeDNS patterns.
- GNOME Developer Docs: GSettings `org.gnome.system.proxy` schema.
- freedesktop.org: `systemd-resolved` D-Bus API documentation.

### Secondary (MEDIUM confidence)
- `tun-rs` and `rtnetlink` crates: READMEs and examples.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Recommended crates are industry standards for Rust networking.
- Architecture: HIGH - Patterns follow official core documentation and Linux networking best practices.
- Pitfalls: HIGH - Common issues (loops, leaks) are well-documented in the community.

**Research date:** 2025-03-27
**Valid until:** 2025-04-26
