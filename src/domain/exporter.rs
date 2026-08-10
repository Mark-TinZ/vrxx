/* exporter.rs
 *
 * Copyright 2026 VRXX Authors
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

use anyhow::{anyhow, Context, Result};
use gtk::glib;
use qrcode::render::svg;
use qrcode::QrCode;
use std::io::Cursor;

/// Generates an SVG string representation of a QR code for the given URI or string.
pub fn generate_qr_svg(content: &str) -> Result<String> {
    if content.trim().is_empty() {
        return Err(anyhow!("Cannot generate QR code for empty content"));
    }

    let code = QrCode::new(content.as_bytes())
        .with_context(|| format!("Failed to encode QR code for content: '{content}'"))?;

    let svg_string = code
        .render::<svg::Color>()
        .min_dimensions(300, 300)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();

    Ok(svg_string)
}

/// Generates PNG-encoded image bytes of a QR code for the given URI or string.
pub fn generate_qr_png_bytes(content: &str, width: u32, height: u32) -> Result<Vec<u8>> {
    if content.trim().is_empty() {
        return Err(anyhow!("Cannot generate QR code for empty content"));
    }

    let code = QrCode::new(content.as_bytes())
        .with_context(|| format!("Failed to encode QR code for content: '{content}'"))?;

    let img_buffer = code
        .render::<image::Rgb<u8>>()
        .min_dimensions(width, height)
        .dark_color(image::Rgb([0, 0, 0]))
        .light_color(image::Rgb([255, 255, 255]))
        .build();

    let mut png_bytes = Vec::new();
    img_buffer
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .context("Failed to encode QR code image buffer to PNG format")?;

    Ok(png_bytes)
}

/// Renders a QR code safely in memory into a `gdk::Texture` without writing temporary files to disk.
pub fn generate_qr_texture(content: &str, size: u32) -> Result<gdk::Texture> {
    let png_bytes = generate_qr_png_bytes(content, size, size)?;
    let bytes = glib::Bytes::from(&png_bytes);
    let texture = gdk::Texture::from_bytes(&bytes)
        .map_err(|e| anyhow!("Failed to create gdk::Texture from PNG bytes: {e}"))?;

    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_qr_svg_valid() {
        let uri = "vless://user@127.0.0.1:443?security=reality#TestServer";
        let svg = generate_qr_svg(uri).expect("SVG generation should succeed");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_generate_qr_png_bytes_valid() {
        let uri = "vmess://eyJhZGQiOiIxMjcuMC4wLjEiLCJwb3J0Ijo0NDN9";
        let bytes =
            generate_qr_png_bytes(uri, 256, 256).expect("PNG bytes generation should succeed");
        assert!(!bytes.is_empty());
        // Check PNG header (0x89 'P' 'N' 'G')
        assert_eq!(&bytes[0..4], &[137, 80, 78, 71]);
    }

    #[test]
    fn test_generate_qr_empty_error() {
        assert!(generate_qr_svg("").is_err());
        assert!(generate_qr_png_bytes("", 200, 200).is_err());
    }
}
