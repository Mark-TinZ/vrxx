use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(meson_build)");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    // Compile GResource for cargo run
    let res_path = PathBuf::from("src/vrxx.gresource.xml");
    if res_path.exists() {
        let compiled_path = out_dir.join("vrxx.gresource");
        Command::new("glib-compile-resources")
            .arg("--target")
            .arg(&compiled_path)
            .arg("--sourcedir")
            .arg("src")
            .arg(&res_path)
            .status()
            .expect("Failed to compile resources");
        println!("cargo:rerun-if-changed=src/vrxx.gresource.xml");
        println!("cargo:rerun-if-changed=src/window.ui");
        println!("cargo:rerun-if-changed=src/ui/pages/vpn_page.ui");
        println!("cargo:rerun-if-changed=src/ui/pages/proxy_page.ui");
        println!("cargo:rerun-if-changed=src/ui/pages/whitelist_page.ui");
        println!("cargo:rerun-if-changed=src/ui/pages/settings_page.ui");
        println!("cargo:rerun-if-changed=src/ui/components/vpn_key_row.ui");
        println!("cargo:rerun-if-changed=src/ui/components/theme_switcher.ui");
        println!("cargo:rerun-if-changed=src/ui/menus.ui");
    }

    // Compile PO files for cargo run
    let po_dir = PathBuf::from("po");
    if po_dir.exists() {
        for entry in fs::read_dir(&po_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "po") {
                if let Some(lang) = path.file_stem().and_then(|s| s.to_str()) {
                    let locale_dir = PathBuf::from(format!("locale/{lang}/LC_MESSAGES"));
                    let _ = fs::create_dir_all(&locale_dir);
                    let mo_path = locale_dir.join("vrxx.mo");
                    let _ = Command::new("msgfmt")
                        .arg("-o")
                        .arg(&mo_path)
                        .arg(&path)
                        .status();
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            }
        }
    }

    let fallback_path = out_dir.join("config_fallback.rs");

    if let Ok(config_path) = env::var("VRXX_CONFIG_RS_PATH") {
        let content = fs::read_to_string(&config_path).expect("Failed to read config.rs from meson");
        fs::write(&fallback_path, content).expect("Failed to write fallback config.rs");
        println!("cargo:rustc-cfg=meson_build");
    } else {
        fs::write(&fallback_path, "").expect("Failed to write empty fallback config.rs");
    }
}