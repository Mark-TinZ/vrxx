#[cfg(test)]
mod tests {
    use gtk::{gio, glib};
    use gtk::subclass::prelude::*;
    use gtk::prelude::*;
    use glib::subclass::types::ObjectSubclassIsExt;
    use gtk::prelude::TextViewExt;
    use gtk::prelude::TextBufferExt;

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

    #[test]
    #[ignore = "Requires main thread for GTK initialization"]
    fn test_log_filtering_integration() {
        use adw::subclass::prelude::*;
        use gtk::prelude::*;
        // --- Раздел: Глобальное тестирование логов ---
        init_gtk();
        let log_window = crate::ui::components::log_window::VrxxLogWindow::new();
        let buffer = log_window.imp().text_view.buffer();

        // 1. Тест фильтра "Все логи"
        log_window.imp().dropdown_filter.set_selected(0);
        log_window.append_log("info", "[Vrxx] App started");
        log_window.append_log("info", "Core accepted connection");

        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false);
        assert!(text.contains("[Vrxx] App started"));
        assert!(text.contains("Core accepted connection"));

        buffer.set_text("");

        // 2. Тест фильтра "Логи приложения"
        log_window.imp().dropdown_filter.set_selected(2);
        log_window.append_log("info", "[Vrxx] Important event");
        log_window.append_log("info", "Random core message");

        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false);
        assert!(text.contains("[Vrxx] Important event"));
        assert!(!text.contains("Random core message"));
    }
}