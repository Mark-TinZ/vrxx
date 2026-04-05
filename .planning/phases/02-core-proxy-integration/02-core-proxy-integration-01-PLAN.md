---
phase: 02-core-proxy-integration
plan: 01
type: execute
wave: 1
depends_on: [01-architecture-ui-foundation-03]
files_modified:
  - src/daemon.rs
  - src/backend.rs
autonomous: true
requirements:
  - CORE-01
  - CORE-02

must_haves:
  truths:
    - The daemon uses `tokio::process::Command` to spawn core processes.
    - Configuration is passed to the core via `stdin` (Zero-Disk).
  artifacts:
    - path: src/daemon.rs
      provides: `ProxyManager` internal logic
      contains: tokio::process::Child
---

<objective>
Move the core management logic from the unprivileged client/backend to the privileged daemon, utilizing `tokio` for asynchronous process control.
</objective>

<tasks>

<task type="auto">
  <name>Implement ProxyManager in Daemon</name>
  <files>src/daemon.rs</files>
  <action>
    Create a `ProxyManager` struct in `src/daemon.rs` that:
    - Stores an `Arc<Mutex<Option<tokio::process::Child>>>`.
    - Implements an async `start_proxy(core_type, config_json)` method.
    - Uses `tokio::process::Command` with `stdin(Stdio::piped())`.
    - Writes the `config_json` to the child's stdin and then closes it.
  </action>
</task>

<task type="auto">
  <name>Implement Stop Logic with SIGTERM</name>
  <files>src/daemon.rs</files>
  <action>
    Implement an async `stop_proxy()` method in `ProxyManager` that:
    - Takes the child process from the mutex.
    - Sends `SIGTERM` using `nix::sys::signal` (or `child.kill()` as fallback).
    - Awaits `child.wait()` with a timeout.
    - If timeout expires, sends `SIGKILL`.
  </action>
</task>

<task type="auto">
  <name>Clean up old backend logic</name>
  <files>src/backend.rs</files>
  <action>
    Remove the `std::process` based spawning logic from `backend.rs` if it's no longer needed (it should have been refactored to a D-Bus proxy in Phase 1).
  </action>
</task>

</tasks>

<verification>
`cargo check` passes and unit tests for `ProxyManager` (mocking the core binary if needed) are successful.
</verification>
