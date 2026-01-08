use gtk::{glib, prelude::*};
use adw::subclass::prelude::*;

mod imp {
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
        pub traffic_down: RefCell<String>,
        #[property(get, set)]
        pub traffic_up: RefCell<String>,
        #[property(get, set)]
        pub time_connected: RefCell<String>,
        #[property(get, set)]
        pub ping: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VpnKeyObject {
        const NAME: &'static str = "VpnKeyObject";
        type Type = super::VpnKeyObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for VpnKeyObject {}
}

glib::wrapper! {
    pub struct VpnKeyObject(ObjectSubclass<imp::VpnKeyObject>);
}

impl VpnKeyObject {
    pub fn new(name: &str, protocol: &str, active: bool) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("protocol", protocol)
            .property("is-active", active)
            .property("traffic-down", "0.0 MB")
            .property("traffic-up", "0.0 MB")
            .property("time-connected", "00:00:00")
            .property("ping", "0 ms")
            .build()
    }
}

