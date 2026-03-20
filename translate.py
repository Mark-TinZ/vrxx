import os
import re

translations = {
    # UI
    "Ключи VPN": "VPN Keys",
    "Прокси": "Proxy",
    "Маршрутизация": "Routing",
    "Настройки": "Settings",
    "Системные логи": "System Logs",
    "Автопрокрутка": "Auto-scroll",
    "Скопировать логи": "Copy logs",
    "Очистить логи": "Clear logs",
    "Добавить подключение": "Add connection",
    "Отключить": "Disconnect",
    "Список подключений": "Connection list",
    "Локальный прокси": "Local proxy",
    "Системный прокси": "System proxy",
    "Автоматическая настройка прокси для всей системы (GNOME)": "Automatic proxy configuration for the whole system (GNOME)",
    "Устанавливать системный прокси": "Set system proxy",
    "При активном подключении трафик приложений пойдет через прокси": "When connected, application traffic will go through the proxy",
    "Входящие подключения (Inbounds)": "Inbound connections (Inbounds)",
    "Локальные порты, на которых работают прокси-серверы ядра": "Local ports where the core proxy servers run",
    "SOCKS5 Порт": "SOCKS5 Port",
    "HTTP Порт": "HTTP Port",
    "Разрешить подключения из LAN": "Allow connections from LAN",
    "Прокси будет доступен для других устройств в вашей локальной сети": "Proxy will be available for other devices in your local network",
    "Добавить домен": "Add domain",
    "Очистить список": "Clear list",
    "Экспорт / Импорт": "Export / Import",
    "Включить пользовательскую маршрутизацию": "Enable custom routing",
    "Использовать правила ниже для направления трафика": "Use rules below to route traffic",
    "Поиск домена или правила (e.g. domain:vk.com, *.org)...": "Search domain or rule (e.g. domain:vk.com, *.org)...",
    "Режим": "Mode",
    "Исключения (обход VPN)": "Exceptions (bypass VPN)",
    "Включения (только VPN)": "Inclusions (VPN only)",
    "Домены и IP адреса": "Domains and IP addresses",
    "Импорт из файла...": "Import from file...",
    "Экспорт в файл...": "Export to file...",
    "Общие": "General",
    "Язык (Language)": "Language",
    "Требуется перезапуск приложения (Requires restart)": "Application restart required",
    "Системный (System)": "System default",
    "Русский": "Russian",
    "Запускать при старте системы": "Run at startup",
    "Автоматически запускать приложение в фоновом режиме": "Automatically start application in the background",
    "Подключаться при запуске": "Connect on startup",
    "Восстанавливать последнее активное подключение": "Restore last active connection",
    "Уведомления": "Notifications",
    "Показывать уведомления при смене статуса подключения": "Show notifications on connection status change",
    "Режим стримера": "Streamer mode",
    "Скрывать IP-адреса и конфиденциальную информацию в интерфейсе": "Hide IP addresses and sensitive information in the interface",
    "Ядро (Core)": "Core",
    "Выберите движок маршрутизации, используемый для подключения": "Select the routing engine used for connection",
    "Используемое ядро": "Used core",
    "Xray-core или Sing-box": "Xray-core or Sing-box",
    "Версия ядра": "Core version",
    "Неизвестно": "Unknown",
    "Режим TUN": "TUN mode",
    "Перенаправлять весь системный трафик через виртуальный интерфейс (Требуется Root)": "Redirect all system traffic through a virtual interface (Requires Root)",
    "Сеть и Маршрутизация": "Network and Routing",
    "Sniffing (Анализ пакетов)": "Sniffing (Packet analysis)",
    "Определять домен из трафика (требуется для маршрутизации)": "Determine domain from traffic (required for routing)",
    "Стратегия доменов (Domain Strategy)": "Domain Strategy",
    "AsIs (Как есть)": "AsIs",
    "Обход LAN (Bypass LAN)": "Bypass LAN",
    "Не маршрутизировать локальный трафик через VPN": "Do not route local traffic through VPN",
    "Возвращать поддельные IP (улучшает скорость, но ломает некоторые приложения)": "Return fake IPs (improves speed but breaks some apps)",
    "Мультиплексирование (Mux)": "Multiplexing (Mux)",
    "Ускоряет соединения, но может вызвать нестабильность": "Speeds up connections but may cause instability",
    "Фрагментация (Fragment)": "Fragmentation (Fragment)",
    "Разделять пакеты для обхода DPI (Только для TCP/TLS)": "Split packets to bypass DPI (TCP/TLS only)",
    "Резервное копирование": "Backup",
    "Импорт настроек": "Import settings",
    "Загрузить профили и настройки из файла": "Load profiles and settings from file",
    "Экспорт настроек": "Export settings",
    "Сохранить текущую конфигурацию в файл": "Save current configuration to file",
    "Диагностика": "Diagnostics",
    "Уровень логирования": "Logging level",
    "Просмотр логов": "View logs",
    "Открыть консоль с логами в реальном времени": "Open console with real-time logs",
    "Открыть папку логов": "Open logs folder",
    "Открыть системную папку с файлами журналов": "Open system folder with log files",
    "Сбросить настройки": "Reset settings",
    "Удалить все сохраненные данные (опасная зона)": "Delete all saved data (danger zone)",
    "Mark-Vless Подключено": "Mark-Vless Connected",
    "Все логи": "All logs",
    "Логи приложения": "Application logs",
    "Логи ядра": "Core logs",
    "Домен уже существует в списке": "Domain already exists in the list",
    "Некорректный формат домена": "Invalid domain format",
    "Требуется перезапуск приложения для применения языка": "Application restart required to apply language",
    "{bin_name} не найден": "{bin_name} not found",
    "Ошибка соединения": "Connection error",
    "Неизвестная ошибка. Пожалуйста, проверьте Системные логи.": "Unknown error. Please check System logs.",
    "Сбой подключения": "Connection failure",
    "ОК": "OK",
    "Не удалось подключиться к выбранному VPN ключу.": "Failed to connect to the selected VPN key.",
    "Подключено": "Connected",
    "Подключение...": "Connecting...",
    "Ошибка конфигурации": "Configuration error",
    "Ошибка запуска ядра": "Core startup error",
    "Адрес сервера": "Server address",
    "Локация": "Location",
    "Часовой пояс": "Timezone",
    "Протокол": "Protocol",
    "Порт": "Port",
    "Сеть": "Network",
    "Безопасность": "Security",
    "Публичный ключ": "Public key",
    "Закрыть": "Close",
    "Редактировать VPN ключ": "Edit VPN key",
    "Имя": "Name",
    "UUID / Пароль": "UUID / Password",
    "Соединение": "Connection",
    "Отмена": "Cancel",
    "Сохранить": "Save",
    "{} (Копия)": "{} (Copy)",
    "Удалить VPN ключ": "Delete VPN key",
    "Удалить": "Delete",
    "Добавить VPN ключ": "Add VPN key",
    "VPN Ссылка": "VPN Link",
    "Добавить": "Add",
    "Отключено": "Disconnected",
    "Вы уверены, что хотите удалить '{key_name}'?": "Are you sure you want to delete '{key_name}'?",
    "Запуск ядра {bin_name}...": "Starting core {bin_name}...",
    "Остановка процесса ядра...": "Stopping core process...",
    "Процесс ядра завершен.": "Core process terminated.",
    "Система переходит в спящий режим! Приостановка мониторинга VPN.": "System is going to sleep! Suspending VPN monitoring.",
    "Система проснулась! Возобновление мониторинга VPN.": "System woke up! Resuming VPN monitoring.",
    "Обнаружено падение процесса ядра! Отключение...": "Core process crash detected! Disconnecting...",
    "Таймаут подключения (более 12 сек).": "Connection timeout (over 12 sec).",
    "Не удалось распарсить ключ для генерации конфигурации": "Failed to parse key for configuration generation",
    "Подключение к VPN ключу: {}": "Connecting to VPN key: {}",
    "Не удалось запустить бэкенд: {e}": "Failed to start backend: {e}",
    "Бэкенд успешно запущен": "Backend successfully started",
    "Ошибка парсинга ключа: {e}": "Key parsing error: {e}",
    "Ошибка парсинга ключа из буфера: {e}": "Key parsing error from buffer: {e}",
    "Отключение VPN": "Disconnecting VPN",
    "Ошибка остановки бэкенда: {e}": "Error stopping backend: {e}",
    "Парсер должен принимать валидный Base64 Vmess": "Parser should accept valid Base64 Vmess",
    "Парсер должен возвращать Err на невалидный Base64": "Parser should return Err on invalid Base64",
    "Процесс ядра неожиданно завершился. Детали лога:\\n\\n{error_details}": "Core process unexpectedly terminated. Log details:\\n\\n{error_details}",
    "Не удалось создать временный файл": "Failed to create temporary file",
    "Не удалось записать конфигурацию": "Failed to write configuration",
    "Ядро {bin_name} не найдено в системе.\\n\\nПожалуйста, установите его (например, через ваш пакетный менеджер) или выберите другое ядро в Настройках.": "Core {bin_name} not found in the system.\\n\\nPlease install it (e.g., via your package manager) or select another core in Settings.",
    "Режим TUN включен, но ядро {bin_name} не имеет необходимых прав (cap_net_admin).\\n\\nВыполните в терминале:\\nsudo setcap cap_net_admin=ep {core_path}": "TUN mode is enabled, but the core {bin_name} lacks necessary permissions (cap_net_admin).\\n\\nRun in terminal:\\nsudo setcap cap_net_admin=ep {core_path}",
    "Сбой подключения": "Connection failure",
}

import glob

# Apply replacements to source files
files_to_check = []
for root, _, files in os.walk('src'):
    for f in files:
        if f.endswith('.rs') or f.endswith('.ui'):
            files_to_check.append(os.path.join(root, f))

# Sort keys by length descending to avoid partial replacements
sorted_keys = sorted(translations.keys(), key=len, reverse=True)

# Also handle wrapping 'Требуется перезапуск приложения для применения языка' if missing
wrap_rules = {
    '"Требуется перезапуск приложения для применения языка"': '&gettext("Application restart required to apply language")'
}

for file_path in files_to_check:
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    original_content = content
    
    for ru, en in wrap_rules.items():
        content = content.replace(ru, en)
    
    for ru in sorted_keys:
        en = translations[ru]
        # We only want to replace literal occurrences. In UI files they might be bare.
        # In .rs files they might be inside quotes. 
        # But replacing the string directly works well enough if we avoid replacing parts of other strings.
        # Since we sorted by length, it's safer.
        content = content.replace(ru, en)
        
    if content != original_content:
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"Updated {file_path}")

# Update po/ru.po
po_path = 'po/ru.po'
if os.path.exists(po_path):
    with open(po_path, 'r', encoding='utf-8') as f:
        po_content = f.read()
    
    # We will append missing translations
    appended = 0
    with open(po_path, 'a', encoding='utf-8') as f:
        for ru, en in translations.items():
            # Basic check if it exists in po
            # The po file might have newlines and stuff, but we can do a simple check
            en_escaped = en.replace('\n', '\\n').replace('"', '\\"')
            ru_escaped = ru.replace('\n', '\\n').replace('"', '\\"')
            if f'msgid "{en_escaped}"' not in po_content:
                f.write(f'\\nmsgid "{en_escaped}"\\nmsgstr "{ru_escaped}"\\n')
                appended += 1
    print(f"Appended {appended} translations to {po_path}")
else:
    print(f"{po_path} not found")

# Also check if en.po exists, if not we don't necessarily need to create it 
# since English is the source language now, but we can generate a basic one if needed.
