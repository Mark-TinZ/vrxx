# Roadmap

## Phases
- [x] **Phase 1: Architecture & UI Foundation** - Establish the secure, privilege-separated application skeleton with a native interface.
- [x] **Phase 2: Core Proxy Integration** - The backend reliably manages and pipes configurations to proxy engines without touching disk.
- [x] **Phase 3: Networking & Routing** - System traffic is securely routed through the proxy with essential rules applied.
- [x] **Phase 4: User Workflows & Polish** - Users can easily import keys, view status, and learn how to use the client.
- [x] **Phase 5: UI Cleanup & Core Fixes** - Stable UI with descriptive tooltips and reliable core selection logic.
- [x] **Phase 6: Core Stability & Sing-box Refactor** - Transition to Sing-box only, fix proxy/TUN bugs, and improve logging.
- [x] **Phase 7: Log Window Refactor** - Refactor VrxxLogWindow for GNOME HIG compliance, fix autoscroll, and stabilize SSE logs.

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
- [x] 01-architecture-ui-foundation-01-PLAN.md — Introduce DBus and Polkit system configuration files
- [x] 01-architecture-ui-foundation-02-PLAN.md — Implement the privileged daemon and zbus DBus server
- [x] 01-architecture-ui-foundation-03-PLAN.md — Refactor backend to connect to DBus proxy
**UI hint**: yes

### Phase 2: Core Proxy Integration
**Goal**: The backend reliably manages and pipes configurations to proxy engines without touching disk
**Depends on**: Phase 1
**Requirements**: CORE-01, CORE-02, UI-05
**Success Criteria**:
  1. Application can start and cleanly stop a single Xray or Sing-box proxy process
  2. Proxy configurations are generated dynamically and passed to the proxy via stdin (Zero-Disk)
  3. UI remains responsive and does not freeze while the proxy starts and stops
**Plans**: 3 plans
- [x] 02-core-proxy-integration-01-PLAN.md — Implement async core management in the daemon
- [x] 02-core-proxy-integration-02-PLAN.md — Implement D-Bus signals and properties for status/logs
- [x] 02-core-proxy-integration-03-PLAN.md — Refactor UI to handle async status and log streaming
**UI hint**: yes

### Phase 3: Networking & Routing
**Goal**: System traffic is securely routed through the proxy with essential rules applied
**Depends on**: Phase 2
**Requirements**: NET-01, NET-02, NET-03
**Success Criteria**:
  1. User can enable TUN mode for transparent, system-wide proxying without DNS leaks
  2. User can fall back to HTTP/SOCKS5 system proxy if TUN mode is disabled
  3. Basic routing rules automatically bypass local LAN traffic
**Plans**: 3 plans
- [x] 03-networking-routing-01-PLAN.md — Privileged Daemon Networking Foundation
- [x] 03-networking-routing-02-PLAN.md — Enhanced Core Configurations
- [x] 03-networking-routing-03-PLAN.md — UI Integration & System Proxy
**UI hint**: yes

### Phase 4: User Workflows & Polish
**Goal**: Users can easily import keys, view status, and learn how to use the client
**Depends on**: Phase 3
**Requirements**: UI-02, UI-03, UI-04
**Success Criteria**:
  1. User can import connection keys and subscriptions from clipboard or URL
  2. User can view live connection status and debug logs in the UI
  3. User sees educational tooltips explaining technical routing terms inline
**Plans**: 3 plans
- [x] 04-user-workflows-01-PLAN.md — Импорт ключей и управление конфигурациями
- [x] 04-user-workflows-02-PLAN.md — Доработка UI и исправление UX-ошибок
- [x] 04-user-workflows-03-PLAN.md — Локализация и Geo-ресурсы
**UI hint**: yes

### Phase 5: UI Cleanup & Core Fixes
**Goal**: Stable UI with descriptive tooltips and reliable core selection logic
**Depends on**: Phase 4
**Requirements**: UI-01, UI-04, CORE-01
**Success Criteria**:
  1. Dead UI references removed from Settings page to prevent runtime errors
  2. All configuration options have descriptive tooltips in English and Russian
  3. Core selection correctly switches between Xray and Sing-box without race conditions
**Plans**: 3 plans
- [x] 05-01-PLAN.md — UI Cleanup & Tooltips
- [x] 05-02-PLAN.md — Core Selection & Life Cycle Fixes
- [x] 05-03-PLAN.md — Gap Closure (Runtime Panics)
**UI hint**: yes

### Phase 6: Core Stability & Sing-box Refactor
**Goal**: Transition to Sing-box only, fix proxy/TUN bugs, and improve logging.
**Depends on**: Phase 5
**Requirements**: CORE-01, UI-01, UI-02, CORE-02, NET-01
**Success Criteria**:
  1. Application version updated to 0.1.8 and Xray support is completely removed.
  2. Sing-box proxy and TUN modes function reliably across different engine versions.
  3. Log rotation is implemented and UI console displays real-time backend logs.
  4. Import logic is consolidated and prepared for QR-code inputs.
**Plans**: 3 plans
- [x] 06-01-PLAN.md — Version Update & UI Refinement
- [x] **Phase 7: Log Window Refactor** - Refactor VrxxLogWindow for GNOME HIG compliance, fix autoscroll, and stabilize SSE logs.
- [x] **Phase 9: Quality and Localization** - Полная локализация на русский язык, документирование кода и UX-полировка.
- [ ] **Phase 10: Decoupling and Logging Overhaul** - Рефакторинг архитектуры для уменьшения связанности и модернизация системы логирования.

## Phase Details
...
### Phase 9: Quality and Localization
**Goal**: Довести приложение до полной готовности для русскоязычных пользователей и улучшить качество кода
**Depends on**: Phase 8
**Requirements**: UI-02, QUALITY-01
**Success Criteria**:
  1. [x] 100% покрытие интерфейса и кода локализацией (gettext)
  2. [x] Подробные русскоязычные комментарии в ключевых модулях бэкенда
  3. [x] Устранение дублирования настроек TUN Mode (перенос в Settings)
  4. [x] Реализация автоматического обновления Geo-ресурсов
- [x] **Phase 10: Decoupling and Logging Overhaul** - Рефакторинг архитектуры для уменьшения связанности и модернизация системы логирования.

## Phase Details
...
### Phase 10: Decoupling and Logging Overhaul
**Goal**: Ослабить зависимости между UI и Core, внедрить масштабируемую систему логирования
**Depends on**: Phase 9
**Success Criteria**:
  1. [x] Выделение логики IPC в отдельный сервисный слой (Service Pattern)
  2. [x] Переход на асинхронное логирование через кастомный Tracing Layer
  3. [x] Внедрение кольцевого буфера в демоне для хранения истории логов
  4. [x] Оптимизация потребления памяти в окне логов (лимит строк)
**Plans**: 1 plan
- [x] phase-10-decoupling-logging.md — Архитектурная декомпозиция и логирование

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Architecture & UI Foundation | 3/3 | Completed | Today |
| 2. Core Proxy Integration | 3/3 | Completed | Today |
| 3. Networking & Routing | 3/3 | Completed | Today |
| 4. User Workflows & Polish | 3/3 | Completed | Today |
| 5. UI Cleanup & Core Fixes | 3/3 | Completed | Today |
| 6. Core Stability & Sing-box Refactor | 3/3 | Completed | Today |
| 7. Log Window Refactor | 1/1 | Completed | Today |
| 8. Log Window Enhancements | 1/1 | Completed | Today |
| 9. Quality and Localization | 1/1 | Completed | Today |
| 10. Decoupling and Logging Overhaul | 1/1 | Completed | Today |
| 11. Interactive Core Installation Dialog | 1/1 | Completed | Today |
