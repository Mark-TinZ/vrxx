#[cfg(not(meson_build))]
pub const VERSION: &str = "0.1.5";
#[cfg(not(meson_build))]
pub const GETTEXT_PACKAGE: &str = "vrxx";
#[cfg(not(meson_build))]
pub const LOCALEDIR: &str = "/usr/local/share/locale";

#[cfg(meson_build)]
include!(concat!(env!("OUT_DIR"), "/config_fallback.rs"));