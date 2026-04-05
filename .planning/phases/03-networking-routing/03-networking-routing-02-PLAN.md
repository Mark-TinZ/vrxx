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
    - "Sing-box config includes 'tun' inbound with auto_route and strict_route when TUN mode is enabled"
    - "Xray config includes 'tun' inbound and Sniffing support"
    - "Both configs include routing rules for LAN bypass and ad-blocking"
  artifacts:
    - path: "src/domain/singbox_config.rs"
      provides: "Sing-box TUN and routing configuration"
    - path: "src/domain/xray_config.rs"
      provides: "Xray TUN and routing configuration"
---

<objective>
Update Sing-box and Xray configuration generation logic to support TUN mode and routing rules.

Purpose: Enable transparent proxying and smart routing at the engine level.
Output: Updated core config builders.
</objective>

<execution_context>
@$HOME/.gemini/get-shit-done/workflows/execute-plan.md
@$HOME/.gemini/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/ROADMAP.md
@.planning/phases/03-networking-routing/03-RESEARCH.md
@src/domain/singbox_config.rs
@src/domain/xray_config.rs
@src/settings.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Update Sing-box configuration for TUN and routing</name>
  <files>src/domain/singbox_config.rs</files>
  <action>
    - Modify `build_singbox_config` to include a `tun` inbound if `settings.tun_mode` is true.
    - Set `auto_route: true`, `strict_route: true`, and `stack: "gvisor"` for the TUN inbound.
    - Add a `dns` configuration block including a `servers` list with at least one remote DNS server.
    - Update the `route` block to include rules for:
      - Bypassing local LAN traffic (e.g., `192.168.0.0/16`, `10.0.0.0/8`).
      - Blocking ads and trackers if enabled in settings.
      - Routing all other traffic through the remote outbound.
  </action>
  <verify>
    <automated>cargo test</automated>
  </verify>
  <done>Sing-box configuration generator supports TUN and routing.</done>
</task>

<task type="auto">
  <name>Task 2: Update Xray configuration for TUN and routing</name>
  <files>src/domain/xray_config.rs</files>
  <action>
    - Modify `XrayConfig` generation to include a `tun` inbound (e.g., using `fakedns` and `dokodemo-door`).
    - Add `sniffing` settings for the inbound.
    - Implement a `RoutingConfig` that includes:
      - Rules for bypassing LAN IPs.
      - Rules for ad-blocking.
      - Ensuring outbound traffic bypasses the TUN (using fwmark if necessary).
  </action>
  <verify>
    <automated>cargo test</automated>
  </verify>
  <done>Xray configuration generator supports TUN and routing.</done>
</task>

</tasks>

<verification>
Check unit tests for correct JSON generation.
</verification>

<success_criteria>
1. Core configurations correctly include TUN inbounds when `tun_mode` is enabled.
2. Routing rules for LAN bypass and ad-blocking are present in generated configs.
3. Tests for config generation pass.
</success_criteria>

<output>
After completion, create `.planning/phases/03-networking-routing/03-02-SUMMARY.md`
</output>
