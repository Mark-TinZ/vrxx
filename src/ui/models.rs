use gtk::{glib, prelude::*};
use adw::subclass::prelude::*;

// === VPN KEY OBJECT ===
mod imp_vpn {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::VpnKeyObject)]
    pub struct VpnKeyObject {
        #[property(get, set)]
        pub name: RefCell<String>,
        #[property(get, set)]
        pub protocol: RefCell<String>,
        #[property(get, set)]
        pub is_active: RefCell<bool>,
        #[property(get, set)]
        pub is_loading: RefCell<bool>,
        #[property(get, set)]
        pub traffic_down: RefCell<String>,
        #[property(get, set)]
        pub traffic_up: RefCell<String>,
        #[property(get, set)]
        pub time_connected: RefCell<String>,
        #[property(get, set)]
        pub ping: RefCell<String>,
        #[property(get, set)]
        pub server_info: RefCell<String>,
        #[property(get, set)]
        pub location: RefCell<String>,
        #[property(get, set)]
        pub timezone: RefCell<String>,
        #[property(get, set)]
        pub hide_ip: RefCell<bool>,
        #[property(get, set)]
        pub url: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VpnKeyObject {
        const NAME: &'static str = "VpnKeyObject";
        type Type = super::VpnKeyObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for VpnKeyObject {}
}

// === DOMAIN OBJECT ===
mod imp_domain {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::DomainObject)]
    pub struct DomainObject {
        #[property(get, set)]
        pub domain: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DomainObject {
        const NAME: &'static str = "DomainObject";
        type Type = super::DomainObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for DomainObject {}
}

// === WRAPPERS ===

glib::wrapper! {
    pub struct VpnKeyObject(ObjectSubclass<imp_vpn::VpnKeyObject>);
}

impl VpnKeyObject {
    pub fn new(name: &str, protocol: &str, active: bool, url: &str) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("protocol", protocol)
            .property("is-active", active)
            .property("traffic-down", "0.0 MB")
            .property("traffic-up", "0.0 MB")
            .property("time-connected", "00:00:00")
            .property("ping", "0 ms")
            .property("server-info", "0.0.0.0")
            .property("location", "Unknown")
            .property("timezone", "UTC")
            .property("hide-ip", false)
            .property("url", url)
            .build()
    }
}

glib::wrapper! {
    pub struct DomainObject(ObjectSubclass<imp_domain::DomainObject>);
}

impl DomainObject {
    pub fn new(domain: &str) -> Self {
        glib::Object::builder()
            .property("domain", domain)
            .build()
    }
}

