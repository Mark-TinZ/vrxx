#[cfg(test)]
mod tests {
    use gtk::gio::Settings;
    use gtk::prelude::SettingsExt;
    use crate::backend::CoreBackend;

    #[test]
    fn test_proxy_toggle() {
        gtk::init().unwrap(); // Initialize GTK for GSettings
        
        let backend = CoreBackend::new();
        
        // 1. Set to enabled
        backend.update_system_proxy(true);
        let settings = Settings::new("org.gnome.system.proxy");
        let mode: String = settings.string("mode").into();
        assert_eq!(mode, "manual");

        // 2. Set to disabled
        backend.update_system_proxy(false);
        let mode: String = settings.string("mode").into();
        assert_eq!(mode, "none");
    }
}