---
phase: 03-networking-routing
plan: 01
type: execute
wave: 1
depends_on: []
files_modified: [src/daemon/network.rs, src/daemon/dns.rs, src/daemon/mod.rs, src/ipc.rs, Cargo.toml]
autonomous: true
requirements: [NET-01]

must_haves:
  truths:
    - "Daemon can create a named TUN interface (e.g., vrxx-tun)"
    - "Daemon can set IP and bring interface UP using rtnetlink"
    - "Daemon can configure DNS via systemd-resolved D-Bus"
  artifacts:
    - path: "src/daemon/network.rs"
      provides: "TUN device management"
    - path: "src/daemon/dns.rs"
      provides: "systemd-resolved integration"
  key_links:
    - from: "src/daemon/mod.rs"
      to: "src/daemon/network.rs"
      via: "module integration"
    - from: "src/daemon/mod.rs"
      to: "src/daemon/dns.rs"
      via: "module integration"
---

<objective>
Implement the privileged networking foundation in the daemon. This includes TUN device management and system DNS protection via systemd-resolved.

Purpose: Provide the necessary privileges and low-level networking capabilities for transparent proxying.
Output: New network and DNS modules in the daemon, extended D-Bus interface.
</objective>

<execution_context>
@$HOME/.gemini/get-shit-done/workflows/execute-plan.md
@$HOME/.gemini/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/ROADMAP.md
@.planning/phases/03-networking-routing/03-RESEARCH.md
@src/daemon/mod.rs
@src/ipc.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add networking dependencies and create TUN manager</name>
  <files>Cargo.toml, src/daemon/network.rs</files>
  <action>
    - Add `tun-rs`, `rtnetlink`, and `zbus` to Cargo.toml.
    - Create `src/daemon/network.rs` implementing `TunManager`.
    - `TunManager` should use `tun_rs::Device` to create a TUN device named "vrxx-tun".
    - Implement `setup_interface` using `rtnetlink` to:
      1. Set IP address (e.g., 172.19.0.1/30).
      2. Set interface UP.
      3. (Optional) Set routing table rules for Xray if needed later.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>TUN management code is implemented and compiles.</done>
</task>

<task type="auto">
  <name>Task 2: Implement DNS integration via systemd-resolved</name>
  <files>src/daemon/dns.rs</files>
  <action>
    - Create `src/daemon/dns.rs` implementing `DnsManager`.
    - Use `zbus` to call `org.freedesktop.resolve1.Manager` methods.
    - Implement `set_dns(iface_index: i32, dns_servers: Vec<String>)`:
      - Call `SetLinkDNS`.
      - Call `SetLinkDomains` with `[("~.", true)]` to capture all traffic.
    - Implement `reset_dns(iface_index: i32)` to revert changes when disconnected.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>DNS management code is implemented and compiles.</done>
</task>

<task type="auto">
  <name>Task 3: Integrate with Daemon and IPC</name>
  <files>src/daemon/mod.rs, src/daemon/dns.rs, src/ipc.rs</files>
  <action>
    - Update `src/daemon/mod.rs` to include `network` and `dns` modules.
    - Add `NetworkManager` to `ProxyManager` struct.
    - Update `start_proxy` to:
      1. If TUN mode requested, create TUN device and setup interface.
      2. If TUN mode requested, configure DNS using `DnsManager`.
    - Update `stop_proxy` to clean up TUN and DNS.
    - Update `VrxxDaemon` in `src/ipc.rs` to accept a `tun_mode` flag in `start_proxy` or add a new method.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>Daemon integrates networking logic and exposes it via IPC.</done>
</task>

</tasks>

<verification>
Check for compilation and existence of new modules.
Manual verification of TUN creation will be part of integration testing in later plans.
</verification>

<success_criteria>
1. Daemon compiles with new networking dependencies.
2. `src/daemon/network.rs` and `src/daemon/dns.rs` exist with requested logic.
3. IPC interface supports requesting TUN mode.
</success_criteria>

<output>
After completion, create `.planning/phases/03-networking-routing/03-01-SUMMARY.md`
</output>
