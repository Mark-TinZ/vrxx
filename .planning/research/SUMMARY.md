# Research Summary: VRXX VPN Client

## Executive Summary

VRXX is a modern, native Linux VPN client designed to seamlessly integrate with the GNOME desktop. It leverages Rust for memory-safe systems programming and robust asynchronous process management, paired with GTK4 and Libadwaita for a native, responsive user interface. Under the hood, it utilizes industry-standard proxy engines like Xray and Sing-box to handle complex routing and encryption protocols.

The heavily recommended approach centers on a Privilege-Separated Client-Core Architecture. Because establishing TUN interfaces and modifying routing tables requires root privileges, the application must be split into an unprivileged GUI frontend and a privileged backend daemon communicating via D-Bus. Additionally, to enhance security and prevent disk clutter, VRXX must implement a "Zero-Disk Config Footprint" where complex JSON/HCL configurations are generated dynamically in-memory and passed to the proxy cores via stdin.

The most critical risks involve monolithic privilege escalation (running the UI as root), DNS leaks during TUN routing, and orphaned "zombie" proxy processes. Mitigating these requires strict separation of concerns, careful integration with `systemd-resolved`, and robust Unix process group management (using `nix` and `tokio`) to ensure proxy cores are cleanly terminated when the application exits or state changes.

## Key Findings

### Stack & Technology
- **Core Framework:** Rust (2021) with `gtk4-rs` and `libadwaita-rs` for native GNOME integration. Raw `gtk4-rs` is preferred given the existing codebase.
- **Async & Process Management:** Tokio for the async runtime and Nix for POSIX process control (essential for cleanly managing child daemons via signals).
- **Proxy Engines:** Xray and Sing-box (executed as child processes).
- **Data Handling:** Serde for dynamic JSON config generation, and Reqwest for handling subscription updates.

### Features & Scope
- **Table Stakes:** TUN Mode (Transparent Proxy), Key & Subscription Import (Clipboard/URL), Start/Stop functionality, and Connection Status/Logs.
- **Differentiators:** Zero-Disk Config Footprint (in-memory configs), strict single-process backend, native GTK4 UI, and an educational interface.
- **Anti-Features:** Avoid cross-platform bloat (Windows/macOS), in-app JSON editors, and custom protocol implementations.
- **Deferred to v2:** Smart routing multiplexing and advanced background auto-updates.

### Architecture Patterns
- **Privilege Separation (Daemon-UI):** Crucial pattern splitting the app into an unprivileged GUI and a privileged backend service communicating via D-Bus (`zbus`).
- **Domain-to-Core Config Translation:** The app manages high-level domain models (Profiles) and dynamically generates low-level JSON for the core.
- **GTK4 MVC:** Utilizing `gio::ListModel`, `gio::ListStore`, and `gtk::ListView` for efficient, dynamic UI rendering of server lists.

### Critical Pitfalls
- **Monolithic Privilege Escalation:** Running the GTK app as root is a massive security vulnerability. Must use a polkit/DBus daemon.
- **DNS Leaks:** Conflicts with `systemd-resolved` can break internet connectivity or leak DNS to ISPs. Explicit fakedns/hijacking rules must be configured.
- **Zombie Proxy Processes:** Rust `Command` doesn't auto-kill children. Must use process groups (`setpgid`) and catch termination signals.
- **GObject Boilerplate / State Desync:** Strict ownership model clashes with GTK. Requires careful management of `glib::clone!` and state synchronization.

## Implications for Roadmap

Based on the research, the development roadmap should be structured into the following phases to front-load architectural risks:

### Suggested Phases

1. **Phase 1: Architecture & Privilege Separation**
   - *Rationale:* Security and privilege handling must be solved before any networking code is written to avoid rewrites.
   - *Delivers:* Unprivileged GTK4 UI skeleton communicating with a privileged Rust backend daemon via D-Bus.
   - *Features:* Application scaffolding, basic DBus IPC.
   - *Pitfalls to Avoid:* Monolithic Privilege Escalation.

2. **Phase 2: Core Proxy Integration & Process Management**
   - *Rationale:* Proving the engine works reliably with the Zero-Disk Config constraint is the next highest technical risk.
   - *Delivers:* Strict single-process backend, dynamically generating JSON configs in-memory and passing them via stdin to Xray/Sing-box.
   - *Features:* Start/Stop connection, Zero-Disk Config Footprint.
   - *Pitfalls to Avoid:* Zombie Proxy Processes (implement `nix` process groups).

3. **Phase 3: TUN Routing & Network Interception**
   - *Rationale:* With the proxy running, the next step is routing all system traffic through it securely.
   - *Delivers:* System-wide transparent proxying and DNS hijacking.
   - *Features:* TUN Mode, basic routing rules.
   - *Pitfalls to Avoid:* DNS Leaks (strict `systemd-resolved` integration).

4. **Phase 4: User Workflows & UI Polish**
   - *Rationale:* Built on a solid foundation, the UI can now be fleshed out to handle complex data models.
   - *Delivers:* Profile management, server lists, and telemetry.
   - *Features:* Key & Subscription Import, Connection Status & Logs, Native GTK4/Libadwaita UI using `gio::ListModel`.
   - *Pitfalls to Avoid:* GObject "Boilerplate Wall" and State Desync.

### Research Flags
- **Needs Research:** Phase 1 (Specific polkit/DBus authorization flows in Rust), Phase 3 (Exact `systemd-resolved` API integration for TUN devices).
- **Standard Patterns:** Phase 2 (Tokio child process management), Phase 4 (GTK4 standard widgets).

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Based on standard ecosystem practices for Rust/GTK4 apps and verified Cargo dependencies. |
| Features | HIGH | Clear alignment with standard Linux VPN client expectations and project constraints. |
| Architecture | HIGH | Privilege-separated daemon architecture is the gold standard for network-modifying Linux applications. |
| Pitfalls | HIGH | Well-documented issues with Linux network stacks, orphan processes, and GTK ownership models. |

**Gaps to Address:**
- The exact mechanism for Polkit authorization during the D-Bus connection needs to be validated.
- The precise JSON structures required for Xray vs. Sing-box TUN inbound configs need to be codified into Serde models.

## Sources
- gtk4-rs official documentation & HIG guidelines
- Sing-box & Xray-core Official Documentation / GitHub Repositories
- Linux Daemon Architecture (zbus/D-Bus)
- Cybersecurity reports on privilege escalation and DNS leaks