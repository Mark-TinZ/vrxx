/* application.rs
 *
 * Copyright 2026 Unknown
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

use gettextrs::gettext;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, gdk};
use crate::config::VERSION;
use crate::window::VrxxWindow;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct VrxxApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxApplication {
        const NAME: &'static str = "VrxxApplication";
        type Type = super::VrxxApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for VrxxApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<control>q"]);

            obj.setup_icons();
        }
    }

    impl ApplicationImpl for VrxxApplication {
        fn startup(&self) {
            self.parent_startup();

            // Инициализация менеджера стилей LibAdwaita (убирает warning)
            let manager = adw::StyleManager::default();
            // Вы можете задать ForceDark или оставить системную настройку
            // manager.set_color_scheme(adw::ColorScheme::PreferDark);

            // Настройка иконок
            let display = gdk::Display::default().unwrap_or_else(|| {
                // Fallback для систем без дисплея (CI/CD)
                // ИСПРАВЛЕНИЕ: Оборачиваем результат env::var в Some()
                gdk::Display::open(Some(std::env::var("DISPLAY").unwrap_or_default().as_str()))
                    .expect("No display available")
            });

            let icon_theme = gtk::IconTheme::for_display(&display);
            icon_theme.add_resource_path("/ru/mark/vrxx/icons");
        }

        fn activate(&self) {
            let application = self.obj();
            let window = application.active_window().unwrap_or_else(|| {
                let window = VrxxWindow::new(&*application);
                window.upcast()
            });

            window.present();
        }
    }

    impl GtkApplicationImpl for VrxxApplication {}
    impl AdwApplicationImpl for VrxxApplication {}
}

glib::wrapper! {
    pub struct VrxxApplication(ObjectSubclass<imp::VrxxApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl VrxxApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .property("resource-base-path", "/ru/mark/vrxx")
            .build()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        self.add_action_entries([quit_action, about_action]);
    }

    fn setup_icons(&self) {
        if let Some(display) = gdk::Display::default() {
            let theme = gtk::IconTheme::for_display(&display);
            theme.add_resource_path("/ru/mark/vrxx/icons");
        }
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("vrxx")
            .application_icon("ru.mark.vrxx")
            .developer_name("Mark")
            .version(VERSION)
            .developers(vec!["Mark"])
            .artists(vec!["GNOME Design Team"])
            .translator_credits(&gettext("translator-credits"))
            .copyright("© 2026 Mark")
            .license_type(gtk::License::Mpl20)
            .website("https://github.com/Mark-TinZ/vrxx")
            .issue_url("https://github.com/Mark-TinZ/vrxx/issues")
            .comments(&gettext("A graphical interface for Xray-core designed to simplify VPN and proxy configuration on Linux systems."))
            .build();

        about.present(Some(&window));
    }
}

