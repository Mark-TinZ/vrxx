/* subscription.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Загрузка и парсинг VPN-подписок (Subscription Subsystem)
//!
//! Модуль отвечает за:
//! - Асинхронную загрузку содержимого подписки по HTTP/HTTPS URL
//! - Декодирование содержимого подписки (поддержка Base64 и обычного текста)
//! - Извлечение и валидацию индивидуальных VPN-ссылок через [`crate::domain::key_parser::parse_vpn_key`]
//! - Формирование результирующего списка профилей [`crate::domain::key_parser::ParsedKey`]

use base64::{engine::general_purpose, Engine as _};
use std::time::Duration;

use crate::domain::key_parser::{parse_vpn_key, ParsedKey};

/// Парсит текстовое содержимое подписки (Base64 или список URI ссылок, разделенных переводами строк).
///
/// Алгоритм:
/// 1. Сначала проверяется попытка декодировать весь текст как Base64 (стандартный или url-safe).
/// 2. Если декодирование успешно и результат содержит разделители ссылок `://`, используется декодированный текст.
/// 3. Иначе анализируется исходный текст построчно.
/// 4. Строки-комментарии (начинающиеся с `#` или `//`) и пустые строки игнорируются.
/// 5. Каждая валидная строка передается в [`parse_vpn_key`].
pub fn parse_subscription_text(raw_content: &str) -> Vec<ParsedKey> {
    let trimmed = raw_content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Попытка декодировать Base64
    let clean_b64: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    let decoded_text = general_purpose::STANDARD
        .decode(&clean_b64)
        .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(&clean_b64))
        .or_else(|_| general_purpose::URL_SAFE.decode(&clean_b64))
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(&clean_b64))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok());

    let content_to_parse = if let Some(ref text) = decoded_text {
        if text.contains("://") {
            text.as_str()
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let mut keys = Vec::new();

    for line in content_to_parse.lines() {
        let line_trimmed = line.trim();
        if line_trimmed.is_empty()
            || line_trimmed.starts_with('#')
            || line_trimmed.starts_with("//")
        {
            continue;
        }

        match parse_vpn_key(line_trimmed) {
            Ok(parsed) => {
                keys.push(parsed);
            }
            Err(e) => {
                tracing::debug!("Пропущена невалидная строка в подписке: '{line_trimmed}': {e}");
            }
        }
    }

    keys
}

/// Асинхронно скачивает содержимое подписки по указанному URL и возвращает распарсенные ключи.
pub async fn fetch_subscription(url: &str) -> anyhow::Result<Vec<ParsedKey>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("vrxx/26.1 (sing-box; Linux)")
        .build()
        .map_err(|e| anyhow::anyhow!("Не удалось инициализировать HTTP-клиент: {e}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Сетевая ошибка при загрузке подписки: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("Сервер подписки вернул статус ошибки: {status}");
    }

    let text = response
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("Ошибка чтения ответа сервера подписки: {e}"))?;

    let keys = parse_subscription_text(&text);
    if keys.is_empty() {
        anyhow::bail!("В ответе подписки не найдено ни одного поддерживаемого VPN-ключа");
    }

    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plain_subscription() {
        let content = r#"
# Тестовая подписка
vless://b831381d-6324-4d53-ad4f-8cda48b30811@example.com:443?type=tcp&security=reality&pbk=1111111111111111111111111111111111111111111=&fp=chrome&sni=example.com#Server1
// Второй сервер
vless://c931381d-6324-4d53-ad4f-8cda48b30812@example.org:443?type=tcp&security=reality&pbk=2222222222222222222222222222222222222222222=&fp=chrome&sni=example.org#Server2
"#;
        let keys = parse_subscription_text(content);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].name, "Server1");
        assert_eq!(keys[1].name, "Server2");
    }

    #[test]
    fn test_parse_base64_subscription() {
        let content = "vless://b831381d-6324-4d53-ad4f-8cda48b30811@example.com:443?type=tcp&security=reality&pbk=1111111111111111111111111111111111111111111=&fp=chrome&sni=example.com#B64Server\n";
        let b64 = general_purpose::STANDARD.encode(content);
        let keys = parse_subscription_text(&b64);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "B64Server");
    }
}
