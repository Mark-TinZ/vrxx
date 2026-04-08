#[cfg(test)]
mod tests {
    use gtk::{gio, glib};

    // Need to initialize GTK before creating widgets.
    fn init_gtk() {
        let _ = gtk::init();
        
        // Load resources for templates
        let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
        if let Ok(res) = gio::Resource::from_data(&glib::Bytes::from(res_data)) {
            gio::resources_register(&res);
        }
    }

    #[test]
    #[ignore = "Requires main thread for GTK initialization"]
    fn test_ui_components_init() {
        init_gtk();

        // Testing that templates are correctly bound and can be instantiated without panic
        let _log_window = crate::ui::components::log_window::VrxxLogWindow::new();
        let _settings_page = crate::ui::pages::VrxxSettingsPage::new();
        let _vpn_page = crate::ui::pages::VrxxVpnPage::new();
        let _whitelist_page = crate::ui::pages::VrxxWhitelistPage::new();
        let _proxy_page = crate::ui::pages::VrxxProxyPage::new();
    }
}