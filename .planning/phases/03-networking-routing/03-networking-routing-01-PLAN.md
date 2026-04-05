---
phase: 03-networking-routing
plan: 01
type: execute
wave: 1
depends_on: []
files_modified: [src/daemon/network.rs, src/daemon/dns.rs, src/daemon/mod.rs, src/ipc.rs, src/daemon/tests.rs, Cargo.toml]
autonomous: true
requirements: [NET-01]

must_haves:
  truths:
    - "Daemon can create a named TUN interface (vrxx-tun)"
    - "Daemon can set IP 172.19.0.1 and bring interface UP using rtnetlink"
    - "Daemon can configure routing tables and IP rules for vrxx-tun (to capture system traffic)"
    - "Daemon can configure DNS via systemd-resolved D-Bus, pointing to 172.19.0.1"
  artifacts:
    - path: "src/daemon/network.rs"
      provides: "TUN device and routing management"
    - path: "src/daemon/dns.rs"
      provides: "systemd-resolved integration"
    - path: "src/daemon/tests.rs"
      provides: "Integration tests for networking"
---

<objective>
Implement the privileged networking foundation in the daemon, including TUN device management, system routing/rules, and system DNS protection.
</objective>

<execution_context>
@$HOME/.gemini/get-shit-done/workflows/execute-plan.md
@$HOME/.gemini/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/ROADMAP.md
@.planning/phases/03-networking-routing/03-RESEARCH.md
@.planning/phases/03-networking-routing/03-networking-routing-VALIDATION.md
@src/daemon/mod.rs
@src/ipc.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add networking dependencies and implement TunManager</name>
  <files>Cargo.toml, src/daemon/network.rs, src/daemon/tests.rs</files>
  <action>
    - Add `tun-rs`, `rtnetlink`, and `zbus` to Cargo.toml.
    - Create `src/daemon/network.rs` with `TunManager`.
    - `TunManager` must create a TUN device named "vrxx-tun".
    - Use `rtnetlink` to:
      1. Set IPv4 "172.19.0.1/30".
      2. Set interface UP.
      3. Create a new routing table (e.g., table 100) and add a default route through "vrxx-tun".
      4. Add an `ip rule` to direct all traffic (except marked traffic) to table 100.
    - Create `src/daemon/tests.rs` with `test_tun_creation` and `test_routing_rules`.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>TUN management and routing code is implemented and compiles.</done>
</task>

<task type="auto">
  <name>Task 2: Implement DnsManager via systemd-resolved</name>
  <files>src/daemon/dns.rs, src/daemon/tests.rs</files>
  <action>
    - Create `src/daemon/dns.rs` with `DnsManager`.
    - Implement `set_dns(iface_index: i32, dns_servers: Vec<String>)` using `zbus` for `SetLinkDNS` and `SetLinkDomains` (with "~.").
    - Implement `reset_dns(iface_index: i32)` to clear settings.
    - Add `test_dns_protection` to `src/daemon/tests.rs`.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>DNS management code is implemented and compiles.</done>
</task>

<task type="auto">
  <name>Task 3: Integrate Networking into Daemon and IPC</name>
  <files>src/daemon/mod.rs, src/ipc.rs</files>
  <action>
    - Add `network` and `dns` modules to `src/daemon/mod.rs`.
    - Update `ProxyManager` to manage `TunManager` and `DnsManager` lifecycle.
    - In `start_proxy`, if TUN mode is enabled:
      1. Create TUN "vrxx-tun" and setup routing rules.
      2. Set DNS for the TUN interface to "172.19.0.1".
    - Update `VrxxDaemon` D-Bus interface in `src/ipc.rs` to include a `tun_mode: bool` parameter in `start_proxy`.
    - Update the `Daemon` proxy trait accordingly.
  </action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>Daemon integrates networking logic and exposes it via IPC.</done>
</task>

</tasks>

<verification>
Check for compilation and existence of new modules and tests.
</verification>

<success_criteria>
1. Daemon compiles with new networking dependencies.
2. `src/daemon/network.rs` and `src/daemon/dns.rs` exist with requested logic.
3. System routing rules and tables are correctly orchestrated for transparent proxying.
4. IPC interface supports `tun_mode` parameter.
</success_criteria>

<output>
After completion, create `.planning/phases/03-networking-routing/03-01-SUMMARY.md`
</output>
