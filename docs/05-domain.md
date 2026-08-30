# 05. Протоколы и конфигурация ядра

## Обзор

Слой Domain (`src/domain/`) отвечает за обработку VPN-данных независимо от того, кто их вызвал (графический интерфейс или демон). Его основные задачи:
1. Парсинг пользовательских строк/URI в стандартизированную структуру.
2. Трансляция этой структуры + настроек приложения в сложный JSON-формат конфигурации ядра Sing-box спецификации 1.13.18+.

## Поддерживаемые протоколы

| Протокол | Формат ввода (URI Scheme) | Парсер (key_parser) | Генератор Sing-box | Особенности реализации |
| --- | --- | --- | --- | --- |
| **VLESS** | `vless://uuid@host:port?params...` | ✅ Полный | ✅ Полный | REALITY (`pbk`, `sid`), XTLS-Vision (`flow`), uTLS (`fp`), Mux, TLS Fragment |
| **VMess** | `vmess://[Base64 JSON]` | ✅ Полный | ✅ Полный | gRPC, WebSocket (`ws`), TCP, AlterId, `xudp`, Mux, TLS Fragment |
| **Trojan** | `trojan://pass@host:port?params...` | ✅ Полный | ✅ Полный | TLS, uTLS отпечаток, Mux, TLS Fragment |
| **Shadowsocks**| `ss://[Base64 URL]@host:port` | ✅ Полный | ✅ Полный | 2022-blake3-aes-128/256-gcm, AEAD (chacha20, aes-gcm), SIP003 плагины (`v2ray-plugin`, `obfs-local`), `udp_over_tcp: true`, Mux |
| **Hysteria2** | `hy2://pass@host:port?params...` | ✅ Полный | ✅ Полный | `up_mbps`, `down_mbps`, obfs (salamander), uTLS, TLS Fragment, QUIC-мультиплексирование |
| **TUIC v5** | `tuic://uuid:pass@host:port?params...` | ✅ Полный | ✅ Полный | `congestion_control` (bbr), `udp_relay_mode` (native/quic), TLS Fragment |
| **WireGuard** | `wg://privkey@host:port?params...` | ✅ Полный | ✅ Полный | Спецификация 1.13.18+ через массив `endpoints` (PSK, `reserved` [u8; 3], `mtu`, `persistent_keepalive_interval`) |
| **SOCKS5** | `socks5://[user:pass@]host:port` | ✅ Полный | ✅ Полный | Анонимная и парольная авторизация, TCP и UDP ретрансляция |
| **HTTP / HTTPS** | `http(s)://[user:pass@]host:port` | ✅ Полный | ✅ Полный | HTTP CONNECT прокси, базовая аутентификация, опциональный TLS |

> [!TIP]
> **Особенности TUN и UDP/TCP**:
> - Протоколы VLESS, VMess, Trojan, plain Shadowsocks, WireGuard, Hysteria2 и TUIC полноценно туннелируют как TCP, так и UDP.
> - HTTP-прокси туннелирует **только TCP**. В режиме TUN для HTTP-прокси DNS автоматически резолвится через DoH (TCP), а QUIC (UDP 443) блокируется.
> - Shadowsocks с SIP003 плагинами (например, WebSocket) туннелирует TCP и автоматически включает `udp_over_tcp: true`.

## Парсинг ключей (key_parser.rs)

Любой валидный VPN-ключ парсится в универсальную структуру `ParsedKey`:

```rust
pub struct ParsedKey {
    pub protocol: String,      // "VLESS", "VMess", "Trojan", "Shadowsocks", "Hysteria2", "TUIC", "WireGuard", "SOCKS", "HTTP"
    pub name: String,          // Имя ключа (обычно берется из фрагмента #name)
    pub host: String,          // IP-адрес или домен сервера
    pub port: u16,
    pub uuid: String,          // Пароль, UUID, приватный ключ или токен
    pub query_params: HashMap<String, String>, // Все параметры (security, type, sni, pbk, obfs, cc, plugin, psk, reserved, mtu)
    pub raw_url: String,       // Исходная строка
}
```

**Особенности парсинга:**
- `VMess` отбрасывает префикс `vmess://`, декодирует Base64 и парсит полученный JSON-объект.
- `Shadowsocks` поддерживает SIP002 Base64 и plain text userinfo (`method:password`), а также SIP003 плагины (`plugin=obfs-local;...`).
- `SOCKS` и `HTTP/HTTPS` парсят анонимные и защищенные учетные данные (`user:pass`).
- `WireGuard` поддерживает `psk`, `reserved` (байты обфускации через запятую), `mtu` и `ip`.
- Функция `build_vpn_key(parsed: &ParsedKey)` выполняет обратную сериализацию структуры в стандартную URI-строку (для экспорта и QR-кодов).

## Генерация и экспорт QR-кодов (exporter.rs)

Модуль `src/domain/exporter.rs` предоставляет универсальный API генерации визуальных QR-кодов:
- `generate_qr_svg(content: &str) -> Result<String>`: Формирует строку векторного формата SVG.
- `generate_qr_png_bytes(content: &str, width: u32, height: u32) -> Result<Vec<u8>>`: Рендерит растровое изображение формата PNG в байтовый буфер `Vec<u8>`.
- `generate_qr_texture(content: &str, size: u32) -> Result<gdk::Texture>`: Безопасно формирует объект `gdk::Texture` напрямую из оперативки в памяти без создания временных файлов на диске.

## Генерация конфигурации Sing-box (singbox_config.rs)

VRXX динамически генерирует полную JSON-конфигурацию с помощью `build_singbox_config(&ParsedKey, &AppSettings)` в строгом соответствии со спецификацией **Sing-box 1.13.18+**:

### 1. Архитектурные особенности спецификации 1.13.18+
- **WireGuard Endpoints**: В версии 1.13+ WireGuard **запрещен в `outbounds`** и генерируется в верхнеуровневом массиве `"endpoints"`. Маршрутизация направляет трафик на `tag: "proxy"` эндпоинта.
- **Мультиплексирование (Mux `h2mux`)**: Применяется к TCP-протоколам (VLESS, VMess, Trojan, Shadowsocks) и автоматически отключается для WireGuard (L3 UDP), Hysteria2/TUIC (QUIC уже мультиплексирует потоки) и HTTP.
- **TLS Фрагментация (`tls.fragment`)**: При включении `enable_fragment = true` разбивает ClientHello TLS на мелкие фрагменты (`fragment_fallback_delay: "500ms"`), обходя DPI-блокировки SNI.
- **Сниффинг (Sniffing)**: объявляется как отдельное правило в цепочке маршрутизации `route.rules` (`{"action": "sniff"}`).
- **Перехват DNS**: централизованный перехват через правило `{"action": "hijack-dns", "port": [53]}` в `route.rules`.
- **TUN интерфейс**: стек `gvisor` с объединенным пулом адресов `address: ["172.19.0.1/30", "fdfe:dcba:9876::1/126"]`.

### 2. Входящие соединения (Inbounds)
- **SOCKS in**: порт из настроек (по умолчанию `10808`).
- **HTTP in**: порт из настроек (по умолчанию `10809`).
- **TUN in**: интерфейс `vrxx-tun`, стек `gvisor`, `auto_route: true`, `strict_route: true`.

### 3. Исходящие соединения (Outbounds) & Endpoints
- **VLESS / REALITY / Vision**: настройка `flow`, `public_key`, `short_id`, `sni`, uTLS, TLS fragment.
- **VMess**: UUID, `alter_id` (0), transport (`grpc`, `ws`), `packet_encoding: "xudp"`.
- **Shadowsocks**: поддержка протоколов 2022-blake3, AEAD и SIP003 плагинов.
- **WireGuard**: верхнеуровневый массив `"endpoints"` со связкой через direct.
- **SOCKS / HTTP**: исходящие прокси-ноды с авторизацией и TLS.
- **Hysteria2 / TUIC v5**: QUIC транспорты с обфускацией.

## Модуль тестирования задержки (Ping Engine: `src/services/ping.rs`)

Модуль `src/services/ping.rs` отвечает за измерение задержки до VPN-серверов.

### 1. Поддерживаемые алгоритмы пинга (PingAlgorithm)

| Алгоритм | Enum Вариант | Описание и статус по умолчанию |
| --- | --- | --- |
| **HTTP GET via Proxy** | `ViaProxyGet` | **По умолчанию (Рекомендуется)**. Выполнение реального сквозного HTTP GET запроса через изолированный sandbox-туннель. Работает для **100%** поддерживаемых протоколов (включая UDP-only WireGuard/TUIC и HTTP). |
| **HTTP HEAD via Proxy** | `ViaProxyHead` | Быстрый HTTP HEAD запрос через прокси-туннель. |
| **TCP Handshake** | `TcpHandshake` | Измерение времени установления TCP 3-way handshake напрямую с `host:port`. *Ограничение:* может завершаться таймаутом на чисто UDP протоколах (WireGuard/TUIC). |
| **ICMP Ping** | `IcmpPing` | Отправка ICMP Echo пакетов к IP хоста. *Ограничение:* может блокироваться файрволами провайдеров/серверов. |

### 2. Типы результатов (PingResult)

- `PingResult::Success(u128)`: Задержка в миллисекундах (`ms`).
- `PingResult::Timeout`: Таймаут ожидания соединения (3 секунды).
- `PingResult::Error(String)`: Сбой сети или ошибка подключения. Никаких паник не возникает.

- Для одновременной проверки списка серверов используется неблокирующий поток `futures::stream::iter` с `tokio::spawn` и лимитированием `buffer_unordered(concurrency_limit)`.
- В GTK UI результаты передаются через `glib::spawn_future_local` и `async_channel`, предотвращая заморозку интерфейса.


