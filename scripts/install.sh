#!/bin/bash
# install.sh
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

echo -e "${BLUE}🚀 Начинаю установку Vrxx...${NC}"

# 1. Сборка проекта
echo -e "${BLUE}🔨 Собираю бинарный файл (релиз-версия)...${NC}"
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ Ошибка сборки. Убедитесь, что установлены все зависимости."
    exit 1
fi

# 2. Установка бинарника
echo -e "${BLUE}📦 Копирую бинарный файл в /usr/local/bin/...${NC}"
sudo cp target/release/vrxx /usr/local/bin/vrxx
sudo chmod +x /usr/local/bin/vrxx

# 3. Установка иконки
echo -e "${BLUE}🎨 Устанавливаю иконку приложения...${NC}"
sudo mkdir -p /usr/share/icons/hicolor/scalable/apps/
sudo cp data/icons/hicolor/scalable/apps/ru.mark.vrxx.svg /usr/share/icons/hicolor/scalable/apps/ru.mark.vrxx.svg

# 4. Установка ярлыка (.desktop файл)
echo -e "${BLUE}📝 Создаю ярлык в системном меню...${NC}"
sudo mkdir -p /usr/share/applications/
sudo cp ru.mark.vrxx.desktop /usr/share/applications/ru.mark.vrxx.desktop

# 5. Установка и запуск системной службы vrxx-daemon
echo -e "${BLUE}⚙️  Настраиваю и запускаю системную службу vrxx-daemon...${NC}"
cat << 'EOF' | sudo tee /etc/systemd/system/vrxx-daemon.service > /dev/null
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

if command -v systemctl >/dev/null 2>&1; then
    sudo systemctl daemon-reload
    sudo systemctl enable --now vrxx-daemon.service
    echo -e "${GREEN}🔌 Служба vrxx-daemon успешно запущена и включена в автозагрузку!${NC}"
fi

# 6. Обновление баз данных иконок и приложений
echo -e "${BLUE}♻️  Обновляю системные кэши...${NC}"
sudo gtk-update-icon-cache /usr/share/icons/hicolor
sudo update-desktop-database /usr/share/applications

echo -e "${GREEN}✅ Установка завершена! Служба запущена. Теперь вы можете открыть Vrxx из меню приложений.${NC}"
