#!/bin/bash
# uninstall.sh
#
# Copyright 2026 Mark
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# SPDX-License-Identifier: MPL-2.0


# Цвета для вывода
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${RED}🗑️ Удаляю Vrxx из системы...${NC}"

# 1. Остановка и удаление системной службы
if command -v systemctl >/dev/null 2>&1; then
    echo -e "${RED}🛑 Останавливаю и отключаю службу vrxx-daemon...${NC}"
    sudo systemctl disable --now vrxx-daemon.service 2>/dev/null || true
    sudo rm -f /etc/systemd/system/vrxx-daemon.service
    sudo systemctl daemon-reload 2>/dev/null || true
fi

# 2. Удаление бинарного файла
sudo rm -f /usr/local/bin/vrxx

# 3. Удаление иконки
sudo rm -f /usr/share/icons/hicolor/scalable/apps/ru.mark.vrxx.svg

# 4. Удаление ярлыка
sudo rm -f /usr/share/applications/ru.mark.vrxx.desktop

# 5. Обновление баз данных
command -v gtk-update-icon-cache >/dev/null 2>&1 && sudo gtk-update-icon-cache -f -t /usr/share/icons/hicolor || true
command -v update-desktop-database >/dev/null 2>&1 && sudo update-desktop-database /usr/share/applications || true

echo -e "${RED}✅ Программа и служба успешно удалены.${NC}"
echo "Примечание: Ваши настройки и ключи в ~/.config/vrxx/ остались нетронутыми."
