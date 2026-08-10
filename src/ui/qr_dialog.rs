/* qr_dialog.rs
 *
 * Copyright 2026 VRXX Authors
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

use adw::prelude::*;
use gettextrs::gettext;
use gtk::gio;

use crate::domain::exporter;

/// Displays an interactive `AdwDialog` presenting a QR code and sharing options for a VPN profile.
///
/// Features:
/// - Renders QR code in memory as `gdk::Texture` without temporary disk files.
/// - White-card framed display ensuring scanning reliability across light/dark desktop themes.
/// - "Copy Link" button to copy profile URI to system clipboard with an `AdwToast` feedback.
/// - "Save QR Code as..." button (`gtk::FileDialog`) exporting to `.png` or `.svg`.
pub fn show_qr_dialog(parent: &gtk::Window, profile_name: &str, uri: &str) {
    let dialog = adw::AlertDialog::builder()
        .heading(gettext("Profile QR Code"))
        .body(profile_name)
        .build();

    let toast_overlay = adw::ToastOverlay::new();

    let content_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .halign(gtk::Align::Center)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(16)
        .margin_end(16)
        .build();

    // Render QR Code Texture in memory
    match exporter::generate_qr_texture(uri, 300) {
        Ok(texture) => {
            let picture = gtk::Picture::builder()
                .paintable(&texture)
                .can_shrink(true)
                .content_fit(gtk::ContentFit::Contain)
                .width_request(260)
                .height_request(260)
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .build();

            // Framed white card for contrast scanability in dark mode
            let qr_card = gtk::Box::builder()
                .margin_top(8)
                .margin_bottom(8)
                .margin_start(8)
                .margin_end(8)
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .build();
            qr_card.add_css_class("card");
            qr_card.set_margin_top(6);
            qr_card.set_margin_bottom(6);
            qr_card.set_margin_start(6);
            qr_card.set_margin_end(6);

            let inner_padding = gtk::Box::builder()
                .margin_top(12)
                .margin_bottom(12)
                .margin_start(12)
                .margin_end(12)
                .build();
            inner_padding.append(&picture);
            qr_card.append(&inner_padding);

            content_box.append(&qr_card);
        }
        Err(e) => {
            tracing::error!("Failed to generate QR texture: {e}");
            let error_label = gtk::Label::builder()
                .label(format!("{}: {e}", gettext("Failed to generate QR code")))
                .wrap(true)
                .build();

            error_label.add_css_class("error");
            content_box.append(&error_label);

            let toast = adw::Toast::new(&format!("{}: {e}", gettext("Failed to generate QR code")));
            toast_overlay.add_toast(toast);
        }
    }

    // URI preview subtitle / label
    let uri_label = gtk::Label::builder()
        .label(uri)
        .selectable(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(36)
        .halign(gtk::Align::Center)
        .build();
    uri_label.add_css_class("dim-label");
    uri_label.add_css_class("caption");
    content_box.append(&uri_label);

    // Action buttons box
    let actions_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Center)
        .margin_top(8)
        .build();

    let btn_copy = gtk::Button::builder()
        .label(gettext("Copy Link"))
        .icon_name("edit-copy-symbolic")
        .build();

    let btn_save = gtk::Button::builder()
        .label(gettext("Save QR Code as..."))
        .icon_name("document-save-as-symbolic")
        .build();

    actions_box.append(&btn_copy);
    actions_box.append(&btn_save);
    content_box.append(&actions_box);

    toast_overlay.set_child(Some(&content_box));

    let clamp = adw::Clamp::builder()
        .maximum_size(420)
        .tightening_threshold(320)
        .child(&toast_overlay)
        .build();

    dialog.set_extra_child(Some(&clamp));
    dialog.add_response("close", &gettext("Close"));
    dialog.set_close_response("close");

    // Button: Copy Link
    let uri_copy = uri.to_string();
    let toast_overlay_copy = toast_overlay.clone();
    let display_clipboard = parent.clipboard();
    btn_copy.connect_clicked(move |_| {
        display_clipboard.set_text(&uri_copy);
        let toast = adw::Toast::new(&gettext("Link copied to clipboard"));
        toast_overlay_copy.add_toast(toast);
    });

    // Button: Save QR Code as...
    let uri_save = uri.to_string();
    let profile_name_save = profile_name.to_string();
    let parent_weak = parent.downgrade();
    let toast_overlay_save = toast_overlay.clone();

    btn_save.connect_clicked(move |_| {
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
        let toast_overlay_export = toast_overlay_save.clone();

        file_dialog.save(Some(&parent_win), gio::Cancellable::NONE, move |result| {
            let file = match result {
                Ok(f) => f,
                Err(_) => return, // User cancelled
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
                    let toast = adw::Toast::new(&gettext("QR code saved successfully"));
                    toast_overlay_export.add_toast(toast);
                }
                Err(e) => {
                    tracing::error!("Failed to save QR code to file: {e}");
                    let toast =
                        adw::Toast::new(&format!("{}: {e}", gettext("Failed to save QR code")));
                    toast_overlay_export.add_toast(toast);
                }
            }
        });
    });

    dialog.present(Some(parent));
}
