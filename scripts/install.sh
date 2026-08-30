#!/usr/bin/env bash
# install.sh
#
# Copyright 2026 Mark
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# SPDX-License-Identifier: MPL-2.0

set -euo pipefail

# Цветовое оформление вывода в терминал
BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}${BOLD}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}${BOLD}[OK]${NC} $1"
}

warn() {
    echo -e "${YELLOW}${BOLD}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}${BOLD}[ERROR]${NC} $1" >&2
}

# Определение корневого каталога репозитория
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${SCRIPT_DIR}"

info "🚀 Подготовка к установке VRXX..."

# 1. Проверка наличия Cargo и Rust
if ! command -v cargo >/dev/null 2>&1; then
    error "Rust toolchain (cargo) не найден в системе. Установите Rust: https://rustup.rs"
    exit 1
fi

# 2. Определение механизма повышения привилегий
SUDO_CMD=""
KEEPALIVE_PID=""

cleanup_elevation() {
    if [[ -n "${KEEPALIVE_PID}" ]] && kill -0 "${KEEPALIVE_PID}" 2>/dev/null; then
        kill "${KEEPALIVE_PID}" 2>/dev/null || true
    fi
}
trap cleanup_elevation EXIT INT TERM

if [[ "${EUID}" -eq 0 ]]; then
    info "Установка выполняется от имени суперпользователя (root)."
    SUDO_CMD=""
else
    if command -v sudo >/dev/null 2>&1; then
        SUDO_CMD="sudo"
        info "Для системной установки требуются права администратора (sudo)."
        if ! sudo -v; then
            error "Не удалось подтвердить права sudo. Установка отменена."
            exit 1
        fi
        # Фоновое поддержание активности sudo-токена на время сборки и установки
        ( while true; do sudo -n true; sleep 50; kill -0 "$$" || exit; done 2>/dev/null ) &
        KEEPALIVE_PID=$!
    elif command -v doas >/dev/null 2>&1; then
        SUDO_CMD="doas"
        info "Используется doas для выполнения системных операций."
    else
        error "Для установки системных компонентов требуются права root или утилита sudo/doas."
        exit 1
    fi
fi

# 3. Сборка проекта от имени текущего пользователя
info "🔨 Сборка релизной версии VRXX (cargo build --release)..."
cargo build --release
success "Сборка успешно завершена."

# 4. Установка бинарного файла
info "📦 Установка исполняемого файла в /usr/local/bin/vrxx..."
${SUDO_CMD} mkdir -p /usr/local/bin
${SUDO_CMD} cp target/release/vrxx /usr/local/bin/vrxx
${SUDO_CMD} chmod 755 /usr/local/bin/vrxx
success "Бинарный файл установлен."

# 5. Установка иконки приложения
info "🎨 Установка иконки приложения в системную тему hicolor..."
${SUDO_CMD} mkdir -p /usr/share/icons/hicolor/scalable/apps
${SUDO_CMD} cp data/icons/hicolor/scalable/apps/ru.mark.vrxx.svg /usr/share/icons/hicolor/scalable/apps/ru.mark.vrxx.svg
${SUDO_CMD} chmod 644 /usr/share/icons/hicolor/scalable/apps/ru.mark.vrxx.svg
success "Иконка установлена."

# 6. Установка ярлыка (.desktop файла)
info "📝 Установка ярлыка меню приложений..."
${SUDO_CMD} mkdir -p /usr/share/applications
${SUDO_CMD} cp ru.mark.vrxx.desktop /usr/share/applications/ru.mark.vrxx.desktop
${SUDO_CMD} chmod 644 /usr/share/applications/ru.mark.vrxx.desktop
success "Ярлык установлен."

# 7. Установка AppStream / AppData метаинформации (если файл существует)
if [[ -f "data/ru.mark.vrxx.metainfo.xml.in" ]]; then
    info "📄 Установка метаинформации AppStream..."
    ${SUDO_CMD} mkdir -p /usr/share/metainfo
    ${SUDO_CMD} cp data/ru.mark.vrxx.metainfo.xml.in /usr/share/metainfo/ru.mark.vrxx.metainfo.xml
    ${SUDO_CMD} chmod 644 /usr/share/metainfo/ru.mark.vrxx.metainfo.xml
    success "Метаинформация AppStream установлена."
fi

# 8. Настройка и запуск системного демона vrxx-daemon
info "⚙️  Настройка системной службы systemd vrxx-daemon..."
${SUDO_CMD} mkdir -p /etc/systemd/system
cat << 'EOF' | ${SUDO_CMD} tee /etc/systemd/system/vrxx-daemon.service > /dev/null
[Unit]
Description=VRXX Privileged VPN & Proxy Daemon
After=network.target network-online.target systemd-resolved.service
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/vrxx --daemon
Restart=always
RestartSec=3s
KillMode=process

[Install]
WantedBy=multi-user.target
EOF
${SUDO_CMD} chmod 644 /etc/systemd/system/vrxx-daemon.service

if command -v systemctl >/dev/null 2>&1 && [[ -d /run/systemd/system ]]; then
    ${SUDO_CMD} systemctl daemon-reload
    ${SUDO_CMD} systemctl enable --now vrxx-daemon.service
    success "Служба vrxx-daemon запущена и добавлена в автозагрузку systemd."
else
    warn "systemd не активен. Служба vrxx-daemon.service создана, но требует ручного запуска."
fi

# 9. Обновление системных кэшей
info "♻️  Обновление системных кэшей иконок и приложений..."
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    ${SUDO_CMD} gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor 2>/dev/null || true
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    ${SUDO_CMD} update-desktop-database -q /usr/share/applications 2>/dev/null || true
fi
success "Системные кэши обновлены."

echo ""
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}  ✅ Установка VRXX успешно завершена!${NC}"
echo -e "${GREEN}${BOLD}  Служба vrxx-daemon активна.${NC}"
echo -e "${GREEN}${BOLD}  Приложение готово к запуску из системного меню или командой 'vrxx'.${NC}"
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════════${NC}"
