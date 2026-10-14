# Детализированный реестр задач по главам и файлам (TASKS.md)

В данном файле сформулированы все конкретные задачи, разбитые по **главам** в соответствии с **затрагиваемыми файлами кодовой базы**. По каждой задаче приведён локальный план выполнения, критерии проверки и граничные случаи.

---

## Глава 1: Гео-данные и настройки (Geo Data & Settings)

### Затронутые файлы:
- `src/settings.rs`
- `src/ui/pages/settings_page.ui`
- `src/ui/pages/settings_page.rs`
- `src/services/geo_updater.rs`
- `src/domain/singbox_config.rs`
- `src/main.rs`
- `src/ui/tests.rs`

---

### Задача 1.1: Расширение модели конфигурации `AppSettings`
- **Файл**: `src/settings.rs`
- **Цель**: Добавить параметры управления автоматическим обновлением гео-баз и периодичностью.
- **План выполнения**:
  1. Добавить поле `pub auto_update_geodata: bool` с атрибутом `#[serde(default = "default_auto_update_geodata")]` (значение по умолчанию: `false` во избежание нежелательного фонового трафика при первом старте).
  2. Добавить поле `pub geodata_update_interval_days: u32` с атрибутом `#[serde(default = "default_geodata_update_interval_days")]` (по умолчанию: `7`).
  3. Реализовать вспомогательные функции по умолчанию `default_auto_update_geodata()` и `default_geodata_update_interval_days()`.
  4. Обновить метод `AppSettings::new()`, включив новые поля.
  5. Добавить unit-тесты на сериализацию/десериализацию и проверку сохранения значений по умолчанию.
- **Граничные случаи**: Старые конфигурационные файлы `settings.json`, в которых отсутствуют эти поля, должны корректно считываться без ошибок serde.
- **Статус**: [x] Выполнено

---

### Задача 1.2: Пользовательский интерфейс управления автообновлением гео-данных
- **Файлы**: `src/ui/pages/settings_page.ui`, `src/ui/pages/settings_page.rs`
- **Цель**: Предоставить пользователю переключатель автообновления и выпадающий список выбора частоты обновления.
- **План выполнения**:
  1. В `src/ui/pages/settings_page.ui` внутри группы «Geo Data» (`AdwPreferencesGroup`):
     - Добавить `AdwSwitchRow` с `id="auto_update_geo_row"`, заголовком «Auto-update Geo Data» и поясняющим подзаголовком.
     - Добавить `AdwComboRow` с `id="geo_interval_row"`, заголовком «Update Frequency» и вариантами (Ежедневно, Раз в 7 дней, Раз в 14 дней, Раз в 30 дней).
  2. В `src/ui/pages/settings_page.rs`:
     - Добавить `auto_update_geo_row` и `geo_interval_row` в структуру `VrxxSettingsPagePrivate` как `TemplateChild`.
     - Связать переключение виджетов с обновлением `SettingsSnapshot` и `SettingsManager`.
     - При деактивации `auto_update_geo_row` отключать чувствительность (`sensitive=false`) для `geo_interval_row`.
     - В `discard_changes()` реализовать корректный откат состояния переключателя `auto_update_geo_row` и интервала `geo_interval_row` к сохраненному снимку.
     - Отображать текущие значения при открытии страницы настроек.
- **Граничные случаи**: Мгновенное сохранение в JSON без зависания GUI потока; откат изменений при переходе/сбросе.
- **Статус**: [x] Выполнено

---

### Задача 1.3: Исправление URL и выборочная загрузка гео-баз
- **Файлы**: `src/services/geo_updater.rs`, `src/domain/singbox_config.rs`
- **Цель**: Устранить ошибку HTTP 404 для `geosite-antifilter.srs`, скачивать только те базы, которые включены в настройках маршрутизации.
- **План выполнения**:
  1. В `src/services/geo_updater.rs`:
     - Заменить битый URL `geosite-antifilter.srs` на рабочий источник sing-box: основной `https://github.com/savely-krasovsky/antizapret-sing-box/releases/latest/download/antizapret.srs` и независимое резервное CDN-зеркало `https://cdn.jsdelivr.net/gh/legiz-ru/sb-rule-sets@release/antifilter.srs`.
     - Настроить функцию `update_geo_databases`: принимать ссылку на настройки `AppSettings` или фильтровать загрузку:
       - Если выключены все правила РФ (`!route_ru && !route_ru_sites && !route_ru_ips`) — не скачивать `geosite-ru.srs` и `geoip-ru.srs`.
       - Если выключены правила CN — не скачивать `geosite-cn.srs` и `geoip-cn.srs`.
       - Если выключены правила IR — не скачивать `geosite-ir.srs` и `geoip-ir.srs`.
       - Если выключен Antifilter — не скачивать `geosite-antifilter.srs`.
       - Если выключена блокировка рекламы — не скачивать `geosite-ads.srs`.
       - Базовые необходимые базы (`geosite-google.srs`, `geoip-private.srs`) скачивать всегда.
     - Добавить обработку ошибок: при недоступности источника логировать `warn` (а не паниковать `error`), не ломать прогресс остальных баз.
  2. В `src/domain/singbox_config.rs`:
     - Обновить URL для `geosite-antifilter` на `https://github.com/savely-krasovsky/antizapret-sing-box/releases/latest/download/antizapret.srs`.
- **Граничные случаи**: Полное отсутствие интернета (офлайн) не должно приводить к крашу фонового потока; сбой GitHub прозрачно компенсируется CDN зеркалом.
- **Статус**: [x] Выполнено

---

### Задача 1.4: Условный запуск фонового апдейтера и проверка актуальности
- **Файл**: `src/main.rs`, `src/services/geo_updater.rs`
- **Цель**: Не инициировать избыточный сетевой трафик при старте программы, проверять mtime баз и поддерживать фоновый таймер живым для динамического включения в UI.
- **План выполнения**:
  1. Реализовать `is_geodata_stale(interval_days)` и `get_last_updated_time()` для проверки времени модификации файлов баз.
  2. В `src/services/geo_updater.rs::spawn_background_updater()` организовать периодический цикл:
     - Опрашивать настройки пользователя `SettingsManager::new().load()`.
     - Скачивать базы только при `auto_update_geodata == true` и их реальном устаревании относительно `geodata_update_interval_days`.
     - Не дублировать запросы на старте приложения (устранена ошибка двойного вызова interval).
     - Поток не завершается при старте с отключенной опцией, а периодически проверяет флаг раз в час.
- **Граничные случаи**: Пользователь переключил флаг автообновления в окне настроек — апдейтер подхватывает изменение без перезапуска программы; повторный перезапуск приложения в течение дня не производит лишних загрузок.
- **Статус**: [x] Выполнено

---

## Глава 2: Состояние демона и обработка отсутствия службы (Daemon State & Disconnect Handling)

### Затронутые файлы:
- `src/ipc.rs`
- `src/window.ui`
- `src/window.rs`
- `src/ui/pages/vpn_page.rs`
- `src/backend.rs`
- `src/ui/components/log_window.rs`

---

### Задача 2.1: Статус подключения и экспоненциальный Backoff в IPC
- **Файл**: `src/ipc.rs`
- **Цель**: Избавиться от спама `WARN` в логах, внедрить состояние подключения демона `DaemonConnectionState` и exponential backoff.
- **План выполнения**:
  1. Ввести публичный enum:
     ```rust
     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
     pub enum DaemonConnectionState {
         Disconnected,
         Connecting,
         Connected,
     }
     ```
  2. Добавить метод `pub async fn ping(&self) -> bool` в `DaemonClient` с коротким таймаутом (1 секунда).
  3. В цикле `subscribe_events`:
     - Внедрить задержку с экспоненциальным backoff (2с -> 4с -> 8с -> 16с, максимум 30с).
     - Добавить флаг `ever_connected: bool`. Если соединение ещё ни разу не было установлено, ошибку логировать на уровне `tracing::debug!`, чтобы не засорять терминал и журнал предупреждениями `WARN`.
     - Если соединение было установлено и оборвалось, логировать однократный `tracing::warn!`.
     - При успешном `Event::Open` сбрасывать счётчик задержки на начальный уровень (2с).
- **Граничные случаи**: Демон запустился позже, чем GUI — клиент должен бесшовно подключиться при очередном тике backoff.
- **Статус**: [x] Выполнено

---

### Задача 2.2: Визуальное оповещение пользователя через `AdwBanner`
- **Файлы**: `src/window.ui`, `src/window.rs`
- **Цель**: Показать нативный баннер в верхней части окна, если демон `vrxx-daemon` не обнаружен.
- **План выполнения**:
  1. В `src/window.ui`:
     - Добавить `AdwBanner` с `id="daemon_status_banner"` в вертикальный контейнер над `AdwNavigationSplitView`.
     - Установить свойства: `revealed: False`, `title: "VRXX system service is not running. Please start vrxx-daemon to enable VPN and routing."`, `button-label: "Check"`.
  2. В `src/window.rs`:
     - Забиндить `daemon_status_banner: TemplateChild<adw::Banner>`.
     - При инициализации окна запустить фоновую проверку `ping()` демона.
     - Если пинг успешен — `banner.set_revealed(false)`.
     - Если пинг неуспешен — `banner.set_revealed(true)`.
     - По нажатию кнопки баннера («Проверить») запускать асинхронную проверку и показывать всплывающий `AdwToast` с результатом.
- **Граничные случаи**: Демон стартует в процессе работы — баннер автоматически скрывается.
- **Статус**: [x] Выполнено

---

### Задача 2.3: Предотвращение сетевых ошибок при попытке подключения
- **Файлы**: `src/ui/pages/vpn_page.rs`, `src/backend.rs`
- **Цель**: Вместо сбоя сетевого запроса и страшного технического лога с `Connection refused` показывать дружелюбное сообщение.
- **План выполнения**:
  1. В методе подключения `vpn_page.rs` перед вызовом `proxy.start_proxy(...)` выполнять быструю проверку доступности демона через `proxy.ping()`.
  2. Если демон недоступен:
     - Не отправлять тяжелый POST-запрос.
     - Сбросить состояние подключения ключа (`set_is_loading(false)`, `set_is_active(false)`).
     - Показать понятный `AdwToast` или диалог: «Для подключения необходимо запустить системную службу vrxx-daemon».
     - Раскрыть баннер `daemon_status_banner` в главном окне.
- **Граничные случаи**: Демон упал ровно в момент рукопожатия — перехват ошибки в блоке `Err` с понятным текстом через `format_human_error`.
- **Статус**: [x] Выполнено

---

### Задача 2.4: Дедупликация и подавление спама SSE в `log_window.rs`
- **Файл**: `src/ui/components/log_window.rs`
- **Цель**: Устранить параллельный спам логов в цикле окна логов при отсутствии демона.
- **План выполнения**:
  1. Убедиться, что подписка в окне системных логов не генерирует циклический `WARN` в собственный буфер логов при старте в отсутствие демона.
- **Граничные случаи**: Быстрое открытие и закрытие окна логов не должно оставлять висячих асинхронных задач.
- **Статус**: [x] Выполнено

---

## Глава 3: Единый стандарт адаптивности и мобильного вида (UI Responsiveness & Breakpoints)

### Затронутые файлы:
- `src/window.ui`
- `src/ui/dialogs/key_dialog.ui`
- `src/ui/dialogs/key_dialog.rs`
- `src/ui/dialogs/rule_dialog.ui`
- `src/ui/dialogs/rule_dialog.rs`
- `src/ui/dialogs/qr_dialog.ui`
- `src/ui/dialogs/qr_dialog.rs`
- `src/ui/dialogs/import_dialog.ui`
- `src/ui/dialogs/import_dialog.rs`
- `src/ui/dialogs/export_dialog.ui`
- `src/ui/dialogs/export_dialog.rs`
- `src/ui/pages/vpn_page.ui`
- `src/ui/pages/vpn_page.rs`
- `src/ui/pages/proxy_page.ui`
- `src/ui/pages/proxy_page.rs`
- `src/ui/pages/routing_page.ui`
- `src/ui/pages/routing_page.rs`
- `src/ui/pages/settings_page.ui`
- `src/ui/pages/settings_page.rs`
- `src/ui/components/log_window.ui`
- `src/ui/components/log_window.rs`
- `src/ui/tests.rs`

---

### Задача 3.1: Коррекция точки останова главного окна до 720sp
- **Файл**: `src/window.ui`
- **Цель**: Устранить предупреждение библиотеки Libadwaita `AdwToastOverlay exceeds VrxxWindow width: requested 680 px, 665 px available`.
- **План выполнения**:
  1. В `src/window.ui` изменить условие брейкпоинта:
     ```xml
     <object class="AdwBreakpoint">
       <condition>max-width: 720sp</condition>
       <setter object="split_view" property="collapsed">True</setter>
     </object>
     ```
  2. Проверить поведение при изменении ширины окна от 360px до 1000px: сайдбар должен плавно скрываться при `<= 720sp`, отдавая всю ширину контентной странице.
- **Граничные случаи**: Минимальная ширина окна 360 px не должна вызывать графических артефактов и обрезания кнопок.
- **Статус**: [x] Выполнено

---

### Задача 3.2: Внедрение `AdwBreakpoint` во все диалоговые окна `AdwDialog`
- **Файлы**: `src/ui/dialogs/key_dialog.ui`, `src/ui/dialogs/rule_dialog.ui`, `src/ui/dialogs/qr_dialog.ui`, `src/ui/dialogs/import_dialog.ui`, `src/ui/dialogs/export_dialog.ui`
- **Цель**: Обеспечить нативный мобильный режим (`bottom-sheet`) и правильное масштабирование на экранах `<= 500sp`.
- **План выполнения**:
  1. В каждом из диалоговых файлов добавить объявление `<child><object class="AdwBreakpoint">` с условием `max-width: 500sp`.
  2. Настроить `presentation-mode="auto"`, обеспечивающий автоматический показ шторки `bottom-sheet` при недостатке ширины экрана.
  3. Для `import_dialog.ui` добавить сеттер сокращения надписи кнопки действия с «Import and Connect» до «Connect» во избежание переполнения тулбара.
  4. Для всех диалогов установить приподнятый стиль шапки `toolbar_view: raised`.
- **Граничные случаи**: Открытие диалога на мобильном экране смартфона (ширина 360-400 px) должно открывать красивую шторку снизу экрана без горизонтального скролла.
- **Статус**: [x] Выполнено

---

### Задача 3.3: Адаптивная группировка кнопок шапки `vpn_page.ui` в компактное меню
- **Файлы**: `src/ui/pages/vpn_page.ui`, `src/ui/pages/vpn_page.rs`
- **Цель**: Предотвратить горизонтальное распирание шапки на экранах `<= 500sp` путем сворачивания 4 кнопок добавления в единое выпадающее меню со значком плюса.
- **План выполнения**:
  1. Перевести `VrxxVpnPage` на наследование от `AdwBreakpointBin` (вместо `AdwBin`), зарегистрировать типы `adw::Breakpoint` и `adw::BreakpointBin` в `class_init`.
  2. В `vpn_page.ui` объявить `AdwBreakpoint` с условием `max-width: 500sp`.
  3. Создать модель меню `vpn_add_menu` с пунктами («Add Key…», «Import from Clipboard», «Add Group…», «Add Subscription…»).
  4. Добавить компактную кнопку `GtkMenuButton` (`id="btn_add_compact_menu"`, иконка `list-add-symbolic`) рядом с панелью отдельных кнопок `header_actions_box`.
  5. В брейкпоинте переключать видимость: при `<= 500sp` `header_actions_box.visible = false`, `btn_add_compact_menu.visible = true`.
- **Граничные случаи**: Узкий экран со свернутым сайдбаром (360-480 px) сохраняет аккуратный заголовок и системные кнопки без смещения.
- **Статус**: [x] Выполнено

---

### Задача 3.4: Адаптивность окна логов `log_window.ui`
- **Файлы**: `src/ui/components/log_window.ui`, `src/ui/components/log_window.rs`
- **Цель**: Убрать жесткий `width-request: 300` у поля поиска, добавить адаптивный `AdwClamp` и поддержку минимальной ширины 360 px.
- **План выполнения**:
  1. В `src/ui/components/log_window.ui` задать минимальный размер окна `width-request: 360` и `height-request: 320`.
  2. Убрать `width-request: 300` у `search_entry`, обернуть его в `AdwClamp` с `maximum-size: 400` и `tightening-threshold: 300`.
  3. Добавить `AdwBreakpoint` для `max-width: 500sp` со стилем `top-bar-style: raised`.
- **Граничные случаи**: Поиск логов на узком окне (360-450 px).
- **Статус**: [x] Выполнено

---

### Задача 3.5: Спецификация минимальных размеров (width-request / height-request) для AdwBreakpointBin, страниц, окон и диалогов
- **Файлы**: `src/window.ui`, `src/window.rs`, `src/ui/pages/vpn_page.ui`, `src/ui/pages/vpn_page.rs`, `src/ui/pages/proxy_page.ui`, `src/ui/pages/proxy_page.rs`, `src/ui/pages/routing_page.ui`, `src/ui/pages/routing_page.rs`, `src/ui/pages/settings_page.ui`, `src/ui/pages/settings_page.rs`, `src/ui/dialogs/*.ui`, `src/ui/dialogs/*.rs`, `src/ui/components/log_window.ui`, `src/ui/components/log_window.rs`, `src/ui/tests.rs`
- **Цель**: Устранить предупреждения Libadwaita `VrxxVpnPage does not have a minimum size, set the 'width-request' and 'height-request' properties to specify it` и визуальные глитчи при динамическом изменении размера окна.
- **План выполнения**:
  1. В `src/ui/pages/vpn_page.ui` и `src/ui/pages/vpn_page.rs` задать явные `width-request: 360` и `height-request: 200` для `VrxxVpnPage` (`AdwBreakpointBin`).
  2. В шаблонах и `constructed` всех контентных страниц (`VrxxProxyPage`, `VrxxRoutingPage`, `VrxxSettingsPage`) задать минимальные размеры `width-request: 360` и `height-request: 200` для единообразия в `GtkStack`.
  3. В главном окне `src/window.ui` и `src/window.rs` задать `width-request: 360`, `height-request: 500`, добавить `hexpand: True` на `split_view`, и зарегистрировать типы `adw::Breakpoint`, `adw::NavigationPage`, `adw::ToolbarView`, `adw::HeaderBar`, `adw::WindowTitle`, `adw::ActionRow`, `adw::Clamp` в `class_init`.
  4. В шаблонах и `constructed` всех модальных диалогов (`VrxxKeyDialog`, `VrxxRuleDialog`, `VrxxQrDialog`, `VrxxImportDialog`, `VrxxExportDialog`) и `VrxxLogWindow` установить явные минимальные размеры (`width-request: 360`, `height-request: 300-360`).
  5. Добавить комплексный интеграционный тест `test_responsive_sizing_and_breakpoints` в `src/ui/tests.rs` для проверки замеров геометрии `measure()`, динамического ресайза окна от 360 px до 1200 px и фактического переключения состояний брейкпоинтов через `allocate()`.
- **Граничные случаи**: Динамический ресайз окна пользователем от 360px до 1200px без единого варнинга в логах, без дёрганья интерфейса и с корректным переключением тулбаров и сайдбара.
- **Статус**: [x] Выполнено

---

### Задача 3.6: Оптимизация производительности адаптивного переключения сайдбара и ликвидация фризов (NavigationSplitView Collapse/Expand Smoothing)
- **Файлы**: `src/window.ui`, `src/window.rs`, `src/ui/pages/vpn_page.ui`, `src/ui/tests.rs`, `PLAN.md`, `TASKS.md`
- **Цель**: Ликвидировать заметные задержки и фризы интерфейса при динамическом изменении размера окна через границу 720sp (переход AdwNavigationSplitView между режимом с боковым меню и компактным мобильным режимом).
- **Первопричины дефекта (Root Causes)**:
  1. *Синхронная перегрузка замеров и кроссфейд-буферов GtkStack*: По умолчанию в GTK4 `GtkStack` имеет свойства `hhomogeneous: True` и `vhomogeneous: True`, а также `transition-type: crossfade` с ненулевой длительностью. При каждом ресайзе и особенно при резком изменении ширины контентной области на 240px в момент сворачивания/разворачивания сайдбара GTK синхронно опрашивал замеры `measure()` для ВСЕХ 4 страниц приложения (`VrxxVpnPage`, `VrxxProxyPage`, `VrxxRoutingPage`, `VrxxSettingsPage` с более чем 50 рядами параметров) и выделял промежуточные оффскрин-текстуры для кроссфейда, блокируя основной поток интерфейса.
  2. *Инвертированная коллизия вложенных брейкпоинтов и риск распирания шапки*: В `VrxxVpnPage` был установлен `AdwBreakpoint` с порогом `max-width: 500sp`. При десктопной ширине окна 721px ширина контента составляет `721 - 240 = 481px` (<= 500sp -> меню компактное, 4 кнопки скрыты). При переходе за 720px сайдбар сворачивается, и ширина контента резко возрастает до 720px (> 500sp -> 4 кнопки отображаются). Это вызывало постоянное срабатывание брейкпоинта в момент переключения сайдбара, вызов `set_visible` внутри `allocate()`, инвалидацию компоновки через `queue_resize` и повторный цикл замеров. Понижение порога до 400sp устраняло коллизию, но создавало дефект распирания тулбара на мобильных экранах 400-450px с кнопкой «Назад» и системными кнопками.
  3. *Избыточные мутации состояния `show-content` и дублирование сигналов навигации*: Вызов `split_view.set_show_content(true)` производился безусловно при каждом переключении и сопоставлении страниц, даже если окно находилось в десктопном режиме или контент уже отображался, провоцируя лишние циклы оповещения свойств GTK. Кроме того, в `setup_callbacks` были одновременно подключены `connect_row_selected` и `connect_row_activated`, вызывавшие `select_page_by_row` дважды на каждый клик.
- **План выполнения**:
  1. В `src/window.ui`:
     - Для `view_stack` установить `hhomogeneous="False"`, `vhomogeneous="False"`, `transition-type="none"`, `transition-duration="0"` и `interpolate-size="False"`. Теперь замеряется только активная страница и полностью исключены накладные расходы на текстурные буферы переходов при ресайзе.
     - Для `split_view` задать явное декларативное свойство `<property name="show-content">True</property>`.
  2. В `src/window.rs`:
     - Оградить вызовы `set_show_content(true)` проверкой `if imp.split_view.is_collapsed() && !imp.split_view.shows_content()`. В десктопном режиме свойство не трогается, исключая лишние циклы оповещения.
     - Устранить дублирующий обработчик `connect_row_activated`, оставив единственный канонический `connect_row_selected`.
  3. В `src/ui/pages/vpn_page.ui`:
     - Установить сбалансированный порог точки останова `AdwBreakpoint` `max-width: 450sp`. При десктопной ширине контента (481px) кнопки всегда развернуты (нет коллизии на границе 720sp), а на мобильных экранах (<= 450sp) 4 кнопки аккуратно сворачиваются в компактное меню, предотвращая горизонтальное переполнение шапки с кнопкой возврата.
  4. В `src/ui/tests.rs`:
     - Обновить тест `test_responsive_sizing_and_breakpoints` с проверкой брейкпоинта 450sp (тестирование на 400px, 450px, 480px, 800px) и граничными проверками переключения сайдбара ровно на 720px (свернут) и 721px (раскрыт).
- **Граничные случаи**: Быстрое перетаскивание границы окна мышью в диапазоне 700px - 740px без просадки FPS; сохранение десктопных 4 кнопок при ширине 480px (рядом с сайдбаром) и аккуратное переключение в компактное меню только на узких мобильных экранах (<= 450sp).
- **Статус**: [x] Выполнено

---

## Глава 4: Инфраструктура тестирования (Test Runner & GTK Isolation)

### Затронутые файлы:
- `src/ui/mod.rs`
- `src/ui/tests.rs`
- `src/ui/components/log_window.rs`

---

### Задача 4.1: Изоляция потока GTK в тестах UI (Предотвращение двухпоточного сбоя GTK)
- **Файлы**: `src/ui/mod.rs`, `src/ui/tests.rs`, `src/ui/components/log_window.rs`
- **Цель**: Устранить панику `Attempted to initialize GTK from two different threads.` при выполнении `cargo test -- --ignored`.
- **План выполнения**:
  1. В `src/ui/tests.rs` реализовать диспетчер `run_test_on_gtk_thread`, создающий выделенный фоновый поток с единожды инициализированным GTK4/Adwaita и каналом выполнения задач `mpsc`.
  2. Перевести все интеграционные тесты UI (`test_ui_components_init`, `test_log_filtering_integration`, `test_log_window_level_filtering_and_sorting`, `test_key_dialog_data_flow`, `test_rule_dialog_data_flow`, `test_log_window_append`) на диспетчеризацию через `run_test_on_gtk_thread`.
  3. Обеспечить корректный проброс паник из потока GTK в вызывающий поток тестов через `catch_unwind` и `resume_unwind`.
- **Граничные случаи**: Запуск всех тестов параллельно в общем тестовом раннере `cargo test` под Xvfb.
- **Статус**: [x] Выполнено
