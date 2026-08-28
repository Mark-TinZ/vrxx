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

# 1. Удаление бинарного файла
sudo rm -f /usr/local/bin/vrxx

# 2. Удаление иконки
sudo rm -f /usr/share/icons/hicolor/scalable/apps/ru.mark.vrxx.svg

# 3. Удаление ярлыка
sudo rm -f /usr/share/applications/ru.mark.vrxx.desktop

# 4. Обновление баз данных
sudo gtk-update-icon-cache /usr/share/icons/hicolor
sudo update-desktop-database /usr/share/applications

echo -e "${RED}✅ Программа успешно удалена.${NC}"
echo "Примечание: Ваши настройки и ключи в ~/.config/vrxx/ остались нетронутыми."
