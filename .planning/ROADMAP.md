# Roadmap

## Phases
- [ ] **Phase 1: Architecture & UI Foundation** - Establish the secure, privilege-separated application skeleton with a native interface.
- [ ] **Phase 2: Core Proxy Integration** - The backend reliably manages and pipes configurations to proxy engines without touching disk.
- [ ] **Phase 3: Networking & Routing** - System traffic is securely routed through the proxy with essential rules applied.
- [ ] **Phase 4: User Workflows & Polish** - Users can easily import keys, view status, and learn how to use the client.

## Phase Details

### Phase 1: Architecture & UI Foundation
**Goal**: Establish the secure, privilege-separated application skeleton with a native interface
**Depends on**: Nothing
**Requirements**: CORE-03, UI-01
**Success Criteria**:
  1. Application launches as an unprivileged GTK4/Libadwaita window matching GNOME HIG
  2. Privileged backend daemon starts securely and communicates with the UI via D-Bus
  3. UI does not require monolithic root privileges to run
**Plans**: 3 plans
- [ ] 01-architecture-ui-foundation-01-PLAN.md — Introduce DBus and Polkit system configuration files
- [ ] 01-architecture-ui-foundation-02-PLAN.md — Implement the privileged daemon and zbus DBus server
- [ ] 01-architecture-ui-foundation-03-PLAN.md — Refactor backend to connect to DBus proxy
**UI hint**: yes

### Phase 2: Core Proxy Integration
**Goal**: The backend reliably manages and pipes configurations to proxy engines without touching disk
**Depends on**: Phase 1
**Requirements**: CORE-01, CORE-02, UI-05
**Success Criteria**:
  1. Application can start and cleanly stop a single Xray or Sing-box proxy process
  2. Proxy configurations are generated dynamically and passed to the proxy via stdin (Zero-Disk)
  3. UI remains responsive and does not freeze while the proxy starts and stops
**Plans**: TBD

### Phase 3: Networking & Routing
**Goal**: System traffic is securely routed through the proxy with essential rules applied
**Depends on**: Phase 2
**Requirements**: NET-01, NET-02, NET-03
**Success Criteria**:
  1. User can enable TUN mode for transparent, system-wide proxying without DNS leaks
  2. User can fall back to HTTP/SOCKS5 system proxy if TUN mode is disabled
  3. Basic routing rules automatically bypass local LAN traffic
**Plans**: TBD

### Phase 4: User Workflows & Polish
**Goal**: Users can easily import keys, view status, and learn how to use the client
**Depends on**: Phase 3
**Requirements**: UI-02, UI-03, UI-04
**Success Criteria**:
  1. User can import connection keys and subscriptions from clipboard or URL
  2. User can view live connection status and debug logs in the UI
  3. User sees educational tooltips explaining technical routing terms inline
**Plans**: TBD
**UI hint**: yes

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Architecture & UI Foundation | 0/3 | Planned | - |
| 2. Core Proxy Integration | 0/0 | Not started | - |
| 3. Networking & Routing | 0/0 | Not started | - |
| 4. User Workflows & Polish | 0/0 | Not started | - |