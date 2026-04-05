---
phase: 02-core-proxy-integration
plan: 03
type: execute
wave: 1
depends_on: [02-core-proxy-integration-02]
files_modified:
  - src/window.rs
  - src/ui/pages/vpn_page.rs
  - src/ui/components/log_window.rs
autonomous: true
requirements:
  - UI-03
  - UI-05

must_haves:
  truths:
    - UI "Connect" button state reflects the daemon status.
    - Log window displays logs received via D-Bus signals.
  artifacts:
    - path: src/ui/pages/vpn_page.rs
      provides: D-Bus signal listener
      contains: glib::MainContext::default().spawn_local
---

<objective>
Update the unprivileged GTK UI to interact with the daemon's status and logs, ensuring a responsive, asynchronous user experience.
</objective>

<tasks>

<task type="auto">
  <name>Listen for Status Changes in VPN Page</name>
  <files>src/ui/pages/vpn_page.rs</files>
  <action>
    Modify `VpnPage` to:
    - Connect to the `VrxxDaemon` D-Bus proxy.
    - Setup a listener for `status` property changes.
    - Use `glib::MainContext::default().spawn_local` to handle signals without blocking.
    - Update the "Connect/Disconnect" button and status label based on the daemon's current state.
  </action>
</task>

<task type="auto">
  <name>Listen for Log Signals in Log Window</name>
  <files>src/ui/components/log_window.rs</files>
  <action>
    Update the `VrxxLogWindow` to:
    - Connect to the `VrxxDaemon` D-Bus proxy.
    - Setup a listener for the `log_message` signal.
    - Append received log lines to the log text view.
  </action>
</task>

<task type="auto">
  <name>Asynchronous Connect/Disconnect</name>
  <files>src/ui/pages/vpn_page.rs</files>
  <action>
    Ensure clicking "Connect" or "Disconnect" calls the daemon's `start_proxy()` or `stop_proxy()` methods asynchronously via the `zbus` proxy.
  </action>
</task>

</tasks>

<verification>
Start the daemon and then the UI. Click "Connect", verify the button updates, and verify logs appear in the log window. Close the UI, reopen it, and confirm the status is still correctly reflected.
</verification>
