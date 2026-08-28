/* exporter.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Экспорт и генерация QR-кодов (QR Code Exporter)
//!
//! Модуль отвечает за:
//! - Генерацию векторных SVG строк QR-кодов для масштабируемого отображения и сохранения
//! - Генерацию растровых PNG байтов с кастомным разрешением в оперативной памяти
//! - Создание текстур GTK/GDK (`gdk::Texture`) напрямую из буфера памяти без создания временных файлов на диске

use anyhow::{anyhow, Context, Result};
use gtk::glib;
use qrcode::render::svg;
use qrcode::QrCode;
use std::io::Cursor;

/// Генерирует векторную SVG-строку с QR-кодом для переданного содержимого (URI или строки).
pub fn generate_qr_svg(content: &str) -> Result<String> {
    if content.trim().is_empty() {
        return Err(anyhow!(
            "Невозможно сгенерировать QR-код для пустого содержимого"
        ));
    }

    let code = QrCode::new(content.as_bytes())
        .with_context(|| format!("Не удалось закодировать QR-код для: '{content}'"))?;

    let svg_string = code
        .render::<svg::Color>()
        .min_dimensions(300, 300)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();

    Ok(svg_string)
}

/// Генерирует массив байтов изображения в формате PNG для QR-кода заданного разрешения (width x height).
pub fn generate_qr_png_bytes(content: &str, width: u32, height: u32) -> Result<Vec<u8>> {
    if content.trim().is_empty() {
        return Err(anyhow!(
            "Невозможно сгенерировать QR-код для пустого содержимого"
        ));
    }

    let code = QrCode::new(content.as_bytes())
        .with_context(|| format!("Не удалось закодировать QR-код для: '{content}'"))?;

    let img_buffer = code
        .render::<image::Rgb<u8>>()
        .min_dimensions(width, height)
        .dark_color(image::Rgb([0, 0, 0]))
        .light_color(image::Rgb([255, 255, 255]))
        .build();

    let mut png_bytes = Vec::new();
    img_buffer
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .context("Не удалось закодировать буфер изображения QR-кода в формат PNG")?;

    Ok(png_bytes)
}

/// Создает объект `gdk::Texture` напрямую в оперативной памяти без записи временных файлов на диск.
pub fn generate_qr_texture(content: &str, size: u32) -> Result<gdk::Texture> {
    let png_bytes = generate_qr_png_bytes(content, size, size)?;
    let bytes = glib::Bytes::from(&png_bytes);
    let texture = gdk::Texture::from_bytes(&bytes)
        .map_err(|e| anyhow!("Не удалось создать gdk::Texture из байтов PNG: {e}"))?;

    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_qr_svg_valid() {
        let uri = "vless://user@127.0.0.1:443?security=reality#TestServer";
        let svg = generate_qr_svg(uri).expect("Генерация SVG должна завершиться успешно");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_generate_qr_png_bytes_valid() {
        let uri = "vmess://eyJhZGQiOiIxMjcuMC4wLjEiLCJwb3J0Ijo0NDN9";
        let bytes = generate_qr_png_bytes(uri, 256, 256)
            .expect("Генерация байтов PNG должна завершиться успешно");
        assert!(!bytes.is_empty());
        // Проверка заголовка PNG (сигнатура 0x89 'P' 'N' 'G')
        assert_eq!(&bytes[0..4], &[137, 80, 78, 71]);
    }

    #[test]
    fn test_generate_qr_empty_error() {
        assert!(generate_qr_svg("").is_err());
        assert!(generate_qr_png_bytes("", 200, 200).is_err());
    }
}
