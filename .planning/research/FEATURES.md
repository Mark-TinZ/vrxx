# Feature Landscape

**Domain:** Linux VPN client using Rust, GTK4/Libadwaita, Xray, Sing-box
**Researched:** 2024-05
**Overall confidence:** HIGH

## Table Stakes

Features Linux users expect from a modern proxy/VPN client (like Nekoray or v2rayA). Missing these means the product feels incomplete or unusable for daily drivers.

| Feature | Why Expected | Complexity | Notes (v1 vs v2) |
|---------|--------------|------------|-------------------|
| **Start/Stop Connection** | The fundamental purpose of the app. | Low | v1: Basic toggle. v2: Auto-connect on specific networks. |
| **TUN Mode (Transparent Proxy)** | Users expect all system traffic to route through the VPN automatically without manual browser setup. | Medium | v1: Core TUN support via Sing-box/Xray. |
| **Key & Subscription Import** | Manual entry is dead. Users need to paste links (VLESS, VMess, Trojan, etc.) or scan QR codes. | Low | v1: Clipboard/URL import. v2: QR code screen capture. |
| **Subscription Auto-Update** | Server lists change frequently. Automatic refreshing is a must. | Low | v1: Manual update button. v2: Background auto-update. |
| **Basic Routing Rules** | Bypassing local LAN and blocking ads/trackers (GeoIP/GeoSite). | Medium | v1: Pre-defined rulesets. |
| **Connection Status & Logs** | Need to see if it's connected, latency, and debug logs if it fails. | Low | v1: Simple status and log view. |
| **System Proxy Configuration** | Fallback for when TUN mode is not desired or fails. | Low | v1: HTTP/SOCKS5 system proxy setting. |

## Differentiators

Features that set VRXX apart from existing complex, non-native Linux clients (like Qt-based Nekoray or Web-based v2rayA).

| Feature | Value Proposition | Complexity | Notes (v1 vs v2) |
|---------|-------------------|------------|-------------------|
| **Zero-Disk Config Footprint** | Security and cleanliness. Passing configurations to Xray/Sing-box strictly in-memory (e.g., via stdin) prevents disk clutter and unauthorized access. | High | v1: Core architectural requirement. |
| **Native GTK4/Libadwaita UI** | Seamless integration with the GNOME desktop, fluid animations, responsive design, and native dark/light mode, contrasting with clunky cross-platform UIs. | Medium | v1: UI foundation. |
| **Educational Interface** | Bridges the gap between regular users and power users by explaining technical routing/proxy terms inline without overwhelming the user. | Low | v1: Tooltips and descriptive settings. |
| **Smart Routing Multiplexing** | Automatically splitting traffic across multiple imported keys/nodes simultaneously based on speed or routing rules, without requiring complex JSON knowledge. | High | v2: Advanced backend orchestration. |
| **Strict Single-Process Backend** | Guaranteed prevention of port conflicts and memory bloat by strictly managing one backend core process at a time. | Medium | v1: Robust process management in Rust. |
| **Fully Asynchronous UI** | The UI never freezes while the backend negotiates connections or parses large subscription lists. | Medium | v1: Rust async/await with GTK main loop. |

## Anti-Features

Features to explicitly NOT build to maintain scope and stability.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Cross-Platform Support (Windows/macOS)** | Leads to "lowest common denominator" UI frameworks (Electron/Qt) and bloat. | Focus exclusively on a premium, native GNOME/Linux experience. |
| **In-App JSON Editor** | Allows users to break the internal logic and requires complex validation. | Use a structured UI to build rules, then generate the JSON in-memory automatically. |
| **Custom Protocol Implementations** | Reinventing the wheel is dangerous for security and stability. | Rely strictly on upstream Xray-core and Sing-box binaries for protocol handling. |
| **Built-In VPN Service/Account Store** | Bloats the app with payment gateways and account management. | Keep the app as a pure, agnostic client where users bring their own keys/subscriptions. |

## Feature Dependencies

```text
Native GTK4/Libadwaita UI → Fully Asynchronous UI (To keep animations smooth)
Zero-Disk Config Footprint → Strict Single-Process Backend (To pipe in-memory configs reliably)
Basic Routing Rules → Smart Routing Multiplexing (Needs basic routing foundation first)
Key & Subscription Import → Subscription Auto-Update
```

## MVP Recommendation

**Prioritize for v1:**
1. Native GTK4/Libadwaita UI framework setup.
2. Start/Stop connection with strictly single-process backend (Xray/Sing-box).
3. Zero-disk config footprint (in-memory config passing).
4. Key & Subscription Import (Clipboard/URL).
5. TUN Mode (Transparent Proxy).

**Defer for v2:** 
- Smart Routing Multiplexing (advanced traffic splitting across multiple nodes).
- Subscription Auto-Update (background tasks).
- QR code screen capture import.
- Advanced network-based auto-connect.

## Sources

- Ecosystem Analysis: Existing Linux clients (Nekoray, v2rayA, Hiddify) heavily rely on TUN mode and routing rules but lack native GNOME integration.
- VRXX Project Guidelines: PROJECT.md strictly dictates zero-disk footprint, native GTK4 UI, and single-process execution.
- Official GTK4/Libadwaita HIG (Human Interface Guidelines) for native app behavior.