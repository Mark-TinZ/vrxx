/* qr_dialog.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Диалог экспорта и отображения QR-кода профиля (VrxxQrDialog)
//!
//! Отвечает за:
//! - Генерацию векторного и растрового QR-кода в оперативной памяти без записи временных файлов
//! - Защиту приватности: размытие (blur) QR-кода с возможностью снятия по клику или при наведении
//! - Копирование ссылки подключения и структурированного JSON профиля в буфер обмена
//! - Экспорт QR-кода в форматы PNG и SVG через нативный диалог сохранения `gtk::FileDialog`

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib, CompositeTemplate};
use std::cell::RefCell;

use crate::domain::exporter;

mod imp {
    use super::*;

    /// Структура CompositeTemplate для виджета содержимого диалога QR-кода
    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/qr_dialog.ui")]
    pub struct VrxxQrDialog {
        #[template_child]
        pub qr_card: TemplateChild<gtk::Box>,
        #[template_child]
        pub qr_picture: TemplateChild<gtk::Picture>,
        #[template_child]
        pub btn_toggle_blur: TemplateChild<gtk::Button>,
        #[template_child]
        pub img_blur_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub lbl_blur_status: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_error: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_uri: TemplateChild<gtk::Label>,
        #[template_child]
        pub btn_copy_link: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_copy_json: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_save_qr: TemplateChild<gtk::Button>,

        pub is_blurred: RefCell<bool>,
        pub is_hovered: RefCell<bool>,
    }

    impl Default for VrxxQrDialog {
        fn default() -> Self {
            Self {
                qr_card: TemplateChild::default(),
                qr_picture: TemplateChild::default(),
                btn_toggle_blur: TemplateChild::default(),
                img_blur_icon: TemplateChild::default(),
                lbl_blur_status: TemplateChild::default(),
                lbl_error: TemplateChild::default(),
                lbl_uri: TemplateChild::default(),
                btn_copy_link: TemplateChild::default(),
                btn_copy_json: TemplateChild::default(),
                btn_save_qr: TemplateChild::default(),
                is_blurred: RefCell::new(true),
                is_hovered: RefCell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxQrDialog {
        const NAME: &'static str = "VrxxQrDialog";
        type Type = super::VrxxQrDialog;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::Clamp::static_type();
            gtk::Box::static_type();
            gtk::Picture::static_type();
            gtk::Overlay::static_type();
            gtk::Button::static_type();
            gtk::Image::static_type();
            gtk::Label::static_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxQrDialog {}
    impl WidgetImpl for VrxxQrDialog {}
    impl BinImpl for VrxxQrDialog {}
}

glib::wrapper! {
    /// Обертка GObject для виджета содержимого диалога QR-кода
    pub struct VrxxQrDialog(ObjectSubclass<imp::VrxxQrDialog>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxQrDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxQrDialog {
    /// Создает экземпляр виджета QR-диалога с начальным состоянием защиты приватности (размытие включено).
    pub fn new() -> Self {
        let content: Self = glib::Object::builder().build();
        content.setup_blur_events();
        content.update_blur_ui();
        content
    }

    /// Настраивает контроллеры клика и наведения мыши для снятия/возврата размытия.
    fn setup_blur_events(&self) {
        let imp = self.imp();

        // 1. Клик в любой точке карточки QR-кода переключает состояние размытия
        let gesture_click = gtk::GestureClick::new();
        let content_weak_click = self.downgrade();
        gesture_click.connect_pressed(move |_, _, _, _| {
            if let Some(content) = content_weak_click.upgrade() {
                let mut blurred = content.imp().is_blurred.borrow_mut();
                *blurred = !*blurred;
                drop(blurred);
                content.update_blur_ui();
            }
        });
        imp.qr_card.add_controller(gesture_click);

        // 2. Отслеживание наведения курсора мыши (ховер)
        let motion_controller = gtk::EventControllerMotion::new();
        let content_weak_enter = self.downgrade();
        motion_controller.connect_enter(move |_, _, _| {
            if let Some(content) = content_weak_enter.upgrade() {
                *content.imp().is_hovered.borrow_mut() = true;
                content.update_blur_ui();
            }
        });

        // 3. Отслеживание ухода курсора мыши
        let content_weak_leave = self.downgrade();
        motion_controller.connect_leave(move |_| {
            if let Some(content) = content_weak_leave.upgrade() {
                *content.imp().is_hovered.borrow_mut() = false;
                content.update_blur_ui();
            }
        });
        imp.qr_card.add_controller(motion_controller);
    }

    /// Обновляет визуальные стили размытия и видимость плавающей кнопки-глазика.
    fn update_blur_ui(&self) {
        let imp = self.imp();
        let is_blurred = *imp.is_blurred.borrow();
        let is_hovered = *imp.is_hovered.borrow();

        if is_blurred {
            imp.qr_picture.add_css_class("blurred-qr");
            imp.img_blur_icon
                .set_icon_name(Some("view-reveal-symbolic"));
            imp.lbl_blur_status.set_label(&gettext("Show QR Code"));
            // Когда изображение размыто — кнопка видна всегда
            imp.btn_toggle_blur.set_visible(true);
        } else {
            imp.qr_picture.remove_css_class("blurred-qr");
            imp.img_blur_icon
                .set_icon_name(Some("view-conceal-symbolic"));
            imp.lbl_blur_status.set_label(&gettext("Hide QR Code"));
            // Когда изображение открыто — кнопка видна только при наведении мыши
            imp.btn_toggle_blur.set_visible(is_hovered);
        }
    }
}

/// Отправляет всплывающее уведомление `AdwToast` в родительское главное окно.
fn add_parent_toast(parent: &gtk::Window, message: &str) {
    if let Some(win) = parent.downcast_ref::<crate::window::VrxxWindow>() {
        win.add_toast(adw::Toast::new(message));
    }
}

/// Отображает интерактивный модальный диалог `AdwAlertDialog` с QR-кодом и опциями шеринга.
///
/// Возможности:
/// - Декларативная разметка в `src/ui/qr_dialog.ui`
/// - Защита приватности с эффектом размытия (Privacy Blur)
/// - Кнопка «Ссылка»: копирование URI в буфер обмена
/// - Кнопка «JSON»: парсинг и копирование JSON-конфигурации
/// - Кнопка «Сохранить QR»: экспорт в файлы PNG или SVG через `gtk::FileDialog`
pub fn show_qr_dialog(parent: &gtk::Window, profile_name: &str, uri: &str) {
    let alert_dialog = adw::AlertDialog::new(Some(&gettext("Share Profile")), Some(profile_name));
    alert_dialog.add_response("close", &gettext("Close"));
    alert_dialog.set_close_response("close");

    let content = VrxxQrDialog::new();
    let imp = content.imp();
    imp.lbl_uri.set_text(uri);

    // Рендеринг текстуры QR-кода в оперативной памяти (300x300 px)
    match exporter::generate_qr_texture(uri, 300) {
        Ok(texture) => {
            imp.qr_picture.set_paintable(Some(&texture));
            imp.lbl_error.set_visible(false);
        }
        Err(e) => {
            tracing::error!("Failed to generate QR code texture: {e}");
            imp.lbl_error
                .set_text(&format!("{}: {e}", gettext("Failed to generate QR code")));
            imp.lbl_error.set_visible(true);
            add_parent_toast(
                parent,
                &format!("{}: {e}", gettext("Failed to generate QR code")),
            );
        }
    }

    // Кнопка: Копировать ссылку
    let uri_copy = uri.to_string();
    let parent_copy = parent.clone();
    let display_clipboard = parent.clipboard();
    imp.btn_copy_link.connect_clicked(move |_| {
        display_clipboard.set_text(&uri_copy);
        add_parent_toast(&parent_copy, &gettext("Link copied to clipboard"));
    });

    // Кнопка: Копировать структурированный JSON
    let uri_json = uri.to_string();
    let parent_json = parent.clone();
    let display_clipboard_json = parent.clipboard();
    imp.btn_copy_json.connect_clicked(move |_| {
        if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&uri_json) {
            if let Ok(json_str) = serde_json::to_string_pretty(&parsed) {
                display_clipboard_json.set_text(&json_str);
                add_parent_toast(&parent_json, &gettext("JSON copied to clipboard"));
                return;
            }
        }
        add_parent_toast(&parent_json, &gettext("Failed to generate JSON for key"));
    });

    // Кнопка: Сохранить QR-код в файл (PNG или SVG)
    let uri_save = uri.to_string();
    let profile_name_save = profile_name.to_string();
    let parent_weak = parent.downgrade();

    imp.btn_save_qr.connect_clicked(move |_| {
        let parent_win = match parent_weak.upgrade() {
            Some(w) => w,
            None => return,
        };

        let file_dialog = gtk::FileDialog::builder()
            .title(gettext("Save QR Code As"))
            .build();

        let filter_png = gtk::FileFilter::new();
        filter_png.set_name(Some(&gettext("PNG Image (*.png)")));
        filter_png.add_mime_type("image/png");
        filter_png.add_pattern("*.png");

        let filter_svg = gtk::FileFilter::new();
        filter_svg.set_name(Some(&gettext("SVG Vector (*.svg)")));
        filter_svg.add_mime_type("image/svg+xml");
        filter_svg.add_pattern("*.svg");

        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter_png);
        filters.append(&filter_svg);

        file_dialog.set_filters(Some(&filters));
        file_dialog.set_default_filter(Some(&filter_png));

        let sanitized_name = profile_name_save
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        file_dialog.set_initial_name(Some(&format!("{sanitized_name}_qr.png")));

        let uri_export = uri_save.clone();
        let parent_export = parent_win.clone();

        file_dialog.save(Some(&parent_win), gio::Cancellable::NONE, move |result| {
            let file = match result {
                Ok(f) => f,
                Err(_) => return, // Пользователь отменил диалог
            };

            let path = match file.path() {
                Some(p) => p,
                None => return,
            };

            let is_svg = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("svg"))
                .unwrap_or(false);

            let export_res = if is_svg {
                exporter::generate_qr_svg(&uri_export)
                    .and_then(|svg_str| std::fs::write(&path, svg_str).map_err(Into::into))
            } else {
                exporter::generate_qr_png_bytes(&uri_export, 512, 512)
                    .and_then(|png_bytes| std::fs::write(&path, png_bytes).map_err(Into::into))
            };

            match export_res {
                Ok(_) => {
                    add_parent_toast(&parent_export, &gettext("QR code saved successfully"));
                }
                Err(e) => {
                    tracing::error!("Failed to save QR code to file: {e}");
                    add_parent_toast(
                        &parent_export,
                        &format!("{}: {e}", gettext("Failed to save QR code")),
                    );
                }
            }
        });
    });

    alert_dialog.set_extra_child(Some(&content));
    alert_dialog.present(Some(parent));
}
