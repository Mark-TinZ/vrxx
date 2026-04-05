---
phase: 03-networking-routing
plan: 02
type: execute
wave: 1
depends_on: []
files_modified: [src/domain/singbox_config.rs, src/domain/xray_config.rs]
autonomous: true
requirements: [NET-01, NET-03]

must_haves:
  truths:
    - "Generated configs correctly route all traffic to 'vrxx-tun' interface when TUN mode is enabled"
    - "Both configs correctly bypass LAN traffic from the TUN"
    - "Generated configuration passes core validation"
  artifacts:
    - path: "src/domain/singbox_config.rs"
      provides: "Sing-box TUN and routing configuration"
    - path: "src/domain/xray_config.rs"
      provides: "Xray TUN and routing configuration"
---

<objective>
Update core configuration generation to support TUN mode and routing rules, synchronizing with the daemon's networking settings.
</objective>

<execution_context>
@$HOME/.gemini/get-shit-done/workflows/execute-plan.md
@$HOME/.gemini/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/ROADMAP.md
@.planning/phases/03-networking-routing/03-RESEARCH.md
@.planning/phases/03-networking-routing/03-networking-routing-VALIDATION.md
@src/domain/singbox_config.rs
@src/domain/xray_config.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Update Sing-box config builder for TUN and routing</name>
  <files>src/domain/singbox_config.rs</files>
  <action>
    - Update `build_singbox_config` to include a `tun` inbound if `settings.tun_mode` is true.
    - Set the interface name to "vrxx-tun" and IP address to "172.19.0.1".
    - Enable `auto_route: true`, `strict_route: true`, and `stack: "gvisor"`.
    - Update routing rules to bypass private network IPs (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16).
    - Add ad-blocking rules if enabled in settings.
  </action>
  <verify>
    <automated>cargo test</automated>
  </verify>
  <done>Sing-box builder supports TUN and routing.</done>
</task>

<task type="auto">
  <name>Task 2: Update Xray config builder for TUN and routing</name>
  <files>src/domain/xray_config.rs</files>
  <action>
    - Update `build_xray_config` to include a `tun` inbound named "vrxx-tun" with IP "172.19.0.1".
    - Implement `FakeDNS` and sniffing in the configuration.
    - Implement routing rules for LAN bypass and ad-blocking.
    - Ensure proxy outbound uses a specific `sockopt.mark` (e.g., 255) to avoid routing loops.
  </action>
  <verify>
    <automated>cargo test</automated>
  </verify>
  <done>Xray builder supports TUN and routing.</done>
</task>

</tasks>

<verification>
Automated: `cargo test` verifying JSON generation for routing rules.
</verification>

<success_criteria>
1. Core configurations include "vrxx-tun" and "172.19.0.1" in TUN mode.
2. Routing rules correctly bypass LAN traffic and block ads.
3. Tests for configuration generation pass.
</success_criteria>

<output>
After completion, create `.planning/phases/03-networking-routing/03-02-SUMMARY.md`
</output>
