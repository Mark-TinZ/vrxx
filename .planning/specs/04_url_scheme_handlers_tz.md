# ТЗ 04: Обработка URL-ссылок (Deep Linking / Scheme Handlers)

## 1. Назначение и Архитектурный Слой
- **Слой:** UI & Application (`src/application.rs`, `ru.mark.vrxx.desktop`)
- **Назначение:** Автоматический перехват кликов по ссылкам формата `vless://`, `vmess://`, `trojan://`, `ss://`, `hysteria2://`, `tuic://` в браузере или мессенджере с предложением импорта ключа.

## 2. Требования к реализации
1. **Desktop Integration:**
   - Регистрация в `ru.mark.vrxx.desktop`:
     `MimeType=x-scheme-handler/vless;x-scheme-handler/vmess;x-scheme-handler/trojan;x-scheme-handler/ss;x-scheme-handler/hysteria2;x-scheme-handler/tuic;`
2. **Обработка синглтона и аргументов:**
   - Использование `gio::ApplicationFlags::HANDLES_OPEN` или `HANDLES_COMMAND_LINE`.
   - При клике по ссылке во втором процессе аргумент передается в уже запущенный экземпляр `vrxx`.
3. **Интерактивный диалог импорта (`AdwDialog`):**
   - Компонент `src/ui/import_dialog.rs`.
   - Отображает распарсенные данные: тип протокола, хост, порт, имя конфигурации, параметры безопасности.
   - Быстрый замер задержки до узла прямо в диалоговом окне до сохранения.
   - Кнопки: «Импортировать профиль», «Импортировать и подключить», «Отмена».

## 3. Требования из CONTRIBUTING.md
- **Локализация (i18n):** Все текстовые строки диалога заворачиваются в `gettextrs::gettext("...")` в Rust или `translatable="yes"` в `.ui` XML макете.
- **Неблокирующий UI:** Парсинг ключа и стартовая проверка доступности узла выполняются асинхронно через `glib::spawn_future_local`.
