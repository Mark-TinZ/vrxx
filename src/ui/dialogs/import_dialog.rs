/* import_dialog.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Интерактивный диалог импорта VPN-профилей (VrxxImportDialog)
//!
//! Отвечает за:
//! - Декларативное описание в `src/ui/import_dialog.ui`
//! - Отображение параметров импортируемого профиля (протокол, хост, порт, SNI, Fingerprint, Flow)
//! - Асинхронный фоновый TCP-замер задержки (ping) до целевого сервера в момент показа диалога
//! - Возможность редактирования названия профиля перед сохранением
//! - Варианты действия: «Импортировать» или «Импортировать и подключиться»
//! - Адаптивность к мобильным экранам через `AdwBreakpoint`

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::domain::key_parser::ParsedKey;

mod imp {
    use super::*;

    /// Структура CompositeTemplate для диалога импорта профиля
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/dialogs/import_dialog.ui")]
    pub struct VrxxImportDialog {
        #[template_child]
        pub btn_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_import: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_import_connect: TemplateChild<gtk::Button>,
        #[template_child]
        pub entry_name: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub row_protocol: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub row_host: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub row_port: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub row_latency: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub spinner_latency: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub group_security: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub row_security: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub row_sni: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub row_fingerprint: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub row_flow: TemplateChild<adw::ActionRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxImportDialog {
        const NAME: &'static str = "VrxxImportDialog";
        type Type = super::VrxxImportDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            adw::ToolbarView::static_type();
            adw::HeaderBar::static_type();
            adw::WindowTitle::static_type();
            adw::Breakpoint::static_type();
            adw::PreferencesPage::static_type();
            adw::PreferencesGroup::static_type();
            adw::ActionRow::static_type();
            adw::EntryRow::static_type();
            gtk::Spinner::static_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxImportDialog {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_width_request(360);
            self.obj().set_height_request(360);
        }
    }
    impl WidgetImpl for VrxxImportDialog {}
    impl adw::subclass::dialog::AdwDialogImpl for VrxxImportDialog {}
}

glib::wrapper! {
    /// Обертка GObject для диалога импорта
    pub struct VrxxImportDialog(ObjectSubclass<imp::VrxxImportDialog>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxImportDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxImportDialog {
    /// Создает новый экземпляр диалога импорта
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

/// Отображает модальный диалог `AdwDialog` для импорта VPN-профиля из URL-схемы.
pub fn show_import_dialog<F1, F2>(
    parent: &gtk::Window,
    parsed: ParsedKey,
    on_import_only: F1,
    on_import_and_connect: F2,
) where
    F1: Fn(ParsedKey) + 'static,
    F2: Fn(ParsedKey) + 'static,
{
    let dialog = VrxxImportDialog::new();
    let imp = dialog.imp();

    // Заполнение основных полей
    imp.entry_name.set_text(&parsed.name);
    imp.row_protocol.set_subtitle(&parsed.protocol);
    imp.row_host.set_subtitle(&parsed.host);
    imp.row_port.set_subtitle(&parsed.port.to_string());

    // Заполнение опциональных параметров безопасности
    let mut has_sec = false;

    if let Some(sec) = parsed
        .query_params
        .get("security")
        .or_else(|| parsed.query_params.get("type"))
    {
        imp.row_security.set_subtitle(sec);
        imp.row_security.set_visible(true);
        has_sec = true;
    }

    if let Some(sni) = parsed.query_params.get("sni") {
        imp.row_sni.set_subtitle(sni);
        imp.row_sni.set_visible(true);
        has_sec = true;
    }

    if let Some(fp) = parsed.query_params.get("fp") {
        imp.row_fingerprint.set_subtitle(fp);
        imp.row_fingerprint.set_visible(true);
        has_sec = true;
    }

    if let Some(flow) = parsed.query_params.get("flow") {
        if !flow.is_empty() {
            imp.row_flow.set_subtitle(flow);
            imp.row_flow.set_visible(true);
            has_sec = true;
        }
    }

    imp.group_security.set_visible(has_sec);

    // Асинхронный фоновый TCP-замер задержки
    let target_host = parsed.host.clone();
    let target_port = parsed.port;
    let (sender, receiver) = async_channel::unbounded::<(bool, u128)>();

    let row_latency_ui = imp.row_latency.clone();
    let spinner_latency_ui = imp.spinner_latency.clone();

    glib::spawn_future_local(async move {
        if let Ok((success, ms)) = receiver.recv().await {
            spinner_latency_ui.set_spinning(false);
            spinner_latency_ui.set_visible(false);
            if success {
                row_latency_ui.set_subtitle(&format!("{ms} ms"));
            } else {
                row_latency_ui.set_subtitle(&gettext("Connection timeout"));
            }
        }
    });

    std::thread::spawn(move || {
        let start_ping = Instant::now();
        let mut success = false;
        let timeout = Duration::from_secs(3);

        let addr = format!("{target_host}:{target_port}");
        if let Ok(mut addrs) = addr.to_socket_addrs() {
            if let Some(socket_addr) = addrs.next() {
                if let Ok(stream) = TcpStream::connect_timeout(&socket_addr, timeout) {
                    success = true;
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                }
            }
        }

        let elapsed = start_ping.elapsed().as_millis();
        let _ = sender.send_blocking((success, elapsed));
    });

    // Обработчик кнопки Отмена
    let d_cancel = dialog.downgrade();
    imp.btn_cancel.connect_clicked(move |_| {
        if let Some(d) = d_cancel.upgrade() {
            d.close();
        }
    });

    // Обработчик кнопки Импортировать
    let d_import = dialog.downgrade();
    let parsed_imp = parsed.clone();
    let entry_name_imp = imp.entry_name.clone();
    imp.btn_import.connect_clicked(move |_| {
        let mut updated = parsed_imp.clone();
        let name_text = entry_name_imp.text().to_string();
        if !name_text.trim().is_empty() {
            updated.name = name_text;
        }
        if let Some(d) = d_import.upgrade() {
            d.close();
        }
        on_import_only(updated);
    });

    // Обработчик кнопки Импортировать и подключиться
    let d_conn = dialog.downgrade();
    let parsed_conn = parsed;
    let entry_name_conn = imp.entry_name.clone();
    imp.btn_import_connect.connect_clicked(move |_| {
        let mut updated = parsed_conn.clone();
        let name_text = entry_name_conn.text().to_string();
        if !name_text.trim().is_empty() {
            updated.name = name_text;
        }
        if let Some(d) = d_conn.upgrade() {
            d.close();
        }
        on_import_and_connect(updated);
    });

    dialog.present(Some(parent));
    imp.entry_name.grab_focus();
}
