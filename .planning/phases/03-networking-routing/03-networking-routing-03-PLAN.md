---
phase: 03-networking-routing
plan: 03
type: execute
wave: 2
depends_on: [01, 02]
files_modified: [src/ui/pages/proxy_page.rs, src/ui/pages/proxy_page.ui, src/backend.rs, src/application.rs]
autonomous: false
requirements: [NET-01, NET-02]

must_haves:
  truths:
    - "User can toggle 'TUN Mode' in the UI"
    - "Toggling 'System Proxy' updates GNOME system settings"
    - "Enabling TUN mode correctly communicates with the daemon via D-Bus"
  artifacts:
    - path: "src/ui/pages/proxy_page.rs"
      provides: "UI for networking controls"
    - path: "src/backend.rs"
      provides: "GSettings and D-Bus integration logic"
  key_links:
    - from: "src/ui/pages/proxy_page.rs"
      to: "src/backend.rs"
      via: "method calls"
    - from: "src/backend.rs"
      to: "org.gnome.system.proxy GSettings"
      via: "gio crate"
---

<objective>
Implement the UI switches for TUN mode and system proxy. Configure GNOME system-wide proxy settings using GSettings.

Purpose: Provide user control over the networking modes.
Output: Integrated UI for networking settings, functioning GSettings proxy toggle.
</objective>

<execution_context>
@$HOME/.gemini/get-shit-done/workflows/execute-plan.md
@$HOME/.gemini/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/ROADMAP.md
@.planning/phases/03-networking-routing/03-RESEARCH.md
@src/ui/pages/proxy_page.rs
@src/ui/pages/proxy_page.ui
@src/backend.rs
@src/settings.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add TUN mode switch to the UI</name>
  <files>src/ui/pages/proxy_page.ui, src/ui/pages/proxy_page.rs</files>
  <action>
    - Add a new `adw::SwitchRow` for "TUN Mode" to `proxy_page.ui` above "System Proxy".
    - Update `src/ui/pages/proxy_page.rs` to bind the new switch.
    - Set its state based on `settings.tun_mode`.
    - Connect signals to save the new setting and mark changes.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>TUN mode switch is present and functional in the UI.</done>
</task>

<task type="auto">
  <name>Task 2: Implement GNOME system proxy via GSettings</name>
  <files>src/backend.rs</files>
  <action>
    - Update `src/backend.rs` to implement `update_system_proxy(enabled: bool, host: &str, http_port: u16, socks_port: u16)`.
    - Use the `gio` crate to access `org.gnome.system.proxy` GSettings schema.
    - If `enabled`:
      - Set `mode` to "manual".
      - Set `org.gnome.system.proxy.http` (host, port).
      - Set `org.gnome.system.proxy.https` (host, port).
      - Set `org.gnome.system.proxy.socks` (host, port).
    - If `!enabled`:
      - Set `mode` to "none".
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>GNOME system proxy configuration is implemented.</done>
</task>

<task type="checkpoint:human-verify">
  <name>Task 3: Integration verification</name>
  <what-built>Full networking and routing integration</what-built>
  <how-to-verify>
    1. Run the application (`ninja -C builddir && builddir/src/vrxx`).
    2. Go to the Proxy page.
    3. Toggle "System Proxy" ON and check GNOME Settings -> Network -> Proxy.
    4. Toggle "TUN Mode" ON and connect.
    5. Verify a "vrxx-tun" interface exists: `ip addr show vrxx-tun`.
    6. Verify system traffic routes through proxy (e.g., `curl -v https://google.com`).
  </how-to-verify>
  <resume-signal>approved</resume-signal>
</task>

</tasks>

<verification>
Automated: `cargo check`.
Manual: UI interaction and system status check.
</verification>

<success_criteria>
1. User can toggle TUN and System Proxy in the UI.
2. System proxy correctly updates GNOME settings.
3. TUN mode creates a functional network interface.
</success_criteria>

<output>
After completion, create `.planning/phases/03-networking-routing/03-03-SUMMARY.md`
</output>
