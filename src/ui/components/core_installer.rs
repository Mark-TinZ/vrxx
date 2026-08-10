use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

pub fn check_and_prompt(parent: &gtk::Window) {
    if crate::daemon::updater::check_core_exists() {
        return;
    }

    let dialog = adw::Window::builder()
        .modal(true)
        .transient_for(parent)
        .hide_on_close(true)
        .default_width(450)
        .default_height(200)
        .title(gettext("Core Required"))
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    let header_bar = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .show_start_title_buttons(false)
        .build();
    content.append(&header_bar);

    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let label = gtk::Label::builder()
        .label(gettext("VRXX cannot work without the sing-box core. Would you like to download it automatically, or manually select a release archive (.tar.gz / .zip)?"))
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    vbox.append(&label);

    let progress_bar = gtk::ProgressBar::builder().visible(false).build();
    vbox.append(&progress_bar);

    let status_label = gtk::Label::builder().visible(false).wrap(true).build();
    vbox.append(&status_label);

    let btn_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Center)
        .build();

    let btn_manual = gtk::Button::builder()
        .label(gettext("Select Archive"))
        .build();
    let btn_auto = gtk::Button::builder()
        .label(gettext("Download Automatically"))
        .css_classes(["suggested-action"])
        .build();
    let btn_ok = gtk::Button::builder()
        .label(gettext("OK"))
        .visible(false)
        .css_classes(["suggested-action"])
        .build();

    btn_box.append(&btn_manual);
    btn_box.append(&btn_auto);
    btn_box.append(&btn_ok);
    vbox.append(&btn_box);

    content.append(&vbox);
    dialog.set_content(Some(&content));

    btn_ok.connect_clicked(glib::clone!(
        #[weak]
        dialog,
        move |_| {
            dialog.close();
        }
    ));

    btn_manual.connect_clicked(glib::clone!(
        #[weak]
        dialog,
        #[weak]
        btn_manual,
        #[weak]
        btn_auto,
        #[weak]
        progress_bar,
        #[weak]
        status_label,
        #[weak]
        btn_ok,
        move |_| {
            let filter = gtk::FileFilter::new();
            filter.add_pattern("*.tar.gz");
            filter.add_pattern("*.zip");

            let fd = gtk::FileDialog::builder()
                .title(gettext("Select Core Archive"))
                .default_filter(&filter)
                .build();

            fd.open(Some(&dialog), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    let path = file.path().unwrap();
                    btn_manual.set_sensitive(false);
                    btn_auto.set_sensitive(false);
                    progress_bar.set_visible(true);
                    progress_bar.pulse();
                    status_label.set_visible(true);
                    status_label.set_text(&gettext("Extracting and installing..."));

                    glib::spawn_future_local(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            crate::daemon::updater::install_from_archive(&path)
                        })
                        .await
                        .unwrap();

                        match result {
                            Ok(_) => {
                                status_label.set_text(&gettext("Core installed successfully."));
                                btn_manual.set_visible(false);
                                btn_auto.set_visible(false);
                                progress_bar.set_visible(false);
                                btn_ok.set_visible(true);
                            }
                            Err(e) => {
                                status_label.set_text(&format!("{}: {}", gettext("Error"), e));
                                btn_manual.set_sensitive(true);
                                btn_auto.set_sensitive(true);
                                progress_bar.set_visible(false);
                            }
                        }
                    });
                }
            });
        }
    ));

    btn_auto.connect_clicked(glib::clone!(
        #[weak]
        btn_manual,
        #[weak]
        btn_auto,
        #[weak]
        progress_bar,
        #[weak]
        status_label,
        #[weak]
        btn_ok,
        move |_| {
            btn_manual.set_sensitive(false);
            btn_auto.set_sensitive(false);
            progress_bar.set_visible(true);
            progress_bar.set_fraction(0.0);
            status_label.set_visible(true);
            status_label.set_text(&gettext("Downloading..."));

            let (tx, rx) = async_channel::unbounded();

            glib::spawn_future_local(glib::clone!(
                #[weak]
                progress_bar,
                async move {
                    while let Ok(fraction) = rx.recv().await {
                        progress_bar.set_fraction(fraction);
                    }
                }
            ));

            glib::spawn_future_local(async move {
                let result = crate::daemon::updater::download_core(Some(tx)).await;
                match result {
                    Ok(_) => {
                        status_label
                            .set_text(&gettext("Core downloaded and installed successfully."));
                        btn_manual.set_visible(false);
                        btn_auto.set_visible(false);
                        progress_bar.set_fraction(1.0);
                        btn_ok.set_visible(true);
                    }
                    Err(e) => {
                        status_label.set_text(&format!("{}: {}", gettext("Error"), e));
                        btn_manual.set_sensitive(true);
                        btn_auto.set_sensitive(true);
                    }
                }
            });
        }
    ));

    dialog.present();
}
