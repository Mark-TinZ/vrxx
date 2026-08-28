/* build.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Сборочный скрипт Cargo (Build Script)
//!
//! Отвечает за:
//! - Компиляцию XML-ресурсов GResource (`glib-compile-resources`) в бинарный файл `vrxx.gresource`
//! - Компиляцию файлов локализации `.po` (`msgfmt`) в бинарные каталоги `.mo`
//! - Настройку интеграции с Meson (`VRXX_CONFIG_RS_PATH` и флаг `meson_build`)

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(meson_build)");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // 1. Компиляция GResource для запуска через cargo run
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
            .expect("Не удалось скомпилировать GResource ресурсы");
        println!("cargo:rerun-if-changed=src/vrxx.gresource.xml");
        println!("cargo:rerun-if-changed=src/window.ui");
        println!("cargo:rerun-if-changed=src/shortcuts-dialog.ui");
        println!("cargo:rerun-if-changed=src/ui/pages/vpn_page.ui");
        println!("cargo:rerun-if-changed=src/ui/pages/proxy_page.ui");
        println!("cargo:rerun-if-changed=src/ui/pages/routing_page.ui");
        println!("cargo:rerun-if-changed=src/ui/pages/settings_page.ui");
        println!("cargo:rerun-if-changed=src/ui/components/vpn_key_row.ui");
        println!("cargo:rerun-if-changed=src/ui/components/routing_rule_row.ui");
        println!("cargo:rerun-if-changed=src/ui/components/theme_switcher.ui");
        println!("cargo:rerun-if-changed=src/ui/components/log_window.ui");
        println!("cargo:rerun-if-changed=src/ui/qr_dialog.ui");
        println!("cargo:rerun-if-changed=src/ui/rule_dialog.ui");
        println!("cargo:rerun-if-changed=src/ui/menus.ui");
        println!("cargo:rerun-if-changed=src/style.css");
    }

    // 2. Компиляция файлов переводов gettext (.po -> .mo) для cargo run
    let po_dir = PathBuf::from("po");
    if po_dir.exists() {
        println!("cargo:rerun-if-changed=po");
        println!("cargo:rerun-if-changed=po/vrxx.pot");
        println!("cargo:rerun-if-changed=po/ru.po");
        println!("cargo:rerun-if-changed=po/en.po");
        for entry in fs::read_dir(&po_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "po") {
                if let Some(lang) = path.file_stem().and_then(|s| s.to_str()) {
                    let locale_dir = PathBuf::from(format!("locale/{lang}/LC_MESSAGES"));
                    let _ = fs::create_dir_all(&locale_dir);
                    let mo_path = locale_dir.join("vrxx.mo");
                    let output = Command::new("msgfmt")
                        .arg("-c")
                        .arg("-o")
                        .arg(&mo_path)
                        .arg(&path)
                        .output()
                        .expect("Не удалось запустить утилиту msgfmt");

                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        panic!(
                            "Ошибка компиляции файла перевода {} с помощью msgfmt:\n{}",
                            path.display(),
                            stderr
                        );
                    }
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            }
        }
    }

    let fallback_path = out_dir.join("config_fallback.rs");

    if let Ok(config_path) = env::var("VRXX_CONFIG_RS_PATH") {
        let content =
            fs::read_to_string(&config_path).expect("Не удалось прочитать config.rs из meson");
        fs::write(&fallback_path, content).expect("Не удалось записать fallback config.rs");
        println!("cargo:rustc-cfg=meson_build");
    } else {
        fs::write(&fallback_path, "").expect("Не удалось записать пустой fallback config.rs");
    }
}
