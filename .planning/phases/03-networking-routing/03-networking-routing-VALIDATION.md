# Validation: Phase 03 - Networking & Routing

## 1. Requirement Coverage

| ID | Requirement | Test Type | File | Status |
|----|-------------|-----------|------|--------|
| NET-01 | TUN Mode | Integration | `src/daemon/tests.rs` | ❌ Pending |
| NET-02 | System Proxy (GSettings) | Integration | `src/ui/proxy_tests.rs` | ❌ Pending |
| NET-03 | Routing rules (LAN bypass) | Unit | `src/domain/tests.rs` | ❌ Pending |

## 2. Automated Tests (Wave 0)

### 2.1. Daemon Networking Integration
**Path:** `src/daemon/tests.rs`
**Goal:** Verify TUN device creation and `systemd-resolved` interaction.
- `test_tun_creation`: Check if `vrxx-tun` is created and brought UP with 172.19.0.1.
- `test_dns_protection`: Check if `SetLinkDNS` is called via D-Bus (mocked or live).

### 2.2. UI GSettings Integration
**Path:** `src/ui/proxy_tests.rs`
**Goal:** Verify GSettings interaction for system proxy.
- `test_proxy_toggle`: Check if `org.gnome.system.proxy` mode changes when toggled.

### 2.3. Config Routing Rules
**Path:** `src/ui/tests.rs` (update existing)
**Goal:** Verify JSON generation for routing rules.
- `test_singbox_routing`: Verify LAN bypass rules in JSON.
- `test_xray_routing`: Verify LAN bypass rules in JSON.

## 3. Manual Verification Checklist

- [ ] Run `ip addr show vrxx-tun` - verify interface existence.
- [ ] Run `gsettings get org.gnome.system.proxy mode` - verify toggle changes mode to 'manual'.
- [ ] Run `resolvectl dns vrxx-tun` - verify DNS server is set to 172.19.0.1.
- [ ] Connect with TUN mode and browse - verify connectivity.
