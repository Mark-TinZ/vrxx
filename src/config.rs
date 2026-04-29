#[cfg(not(meson_build))]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
#[cfg(not(meson_build))]
pub const GETTEXT_PACKAGE: &str = "vrxx";
#[cfg(not(meson_build))]
pub const LOCALEDIR: &str = "locale";

#[cfg(meson_build)]
include!(concat!(env!("OUT_DIR"), "/config_fallback.rs"));
