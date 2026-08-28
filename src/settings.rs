/* settings.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Менеджер настроек приложения (SettingsManager)
//!
//! Отвечает за:
//! - Структуры хранения конфигурации (`AppSettings`, `VpnKeyData`, `RoutingRule`)
//! - Персистентное сохранение на диске в `~/.config/vrxx/settings.json`
//! - Ограничение прав доступа к файлу настроек (`0600`) для защиты ключей и паролей
//! - Асинхронный канал уведомления о необходимости перезапуска ядра (`core_restart_channel`)

use async_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Глобальный асинхронный канал для отправки сигнала перезапуска ядра при изменении критических настроек.
pub fn core_restart_channel() -> (Sender<()>, Receiver<()>) {
    static CHANNEL: OnceLock<(Sender<()>, Receiver<()>)> = OnceLock::new();
    CHANNEL.get_or_init(async_channel::unbounded).clone()
}

/// Пользовательское правило маршрутизации трафика (домен или IP-диапазон).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RoutingRule {
    /// Человекочитаемое имя правила
    pub name: String,
    /// Тип правила (domain, domain_suffix, domain_keyword, ip_cidr, geosite, geoip)
    pub type_: String,
    /// Значение правила (например, "google.com", "192.168.1.0/24", "geosite:youtube")
    pub value: String,
    /// Действие правила ("proxy", "direct", "block")
    pub action: String,
}

/// Структура метаданных сохраненного VPN-профиля / ключа.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct VpnKeyData {
    /// Отображаемое имя профиля
    pub name: String,
    /// Протокол подключения (VLESS, VMess, Shadowsocks, Trojan, Hysteria2, WireGuard, TUIC)
    pub protocol: String,
    /// Флаг активного подключения в текущий момент
    #[serde(default)]
    pub is_active: bool,
    /// Объем входящего трафика (динамическое поле UI, не сериализуется)
    #[serde(skip, default)]
    pub traffic_down: String,
    /// Объем исходящего трафика (динамическое поле UI, не сериализуется)
    #[serde(skip, default)]
    pub traffic_up: String,
    /// Время непрерывного подключения (динамическое поле UI, не сериализуется)
    #[serde(skip, default)]
    pub time_connected: String,
    /// Измеренная задержка/пинг (сохраняется в кеш настроек)
    #[serde(default)]
    pub ping: String,
    /// Географическая локация сервера (страна/город)
    #[serde(default)]
    pub location: String,
    /// Часовой пояс сервера
    #[serde(default)]
    pub timezone: String,
    /// Сырой URI ключ подключения (например, "vless://...")
    #[serde(default)]
    pub url: String,
}

/// Глобальные настройки приложения VRXX.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AppSettings {
    /// Цветовая схема интерфейса ("default", "force-light", "force-dark")
    pub theme: String,
    /// Язык интерфейса ("system", "en", "ru")
    #[serde(default = "default_language")]
    pub language: String,
    /// Выбранный движок маршрутизации
    #[serde(default = "default_core")]
    pub core: String,
    /// Режим TUN (прозрачная системная виртуальная сетевая карта)
    #[serde(default)]
    pub tun_mode: bool,

    // --- Общие параметры приложения ---
    /// Запуск приложения в фоне при старте системы
    #[serde(default = "default_autostart")]
    pub autostart: bool,
    /// Автоматическое подключение к последнему активному профилю при запуске
    #[serde(default = "default_connect_startup")]
    pub connect_on_startup: bool,
    /// Включить всплывающие системные уведомления
    #[serde(default = "default_notifications")]
    pub notifications: bool,
    /// Уровень уведомлений ("all", "important", "none")
    #[serde(default = "default_notification_level")]
    pub notification_level: String,
    /// Режим стримера (скрытие реальных IP и секретных ссылок в GUI)
    #[serde(default = "default_streamer_mode")]
    pub streamer_mode: bool,
    /// Уровень логирования процесса sing-box ("error", "warning", "info", "debug")
    #[serde(default = "default_log_level")]
    pub log_level: String,

    // --- Настройки системного и локального прокси ---
    /// Автоматическая настройка системного прокси GNOME через GSettings
    #[serde(default = "default_system_proxy")]
    pub set_system_proxy: bool,
    /// Локальный порт SOCKS5 прокси
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    /// Локальный порт HTTP прокси
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    /// Разрешить входящие подключения к прокси из локальной сети (LAN)
    #[serde(default)]
    pub allow_lan: bool,

    // --- Расширенные сетевые настройки ---
    /// Сниффинг пакетов (извлечение доменов из SNI/HTTP для маршрутизации)
    #[serde(default = "default_enable_sniffing")]
    pub enable_sniffing: bool,
    /// Блокировка QUIC (UDP 443) для принудительного использования TCP
    #[serde(default = "default_block_quic")]
    pub block_quic: bool,
    /// Стратегия разрешения доменов ("AsIs", "PreferIPv4", "PreferIPv6")
    #[serde(default = "default_domain_strategy")]
    pub domain_strategy: String,
    /// Обход локальной сети (не направлять локальный трафик в VPN)
    #[serde(default = "default_bypass_lan")]
    pub bypass_lan: bool,
    /// Использование Fake DNS (198.18.0.0/15) для ускорения ответа
    #[serde(default = "default_enable_fake_dns")]
    pub enable_fake_dns: bool,
    /// Локальный DNS сервер
    #[serde(default)]
    pub enable_local_dns: bool,
    /// Мультиплексирование TCP-соединений (Mux)
    #[serde(default)]
    pub enable_mux: bool,
    /// Количество параллельных потоков мультиплексирования
    #[serde(default = "default_mux_concurrency")]
    pub mux_concurrency: i32,
    /// Фрагментация TLS-пакетов для обхода DPI
    #[serde(default)]
    pub enable_fragment: bool,

    // --- Параметры замера задержки (Ping) ---
    /// Выбранный алгоритм пинга ("tcp_handshake", "icmp_ping", "http_get", "http_head")
    #[serde(default = "default_ping_algorithm")]
    pub ping_algorithm: String,
    /// Целевой URL для замера HTTP задержки
    #[serde(default = "default_ping_target_url")]
    pub ping_target_url: String,

    /// Список сохраненных профилей VPN (сохраняется в зашифрованном виде в data.dat)
    #[serde(skip, default)]
    pub keys: Vec<VpnKeyData>,
    /// Белый список адресов и доменов
    #[serde(default)]
    pub whitelist: Vec<String>,
    /// Флаг включения кастомной маршрутизации
    #[serde(default = "default_enable_routing")]
    pub enable_routing: bool,
    /// Глобальный режим маршрутизации ("bypass" или "proxy")
    #[serde(default = "default_routing_mode")]
    pub routing_mode: String,

    // --- Предустановленные региональные правила ---
    /// Направлять трафик РФ напрямую (обход, легаси флаг)
    #[serde(default = "default_route_ru")]
    pub route_ru: bool,
    /// Направлять сайты РФ напрямую (GeoSite RU category-ru)
    #[serde(default = "default_route_ru_sites")]
    pub route_ru_sites: bool,
    /// Направлять IP-адреса РФ напрямую (GeoIP RU ru)
    #[serde(default = "default_route_ru_ips")]
    pub route_ru_ips: bool,

    /// Направлять трафик Ирана напрямую (обход, легаси флаг)
    #[serde(default)]
    pub route_ir: bool,
    /// Направлять сайты Ирана напрямую (GeoSite IR category-ir)
    #[serde(default)]
    pub route_ir_sites: bool,
    /// Направлять IP-адреса Ирана напрямую (GeoIP IR ir)
    #[serde(default)]
    pub route_ir_ips: bool,

    /// Направлять трафик Китая напрямую (обход, легаси флаг)
    #[serde(default)]
    pub route_cn: bool,
    /// Направлять сайты Китая напрямую (GeoSite CN cn)
    #[serde(default)]
    pub route_cn_sites: bool,
    /// Направлять IP-адреса Китая напрямую (GeoIP CN geoip-cn)
    #[serde(default)]
    pub route_cn_ips: bool,

    /// Направлять ресурсы из базы Antifilter через VPN
    #[serde(default)]
    pub route_antifilter: bool,
    /// Блокировать рекламные домены (GeoSite category-ads-all)
    #[serde(default)]
    pub block_ads: bool,

    /// Принудительно отключить IPv6
    #[serde(default = "default_disable_ipv6")]
    pub disable_ipv6: bool,
    /// Список пользовательских правил маршрутизации
    #[serde(default)]
    pub routing_rules: Vec<RoutingRule>,
}

// Функции значений по умолчанию для serde
fn default_language() -> String {
    "system".to_string()
}
fn default_core() -> String {
    "sing-box".to_string()
}
fn default_system_proxy() -> bool {
    true
}
fn default_socks_port() -> u16 {
    10808
}
fn default_http_port() -> u16 {
    10809
}
fn default_autostart() -> bool {
    true
}
fn default_connect_startup() -> bool {
    false
}
fn default_notifications() -> bool {
    true
}
fn default_notification_level() -> String {
    "all".to_string()
}
fn default_streamer_mode() -> bool {
    false
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_domain_strategy() -> String {
    "PreferIPv4".to_string()
}
fn default_enable_sniffing() -> bool {
    true
}
fn default_block_quic() -> bool {
    true
}
fn default_bypass_lan() -> bool {
    true
}
fn default_enable_fake_dns() -> bool {
    true
}
fn default_disable_ipv6() -> bool {
    true
}
fn default_enable_routing() -> bool {
    true
}
fn default_routing_mode() -> String {
    "bypass".to_string()
}
fn default_route_ru() -> bool {
    true
}
fn default_route_ru_sites() -> bool {
    true
}
fn default_route_ru_ips() -> bool {
    true
}
fn default_mux_concurrency() -> i32 {
    8
}
fn default_ping_algorithm() -> String {
    "tcp_handshake".to_string()
}
fn default_ping_target_url() -> String {
    "https://www.gstatic.com/generate_204".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl AppSettings {
    /// Создает новый экземпляр настроек с параметрами по умолчанию.
    pub fn new() -> Self {
        Self {
            theme: "default".to_string(),
            language: default_language(),
            core: default_core(),
            tun_mode: false,
            autostart: default_autostart(),
            connect_on_startup: default_connect_startup(),
            notifications: default_notifications(),
            notification_level: default_notification_level(),
            streamer_mode: default_streamer_mode(),
            log_level: default_log_level(),
            set_system_proxy: default_system_proxy(),
            socks_port: default_socks_port(),
            http_port: default_http_port(),
            allow_lan: false,
            enable_sniffing: default_enable_sniffing(),
            block_quic: default_block_quic(),
            domain_strategy: default_domain_strategy(),
            bypass_lan: default_bypass_lan(),
            enable_fake_dns: default_enable_fake_dns(),
            enable_local_dns: false,
            enable_mux: false,
            mux_concurrency: default_mux_concurrency(),
            enable_fragment: false,
            ping_algorithm: default_ping_algorithm(),
            ping_target_url: default_ping_target_url(),
            keys: vec![],
            whitelist: vec![],
            enable_routing: default_enable_routing(),
            routing_mode: default_routing_mode(),
            routing_rules: vec![],
            route_ru: default_route_ru(),
            route_ru_sites: default_route_ru_sites(),
            route_ru_ips: default_route_ru_ips(),
            route_ir: false,
            route_ir_sites: false,
            route_ir_ips: false,
            route_cn: false,
            route_cn_sites: false,
            route_cn_ips: false,
            route_antifilter: false,
            block_ads: false,
            disable_ipv6: default_disable_ipv6(),
        }
    }
}

/// Управляющий объект для чтения и записи настроек и защищенного хранилища ключей на диск.
pub struct SettingsManager {
    config_path: PathBuf,
    data_path: PathBuf,
}

impl Default for SettingsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsManager {
    /// Создает экземпляр менеджера и инициализирует пути `~/.config/vrxx/settings.json` и `~/.config/vrxx/data.dat`.
    pub fn new() -> Self {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("vrxx");
        fs::create_dir_all(&path).ok();
        let config_path = path.join("settings.json");
        let data_path = path.join("data.dat");
        Self {
            config_path,
            data_path,
        }
    }

    /// Создает экземпляр менеджера с заданными путями (для изолированного модульного тестирования).
    pub fn with_paths(config_path: PathBuf, data_path: PathBuf) -> Self {
        Self {
            config_path,
            data_path,
        }
    }

    /// Загружает настройки из JSON файла на диске и профили из зашифрованного `data.dat`.
    pub fn load(&self) -> AppSettings {
        // 1. Проверка на необходимость миграции старого формата settings.json в зашифрованный data.dat
        self.try_migrate_legacy_keys();

        // 2. Попытка прочитать и распарсить открытый файл настроек
        let mut settings = if let Ok(content) = fs::read_to_string(&self.config_path) {
            if let Ok(mut parsed) = serde_json::from_str::<AppSettings>(&content) {
                self.migrate_legacy_rules(&mut parsed);
                parsed
            } else {
                tracing::warn!(
                    "Файл настроек {:?} поврежден или пуст. Попытка восстановления из резервной копии...",
                    self.config_path
                );
                self.load_settings_backup()
            }
        } else {
            self.load_settings_backup()
        };

        // 3. Загрузка профилей из зашифрованного data.dat
        settings.keys = self.load_keys();

        settings
    }

    /// Загружает настройки из резервной копии `.bak`.
    fn load_settings_backup(&self) -> AppSettings {
        let bak_path = self.config_path.with_extension("json.bak");
        if bak_path.exists() {
            if let Ok(bak_content) = fs::read_to_string(&bak_path) {
                if let Ok(mut settings) = serde_json::from_str::<AppSettings>(&bak_content) {
                    tracing::info!("Настройки успешно восстановлены из {:?}", bak_path);
                    self.migrate_legacy_rules(&mut settings);
                    let _ = fs::copy(&bak_path, &self.config_path);
                    return settings;
                }
            }
        }
        AppSettings::new()
    }

    /// Проверяет наличие открытых ключей в старом файле `settings.json` и переносит их в `data.dat`.
    fn try_migrate_legacy_keys(&self) {
        if self.data_path.exists() || !self.config_path.exists() {
            return;
        }

        if let Ok(content) = fs::read_to_string(&self.config_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(keys_val) = val.get("keys") {
                    if let Ok(legacy_keys) =
                        serde_json::from_value::<Vec<VpnKeyData>>(keys_val.clone())
                    {
                        if !legacy_keys.is_empty() {
                            tracing::info!(
                                "Обнаружено {} ключей в открытом settings.json. Выполняется автоматическая миграция в зашифрованный data.dat...",
                                legacy_keys.len()
                            );
                            self.save_keys(&legacy_keys);

                            // Перезаписываем settings.json в чистом виде без открытых ключей
                            if let Ok(settings) = serde_json::from_value::<AppSettings>(val.clone())
                            {
                                self.save_settings_only(&settings);
                            }
                        }
                    }
                }
            }
        }
    }

    fn migrate_legacy_rules(&self, settings: &mut AppSettings) {
        if settings.core == "xray" || settings.core.is_empty() {
            settings.core = "sing-box".to_string();
        }
        if settings.route_ru && !settings.route_ru_sites && !settings.route_ru_ips {
            settings.route_ru_sites = true;
            settings.route_ru_ips = true;
        }
        if settings.route_cn && !settings.route_cn_sites && !settings.route_cn_ips {
            settings.route_cn_sites = true;
            settings.route_cn_ips = true;
        }
        if settings.route_ir && !settings.route_ir_sites && !settings.route_ir_ips {
            settings.route_ir_sites = true;
            settings.route_ir_ips = true;
        }
    }

    /// Синхронно и атомарно сохраняет только настройки приложения в `settings.json` (права 0600).
    pub fn save_settings_only(&self, settings: &AppSettings) {
        if let Ok(content) = serde_json::to_string_pretty(settings) {
            let tmp_path = self.config_path.with_extension("json.tmp");
            let bak_path = self.config_path.with_extension("json.bak");

            let write_res = (|| -> std::io::Result<()> {
                #[cfg(unix)]
                {
                    use std::io::Write;
                    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
                    let mut opts = std::fs::OpenOptions::new();
                    opts.create(true).write(true).truncate(true).mode(0o600);

                    let mut file = opts.open(&tmp_path)?;
                    let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
                    file.write_all(content.as_bytes())?;
                    file.sync_all()?;
                }
                #[cfg(not(unix))]
                {
                    std::fs::write(&tmp_path, content.as_bytes())?;
                }

                std::fs::rename(&tmp_path, &self.config_path)?;
                let _ = std::fs::copy(&self.config_path, &bak_path);
                Ok(())
            })();

            if let Err(e) = write_res {
                tracing::error!(
                    "Ошибка атомарного сохранения настроек в {:?}: {}",
                    self.config_path,
                    e
                );
            }
        }
    }

    /// Синхронно и атомарно сохраняет настройки в `settings.json` и профили в `data.dat`.
    pub fn save(&self, settings: &AppSettings) {
        self.save_settings_only(settings);
        self.save_keys(&settings.keys);
    }

    /// Загружает и расшифровывает список VPN-профилей из `data.dat` (с fallback на `data.dat.bak`).
    pub fn load_keys(&self) -> Vec<VpnKeyData> {
        // 1. Попытка чтения и дешифрования основного файла data.dat
        if self.data_path.exists() {
            match fs::read(&self.data_path) {
                Ok(bytes) => match crate::crypto::keystore::decrypt_keys(&bytes) {
                    Ok(keys) => return keys,
                    Err(e) => {
                        tracing::warn!(
                            "Ошибка дешифрования {:?}: {e}. Попытка восстановления из резервной копии .bak...",
                            self.data_path
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!("Ошибка чтения файла {:?}: {e}", self.data_path);
                }
            }
        }

        // 2. Попытка восстановления из резервной копии data.dat.bak
        let bak_path = self.data_path.with_extension("dat.bak");
        if bak_path.exists() {
            if let Ok(bak_bytes) = fs::read(&bak_path) {
                if let Ok(keys) = crate::crypto::keystore::decrypt_keys(&bak_bytes) {
                    tracing::info!(
                        "Зашифрованные ключи успешно восстановлены из резервной копии {:?}",
                        bak_path
                    );
                    let _ = fs::copy(&bak_path, &self.data_path);
                    return keys;
                }
            }
        }

        Vec::new()
    }

    /// Атомарно сохраняет список VPN-профилей в зашифрованный файл `data.dat` с правами 0600.
    pub fn save_keys(&self, keys: &[VpnKeyData]) {
        let encrypted_container = match crate::crypto::keystore::encrypt_keys(keys) {
            Ok(data) => data,
            Err(e) => {
                tracing::error!("Критическая ошибка шифрования профилей VPN: {e}");
                return;
            }
        };

        let tmp_path = self.data_path.with_extension("dat.tmp");
        let bak_path = self.data_path.with_extension("dat.bak");

        let write_res = (|| -> std::io::Result<()> {
            #[cfg(unix)]
            {
                use std::io::Write;
                use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
                let mut opts = std::fs::OpenOptions::new();
                opts.create(true).write(true).truncate(true).mode(0o600);

                let mut file = opts.open(&tmp_path)?;
                let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
                file.write_all(&encrypted_container)?;
                file.sync_all()?;
            }
            #[cfg(not(unix))]
            {
                std::fs::write(&tmp_path, &encrypted_container)?;
            }

            std::fs::rename(&tmp_path, &self.data_path)?;
            let _ = std::fs::copy(&self.data_path, &bak_path);
            Ok(())
        })();

        if let Err(e) = write_res {
            tracing::error!(
                "Ошибка атомарного сохранения зашифрованных ключей в {:?}: {}",
                self.data_path,
                e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_manager_atomic_save_and_backup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("settings.json");
        let data_path = temp_dir.path().join("data.dat");
        let manager = SettingsManager::with_paths(config_path.clone(), data_path.clone());

        let mut settings = AppSettings::new();
        settings.enable_fake_dns = true;
        settings.keys.push(VpnKeyData {
            name: "Test Server".to_string(),
            protocol: "VLESS".to_string(),
            is_active: false,
            traffic_down: "0 MB".to_string(),
            traffic_up: "0 MB".to_string(),
            time_connected: "00:00:00".to_string(),
            ping: "50 ms".to_string(),
            location: "DE".to_string(),
            timezone: "UTC+1".to_string(),
            url: "vless://test@1.1.1.1:443#Test".to_string(),
        });

        manager.save(&settings);

        // Проверяем, что созданы оба файла: открытый settings.json и зашифрованный data.dat
        assert!(config_path.exists(), "settings.json должен существовать");
        assert!(data_path.exists(), "data.dat должен существовать");

        // Проверяем, что в settings.json нет открытых ссылок vless://
        let json_content = fs::read_to_string(&config_path).unwrap();
        assert!(
            !json_content.contains("vless://"),
            "settings.json не должен содержать открытые ключи"
        );

        // Проверяем, что data.dat является зашифрованным бинарным файлом
        let dat_content = fs::read(&data_path).unwrap();
        assert_eq!(&dat_content[0..8], crate::crypto::keystore::MAGIC_HEADER);

        // Проверяем корректность загрузки
        let loaded = manager.load();
        assert!(loaded.enable_fake_dns);
        assert_eq!(loaded.keys.len(), 1);
        assert_eq!(loaded.keys[0].name, "Test Server");
        assert_eq!(loaded.keys[0].ping, "50 ms");
        assert_eq!(loaded.keys[0].url, "vless://test@1.1.1.1:443#Test");

        // Проверяем обновление и создание бэкапа data.dat.bak
        let mut updated = loaded.clone();
        updated.keys.push(VpnKeyData {
            name: "Second Server".to_string(),
            protocol: "Trojan".to_string(),
            is_active: false,
            traffic_down: "0 MB".to_string(),
            traffic_up: "0 MB".to_string(),
            time_connected: "00:00:00".to_string(),
            ping: "30 ms".to_string(),
            location: "NL".to_string(),
            timezone: "UTC+1".to_string(),
            url: "trojan://test@1.1.1.1:443#Trojan".to_string(),
        });
        manager.save(&updated);

        let bak_path = data_path.with_extension("dat.bak");
        assert!(
            bak_path.exists(),
            "Резервная копия data.dat.bak должна быть создана"
        );

        // Симулируем повреждение основного data.dat -> должен восстановиться из .bak
        std::fs::write(&data_path, b"corrupted data").unwrap();
        let recovered = manager.load();
        assert_eq!(
            recovered.keys.len(),
            2,
            "Должны восстановиться 2 ключа из data.dat.bak"
        );
    }

    #[test]
    fn test_settings_manager_legacy_migration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("settings.json");
        let data_path = temp_dir.path().join("data.dat");
        let manager = SettingsManager::with_paths(config_path.clone(), data_path.clone());

        // Создаем старый settings.json с открытым массивом keys
        let legacy_json = r#"{
            "theme": "force-dark",
            "language": "ru",
            "keys": [
                {
                    "name": "Legacy Server",
                    "protocol": "VLESS",
                    "url": "vless://legacy-uuid@1.1.1.1:443#Legacy",
                    "location": "FI",
                    "timezone": "UTC+2",
                    "ping": "25 ms"
                }
            ]
        }"#;
        fs::write(&config_path, legacy_json).unwrap();

        // Загрузка должна обнаружить старые ключи, зашифровать их в data.dat и очистить settings.json
        let loaded = manager.load();
        assert_eq!(loaded.theme, "force-dark");
        assert_eq!(loaded.language, "ru");
        assert_eq!(loaded.keys.len(), 1);
        assert_eq!(loaded.keys[0].name, "Legacy Server");
        assert_eq!(loaded.keys[0].url, "vless://legacy-uuid@1.1.1.1:443#Legacy");

        // Проверяем, что data.dat теперь существует
        assert!(
            data_path.exists(),
            "data.dat должен быть создан после миграции"
        );

        // Проверяем, что исходный settings.json очищен от открытых ключей
        let cleaned_json = fs::read_to_string(&config_path).unwrap();
        assert!(
            !cleaned_json.contains("vless://"),
            "settings.json больше не должен содержать открытых ключей"
        );
    }
}
