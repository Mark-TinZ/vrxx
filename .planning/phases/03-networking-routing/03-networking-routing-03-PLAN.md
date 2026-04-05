---
phase: 03-networking-routing
plan: 03
type: execute
wave: 2
depends_on: [01, 02]
files_modified: [src/ui/pages/proxy_page.rs, src/ui/pages/proxy_page.ui, src/backend.rs, src/ui/pages/vpn_page.rs, src/ui/proxy_tests.rs]
autonomous: false
requirements: [NET-01, NET-02]

must_haves:
  truths:
    - "User can toggle 'TUN Mode' in the UI and it persists"
    - "System Proxy toggle updates GSettings 'org.gnome.system.proxy' mode"
    - "Starting proxy with TUN mode sends 'tun_mode: true' via D-Bus"
  artifacts:
    - path: "src/ui/pages/proxy_page.rs"
      provides: "UI integration for networking"
    - path: "src/ui/proxy_tests.rs"
      provides: "Integration tests for GSettings"
---

<objective>
Implement the UI networking switches, wire them to the daemon IPC, and implement GSettings proxy configuration.
</objective>

<execution_context>
@$HOME/.gemini/get-shit-done/workflows/execute-plan.md
@$HOME/.gemini/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/ROADMAP.md
@.planning/phases/03-networking-routing/03-RESEARCH.md
@.planning/phases/03-networking-routing/03-networking-routing-VALIDATION.md
@src/ui/pages/proxy_page.rs
@src/ui/pages/proxy_page.ui
@src/backend.rs
@src/settings.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Update UI and wire switches to logic</name>
  <files>src/ui/pages/proxy_page.ui, src/ui/pages/proxy_page.rs, src/ui/proxy_tests.rs</files>
  <action>
    - Add `adw::SwitchRow` for "TUN Mode" to `proxy_page.ui`.
    - Update `src/ui/pages/proxy_page.rs` to handle switch toggles and save to `settings.tun_mode`.
    - Ensure the "System Proxy" switch calls `CoreBackend::update_system_proxy`.
    - Create `src/ui/proxy_tests.rs` with `test_proxy_toggle` to verify GSettings interaction.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>UI controls are integrated and functional.</done>
</task>

<task type="auto">
  <name>Task 2: Implement GSettings proxy and wire TUN Mode to IPC</name>
  <files>src/backend.rs, src/ui/pages/vpn_page.rs</files>
  <action>
    - Update `src/backend.rs` to implement `update_system_proxy` using `gio` to set "org.gnome.system.proxy" mode.
    - Update the `start_proxy` call in `src/ui/pages/vpn_page.rs` (and any other place) to include the `tun_mode` parameter from settings.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>System proxy logic and IPC wiring are complete.</done>
</task>

<task type="checkpoint:human-verify">
  <name>Task 3: Full networking verification</name>
  <what-built>Full end-to-end networking stack</what-built>
  <how-to-verify>
    1. Build and run: `ninja -C builddir && builddir/src/vrxx`.
    2. Toggle "System Proxy" and check `gsettings get org.gnome.system.proxy mode`.
    3. Toggle "TUN Mode" and connect.
    4. Run `ip addr show vrxx-tun` to confirm interface exists.
    5. Confirm DNS is routed: `resolvectl dns vrxx-tun` should show 172.19.0.1.
  </how-to-verify>
  <resume-signal>approved</resume-signal>
</task>

</tasks>

<verification>
Automated: `cargo check` and `cargo test src/ui/proxy_tests.rs`.
Manual: UI and system check.
</verification>

<success_criteria>
1. UI reflects networking settings accurately.
2. GSettings are updated correctly by the unprivileged client.
3. Proxy connects with TUN mode, creating a functional interface and DNS setup.
</success_criteria>

<output>
After completion, create `.planning/phases/03-networking-routing/03-03-SUMMARY.md`
</output>
