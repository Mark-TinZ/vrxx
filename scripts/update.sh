#!/bin/bash
# update.sh
#
# Copyright 2026 Mark
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# SPDX-License-Identifier: MPL-2.0


# Цвета для вывода
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🔄 Начинаю обновление Vrxx...${NC}"

# 1. Пересборка
echo -e "${BLUE}🔨 Собираю новую версию...${NC}"
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ Ошибка сборки. Обновление прервано."
    exit 1
fi

# 2. Перезапись системных файлов
echo -e "${BLUE}📦 Обновляю системные файлы...${NC}"
sudo cp target/release/vrxx /usr/local/bin/vrxx
sudo cp data/icons/hicolor/scalable/apps/ru.mark.vrxx.svg /usr/share/icons/hicolor/scalable/apps/ru.mark.vrxx.svg
sudo cp ru.mark.vrxx.desktop /usr/share/applications/ru.mark.vrxx.desktop

# 3. Кэш
sudo gtk-update-icon-cache /usr/share/icons/hicolor
sudo update-desktop-database /usr/share/applications

echo -e "${GREEN}✅ Обновление успешно завершено!${NC}"
