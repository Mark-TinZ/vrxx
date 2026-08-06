# ТЗ 03: Генерация Конфигураций sing-box 1.13+ и Поддержка Версий

## 1. Назначение и Архитектурный Слой
- **Слой:** Domain (`src/domain/singbox_config.rs`)
- **Назначение:** Формирование валидного JSON-файла конфигурации для `sing-box` на основе распарсенных ключей и настроек пользователя с учетом версии установленного ядра (от 1.8 до 1.13+).

## 2. Требования к реализации
1. **Динамическая адаптация к версиям (`get_singbox_version()`):**
   - Автоматическое определение версии `sing-box version`.
   - **Для sing-box >= 1.13:**
     - Использование массива `address: ["172.19.0.1/30", "fdfe:dcba:9876::1/126"]` в `inbounds.tun`.
     - WireGuard конфигурируется через верхнеуровневый массив `"endpoints"`.
     - В `dns.rules` отклонение IPv6 задается через `{"query_type": ["AAAA"], "action": "reject"}`.
     - В `route.rules` перехват DNS задается через `{"protocol": "dns", "action": "hijack-dns"}`.
   - **Для sing-box < 1.12:**
     - Использование полей `inet4_address` и `inet6_address` в TUN inbound при работе со старыми ядрами.
2. **Поддержка протоколов:**
   - VLESS (с поддержкой XTLS-Vision и REALITY `pbk`, `sid`, `sni`, `fingerprint`).
   - VMess (gRPC, WebSocket, TCP, AlterId).
   - Trojan (TLS, uTLS fingerprint).
   - Shadowsocks (2022-blake3-aes-128-gcm / AEAD).
   - Hysteria2 (`up_mbps`, `down_mbps`, `obfs`).
   - TUIC v5 (`congestion_control`, `udp_relay_mode`).
3. **Маршрутизация и Гео-правила:**
   - Подключение удаленных наборов правил SRS (MetaCubeX / SagerNet geosite & geoip).
   - Обход LAN, локальных IP, блокировка рекламы.

## 3. Требования из CONTRIBUTING.md
- **Чистота слоя Domain:** Никаких зависимостей от GTK4 / `gio` / `gdk`. Только чистые структуры Rust (`serde`, `serde_json`).
- **Обработка ошибок:** Отсутствие `.unwrap()`. Безопасная сборка JSON с фоллбеками через `unwrap_or_default()`.
- **Юнит-тестирование:** Наличие тестов `test_singbox_config_validity` в `singbox_config.rs` с проверкой генерации JSON для всех поддерживаемых протоколов.
