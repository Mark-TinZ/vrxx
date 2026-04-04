# VRXX

## What This Is

VRXX is a modern, native VPN client for Linux built on GTK4 and Libadwaita. It serves as an intuitive bridge between maximum simplicity for regular users and ultimate flexibility for advanced users, combining the power of Xray and Sing-box under the hood.

## Core Value

Providing a seamless, natively integrated GNOME experience with transparent, intelligent smart routing and absolute backend stability.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] **Backend Stability**: Zero-disk footprint (in-memory configs), robust error handling preventing silent crashes, and strict network control (TIME_WAIT resolution).
- [ ] **Smart Routing**: Single process execution, on-the-fly multiplexing of multiple keys, and automatic traffic splitting using built-in routing modules.
- [ ] **Native UI/UX (KISS + GNOME HIG)**: Informative animated status widget, educational interface to explain technical terms, and fully asynchronous UI that never freezes.
- [ ] **Open Source & Clean Architecture**: Licensed under MPL-2.0, with a modular, well-commented, community-friendly codebase.

### Out of Scope

- Non-Linux platforms — The focus is entirely on a native GNOME/Linux experience.

## Context

- **Tech Stack**: Rust, GTK4, Libadwaita, Xray, Sing-box.
- **Problem solved**: Existing VPN clients are either too complex for regular users or too limited for power users. VRXX aims to bridge this gap with an educational, modern native interface while retaining powerful smart routing capabilities.
- **Existing codebase**: This is a brownfield project; a codebase map already exists and should be used as a reference.

## Constraints

- **Platform**: Must look and feel like a natural part of the GNOME ecosystem (HIG compliance).
- **Architecture**: Strict single-process backend (only one core running at a time) and strict zero-disk config footprint.
- **Licensing**: MPL-2.0.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| In-memory config only | Improves security and prevents disk clutter. | — Pending |
| GTK4/Libadwaita | Ensures a native, modern GNOME experience. | — Pending |
| Single core process | Saves memory and prevents port conflicts. | — Pending |

---
*Last updated: Today after initialization*