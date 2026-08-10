#[cfg(test)]
mod proxy_testing {
    use crate::backend::{
        detect_desktop_environment, is_gnome_proxy_schema_available, set_process_proxy_env,
        CoreBackend, DesktopEnvironment, SystemProxyResult,
    };

    #[test]
    fn test_desktop_environment_detection() {
        std::env::set_var("XDG_CURRENT_DESKTOP", "KDE");
        assert_eq!(detect_desktop_environment(), DesktopEnvironment::Kde);

        std::env::set_var("XDG_CURRENT_DESKTOP", "ubuntu:GNOME");
        assert_eq!(detect_desktop_environment(), DesktopEnvironment::Gnome);

        std::env::set_var("XDG_CURRENT_DESKTOP", "XFCE");
        assert_eq!(detect_desktop_environment(), DesktopEnvironment::Xfce);

        std::env::set_var("XDG_CURRENT_DESKTOP", "sway");
        assert_eq!(detect_desktop_environment(), DesktopEnvironment::Sway);

        std::env::remove_var("XDG_CURRENT_DESKTOP");
        std::env::remove_var("XDG_SESSION_DESKTOP");
        assert_eq!(
            detect_desktop_environment(),
            DesktopEnvironment::Other("Unknown".to_string())
        );
    }

    #[test]
    fn test_process_proxy_env() {
        set_process_proxy_env(2080, true);
        assert_eq!(
            std::env::var("HTTP_PROXY").unwrap(),
            "http://127.0.0.1:2080"
        );
        assert_eq!(
            std::env::var("HTTPS_PROXY").unwrap(),
            "http://127.0.0.1:2080"
        );

        set_process_proxy_env(2080, false);
        assert!(std::env::var("HTTP_PROXY").is_err());
        assert!(std::env::var("HTTPS_PROXY").is_err());
    }

    #[test]
    fn test_proxy_toggle_panic_free() {
        if gtk::init().is_err() {
            return;
        }

        let backend = CoreBackend::new();
        let schema_available = is_gnome_proxy_schema_available();

        let res_enable = backend.update_system_proxy(true);
        let res_disable = backend.update_system_proxy(false);

        if schema_available {
            assert_eq!(res_enable, SystemProxyResult::Success);
            assert_eq!(res_disable, SystemProxyResult::Success);
        } else {
            match res_enable {
                SystemProxyResult::SchemaUnavailable { .. } => {}
                other => panic!("Expected SchemaUnavailable, got {:?}", other),
            }
            match res_disable {
                SystemProxyResult::SchemaUnavailable { .. } => {}
                other => panic!("Expected SchemaUnavailable, got {:?}", other),
            }
        }
    }
}
