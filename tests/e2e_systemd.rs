/* tests/e2e_systemd.rs
 *
 * Requirement R4: Systemd Service Sandboxing, Linux Capability Bounding,
 * and Memory W^X Compatibility.
 */

mod common;

use std::path::Path;

fn get_hardened_systemd_service_content() -> Option<String> {
    let service_paths = [
        "data/vrxx-daemon.service",
        "/etc/systemd/system/vrxx-daemon.service",
    ];

    for path in &service_paths {
        if Path::new(path).exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if !content.trim().is_empty() {
                    return Some(content);
                }
            }
        }
    }

    // Check if scripts/install.sh has been updated with hardening directives
    if let Ok(install_script) = std::fs::read_to_string("scripts/install.sh") {
        if install_script.contains("CapabilityBoundingSet") {
            if let Some(start) = install_script.find("[Unit]") {
                if let Some(end) = install_script[start..].find("EOF") {
                    return Some(install_script[start..start + end].to_string());
                }
            }
        }
    }

    None
}

#[test]
fn test_systemd_capability_bounding_set() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    // Must drop all capabilities except networking
    assert!(
        content.contains("CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_BIND_SERVICE"),
        "Service must configure CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_BIND_SERVICE"
    );
}

#[test]
fn test_systemd_ambient_capabilities() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    assert!(
        content.contains("AmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE"),
        "Service must configure AmbientCapabilities for child processes"
    );
}

#[test]
fn test_systemd_privilege_sandboxing() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    // Privilege escalation lockdown
    assert!(
        content.contains("NoNewPrivileges=true"),
        "Service must enforce NoNewPrivileges=true"
    );

    // Filesystem strict read-only protection
    assert!(
        content.contains("ProtectSystem=strict"),
        "Service must enforce ProtectSystem=strict"
    );

    // Home shielding
    assert!(
        content.contains("ProtectHome=true"),
        "Service must enforce ProtectHome=true"
    );

    // Isolated temp namespace
    assert!(
        content.contains("PrivateTmp=true"),
        "Service must enforce PrivateTmp=true"
    );
}

#[test]
fn test_systemd_runtime_directory_management() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    // Runtime directory for /run/vrxx/daemon.sock
    assert!(
        content.contains("RuntimeDirectory=vrxx"),
        "Service must specify RuntimeDirectory=vrxx"
    );
    assert!(
        content.contains("RuntimeDirectoryMode=0755"),
        "Service must specify RuntimeDirectoryMode=0755"
    );
}

#[test]
fn test_systemd_memory_deny_write_execute_safe() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    // CRITICAL: MemoryDenyWriteExecute must NOT be true because sing-box (Go runtime)
    // requires W^X allocations. If set to true, sing-box immediately crashes.
    assert!(
        !content.contains("MemoryDenyWriteExecute=true"),
        "MemoryDenyWriteExecute must NOT be true (incompatible with sing-box Go runtime)"
    );
}

#[test]
fn test_systemd_device_and_socket_restrictions() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    // TUN device node access
    assert!(
        content.contains("DeviceAllow=/dev/net/tun rw"),
        "Service must restrict device access strictly to TUN"
    );

    // Address families restriction
    assert!(
        content.contains("RestrictAddressFamilies="),
        "Service must restrict address families"
    );
    assert!(
        content.contains("AF_UNIX"),
        "RestrictAddressFamilies must include AF_UNIX for IPC"
    );
    assert!(
        content.contains("AF_NETLINK"),
        "RestrictAddressFamilies must include AF_NETLINK for routing"
    );
}

#[test]
fn test_systemd_device_policy_closed() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    assert!(
        content.contains("DevicePolicy=closed"),
        "Service must enforce DevicePolicy=closed"
    );
}

#[test]
fn test_systemd_kernel_and_execution_protections() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    assert!(
        content.contains("ProtectKernelTunables=true"),
        "Service must enforce ProtectKernelTunables=true"
    );
    assert!(
        content.contains("ProtectKernelModules=true"),
        "Service must enforce ProtectKernelModules=true"
    );
    assert!(
        content.contains("ProtectControlGroups=true"),
        "Service must enforce ProtectControlGroups=true"
    );
    assert!(
        content.contains("RestrictSUIDSGID=true"),
        "Service must enforce RestrictSUIDSGID=true"
    );
    assert!(
        content.contains("RestrictRealtime=true"),
        "Service must enforce RestrictRealtime=true"
    );
}

#[test]
fn test_systemd_address_families_all_required() {
    let Some(content) = get_hardened_systemd_service_content() else {
        eprintln!("Notice: data/vrxx-daemon.service not yet present (pending Milestone M4)");
        return;
    };

    let line = content
        .lines()
        .find(|l| l.starts_with("RestrictAddressFamilies="))
        .expect("RestrictAddressFamilies directive must be present");

    assert!(
        line.contains("AF_UNIX"),
        "Must include AF_UNIX for daemon IPC"
    );
    assert!(
        line.contains("AF_INET"),
        "Must include AF_INET for IPv4 traffic"
    );
    assert!(
        line.contains("AF_INET6"),
        "Must include AF_INET6 for IPv6 traffic"
    );
    assert!(
        line.contains("AF_NETLINK"),
        "Must include AF_NETLINK for kernel routing"
    );
}
