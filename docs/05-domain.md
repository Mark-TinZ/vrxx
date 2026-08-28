# 05. Протоколы и конфигурация ядра

## Обзор

Слой Domain (`src/domain/`) отвечает за обработку VPN-данных независимо от того, кто их вызвал (графический интерфейс или демон). Его основные задачи:
1. Парсинг пользовательских строк/URI в стандартизированную структуру.
2. Трансляция этой структуры + настроек приложения в сложный JSON-формат конфигурации ядра Sing-box с динамической версионной адаптацией (от 1.8 до 1.13+).

## Поддерживаемые протоколы

| Протокол | Формат ввода (URI Scheme) | Парсер (key_parser) | Генератор Sing-box | Особенности реализации |
| --- | --- | --- | --- | --- |
| **VLESS** | `vless://uuid@host:port?params...` | ✅ Полный | ✅ Полный | REALITY (`pbk`, `sid`), XTLS-Vision (`flow`), uTLS (`fp`) |
| **VMess** | `vmess://[Base64 JSON]` | ✅ Полный | ✅ Полный | gRPC, WebSocket (`ws`), TCP, AlterId, `xudp` |
| **Trojan** | `trojan://pass@host:port?params...` | ✅ Полный | ✅ Полный | TLS, uTLS отпечаток |
| **Shadowsocks**| `ss://[Base64 URL]@host:port` | ✅ Полный | ✅ Полный | 2022-blake3-aes-128/256-gcm, AEAD (chacha20, aes-gcm), `xudp` |
| **Hysteria2** | `hy2://pass@host:port?params...` | ✅ Полный | ✅ Полный | `up_mbps`, `down_mbps`, obfs (salamander), uTLS, TLS |
| **TUIC v5** | `tuic://uuid:pass@host:port?params...` | ✅ Полный | ✅ Полный | `congestion_control` (bbr), `udp_relay_mode` (native/quic) |
| **WireGuard** | `wg://privkey@host:port?params...` | ✅ Полный | ✅ Полный | 1.13+ через верхнеуровневый массив `endpoints` / классический outbound |

> **Примечание:** Архитектура позволяет легко добавлять новые протоколы. Достаточно реализовать вариативный парсинг в `key_parser.rs` и сформировать соответствующий исходящий блок в `singbox_config.rs`.

## Парсинг ключей (key_parser.rs)

Любой валидный VPN-ключ парсится в универсальную структуру `ParsedKey`:

```rust
pub struct ParsedKey {
    pub protocol: String,      // "VLESS", "VMess", "Trojan", "Shadowsocks", "Hysteria2", "TUIC", "WireGuard"
    pub name: String,          // Имя ключа (обычно берется из фрагмента #name)
    pub host: String,          // IP-адрес или домен сервера
    pub port: u16,
    pub uuid: String,          // Пароль, UUID, приватный ключ или токен
    pub query_params: HashMap<String, String>, // Все параметры (security, type, sni, pbk, obfs, cc)
    pub raw_url: String,       // Исходная строка
}
```

**Особенности парсинга:**
- `VMess` отбрасывает префикс `vmess://`, декодирует Base64 и парсит полученный JSON-объект.
- `Shadowsocks` поддерживает SIP002 Base64 и plain text userinfo (`method:password`), автоматически определяя методы шифрования 2022.
- `VLESS`, `Trojan`, `Hysteria2`, `TUIC`, `WireGuard` используют парсинг URL-схем с декодированием фрагментов `#name` и вычленением всех Query-параметров.
- Функция `build_vpn_key(parsed: &ParsedKey)` выполняет обратную сериализацию структуры в стандартную URI-строку (для экспорта).

## Генерация и экспорт QR-кодов (exporter.rs)

Модуль `src/domain/exporter.rs` предоставляет универсальный API генерации визуальных QR-кодов:
- `generate_qr_svg(content: &str) -> Result<String>`: Формирует строку векторного формата SVG.
- `generate_qr_png_bytes(content: &str, width: u32, height: u32) -> Result<Vec<u8>>`: Рендерит растровое изображение формата PNG в байтовый буфер `Vec<u8>`.
- `generate_qr_texture(content: &str, size: u32) -> Result<gdk::Texture>`: Безопасно формирует объект `gdk::Texture` напрямую из оперативки в памяти без создания временных файлов на диске.

## Генерация конфигурации Sing-box (singbox_config.rs)


VRXX динамически генерирует полную JSON-конфигурацию с помощью `build_singbox_config(&ParsedKey, &AppSettings)` и `build_singbox_config_with_version(...)`.

### 1. Динамическая версионная адаптивность (1.8 – 1.13+)

Функция `get_singbox_version()` определяет версию ядра (`sing-box version`). Код генератора адаптирует структуры JSON под особенности каждой версии:

| Версия ядра | Inbounds TUN | WireGuard | DNS servers & rules | Route rules |
| --- | --- | --- | --- | --- |
| **sing-box >= 1.13** | `address: ["172.19.0.1/30", "fdfe:dcba:9876::1/126"]` | Верхнеуровневый массив `"endpoints"` | `type: "https"` / `type: "local"`, IPv6 `action: "reject"` | `default_domain_resolver`, `{"action": "hijack-dns"}` |
| **sing-box == 1.12** | `address: [...]` | Классический outbound | `type: "https"` / `type: "local"`, `action: "reject"` | `default_domain_resolver: "local-dns"` |
| **sing-box < 1.12** | `inet4_address`, `inet6_address` | Классический outbound | `address: "https://..."` / `address: "local"` | `sniff` в inbounds, `outbound: "dns-out"` |

### 2. Входящие соединения (Inbounds)
- **SOCKS in**: порт из настроек (по умолчанию `10808`).
- **HTTP in**: порт из настроек (по умолчанию `10809`).
- **TUN in**: интерфейс `vrxx-tun`, стек `gvisor`, `auto_route: true`, `strict_route: true`.

### 3. Исходящие соединения (Outbounds) & Endpoints
- **VLESS / REALITY / Vision**: настройка `flow`, `public_key`, `short_id`, `sni`, uTLS.
- **VMess**: UUID, `alter_id` (0), transport (`grpc`, `ws`), `packet_encoding: "xudp"`.
- **Shadowsocks**: поддержка протоколов 2022-blake3 и AEAD.
- **Hysteria2**: лимиты пропускной способности `up_mbps` / `down_mbps`, `obfs` (`salamander`), uTLS.
- **TUIC v5**: `congestion_control` (BBR), `udp_relay_mode` (native).
- **WireGuard**: верхнеуровневый массив `"endpoints"` для sing-box 1.13+ с детуром через direct, либо прямое исходящее соединение `wireguard` для старых ядер.

### 4. Настройка DNS и Маршрутизация
- **DNS**: `remote-dns` (DoH 1.1.1.1 через proxy) и `local-dns` (local через direct).
- **Сниффинг трафика**: правило `action: "sniff"` в `route.rules` для ядер >= 1.11.
- **Перехват DNS**: правило `action: "hijack-dns"` для ядер >= 1.13.
- **Обход LAN**: автоматический перевод локальных подсетей в `direct`.
- **Удаленные наборы правил (Remote SRS Rule Sets)**: MetaCubeX geosite & geoip SRS для блокировки рекламы и регионального роутинга (RU, CN, IR, Antifilter).

Сгенерированный JSON передается демоном в `stdin` процесса ядра, не оставляя следов на диске.

## Модуль тестирования задержки (Ping Engine: `src/services/ping.rs`)

Модуль `src/services/ping.rs` отвечает за измерение задержки до VPN-серверов. Поддерживаются 4 алгоритма тестирования, тип `PingResult`, настраиваемый URL проверки и параллельное неблокирующее исполнение.

### 1. Поддерживаемые алгоритмы пинга (PingAlgorithm)

| Алгоритм | Enum Вариант | Описание и особенности |
| --- | --- | --- |
| **TCP Handshake** | `TcpHandshake` | Измерение времени установления TCP 3-way handshake напрямую с `host:port` сервера. Алгоритм по умолчанию. |
| **ICMP Ping** | `IcmpPing` | Отправка ICMP Echo Request пакетов к IP-адресу или хосту сервера с вызовом системной утилиты `ping`. |
| **HTTP GET via Proxy** | `ViaProxyGet` | Выполнение полных HTTP GET запросов к целевому URL через SOCKS5/HTTP прокси (проверка прохождения данных и HTTP-стека). |
| **HTTP HEAD via Proxy** | `ViaProxyHead` | Выполнение быстрых HTTP HEAD запросов к целевому URL через прокси. |

### 2. Типы результатов (PingResult)

- `PingResult::Success(u128)`: Задержка в миллисекундах (`ms`).
- `PingResult::Timeout`: Таймаут ожидания соединения (по умолчанию 3 секунды).
- `PingResult::Error(String)`: Сбой сети или ошибка подключения. Никаких паник не возникает.

### 3. Параллельное исполнение и UI-интеграция

- Для одновременной проверки списка серверов используется неблокирующий поток `futures::stream::iter` с `tokio::spawn` и лимитированием `buffer_unordered(concurrency_limit)`.
- В GTK UI результаты передаются через `glib::spawn_future_local` и `async_channel`, предотвращая заморозку интерфейса.


