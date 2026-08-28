/* models.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Реактивные GObject модели данных (Data Models)
//!
//! Содержит обертки GObject для списков GTK (`gio::ListStore`):
//! - [`VpnKeyObject`]: Модель профиля VPN с реактивными свойствами (трафик, пинг, статус)
//! - [`DomainObject`]: Модель доменного имени для белых списков
//! - [`RoutingRuleObject`]: Модель правила маршрутизации трафика

use adw::subclass::prelude::*;
use gtk::{glib, prelude::*};

// =============================================================================
// 1. МОДЕЛЬ VPN-ПРОФИЛЯ (VPN KEY OBJECT)
// =============================================================================
mod imp_vpn {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::VpnKeyObject)]
    pub struct VpnKeyObject {
        /// Отображаемое имя ключа
        #[property(get, set)]
        pub name: RefCell<String>,
        /// Протокол подключения (VLESS, VMess, Trojan, ShadowSocks, Hysteria2, WireGuard, TUIC)
        #[property(get, set)]
        pub protocol: RefCell<String>,
        /// Флаг активного подключения
        #[property(get, set)]
        pub is_active: RefCell<bool>,
        /// Флаг процесса подключения (крутящийся спиннер)
        #[property(get, set)]
        pub is_loading: RefCell<bool>,
        /// Флаг ошибки подключения (щит с предупреждением)
        #[property(get, set)]
        pub is_error: RefCell<bool>,
        /// Объем входящего трафика
        #[property(get, set)]
        pub traffic_down: RefCell<String>,
        /// Объем исходящего трафика
        #[property(get, set)]
        pub traffic_up: RefCell<String>,
        /// Время активности сессии (ЧЧ:ММ:СС)
        #[property(get, set)]
        pub time_connected: RefCell<String>,
        /// Измеренная задержка (пинг)
        #[property(get, set)]
        pub ping: RefCell<String>,
        /// Хост/IP сервера для отображения
        #[property(get, set)]
        pub server_info: RefCell<String>,
        /// Географическая локация (страна/город)
        #[property(get, set)]
        pub location: RefCell<String>,
        /// Часовой пояс сервера
        #[property(get, set)]
        pub timezone: RefCell<String>,
        /// Скрыть IP-адрес в режиме стримера
        #[property(get, set)]
        pub hide_ip: RefCell<bool>,
        /// Сырой URI ключ подключения
        #[property(get, set)]
        pub url: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VpnKeyObject {
        const NAME: &'static str = "VpnKeyObject";
        type Type = super::VpnKeyObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for VpnKeyObject {}
}

// =============================================================================
// 2. МОДЕЛЬ ДОМЕНА (DOMAIN OBJECT)
// =============================================================================
mod imp_domain {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::DomainObject)]
    pub struct DomainObject {
        #[property(get, set)]
        pub domain: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DomainObject {
        const NAME: &'static str = "DomainObject";
        type Type = super::DomainObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for DomainObject {}
}

// =============================================================================
// 3. МОДЕЛЬ ПРАВИЛА МАРШРУТИЗАЦИИ (ROUTING RULE OBJECT)
// =============================================================================
mod imp_routing {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::RoutingRuleObject)]
    pub struct RoutingRuleObject {
        #[property(get, set)]
        pub name: RefCell<String>,
        #[property(get, set)]
        pub rule_type: RefCell<String>,
        #[property(get, set)]
        pub value: RefCell<String>,
        #[property(get, set)]
        pub action: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoutingRuleObject {
        const NAME: &'static str = "RoutingRuleObject";
        type Type = super::RoutingRuleObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for RoutingRuleObject {}
}

// =============================================================================
// ОБЕРТКИ GLIB (GOBJECT WRAPPERS)
// =============================================================================

glib::wrapper! {
    /// GObject представление VPN-ключа для использования в моделях списков GTK
    pub struct VpnKeyObject(ObjectSubclass<imp_vpn::VpnKeyObject>);
}

impl VpnKeyObject {
    /// Создает новый объект VPN-ключа со значениями по умолчанию.
    pub fn new(name: &str, protocol: &str, active: bool, url: &str) -> Self {
        let server_info = if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(url) {
            if !parsed.host.is_empty() {
                parsed.host
            } else {
                "0.0.0.0".to_string()
            }
        } else {
            "0.0.0.0".to_string()
        };

        glib::Object::builder()
            .property("name", name)
            .property("protocol", protocol)
            .property("is-active", active)
            .property("is-loading", false)
            .property("is-error", false)
            .property("traffic-down", "0.0 MB")
            .property("traffic-up", "0.0 MB")
            .property("time-connected", "00:00:00")
            .property("ping", "0 ms")
            .property("server-info", server_info)
            .property("location", "Unknown")
            .property("timezone", "UTC")
            .property("hide-ip", false)
            .property("url", url)
            .build()
    }
}

glib::wrapper! {
    /// GObject представление доменного имени для списков
    pub struct DomainObject(ObjectSubclass<imp_domain::DomainObject>);
}

glib::wrapper! {
    /// GObject представление правила маршрутизации
    pub struct RoutingRuleObject(ObjectSubclass<imp_routing::RoutingRuleObject>);
}

impl DomainObject {
    /// Создает новый объект домена.
    pub fn new(domain: &str) -> Self {
        glib::Object::builder().property("domain", domain).build()
    }
}

impl RoutingRuleObject {
    /// Создает новый объект правила маршрутизации.
    pub fn new(name: &str, rule_type: &str, value: &str, action: &str) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("rule-type", rule_type)
            .property("value", value)
            .property("action", action)
            .build()
    }
}
