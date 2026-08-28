/* mod.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Подсистема криптографической защиты данных (Crypto)
//!
//! Отвечает за:
//! - Генерацию и деривацию аппаратных ключей шифрования на базе системного `machine-id` ([`keystore`])
//! - Аутентифицированное шифрование и дешифрование профилей VPN (AES-256-GCM AEAD)
//! - Валидацию целостности зашифрованных контейнеров `data.dat`

pub mod keystore;
