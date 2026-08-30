#!/usr/bin/env bash
# uninstall.sh
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
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
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

info "🗑️  Подготовка к удалению VRXX из системы..."

# Определение механизма повышения привилегий
SUDO_CMD=""
if [[ "${EUID}" -eq 0 ]]; then
    SUDO_CMD=""
else
    if command -v sudo >/dev/null 2>&1; then
        SUDO_CMD="sudo"
        if ! sudo -v; then
            error "Не удалось подтвердить права sudo. Удаление отменено."
            exit 1
        fi
    elif command -v doas >/dev/null 2>&1; then
        SUDO_CMD="doas"
    else
        error "Для удаления системных компонентов требуются права root или утилита sudo/doas."
        exit 1
    fi
fi

# 1. Остановка и удаление системной службы vrxx-daemon
if command -v systemctl >/dev/null 2>&1 && [[ -d /run/systemd/system ]]; then
    info "🛑 Остановка и отключение службы vrxx-daemon..."
    ${SUDO_CMD} systemctl disable --now vrxx-daemon.service 2>/dev/null || true
    ${SUDO_CMD} rm -f /etc/systemd/system/vrxx-daemon.service
    ${SUDO_CMD} systemctl daemon-reload 2>/dev/null || true
    success "Служба vrxx-daemon остановлена и удалена."
fi

# 2. Удаление исполняемого файла
info "Удаление /usr/local/bin/vrxx..."
${SUDO_CMD} rm -f /usr/local/bin/vrxx

# 3. Удаление иконок
info "Удаление иконки приложения..."
${SUDO_CMD} rm -f /usr/share/icons/hicolor/scalable/apps/ru.mark.vrxx.svg

# 4. Удаление ярлыка
info "Удаление ярлыка .desktop..."
${SUDO_CMD} rm -f /usr/share/applications/ru.mark.vrxx.desktop

# 5. Удаление метаинформации AppStream
${SUDO_CMD} rm -f /usr/share/metainfo/ru.mark.vrxx.metainfo.xml

# 6. Обновление системных кэшей
info "Обновление системных кэшей..."
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    ${SUDO_CMD} gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor 2>/dev/null || true
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    ${SUDO_CMD} update-desktop-database -q /usr/share/applications 2>/dev/null || true
fi

echo ""
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════════${NC}"
success "Программа VRXX и системная служба успешно удалены."
info "Примечание: Ваши пользовательские настройки и ключи сохранены в ~/.config/vrxx/."
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════════${NC}"
