# Technology Stack

**Project:** vrxx (Linux VPN client using Rust, GTK4/Libadwaita, Xray, Sing-box)
**Researched:** 2024
**Overall confidence:** HIGH

## Recommended Stack

### Core Framework
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Rust | 2021 Edition | System language | Memory safety, robust async networking, and the most rapidly growing GTK language bindings. |
| GTK4 (`gtk4-rs`) | v0.10+ | UI framework | The modern standard for Linux desktop interfaces, tightly integrated with GNOME. |
| Libadwaita (`libadwaita-rs`) | v0.8+ | UI components | Provides official GNOME HIG (Human Interface Guidelines) widgets, adaptive design, and responsive layouts natively. |

### Core Networking & Process Management
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Tokio | v1.x | Async runtime | Standard async runtime in Rust. Essential for bridging UI event loops with asynchronous child processes (Xray/Sing-box) and background networking. |
| Nix | v0.31+ | Unix signal & process control | Clean Rust bindings for POSIX APIs. Used for cleanly managing child daemon processes (Xray/Sing-box) via Unix signals (e.g., SIGTERM, SIGHUP). |
| Xray / Sing-box | latest | Underlying proxy engine | Executed as child processes or background services. Sing-box provides robust TUN routing; Xray provides strong protocol implementations (VLESS, VMess, XTLS). |

### Configuration & Data Handling
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Serde (`serde_json`) | v1.0+ | Configuration parsing | De facto standard for serialization. Essential for dynamically generating and parsing complex JSON configurations required by Xray and Sing-box. |
| Base64 & URL | latest | Parsing proxy links | Used to parse standard `vmess://`, `vless://`, `ss://` URI links and decode base64 encoded connection strings within those URIs. |

### Network Clients & Requests
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Reqwest | v0.12+ | HTTP Client (async) | Downloading subscription updates, GeoIP/GeoSite database files. Can be configured with `rustls` and SOCKS5 support to proxy its own traffic. |
| Ureq | v2.9+ | HTTP Client (sync) | Lightweight alternative for blocking requests where a Tokio async context isn't necessary or available. |

### Observability & Logging
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Tracing & Tracing-subscriber | v0.1.x / v0.3.x | Structured logging | A powerful alternative to `log` that captures hierarchical events. Extremely helpful for debugging across async tasks and UI events. |

### System Integration
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Gettext-rs | v0.7+ | i18n / Localization | Standard GNU gettext bindings for translating UI elements in native Linux environments. |
| Dirs | v6.x | Directory management | Safe and standard way to resolve XDG Base Directory specification paths (e.g., `~/.config`, `~/.local/share`). |

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| UI Architecture | raw `gtk4-rs` | `relm4` | Raw `gtk4-rs` provides direct control and is standard for minimal apps, which the current codebase uses. `relm4` offers superior Elm-like state management and makes async bridging easier, but migrating an existing codebase presents high friction. |
| Engine Integration | JSON configs + Process spawn | `xray-core` (gRPC crate) | Generating JSON config files and spawning the binary is simpler, more resilient to upstream API changes, and natively supports Sing-box. Direct gRPC bindings require heavy dependencies (`tonic`, `prost`) and lock you into a specific engine's API. |
| Privilege Management | Setcap / Polkit with App | Background Daemon (DBus IPC) | Running the GUI directly with elevated privileges (or polkit wrappers) is a security anti-pattern. While a background root daemon with DBus IPC (via `zbus`) is considered the gold standard for security, the current MVP approach using local processes provides simplicity without deep system DBus integration overhead. |

## Installation

```bash
# Core framework and UI dependencies
cargo add gtk4 --features v4_16
cargo add libadwaita --features v1_7

# Async and networking
cargo add tokio --features full
cargo add reqwest --features socks,json,rustls-tls-native-roots --no-default-features

# Serialization
cargo add serde serde_json --features serde/derive

# System tools
cargo add nix --features signal
cargo add tracing tracing-subscriber
```

## Sources

- [gtk4-rs official documentation](https://gtk-rs.org/) - HIGH confidence (Framework standard)
- [Sing-box Documentation](https://sing-box.sagernet.org/) - HIGH confidence (Engine standard)
- [Project Cargo.toml dependencies list] - HIGH confidence (Verified usage within the environment)
- Community best practices for Rust Linux apps (e.g., preference for `tokio`, `tracing`, `serde`) - HIGH confidence
