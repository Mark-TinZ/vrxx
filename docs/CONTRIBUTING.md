# Руководство для контрибьюторов (CONTRIBUTING.md)

Добро пожаловать в проект Vrxx! Мы рады любой помощи в улучшении нашего клиента для Xray и Sing-box.

## Подготовка окружения для разработки
Вам понадобятся:
- Rust (установленный через rustup)
- Зависимости GTK4 и libadwaita:
  - **Ubuntu/Debian**: `sudo apt install libgtk-4-dev libadwaita-1-dev gettext`
  - **Arch/Manjaro**: `sudo pacman -S gtk4 libadwaita gettext`
  - **Fedora**: `sudo pacman -S gtk4 libadwaita gettext` (через pacman - неточно, Fedora использует `sudo dnf install gtk4-devel libadwaita-devel gettext`)

## Сборка и запуск
Сборка осуществляется стандартным инструментом Cargo:
```bash
cargo build
cargo run
```

## Правила кода (Coding Standards)
1. **Форматирование:** Обязательно используйте `cargo fmt` перед отправкой коммита.
2. **Линтинг:** Мы стремимся к коду без предупреждений. Запустите `cargo clippy -- -D warnings`.
3. **Безопасность:** Запрещено использовать методы `.unwrap()` и `.expect()` (за исключением файлов тестов). Вместо них возвращайте `anyhow::Result` или используйте обработку через `match`/`if let`.
4. **GTK & Потоки:** Блокировка главного потока UI системными вызовами строго запрещена. Любые операции ввода-вывода (чтение файлов, вызовы `Command::new`) должны выполняться либо в асинхронном контексте (через `glib::spawn_future_local`), либо в фоновом потоке.

## Создание Pull Request
1. Сделайте форк репозитория.
2. Создайте ветку для вашей фичи: `git checkout -b feature/my-new-feature`
3. Сделайте коммиты с понятными описаниями.
4. Откройте PR к ветке `main`. Убедитесь, что все CI проверки проходят успешно.

Мы будем рады вашим PR!