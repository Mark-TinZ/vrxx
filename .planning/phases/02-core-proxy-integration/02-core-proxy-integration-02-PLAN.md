---
phase: 02-core-proxy-integration
plan: 02
type: execute
wave: 1
depends_on: [02-core-proxy-integration-01]
files_modified:
  - src/daemon.rs
autonomous: true
requirements:
  - UI-03
  - UI-05

must_haves:
  truths:
    - Status property accurately reflects the proxy state.
    - Log signals are emitted for proxy stdout/stderr.
  artifacts:
    - path: src/daemon.rs
      provides: D-Bus interface signals/properties
      contains: #[zbus(property)], #[zbus(signal)]
---

<objective>
Expose real-time status and proxy logs from the privileged daemon to the unprivileged UI using D-Bus properties and signals.
</objective>

<tasks>

<task type="auto">
  <name>Implement Status Property in D-Bus Daemon</name>
  <files>src/daemon.rs</files>
  <action>
    Add a `status` property to the `VrxxDaemon` D-Bus interface:
    - Possible values: "Disconnected", "Connecting", "Connected", "Disconnecting", "Error".
    - Update the status within `start_proxy()` and `stop_proxy()`.
    - Emit a property change signal when the status is updated.
  </action>
</task>

<task type="auto">
  <name>Implement Log Streaming Signal</name>
  <files>src/daemon.rs</files>
  <action>
    Add a `log_message` signal to the `VrxxDaemon` D-Bus interface:
    - Signal signature: `(level: s, message: s)`.
    - Inside `start_proxy()`, spawn a tokio task for the child's `stdout` and `stderr`.
    - Read lines from the outputs and emit the `log_message` signal for each line.
  </action>
</task>

<task type="auto">
  <name>Handle Proxy Crash</name>
  <files>src/daemon.rs</files>
  <action>
    Spawn a monitor task when the proxy starts:
    - Await `child.wait()`.
    - If it exits unexpectedly, set the status to "Error" and emit a signal.
  </action>
</task>

</tasks>

<verification>
Monitor D-Bus traffic using `dbus-monitor` or `gdbus` to confirm status changes and log signals are emitted correctly.
</verification>
