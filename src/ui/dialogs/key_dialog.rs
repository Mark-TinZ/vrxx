/* key_dialog.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Диалоговое окно создания и редактирования VPN-профилей (VrxxKeyDialog)
//!
//! Отвечает за:
//! - Декларативное отображение параметров VPN-профиля по стандартам Libadwaita 1.5+ (`AdwDialog`)
//! - Адаптивную компоновку с точками останова (`AdwBreakpoint`) для поддержки любых размеров родительского окна (от 360px)
//! - Четкое центрированное отображение заголовка через `AdwWindowTitle`
//! - Автоматический мгновенный парсинг вставляемых ссылок (`vless://`, `vmess://`, `ss://`, `trojan://` и др.)
//! - Асинхронный неблокирующий TCP-замер задержки (Ping) до целевого сервера
//! - Поддержку режимов создания нового профиля (`Add`) и редактирования существующего (`Edit`)

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::domain::key_parser::{build_vpn_key, parse_vpn_key, ParsedKey};

/// Режим работы диалогового окна профиля
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyDialogMode {
    /// Режим добавления нового профиля с возможностью быстрого импорта по ссылке
    #[default]
    Add,
    /// Режим редактирования существующего сохраненного профиля
    Edit,
}

mod imp {
    use super::*;
    use std::cell::Cell;

    /// Составной шаблон виджета диалога VPN-ключа
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/dialogs/key_dialog.ui")]
    pub struct VrxxKeyDialog {
        #[template_child]
        pub dialog_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub btn_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_save: TemplateChild<gtk::Button>,

        #[template_child]
        pub group_quick: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub entry_link: TemplateChild<adw::EntryRow>,

        #[template_child]
        pub group_general: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub entry_name: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub combo_proto: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub switch_connect: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub group_conn: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub entry_host: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub entry_port: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub entry_uuid: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub row_latency: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub spinner_latency: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub btn_ping: TemplateChild<gtk::Button>,

        #[template_child]
        pub group_sec: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub entry_sec: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub entry_sni: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub entry_pbk: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub entry_fp: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub entry_flow: TemplateChild<adw::EntryRow>,

        pub mode: Cell<KeyDialogMode>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxKeyDialog {
        const NAME: &'static str = "VrxxKeyDialog";
        type Type = super::VrxxKeyDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            adw::ToolbarView::static_type();
            adw::WindowTitle::static_type();
            adw::Breakpoint::static_type();
            adw::PreferencesPage::static_type();
            adw::PreferencesGroup::static_type();
            adw::EntryRow::static_type();
            adw::ComboRow::static_type();
            adw::SwitchRow::static_type();
            adw::ActionRow::static_type();
            gtk::Spinner::static_type();
            gtk::Button::static_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxKeyDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_width_request(360);
            obj.set_height_request(360);
            obj.setup_signals();
        }
    }

    impl WidgetImpl for VrxxKeyDialog {}
    impl adw::subclass::dialog::AdwDialogImpl for VrxxKeyDialog {}
}

glib::wrapper! {
    /// Обертка GObject для расширенного диалога настройки и добавления VPN-профилей
    pub struct VrxxKeyDialog(ObjectSubclass<imp::VrxxKeyDialog>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxKeyDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxKeyDialog {
    /// Создает новый экземпляр диалога с настройками по умолчанию.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Устанавливает режим работы диалогового окна (`Add` или `Edit`).
    pub fn set_mode(&self, mode: KeyDialogMode) {
        let imp = self.imp();
        imp.mode.set(mode);

        match mode {
            KeyDialogMode::Add => {
                let title = gettext("Add VPN Profile");
                self.set_title(&title);
                imp.dialog_title.set_title(&title);
                imp.btn_save.set_label(&gettext("Add"));
                imp.group_quick.set_visible(true);
                imp.switch_connect.set_visible(true);
            }
            KeyDialogMode::Edit => {
                let title = gettext("Edit VPN key");
                self.set_title(&title);
                imp.dialog_title.set_title(&title);
                imp.btn_save.set_label(&gettext("Save"));
                imp.group_quick.set_visible(false);
                imp.switch_connect.set_visible(false);
            }
        }
    }

    /// Настройка внутренних сигналов виджетов (кнопка закрытия, автопарсинг, пинг).
    fn setup_signals(&self) {
        let imp = self.imp();

        // 1. Кнопка «Отмена» — закрывает диалог
        let dialog_weak = self.downgrade();
        imp.btn_cancel.connect_clicked(move |_| {
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
        });

        // 2. Реактивное автозаполнение полей при вводе или вставке ссылки в строку быстрого импорта
        let dialog_weak_link = self.downgrade();
        imp.entry_link.connect_changed(move |entry| {
            let text = entry.text().trim().to_string();
            if text.is_empty() {
                return;
            }
            if let Ok(parsed) = parse_vpn_key(&text) {
                if let Some(dialog) = dialog_weak_link.upgrade() {
                    dialog.populate_from_parsed(&parsed);
                }
            }
        });

        // 3. Обработчик неблокирующего замера задержки (TCP ping)
        let dialog_weak_ping = self.downgrade();
        imp.btn_ping.connect_clicked(move |_| {
            if let Some(dialog) = dialog_weak_ping.upgrade() {
                dialog.perform_ping_test();
            }
        });

        // 4. Динамическая адаптация полей формы под выбранный протокол
        let dialog_weak_proto = self.downgrade();
        imp.combo_proto.connect_selected_notify(move |_| {
            if let Some(dialog) = dialog_weak_proto.upgrade() {
                dialog.update_protocol_fields();
            }
        });

        // 5. Динамическая адаптация при смене режима безопасности
        let dialog_weak_sec = self.downgrade();
        imp.entry_sec.connect_changed(move |_| {
            if let Some(dialog) = dialog_weak_sec.upgrade() {
                dialog.update_protocol_fields();
            }
        });

        self.update_protocol_fields();
    }

    /// Динамически настраивает видимость и подсказки полей в зависимости от выбранного протокола и режима шифрования.
    pub fn update_protocol_fields(&self) {
        let imp = self.imp();
        let selected = imp.combo_proto.selected();
        let sec = imp.entry_sec.text().trim().to_lowercase();

        match selected {
            // VLESS
            0 => {
                imp.entry_uuid.set_title(&gettext("UUID"));
                imp.group_sec.set_visible(true);
                imp.entry_sec.set_visible(true);
                imp.entry_sni.set_visible(sec != "none");
                let is_reality = sec == "reality";
                imp.entry_pbk.set_visible(is_reality);
                imp.entry_pbk
                    .set_title(&gettext("Reality Public Key (pbk)"));
                imp.entry_fp.set_visible(sec != "none");
                imp.entry_flow.set_visible(is_reality);
            }
            // VMess
            1 => {
                imp.entry_uuid.set_title(&gettext("UUID"));
                imp.group_sec.set_visible(true);
                imp.entry_sec.set_visible(true);
                imp.entry_sni.set_visible(sec != "none");
                imp.entry_pbk.set_visible(false);
                imp.entry_fp.set_visible(sec != "none");
                imp.entry_flow.set_visible(false);
            }
            // Trojan
            2 => {
                imp.entry_uuid.set_title(&gettext("Password"));
                imp.group_sec.set_visible(true);
                imp.entry_sec.set_visible(false);
                imp.entry_sni.set_visible(true);
                imp.entry_pbk.set_visible(false);
                imp.entry_fp.set_visible(false);
                imp.entry_flow.set_visible(false);
            }
            // Shadowsocks
            3 => {
                imp.entry_uuid.set_title(&gettext("Password / Method"));
                imp.group_sec.set_visible(false);
            }
            // Hysteria2
            4 => {
                imp.entry_uuid.set_title(&gettext("Auth / Password"));
                imp.group_sec.set_visible(true);
                imp.entry_sec.set_visible(false);
                imp.entry_sni.set_visible(true);
                imp.entry_pbk.set_visible(false);
                imp.entry_fp.set_visible(false);
                imp.entry_flow.set_visible(false);
            }
            // TUIC
            5 => {
                imp.entry_uuid.set_title(&gettext("Password / Token"));
                imp.group_sec.set_visible(true);
                imp.entry_sec.set_visible(false);
                imp.entry_sni.set_visible(true);
                imp.entry_pbk.set_visible(false);
                imp.entry_fp.set_visible(false);
                imp.entry_flow.set_visible(false);
            }
            // WireGuard
            6 => {
                imp.entry_uuid.set_title(&gettext("Private Key"));
                imp.group_sec.set_visible(true);
                imp.entry_sec.set_visible(false);
                imp.entry_sni.set_visible(false);
                imp.entry_pbk.set_visible(true);
                imp.entry_pbk.set_title(&gettext("Public Key (Peer)"));
                imp.entry_fp.set_visible(false);
                imp.entry_flow.set_visible(false);
            }
            // SOCKS5
            7 => {
                imp.entry_uuid
                    .set_title(&gettext("Password / Credentials (optional)"));
                imp.group_sec.set_visible(false);
            }
            // HTTP
            8 => {
                imp.entry_uuid
                    .set_title(&gettext("Password / Credentials (optional)"));
                imp.group_sec.set_visible(false);
            }
            _ => {
                imp.group_sec.set_visible(true);
            }
        }
    }

    /// Заполняет все строки интерфейса из разобранной структуры ключа `ParsedKey`.
    pub fn populate_from_parsed(&self, parsed: &ParsedKey) {
        let imp = self.imp();

        if !parsed.name.trim().is_empty() {
            imp.entry_name.set_text(&parsed.name);
        }

        let proto_idx = match parsed.protocol.to_uppercase().as_str() {
            "VLESS" => 0,
            "VMESS" => 1,
            "TROJAN" => 2,
            "SHADOWSOCKS" | "SS" => 3,
            "HYSTERIA2" | "HY2" => 4,
            "TUIC" => 5,
            "WIREGUARD" | "WG" => 6,
            "SOCKS" | "SOCKS5" => 7,
            "HTTP" | "HTTPS" => 8,
            _ => 0,
        };
        imp.combo_proto.set_selected(proto_idx);

        imp.entry_host.set_text(&parsed.host);
        imp.entry_port.set_text(&parsed.port.to_string());
        imp.entry_uuid.set_text(&parsed.uuid);

        let sec = parsed
            .query_params
            .get("security")
            .or_else(|| parsed.query_params.get("type"))
            .map(|s| s.as_str())
            .unwrap_or("");
        imp.entry_sec.set_text(sec);

        let sni = parsed
            .query_params
            .get("sni")
            .map(|s| s.as_str())
            .unwrap_or("");
        imp.entry_sni.set_text(sni);

        let pbk = if parsed.protocol.to_uppercase() == "WIREGUARD"
            || parsed.protocol.to_uppercase() == "WG"
        {
            parsed
                .query_params
                .get("public_key")
                .map(|s| s.as_str())
                .unwrap_or("")
        } else {
            parsed
                .query_params
                .get("pbk")
                .map(|s| s.as_str())
                .unwrap_or("")
        };
        imp.entry_pbk.set_text(pbk);

        let fp = parsed
            .query_params
            .get("fp")
            .map(|s| s.as_str())
            .unwrap_or("");
        imp.entry_fp.set_text(fp);

        let flow = parsed
            .query_params
            .get("flow")
            .map(|s| s.as_str())
            .unwrap_or("");
        imp.entry_flow.set_text(flow);

        self.update_protocol_fields();
    }

    /// Выполняет сбор параметров из полей формы и возвращает валидированный объект `ParsedKey`.
    pub fn collect_parsed_key(&self) -> Result<ParsedKey, String> {
        let imp = self.imp();

        let host = imp.entry_host.text().trim().to_string();
        if host.is_empty() {
            return Err(gettext("Please enter a valid server host address"));
        }

        let port_str = imp.entry_port.text().trim().to_string();
        let port = match port_str.parse::<u16>() {
            Ok(p) if p > 0 => p,
            _ => return Err(gettext("Please enter a valid port number (1-65535)")),
        };

        let uuid = imp.entry_uuid.text().trim().to_string();
        let name_input = imp.entry_name.text().trim().to_string();
        let name = if name_input.is_empty() {
            format!("{host}:{port}")
        } else {
            name_input
        };

        let proto_str = match imp.combo_proto.selected() {
            0 => "VLESS",
            1 => "VMess",
            2 => "Trojan",
            3 => "Shadowsocks",
            4 => "Hysteria2",
            5 => "TUIC",
            6 => "WireGuard",
            7 => "SOCKS5",
            8 => "HTTP",
            _ => "VLESS",
        };

        let mut query_params = HashMap::new();

        let sec = imp.entry_sec.text().trim().to_string();
        if !sec.is_empty() {
            query_params.insert("security".to_string(), sec);
        }

        let sni = imp.entry_sni.text().trim().to_string();
        if !sni.is_empty() {
            query_params.insert("sni".to_string(), sni);
        }

        let pbk = imp.entry_pbk.text().trim().to_string();
        if !pbk.is_empty() {
            if proto_str == "WireGuard" {
                query_params.insert("public_key".to_string(), pbk);
            } else {
                query_params.insert("pbk".to_string(), pbk);
            }
        }

        let fp = imp.entry_fp.text().trim().to_string();
        if !fp.is_empty() {
            query_params.insert("fp".to_string(), fp);
        }

        let flow = imp.entry_flow.text().trim().to_string();
        if !flow.is_empty() {
            query_params.insert("flow".to_string(), flow);
        }

        let mut parsed = ParsedKey {
            protocol: proto_str.to_string(),
            name,
            host,
            port,
            uuid,
            query_params,
            raw_url: String::new(),
        };

        let built = build_vpn_key(&parsed);
        let link_text = imp.entry_link.text().trim().to_string();
        parsed.raw_url = if !link_text.is_empty() {
            link_text
        } else {
            built
        };

        Ok(parsed)
    }

    /// Выполняет асинхронную TCP-проверку доступности сервера и замер задержки.
    fn perform_ping_test(&self) {
        let imp = self.imp();
        let host = imp.entry_host.text().trim().to_string();
        let port = imp.entry_port.text().trim().parse::<u16>().unwrap_or(443);

        if host.is_empty() {
            imp.row_latency
                .set_subtitle(&gettext("Enter host address first"));
            return;
        }

        imp.spinner_latency.set_visible(true);
        imp.spinner_latency.set_spinning(true);
        imp.row_latency.set_subtitle(&gettext("Measuring ping..."));

        let (tx, rx) = async_channel::unbounded::<(bool, u128)>();
        let row_latency_ui = imp.row_latency.clone();
        let spinner_ui = imp.spinner_latency.clone();

        glib::spawn_future_local(async move {
            if let Ok((success, ms)) = rx.recv().await {
                spinner_ui.set_spinning(false);
                spinner_ui.set_visible(false);
                if success {
                    row_latency_ui.set_subtitle(&format!("{ms} ms"));
                } else {
                    row_latency_ui.set_subtitle(&gettext("Connection timeout"));
                }
            }
        });

        std::thread::spawn(move || {
            let start = Instant::now();
            let mut success = false;
            let addr = format!("{host}:{port}");
            if let Ok(mut addrs) = addr.to_socket_addrs() {
                if let Some(sock) = addrs.next() {
                    if let Ok(stream) = TcpStream::connect_timeout(&sock, Duration::from_secs(3)) {
                        success = true;
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                    }
                }
            }
            let elapsed = start.elapsed().as_millis();
            let _ = tx.send_blocking((success, elapsed));
        });
    }

    /// Отображает модальный диалог в режиме **добавления нового профиля** (`Add`).
    ///
    /// Параметры:
    /// - `parent`: родительский виджет или окно приложения
    /// - `on_add`: колбэк при обычном сохранении профиля
    /// - `on_add_and_connect`: колбэк при сохранении с включенным переключателем «Подключиться после добавления»
    pub fn present_add<F1, F2>(parent: &impl IsA<gtk::Widget>, on_add: F1, on_add_and_connect: F2)
    where
        F1: Fn(ParsedKey) + 'static,
        F2: Fn(ParsedKey) + 'static,
    {
        let dialog = Self::new();
        dialog.set_mode(KeyDialogMode::Add);

        let dialog_weak = dialog.downgrade();
        dialog.imp().btn_save.connect_clicked(move |_| {
            if let Some(d) = dialog_weak.upgrade() {
                match d.collect_parsed_key() {
                    Ok(parsed) => {
                        let connect_now = d.imp().switch_connect.is_active();
                        if connect_now {
                            on_add_and_connect(parsed);
                        } else {
                            on_add(parsed);
                        }
                        d.close();
                    }
                    Err(err_msg) => {
                        d.imp().row_latency.set_subtitle(&err_msg);
                    }
                }
            }
        });

        dialog.present(Some(parent));
        dialog.imp().entry_link.grab_focus();
    }

    /// Отображает модальный диалог в режиме **редактирования существующего профиля** (`Edit`).
    ///
    /// Параметры:
    /// - `parent`: родительский виджет или окно приложения
    /// - `initial_key`: текущие параметры редактируемого профиля
    /// - `on_save`: колбэк сохранения обновленных параметров
    pub fn present_edit<F>(parent: &impl IsA<gtk::Widget>, initial_key: &ParsedKey, on_save: F)
    where
        F: Fn(ParsedKey) + 'static,
    {
        let dialog = Self::new();
        dialog.set_mode(KeyDialogMode::Edit);
        dialog.populate_from_parsed(initial_key);

        let dialog_weak = dialog.downgrade();
        dialog.imp().btn_save.connect_clicked(move |_| {
            if let Some(d) = dialog_weak.upgrade() {
                match d.collect_parsed_key() {
                    Ok(parsed) => {
                        on_save(parsed);
                        d.close();
                    }
                    Err(err_msg) => {
                        d.imp().row_latency.set_subtitle(&err_msg);
                    }
                }
            }
        });

        dialog.present(Some(parent));
        dialog.imp().entry_name.grab_focus();
    }
}
