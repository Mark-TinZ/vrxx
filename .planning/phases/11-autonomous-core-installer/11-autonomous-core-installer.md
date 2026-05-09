# Phase 11: Interactive Core Installation Dialog

**Goal**: Implement an autonomous and user-friendly GNOME HIG-compliant dialog to handle the absence of the `sing-box` core.
**Depends on**: Phase 10
**Success Criteria**:
  1. [x] Application detects missing core on startup and prompts user.
  2. [x] User can manually select a release archive (.tar.gz / .zip) for installation.
  3. [x] User can automatically download the correct core for their OS/Arch with a progress bar.
  4. [x] Core functionality is verified post-installation before enabling connection.

## Implementation Details
- Module `src/daemon/updater.rs` handles the download, extraction, and OS/Arch detection.
- Component `src/ui/components/core_installer.rs` provides the interactive Adwaita dialog.
- Integration in `src/application.rs` ensures the check runs early in the application lifecycle.

## Plans
- [x] phase-11.md — Autonomous Core Installation Logic & UI Dialog