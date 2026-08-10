/* import_dialog.rs
 *
 * Copyright 2026 Unknown
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::domain::key_parser::ParsedKey;

/// Displays an interactive AdwDialog for importing a VPN profile from a URL scheme.
///
/// Features:
/// - Displays parsed key details (Protocol, Host, Port, Name, Security parameters)
/// - Performs an asynchronous, non-blocking TCP latency check before importing
/// - Allows editing the profile configuration name
/// - Offers "Import Profile", "Import and Connect", and "Cancel" buttons
pub fn show_import_dialog<F1, F2>(
    parent: &gtk::Window,
    parsed: ParsedKey,
    on_import_only: F1,
    on_import_and_connect: F2,
) where
    F1: Fn(ParsedKey) + 'static,
    F2: Fn(ParsedKey) + 'static,
{
    let dialog = adw::AlertDialog::builder()
        .heading(gettext("Import VPN Key"))
        .body(gettext(
            "A VPN profile configuration link was received. Would you like to import it?",
        ))
        .build();

    // General Group
    let group_general = adw::PreferencesGroup::builder()
        .title(gettext("General"))
        .build();

    let name_row = adw::EntryRow::builder()
        .title(gettext("Profile Name"))
        .text(&parsed.name)
        .build();
    group_general.add(&name_row);

    let protocol_row = adw::ActionRow::builder()
        .title(gettext("Protocol"))
        .subtitle(&parsed.protocol)
        .build();
    group_general.add(&protocol_row);

    // Connection Details Group
    let group_connection = adw::PreferencesGroup::builder()
        .title(gettext("Server Connection"))
        .build();

    let host_row = adw::ActionRow::builder()
        .title(gettext("Host"))
        .subtitle(&parsed.host)
        .build();
    group_connection.add(&host_row);

    let port_row = adw::ActionRow::builder()
        .title(gettext("Port"))
        .subtitle(parsed.port.to_string())
        .build();
    group_connection.add(&port_row);

    // Security & Parameters Group
    let group_security = adw::PreferencesGroup::builder()
        .title(gettext("Security & Parameters"))
        .build();

    let mut has_security_params = false;

    if let Some(sec) = parsed
        .query_params
        .get("security")
        .or_else(|| parsed.query_params.get("type"))
    {
        let sec_row = adw::ActionRow::builder()
            .title(gettext("Security Mode"))
            .subtitle(sec)
            .build();
        group_security.add(&sec_row);
        has_security_params = true;
    }

    if let Some(sni) = parsed.query_params.get("sni") {
        let sni_row = adw::ActionRow::builder()
            .title(gettext("SNI"))
            .subtitle(sni)
            .build();
        group_security.add(&sni_row);
        has_security_params = true;
    }

    if let Some(fp) = parsed.query_params.get("fp") {
        let fp_row = adw::ActionRow::builder()
            .title(gettext("Fingerprint"))
            .subtitle(fp)
            .build();
        group_security.add(&fp_row);
        has_security_params = true;
    }

    if let Some(flow) = parsed.query_params.get("flow") {
        if !flow.is_empty() {
            let flow_row = adw::ActionRow::builder()
                .title(gettext("Flow"))
                .subtitle(flow)
                .build();
            group_security.add(&flow_row);
            has_security_params = true;
        }
    }

    if let Some(pbk) = parsed.query_params.get("pbk") {
        let pbk_row = adw::ActionRow::builder()
            .title(gettext("Public Key"))
            .subtitle(pbk)
            .build();
        group_security.add(&pbk_row);
        has_security_params = true;
    }

    // Latency pre-check Row
    let latency_spinner = gtk::Spinner::builder()
        .spinning(true)
        .halign(gtk::Align::End)
        .valign(gtk::Align::Center)
        .build();

    let latency_row = adw::ActionRow::builder()
        .title(gettext("Latency Check"))
        .subtitle(gettext("Measuring ping..."))
        .build();
    latency_row.add_suffix(&latency_spinner);
    group_connection.add(&latency_row);

    // Build overall dialog box layout
    let pref_page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    pref_page.append(&group_general);
    pref_page.append(&group_connection);
    if has_security_params {
        pref_page.append(&group_security);
    }

    let clamp = adw::Clamp::builder()
        .maximum_size(460)
        .tightening_threshold(300)
        .child(&pref_page)
        .build();
    clamp.set_margin_top(12);
    clamp.set_margin_bottom(12);
    clamp.set_margin_start(12);
    clamp.set_margin_end(12);

    dialog.set_extra_child(Some(&clamp));

    // Response actions
    dialog.add_response("cancel", &gettext("Cancel"));
    dialog.add_response("import", &gettext("Import Profile"));
    dialog.add_response("connect", &gettext("Import and Connect"));

    dialog.set_response_appearance("connect", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("connect"));
    dialog.set_close_response("cancel");

    // Asynchronous Non-blocking Ping Pre-check
    let target_host = parsed.host.clone();
    let target_port = parsed.port;

    let (sender, receiver) = async_channel::unbounded::<(bool, u128)>();

    let latency_row_ui = latency_row.clone();
    let latency_spinner_ui = latency_spinner.clone();

    glib::spawn_future_local(async move {
        if let Ok((success, ms)) = receiver.recv().await {
            latency_spinner_ui.set_spinning(false);
            latency_spinner_ui.set_visible(false);
            if success {
                latency_row_ui.set_subtitle(&format!("{ms} ms"));
            } else {
                latency_row_ui.set_subtitle(&gettext("Connection timeout"));
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

    // Response Signal Handler
    let parsed_clone = parsed.clone();
    let name_row_clone = name_row.clone();

    dialog.connect_response(None, move |_, response| {
        let mut updated_parsed = parsed_clone.clone();
        let name_text = name_row_clone.text().to_string();
        if !name_text.trim().is_empty() {
            updated_parsed.name = name_text;
        }

        match response {
            "connect" => {
                on_import_and_connect(updated_parsed);
            }
            "import" => {
                on_import_only(updated_parsed);
            }
            _ => {}
        }
    });

    dialog.present(Some(parent));
    name_row.grab_focus();
}
