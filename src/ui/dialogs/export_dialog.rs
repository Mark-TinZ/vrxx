/* export_dialog.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Интерактивный диалог выборочного экспорта конфигурации (VrxxExportDialog)
//!
//! Отвечает за:
//! - Декларативное описание в `src/ui/export_dialog.ui`
//! - Выбор компонентов для экспорта (настройки приложения, ключи/профили)
//! - Валидацию: кнопка «Экспортировать» активна только если выбран хотя бы один компонент
//! - Адаптивность к мобильным экранам через `AdwBreakpoint`

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    /// Структура CompositeTemplate для диалога экспорта
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/dialogs/export_dialog.ui")]
    pub struct VrxxExportDialog {
        #[template_child]
        pub btn_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_export: TemplateChild<gtk::Button>,
        #[template_child]
        pub switch_settings: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub switch_keys: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxExportDialog {
        const NAME: &'static str = "VrxxExportDialog";
        type Type = super::VrxxExportDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            adw::ToolbarView::static_type();
            adw::HeaderBar::static_type();
            adw::WindowTitle::static_type();
            adw::Breakpoint::static_type();
            adw::PreferencesPage::static_type();
            adw::PreferencesGroup::static_type();
            adw::SwitchRow::static_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxExportDialog {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_width_request(360);
            self.obj().set_height_request(300);
        }
    }
    impl WidgetImpl for VrxxExportDialog {}
    impl adw::subclass::dialog::AdwDialogImpl for VrxxExportDialog {}
}

glib::wrapper! {
    /// Обертка GObject для нативного диалога экспорта конфигурации
    pub struct VrxxExportDialog(ObjectSubclass<imp::VrxxExportDialog>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxExportDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxExportDialog {
    /// Создает новый экземпляр диалога экспорта
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

/// Отображает модальный диалог `AdwDialog` для выбора компонентов экспорта.
///
/// Параметры замыкания `on_confirmed`:
/// - `export_settings: bool` — экспортировать ли настройки приложения
/// - `export_keys: bool` — экспортировать ли VPN-профили и ключи
pub fn show_export_dialog<F>(parent: &gtk::Window, on_confirmed: F)
where
    F: Fn(bool, bool) + 'static,
{
    let dialog = VrxxExportDialog::new();
    let imp = dialog.imp();

    // Отмена диалога
    let d_cancel = dialog.downgrade();
    imp.btn_cancel.connect_clicked(move |_| {
        if let Some(d) = d_cancel.upgrade() {
            d.close();
        }
    });

    // Валидация активности кнопки «Экспортировать»
    let update_button_state = {
        let sw_set = imp.switch_settings.clone();
        let sw_keys = imp.switch_keys.clone();
        let btn_exp = imp.btn_export.clone();
        move || {
            let any_selected = sw_set.is_active() || sw_keys.is_active();
            btn_exp.set_sensitive(any_selected);
        }
    };

    update_button_state();

    let u1 = update_button_state.clone();
    imp.switch_settings.connect_active_notify(move |_| u1());

    let u2 = update_button_state;
    imp.switch_keys.connect_active_notify(move |_| u2());

    // Подтверждение экспорта
    let d_exp = dialog.downgrade();
    let sw_set = imp.switch_settings.clone();
    let sw_keys = imp.switch_keys.clone();
    imp.btn_export.connect_clicked(move |_| {
        let exp_settings = sw_set.is_active();
        let exp_keys = sw_keys.is_active();
        if let Some(d) = d_exp.upgrade() {
            d.close();
        }
        on_confirmed(exp_settings, exp_keys);
    });

    dialog.present(Some(parent));
}
