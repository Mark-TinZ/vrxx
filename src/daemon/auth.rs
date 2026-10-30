/* auth.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Аутентификация вызывающей стороны через SO_PEERCRED и systemd-logind
//!
//! Модуль обеспечивает аутентификацию входящих IPC-подключений по Unix Domain Socket:
//! - Извлечение учетных данных процесса ([`CallerIdentity`]) через `SO_PEERCRED` (UID, GID, PID)
//! - Валидация прав вызывающего ([`is_caller_authorized`]):
//!   * UID 0 (root): разрешен (системный администратор)
//!   * UID текущего процесса (getuid): разрешен (для тестов и собственного процесса)
//!   * UID < 1000: запрещен (системные учетные записи сервисов)
//!   * UID >= 1000: проверка активной сессии через systemd-logind по D-Bus (zbus)
//!   * Fallback: проверка наличия каталога PAM-сессии `/run/user/<uid>`

use axum::extract::connect_info::Connected;
use axum::serve::IncomingStream;
use tokio::net::UnixListener;
use zbus::proxy;

/// Учетные данные вызывающего процесса, извлеченные из сокета со стороны ядра (SO_PEERCRED).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallerIdentity {
    pub is_authenticated: bool,
    pub uid: u32,
    pub gid: u32,
    pub pid: Option<i32>,
}

#[cfg(unix)]
impl Connected<IncomingStream<'_, UnixListener>> for CallerIdentity {
    fn connect_info(stream: IncomingStream<'_, UnixListener>) -> Self {
        match stream.io().peer_cred() {
            Ok(ucred) => CallerIdentity {
                is_authenticated: true,
                uid: ucred.uid(),
                gid: ucred.gid(),
                pid: ucred.pid(),
            },
            Err(e) => {
                tracing::warn!("Failed to extract SO_PEERCRED from UnixStream: {e}");
                CallerIdentity {
                    is_authenticated: false,
                    uid: u32::MAX,
                    gid: u32::MAX,
                    pid: None,
                }
            }
        }
    }
}

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1Manager {
    fn get_user(&self, uid: u32) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.login1.User",
    default_service = "org.freedesktop.login1"
)]
trait Login1User {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<String>;
}

/// Проверяет, авторизован ли вызывающий процесс для управления демоном.
pub async fn is_caller_authorized(identity: &CallerIdentity) -> bool {
    // 0. Immediately reject unauthenticated connections or invalid / sentinel UIDs
    if !identity.is_authenticated
        || identity.uid == u32::MAX
        || identity.uid >= 65534
        || identity.uid > 2_000_000_000
    {
        tracing::warn!(
            "Rejecting unauthenticated or invalid caller: is_authenticated={}, uid={}",
            identity.is_authenticated,
            identity.uid
        );
        return false;
    }

    // 1. Root UID 0 is always authorized
    if identity.uid == 0 {
        return true;
    }

    // 2. Reject system service accounts (UID < 1000).
    // MUST be checked before getuid() so system accounts running as the daemon cannot bypass authorization.
    if identity.uid < 1000 {
        tracing::warn!("Rejecting system user UID {}", identity.uid);
        return false;
    }

    // 3. Self / test runner authorized
    if identity.uid == nix::unistd::getuid().as_raw() {
        return true;
    }

    // 4. Query systemd-logind via zbus D-Bus
    if let Ok(conn) = zbus::Connection::system().await {
        if let Ok(manager) = Login1ManagerProxy::new(&conn).await {
            let get_user_res = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                manager.get_user(identity.uid),
            )
            .await;

            if let Ok(Ok(user_path)) = get_user_res {
                if let Ok(builder) = Login1UserProxy::builder(&conn).path(user_path) {
                    if let Ok(user) = builder.build().await {
                        let state_res = tokio::time::timeout(
                            std::time::Duration::from_millis(500),
                            user.state(),
                        )
                        .await;

                        if let Ok(Ok(state)) = state_res {
                            if state == "active" || state == "online" {
                                tracing::debug!(
                                    "User UID {} authorized via systemd-logind (State={})",
                                    identity.uid,
                                    state
                                );
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    // 5. Fallback: check existence of PAM directory /run/user/<uid>
    let run_user = std::path::PathBuf::from(format!("/run/user/{}", identity.uid));
    if run_user.is_dir() {
        tracing::debug!(
            "User UID {} authorized via PAM directory fallback ({:?})",
            identity.uid,
            run_user
        );
        return true;
    }

    tracing::warn!(
        "Rejecting unauthorized UID {}: no active login session found",
        identity.uid
    );
    false
}
