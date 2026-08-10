# 11. Совместимость с GNOME HIG и Кросс-десктопный Fallback (KDE/XFCE/Sway)

## Обзор

Данный документ описывает архитектуру визуального соответствия гайдлайнам **GNOME HIG (Human Interface Guidelines)** на базе крейта `libadwaita` версии 1.5+, а также реализацию безопасного, не приводящего к крашам механизма кросс-десктопного фолбэка для сред **KDE Plasma**, **XFCE**, **Sway** и других оконных менеджеров Linux.

---

## 1. Архитектура и 100% Соответствие GNOME HIG (Libadwaita 1.5+)

Приложение `vrxx` полностью построено на современных виджетах Libadwaita:

| Компонент Libadwaita | Файлы реализации | Назначение в приложениях |
| --- | --- | --- |
| `AdwApplicationWindow` | `src/window.rs`, `src/window.ui` | Главное окно с поддержкой темных/светлых схем и жестов. |
| `AdwHeaderBar` | Все `.ui` файлы страниц | Стандартный заголовок с названием страницы, кнопками действий и главным меню. |
| `AdwPreferencesPage` | `proxy_page.ui`, `settings_page.ui`, `whitelist_page.ui` | Страницы настроек со скругленными блоками и стандартизированными отступами. |
| `AdwPreferencesGroup` | Страницы приложения | Группировка связанных опций с заголовками и описанием. |
| `AdwActionRow` | `settings_page.ui`, `navigation_list` | Интерактивные строки настроек и меню. |
| `AdwSwitchRow` | `proxy_page.ui`, `settings_page.ui` | Строки переключения режимов (системный прокси, TUN, уведомления). |
| `AdwStatusPage` | `vpn_page.rs` | Placeholder для пустых состояний (Empty State) при отсутствии VPN-ключей. |
| `AdwToastOverlay` & `AdwToast` | `window.ui`, `window.rs` | Всплывающие информационные тосты с кнопками действия. |
| `AdwDialog` / `AdwAlertDialog` | `import_dialog.rs`, `qr_dialog.rs`, `application.rs` | Современные модальные диалоги. |

### Адаптивность через `AdwBreakpoint`

В `src/window.ui` добавлен адаптивный брейкпоинт:
```xml
<child>
  <object class="AdwBreakpoint">
    <condition>max-width: 600sp</condition>
    <setter object="split_view" property="collapsed">True</setter>
  </object>
</child>
```
При уменьшении ширины окна менее `600sp` боковая панель `AdwNavigationSplitView` автоматически сворачивается в компактный мобильный вид с кнопкой вызова меню.

### Автоматическое соответствие системной палитре

Управление темами осуществляется через `adw::StyleManager::default()` в `src/application.rs`. При изменении системных настроек оформления GNOME/KDE приложение мгновенно переключает цветовые схемы (`Default`, `ForceLight`, `ForceDark`).

---

## 2. Кросс-десктопный Fallback (KDE / XFCE / Sway)

В средах, отличных от GNOME, схемы GSettings `org.gnome.system.proxy` могут отсутствовать в системе. Прямой вызов `gio::Settings::new("org.gnome.system.proxy")` в таких условиях вызывает критическую панику Glib и завершение приложения.

### 1. Определение окружения

Функция `detect_desktop_environment()` в `src/backend.rs` считывает переменные окружения `XDG_CURRENT_DESKTOP` и `XDG_SESSION_DESKTOP`:

```rust
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Xfce,
    Sway,
    Other(String),
}
```

### 2. Безопасная проверка схем GSettings

Перед инстанцированием настроек выполняется проверка наличия схемы через `SettingsSchemaSource`:

```rust
pub fn is_gnome_proxy_schema_available() -> bool {
    if let Some(source) = gtk::gio::SettingsSchemaSource::default() {
        source.lookup("org.gnome.system.proxy", true).is_some()
    } else {
        false
    }
}
```

При отсутствии схемы метод `update_system_proxy(&self, enable: bool)` возвращает статус `SystemProxyResult::SchemaUnavailable { desktop }` без вызова паник.

### 3. Пользовательский интерфейс Фолбэка (`AdwToast`)

Когда пользователь включает «Системный прокси» на KDE/XFCE/Sway:
1. Вызывается `CoreBackend::update_system_proxy(true)`.
2. Если возвращается `SchemaUnavailable`, приложение показывает тост `AdwToast`:
   > *"GNOME proxy GSettings scheme is unavailable on KDE Plasma. Use TUN mode for system-wide routing."*
3. Тост содержит интерактивную кнопку **«Switch to TUN»**.
4. При нажатии кнопки приложение автоматически:
   - Включает `tun_mode = true` в `AppSettings`.
   - Перезапускает ядро через канал `core_restart_channel()`.
   - Показывает подтверждающий тост *"TUN mode enabled and core restarted."*.

### 4. Поддержка переменных окружения (`HTTP_PROXY` / `HTTPS_PROXY`)

Для консольных утилит и сторонних процессов в `src/backend.rs` реализованы методы:
- `set_process_proxy_env(http_port, enable)`: Устанавливает переменные `HTTP_PROXY`, `HTTPS_PROXY`, `http_proxy`, `https_proxy` для текущего процесса.
- `get_proxy_env_export_cmd(http_port)`: Генерирует готовую команду экпорта для терминала пользователя.

---

## 3. Гарантии стабильности (No-Crash & Main Thread)

1. **Отсутствие panics**: Все методы вызова DBus и GSettings защищены сопоставлением шаблонов (`match`) и безопасными конструкторами (`Settings::new_full`).
2. **Изоляция главного потока UI**: Фоновое сетевое взаимодействие и пингование REST API демона выполняются в отдельном Tokio Runtime thread pool.
