/* window.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Главное окно приложения (VrxxWindow)
//!
//! Отвечает за:
//! - Навигацию между страницами (`VrxxVpnPage`, `VrxxProxyPage`, `VrxxRoutingPage`, `VrxxSettingsPage`)
//! - Управление `AdwNavigationSplitView` и адаптивным переходом в мобильный вид (`AdwBreakpoint`)
//! - Отображение статуса активного подключения внизу боковой панели (`active_connection_btn`)
//! - Всплывающий поповер с детальной телеметрией подключения (пинг, трафик, геолокация, IP)
//! - Перехват и открытие диалога импорта при поступлении URL-схем (`handle_open_uri`)

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::ui::pages::{VrxxProxyPage, VrxxRoutingPage, VrxxSettingsPage, VrxxVpnPage};

mod imp {
    use super::*;

    /// Структура CompositeTemplate для главного окна VrxxWindow
    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/window.ui")]
    pub struct VrxxWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub navigation_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub view_stack: TemplateChild<gtk::Stack>,

        // Виджеты плашки активного соединения внизу сайдбара
        #[template_child]
        pub active_connection_btn: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub active_connection_popover: TemplateChild<gtk::Popover>,
        #[template_child]
        pub active_status_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub active_status_spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub active_popover_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub active_popover_spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub active_server_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_popover_server: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_popover_status_subtitle: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_popover_protocol: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_popover_location: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_popover_host: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_popover_mode: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_popover_ping: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_connection_timer: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_server_traffic: TemplateChild<gtk::Label>,

        pub is_navigating: std::cell::RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxWindow {
        const NAME: &'static str = "VrxxWindow";
        type Type = super::VrxxWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            adw::ToastOverlay::static_type();
            adw::NavigationSplitView::static_type();
            VrxxVpnPage::static_type();
            VrxxProxyPage::static_type();
            VrxxRoutingPage::static_type();
            VrxxSettingsPage::static_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_actions();
            obj.setup_callbacks();
            obj.start_status_polling();

            // Выбор первой страницы (VPN Keys) при запуске
            if let Some(row) = self.navigation_list.row_at_index(0) {
                self.navigation_list.select_row(Some(&row));
                obj.select_page_by_row(&row);
            }
        }
    }
    impl WidgetImpl for VrxxWindow {}
    impl WindowImpl for VrxxWindow {}
    impl ApplicationWindowImpl for VrxxWindow {}
    impl AdwApplicationWindowImpl for VrxxWindow {}
}

glib::wrapper! {
    /// Обертка GObject для главного окна VrxxWindow
    pub struct VrxxWindow(ObjectSubclass<imp::VrxxWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap,
                   gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                   gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl VrxxWindow {
    /// Создает экземпляр главного окна приложения.
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    /// Отображает всплывающее AdwToast уведомление поверх контента окна.
    pub fn add_toast(&self, toast: adw::Toast) {
        self.imp().toast_overlay.add_toast(toast);
    }

    /// Извлекает имя страницы стека из выбранной строки списка сайдбара.
    fn get_page_name_from_row(&self, row: &gtk::ListBoxRow) -> Option<String> {
        let name = row.widget_name();
        if !name.is_empty() && name.starts_with("page_") {
            Some(name.to_string())
        } else {
            match row.index() {
                0 => Some("page_vpn".to_string()),
                1 => Some("page_proxy".to_string()),
                2 => Some("page_routing".to_string()),
                3 => Some("page_settings".to_string()),
                _ => None,
            }
        }
    }

    /// Переключает видимую страницу стека `view_stack` с проверкой навигационного стража.
    fn select_page_by_row(&self, row: &gtk::ListBoxRow) {
        let imp = self.imp();
        if *imp.is_navigating.borrow() {
            return;
        }

        let current_page = imp
            .view_stack
            .visible_child_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "page_vpn".to_string());
        let target_page = match self.get_page_name_from_row(row) {
            Some(name) => name,
            None => return,
        };

        if current_page == target_page {
            imp.split_view.set_show_content(true);
            return;
        }

        // Проверка наличия несохраненных изменений на текущей странице
        let mut has_unsaved = false;
        let mut apply_fn: Option<Box<dyn FnOnce()>> = None;
        let mut discard_fn: Option<Box<dyn FnOnce()>> = None;

        if current_page == "page_routing" {
            if let Some(child) = imp.view_stack.child_by_name("page_routing") {
                if let Some(routing_page) = child.downcast_ref::<VrxxRoutingPage>() {
                    if routing_page.has_changes() {
                        has_unsaved = true;
                        let p1 = routing_page.clone();
                        let p2 = routing_page.clone();
                        apply_fn = Some(Box::new(move || p1.apply_changes()));
                        discard_fn = Some(Box::new(move || p2.discard_changes()));
                    }
                }
            }
        } else if current_page == "page_proxy" {
            if let Some(child) = imp.view_stack.child_by_name("page_proxy") {
                if let Some(proxy_page) = child.downcast_ref::<VrxxProxyPage>() {
                    if proxy_page.has_changes() {
                        has_unsaved = true;
                        let p1 = proxy_page.clone();
                        let p2 = proxy_page.clone();
                        apply_fn = Some(Box::new(move || p1.apply_changes()));
                        discard_fn = Some(Box::new(move || p2.discard_changes()));
                    }
                }
            }
        } else if current_page == "page_settings" {
            if let Some(child) = imp.view_stack.child_by_name("page_settings") {
                if let Some(settings_page) = child.downcast_ref::<VrxxSettingsPage>() {
                    if settings_page.has_changes() {
                        has_unsaved = true;
                        let p1 = settings_page.clone();
                        let p2 = settings_page.clone();
                        apply_fn = Some(Box::new(move || p1.apply_changes()));
                        discard_fn = Some(Box::new(move || p2.discard_changes()));
                    }
                }
            }
        }

        if has_unsaved {
            let win_weak1 = self.downgrade();
            let win_weak2 = self.downgrade();
            let win_weak3 = self.downgrade();
            let target_clone1 = target_page.clone();
            let target_clone2 = target_page.clone();
            let current_clone = current_page.clone();

            crate::ui::change_tracker::show_unsaved_changes_dialog(
                self.upcast_ref::<gtk::Window>(),
                move || {
                    if let Some(apply) = apply_fn {
                        apply();
                    }
                    if let Some(win) = win_weak1.upgrade() {
                        win.switch_to_page(&target_clone1);
                    }
                },
                move || {
                    if let Some(discard) = discard_fn {
                        discard();
                    }
                    if let Some(win) = win_weak2.upgrade() {
                        win.switch_to_page(&target_clone2);
                    }
                },
                move || {
                    if let Some(win) = win_weak3.upgrade() {
                        win.select_row_for_page(&current_clone);
                    }
                },
            );
        } else {
            self.switch_to_page(&target_page);
        }
    }

    /// Программно переключает стек страниц и обновляет выделение в навигационном списке.
    fn switch_to_page(&self, page_name: &str) {
        let imp = self.imp();
        *imp.is_navigating.borrow_mut() = true;

        imp.view_stack.set_visible_child_name(page_name);
        imp.split_view.set_show_content(true);

        let target_idx = match page_name {
            "page_vpn" => 0,
            "page_proxy" => 1,
            "page_routing" => 2,
            "page_settings" => 3,
            _ => 0,
        };

        if let Some(row) = imp.navigation_list.row_at_index(target_idx) {
            imp.navigation_list.select_row(Some(&row));
        }

        *imp.is_navigating.borrow_mut() = false;
    }

    /// Возвращает выбор в навигационном списке к указанной странице.
    fn select_row_for_page(&self, page_name: &str) {
        let imp = self.imp();
        *imp.is_navigating.borrow_mut() = true;

        let target_idx = match page_name {
            "page_vpn" => 0,
            "page_proxy" => 1,
            "page_routing" => 2,
            "page_settings" => 3,
            _ => 0,
        };

        if let Some(row) = imp.navigation_list.row_at_index(target_idx) {
            imp.navigation_list.select_row(Some(&row));
        }

        *imp.is_navigating.borrow_mut() = false;
    }

    /// Настройка локальных действий окна (GActions).
    fn setup_actions(&self) {
        let disconnect_action = gio::SimpleAction::new("disconnect", None);
        let window_weak = self.downgrade();
        disconnect_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                window.disconnect_vpn();
            }
        });
        self.add_action(&disconnect_action);
    }

    /// Инициирует отключение активного VPN соединения через страницу `VrxxVpnPage`.
    pub fn disconnect_vpn(&self) {
        let imp = self.imp();
        imp.active_connection_popover.popdown();

        if let Some(child) = imp.view_stack.child_by_name("page_vpn") {
            if let Some(vpn_page) = child.downcast_ref::<VrxxVpnPage>() {
                vpn_page.disconnect();
            }
        }
    }

    /// Подключение сигналов элементов интерфейса.
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let window_weak = self.downgrade();
        imp.navigation_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                if let Some(window) = window_weak.upgrade() {
                    window.select_page_by_row(row);
                }
            }
        });

        let window_weak2 = self.downgrade();
        imp.navigation_list.connect_row_activated(move |_, row| {
            if let Some(window) = window_weak2.upgrade() {
                window.select_page_by_row(row);
            }
        });

        // При закрытии главного окна автоматически закрываем все зависимые дочерние окна (окно логов)
        let window_weak_close = self.downgrade();
        self.connect_close_request(move |_| {
            if let Some(window) = window_weak_close.upgrade() {
                if let Some(app) = window.application() {
                    for win in app.windows() {
                        if win.is::<crate::ui::components::log_window::VrxxLogWindow>() {
                            win.close();
                        }
                    }
                }
            }
            glib::Propagation::Proceed
        });
    }

    /// Запускает фоновый таймер периодического опроса статуса активного подключения (раз в 1 сек).
    fn start_status_polling(&self) {
        let window_weak = self.downgrade();
        glib::timeout_add_local(std::time::Duration::from_millis(1000), move || {
            if let Some(window) = window_weak.upgrade() {
                window.update_active_connection_widget();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    /// Обновляет текстовые метки и видимость виджета активного подключения внизу сайдбара.
    pub fn update_active_connection_widget(&self) {
        use crate::settings::SettingsManager;
        use gettextrs::gettext;
        let settings = SettingsManager::new().load();

        let imp = self.imp();

        if let Some(active_key) = settings.keys.iter().find(|k| k.is_active) {
            imp.active_connection_btn.set_visible(true);
            imp.active_server_name.set_label(&active_key.name);
            imp.active_popover_server.set_label(&active_key.name);

            let loc = if active_key.location.is_empty() {
                gettext("Unknown Location")
            } else {
                active_key.location.clone()
            };
            imp.active_popover_location.set_label(&loc);
            imp.active_popover_protocol.set_label(&format!(
                "{}: {}",
                gettext("Protocol"),
                active_key.protocol.to_uppercase()
            ));

            let host_display = if settings.streamer_mode {
                "***.***.***.***".to_string()
            } else if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&active_key.url) {
                format!("{}:{}", parsed.host, parsed.port)
            } else {
                "0.0.0.0".to_string()
            };
            imp.active_popover_host
                .set_label(&format!("{}: {host_display}", gettext("Host")));

            let mode_str = if settings.tun_mode {
                gettext("TUN Mode")
            } else {
                gettext("Proxy Mode")
            };
            imp.active_popover_mode.set_label(&mode_str);

            if !active_key.ping.is_empty() && active_key.ping != "0 ms" {
                let ping_str = format!("{}: {}", gettext("Ping"), active_key.ping);
                imp.active_popover_ping.set_label(&ping_str);
            }
        } else {
            imp.active_connection_btn.set_visible(false);
        }
    }

    /// Утилита для сброса предыдущих CSS-классов статуса и применения нового стиля и иконки.
    fn set_icon_status_style(icon: &gtk::Image, class_to_add: &str, icon_name: &str) {
        icon.remove_css_class("success");
        icon.remove_css_class("warning");
        icon.remove_css_class("error");
        icon.remove_css_class("dim-label");
        icon.add_css_class(class_to_add);
        icon.set_icon_name(Some(icon_name));
    }

    /// Обновляет визуальное состояние виджета подключения ("Connected", "Connecting", "Error", "Disconnected").
    pub fn update_status_state(&self, status: &str, name: Option<&str>) {
        use gettextrs::gettext;
        let imp = self.imp();
        match status {
            "Connected" => {
                imp.active_connection_btn.set_visible(true);
                if let Some(n) = name {
                    imp.active_server_name.set_label(n);
                    imp.active_popover_server.set_label(n);
                }

                imp.active_status_spinner.set_spinning(false);
                imp.active_status_spinner.set_visible(false);
                imp.active_status_icon.set_visible(true);
                Self::set_icon_status_style(
                    &imp.active_status_icon,
                    "success",
                    "network-vpn-symbolic",
                );

                imp.active_popover_spinner.set_spinning(false);
                imp.active_popover_spinner.set_visible(false);
                imp.active_popover_icon.set_visible(true);
                Self::set_icon_status_style(
                    &imp.active_popover_icon,
                    "success",
                    "network-vpn-symbolic",
                );

                imp.active_popover_status_subtitle
                    .set_label(&gettext("Connected"));
            }
            "Connecting" | "Disconnecting" => {
                imp.active_connection_btn.set_visible(true);
                if let Some(n) = name {
                    imp.active_server_name.set_label(n);
                    imp.active_popover_server.set_label(n);
                }
                let label_text = if status == "Connecting" {
                    gettext("Connecting...")
                } else {
                    gettext("Disconnecting...")
                };

                imp.active_connection_timer.set_label(&label_text);
                imp.active_popover_status_subtitle.set_label(&label_text);

                imp.active_status_icon.set_visible(false);
                imp.active_status_spinner.set_visible(true);
                imp.active_status_spinner.set_spinning(true);

                imp.active_popover_icon.set_visible(false);
                imp.active_popover_spinner.set_visible(true);
                imp.active_popover_spinner.set_spinning(true);
            }
            "Error" => {
                imp.active_connection_btn.set_visible(true);
                imp.active_connection_timer.set_label(&gettext("Error"));
                imp.active_popover_status_subtitle
                    .set_label(&gettext("Connection Error"));

                imp.active_status_spinner.set_spinning(false);
                imp.active_status_spinner.set_visible(false);
                imp.active_status_icon.set_visible(true);
                Self::set_icon_status_style(
                    &imp.active_status_icon,
                    "error",
                    "dialog-error-symbolic",
                );

                imp.active_popover_spinner.set_spinning(false);
                imp.active_popover_spinner.set_visible(false);
                imp.active_popover_icon.set_visible(true);
                Self::set_icon_status_style(
                    &imp.active_popover_icon,
                    "error",
                    "dialog-error-symbolic",
                );
            }
            "Disconnected" => {
                imp.active_status_spinner.set_spinning(false);
                imp.active_popover_spinner.set_spinning(false);
                imp.active_connection_btn.set_visible(false);
            }
            _ => {}
        }
    }

    /// Обновляет статистику трафика и времени соединения во всплывающем поповере.
    pub fn update_stats(&self, time: &str, down: &str, up: &str, ping: &str) {
        use gettextrs::gettext;
        let imp = self.imp();
        imp.active_connection_timer.set_label(time);
        imp.active_popover_status_subtitle.set_label(&format!(
            "{} • {}",
            gettext("Connected"),
            time
        ));
        imp.active_server_traffic
            .set_label(&format!("↓ {} | ↑ {}", down, up));

        if !ping.is_empty() && ping != "0 ms" {
            imp.active_popover_ping
                .set_label(&format!("{}: {ping}", gettext("Ping")));
        }
    }

    /// Обрабатывает открытие ссылки URL-схемы (vless://, vmess:// и др.) и показывает диалог импорта.
    pub fn handle_open_uri(&self, uri: &str) {
        match crate::domain::key_parser::parse_vpn_key(uri) {
            Ok(parsed) => {
                self.present();
                let window_weak = self.downgrade();
                crate::ui::import_dialog::show_import_dialog(
                    self.upcast_ref::<gtk::Window>(),
                    parsed,
                    move |parsed_import| {
                        if let Some(window) = window_weak.upgrade() {
                            window.import_key_to_vpn_page(parsed_import, false);
                        }
                    },
                    {
                        let window_weak = self.downgrade();
                        move |parsed_connect| {
                            if let Some(window) = window_weak.upgrade() {
                                window.import_key_to_vpn_page(parsed_connect, true);
                            }
                        }
                    },
                );
            }
            Err(e) => {
                tracing::error!("Не удалось распарсить ссылку URL-схемы '{uri}': {e}");
            }
        }
    }

    /// Передает распарсенный ключ на страницу `VrxxVpnPage` для сохранения и/или подключения.
    fn import_key_to_vpn_page(&self, parsed: crate::domain::key_parser::ParsedKey, connect: bool) {
        let imp = self.imp();
        if let Some(row) = imp.navigation_list.row_at_index(0) {
            imp.navigation_list.select_row(Some(&row));
            self.select_page_by_row(&row);
        }
        if let Some(vpn_widget) = imp.view_stack.child_by_name("page_vpn") {
            if let Some(vpn_page) = vpn_widget.downcast_ref::<VrxxVpnPage>() {
                vpn_page.import_key(parsed, connect);
            }
        }
    }
}
