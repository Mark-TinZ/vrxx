/* theme_switcher.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Компонент переключения темы оформления (VrxxThemeSwitcher)
//!
//! Предоставляет горизонтальный блок круглых кнопок для выбора цветовой схемы:
//! - `System` (автоматически по системным настройкам GNOME / FreeDesktop)
//! - `Light` (принудительно светлая)
//! - `Dark` (принудительно темная)

use adw::prelude::*;
use gtk::{glib, subclass::prelude::*, CompositeTemplate, TemplateChild};

mod imp {
    use super::*;

    /// Внутренняя реализация CompositeTemplate для VrxxThemeSwitcher
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/components/theme_switcher.ui")]
    pub struct VrxxThemeSwitcher {
        #[template_child]
        pub btn_system: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub btn_light: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub btn_dark: TemplateChild<gtk::CheckButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxThemeSwitcher {
        const NAME: &'static str = "VrxxThemeSwitcher";
        type Type = super::VrxxThemeSwitcher;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxThemeSwitcher {
        fn constructed(&self) {
            self.parent_constructed();

            // Если система не поддерживает автоопределение цветовой схемы — скрываем системную кнопку
            let style_manager = adw::StyleManager::default();
            if !style_manager.system_supports_color_schemes() {
                self.btn_system.set_visible(false);
            }
        }
    }

    impl WidgetImpl for VrxxThemeSwitcher {}
    impl BoxImpl for VrxxThemeSwitcher {}
}

glib::wrapper! {
    /// Обертка GObject для виджета переключения тем
    pub struct VrxxThemeSwitcher(ObjectSubclass<imp::VrxxThemeSwitcher>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxThemeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxThemeSwitcher {
    /// Создает новый экземпляр виджета переключения тем.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}
