---
phase: 01-architecture-ui-foundation
plan: 02
type: execute
wave: 2
depends_on: ["01"]
files_modified:
  - src/ipc.rs
  - src/daemon/mod.rs
  - src/main.rs
autonomous: true
requirements:
  - CORE-03

must_haves:
  truths:
    - Running `vrxx --daemon` starts the daemon logic instead of the GTK UI
    - The daemon successfully connects to the D-Bus System Bus
  artifacts:
    - path: src/ipc.rs
      provides: D-Bus interface definition for zbus
      contains: '#[zbus::interface]'
    - path: src/daemon/mod.rs
      provides: Daemon execution logic and DBus server implementation
      contains: pub async fn run()
    - path: src/main.rs
      provides: CLI flag interception for the daemon
      contains: if args.contains(&"--daemon".to_string())
  key_links:
    - from: src/main.rs
      to: src/daemon/mod.rs
      via: Calling daemon::run() when --daemon is passed
      pattern: daemon::run()
---

<objective>
Implement the privileged D-Bus daemon process.

Purpose: To create a background daemon that runs as root, binds to the `ru.mark.vrxx.daemon` D-Bus name, and can listen to proxy start/stop commands securely.
Output: A new internal daemon module and D-Bus trait, executable via `--daemon` CLI flag.
</objective>

<execution_context>
@$HOME/.gemini/get-shit-done/workflows/execute-plan.md
@$HOME/.gemini/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@src/main.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Define the D-Bus IPC protocol</name>
  <files>src/ipc.rs</files>
  <action>
    Create `src/ipc.rs` containing the `zbus::interface` definitions. 
    Define a struct `VrxxDaemon` and implement `#[zbus::interface(name = "ru.mark.vrxx.Daemon")]`.
    Add stub methods `async fn ping(&self) -> zbus::fdo::Result<String>` and `async fn start_proxy(&self) -> zbus::fdo::Result<String>`.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>The IPC trait is correctly defined using zbus macros.</done>
</task>

<task type="auto">
  <name>Task 2: Implement the daemon process loop</name>
  <files>src/daemon/mod.rs</files>
  <action>
    Create `src/daemon/mod.rs`. Define `pub async fn run() -> anyhow::Result<()>`.
    Inside the run loop, initialize `zbus::ConnectionBuilder::system()`, serve the `VrxxDaemon` struct at `/ru/mark/vrxx/Daemon`, build the connection, and wait indefinitely (e.g. using `std::future::pending::<()>().await;`). Add tracing logs to indicate daemon started.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>The daemon server logic compiles without errors.</done>
</task>

<task type="auto">
  <name>Task 3: Connect daemon to main application entrypoint</name>
  <files>src/main.rs</files>
  <action>
    Modify `src/main.rs`. Import `pub mod daemon;` and `pub mod ipc;`.
    At the start of `main()`, check `std::env::args()`. If `--daemon` is present, initialize a tokio runtime (e.g. `tokio::runtime::Runtime::new().unwrap().block_on(daemon::run())`) and exit early (`std::process::exit(0)`).
    Ensure the regular GTK flow is bypassed entirely if running as daemon.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>The application correctly branches based on the `--daemon` CLI flag.</done>
</task>

</tasks>

<verification>
Run `cargo check` and review the structure. Note that we won't manually run `--daemon` fully tested in CI without DBus config installed, but compiling is sufficient for execution.
</verification>

<success_criteria>
The Rust codebase builds with the new daemon and IPC modules.
</success_criteria>

<output>
After completion, create `.planning/phases/01-architecture-ui-foundation/01-architecture-ui-foundation-02-SUMMARY.md`
</output>
