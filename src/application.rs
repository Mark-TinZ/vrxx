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
use crate::VrxxWindow;

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
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();
            // Get the current window or create one if necessary
            let window = application.active_window().unwrap_or_else(|| {
                let window = VrxxWindow::new(&*application);
                window.upcast()
            });

            // Ask the window manager/compositor to present the window
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

    // Функция для регистрации иконок из ресурсов
    fn setup_icons(&self) {
        if let Some(display) = gdk::Display::default() {
            let theme = gtk::IconTheme::for_display(&display);
            // Добавляем путь внутри gresource
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
            .developers(vec!["Mark <marktin@duck.com>"])
            .artists(vec!["GNOME Design Team"])
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(&gettext("translator-credits"))
            .copyright("© 2026 Mark")
            .license_type(gtk::License::Mpl20)
            .website("https://github.com/Mark-TinZ/vrxx")
            .issue_url("https://github.com/Mark-TinZ/vrxx/issues")
            .comments(&gettext("A graphical interface for Xray-core designed to simplify VPN and proxy configuration on Linux systems. Features include TUN device management, traffic monitoring, and an intuitive user interface for managing connection profiles."))
            .build();

        about.present(Some(&window));
    }
}
