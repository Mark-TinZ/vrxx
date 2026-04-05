# Requirements

## v1 Requirements

### Core/Backend
- CORE-01: Application implements a strict single-process backend managing one Xray/Sing-box core at a time.
- CORE-02: Application implements a Zero-Disk config footprint, generating and passing configurations strictly in-memory.
- CORE-03: System uses privilege-separated client-core architecture (unprivileged UI, privileged daemon via D-Bus).

### Network
- NET-01: Application provides TUN Mode (Transparent Proxy) to route all system traffic automatically.
- NET-02: Application provides fallback System Proxy Configuration (HTTP/SOCKS5).
- NET-03: Application supports basic routing rules (e.g., bypassing local LAN, blocking ads/trackers).

### UI/UX
- UI-01: Interface is built with native GTK4/Libadwaita adhering to GNOME HIG.
- UI-02: User can import keys and subscriptions via Clipboard and URL.
- UI-03: User can view connection status and basic system logs.
- UI-04: Interface includes educational tooltips and descriptions for technical terms.
- UI-05: Interface remains fully asynchronous and never freezes during backend operations.

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| CORE-01 | Phase 2 | Pending |
| CORE-02 | Phase 2 | Pending |
| CORE-03 | Phase 1 | Pending |
| NET-01 | Phase 3 | Pending |
| NET-02 | Phase 3 | Pending |
| NET-03 | Phase 3 | Pending |
| UI-01 | Phase 1 | Pending |
| UI-02 | Phase 4 | Pending |
| UI-03 | Phase 4 | Pending |
| UI-04 | Phase 4 | Pending |
| UI-05 | Phase 2 | Pending |