# Coding Conventions

**Analysis Date:** 2025-02-11

## Naming Patterns

**Files:**
- Use snake_case for all file names.
- Module names match file names: `src/application.rs` -> `mod application;`.

**Functions:**
- Use snake_case for functions and methods: `setup_gactions()`, `is_running()`.

**Variables:**
- Use snake_case for variables and parameters: `app_settings`, `config_json`.

**Types:**
- Use PascalCase for structs, enums, and traits: `VrxxApplication`, `VpnCore`, `AppSettings`.
- Interface/Proxy traits for D-Bus are named clearly: `Daemon`.

## Code Style

**Formatting:**
- Standard Rust formatting (default `rustfmt` rules).
- 4-space indentation.

**GTK-rs Idioms:**
- **Subclassing:** Use the `imp` module pattern for GTK/Adwaita subclasses.
- **Templates:** Use `CompositeTemplate` and `#[template(resource = "...")]` for UI definitions.
- **Properties:** Use `glib::Properties` derive macro in the implementation struct to define GObject properties.
- **Models:** Data objects for ListBoxes (like `VpnKeyObject`) are GObjects derived from `ObjectSubclass`.

## Import Organization

**Order:**
1. Standard library `use std::...`
2. External crates (e.g., `use gtk::{...}`, `use adw::{...}`)
3. Local modules/crates (e.g., `use crate::config::...`)

**Path Aliases:**
- Not extensively used. `self::` and `crate::` are preferred for clarity.

## Error Handling

**Patterns:**
- Use `anyhow::Result` for application-level operations (e.g., in `src/backend.rs`).
- Use `thiserror` for custom error types in domain or library-like modules.
- Explicitly use `.context("...")` with `anyhow` to provide better error messages.
- For GTK-related code, handle `glib::Error` where necessary.

## Logging

**Framework:** `tracing` and custom `MultiWriter` in `src/main.rs`.

**Patterns:**
- `tracing::info!`, `tracing::warn!`, `tracing::error!` for application events.
- Logs are written to both a file (`app.log` and `all.log` in the config directory) and potentially console.

## Comments

**When to Comment:**
- Use comments for complex logic and sections (e.g., in `src/main.rs` for language initialization).
- Documentation comments (`///`) are used for public traits and their methods (e.g., `VpnCore` in `src/backend.rs`).

**JSDoc/TSDoc:**
- Not applicable.

## Function Design

**Size:**
- Generally small to medium functions. GTK implementation methods like `startup` and `activate` contain setup logic.

**Parameters:**
- Uses `&str` and `&String` where appropriate.
- Often passes `Arc<Runtime>` or `Arc<ProxyManager>` for shared state in async/IPC contexts.

**Return Values:**
- `anyhow::Result<()>` or `Result<T, E>` for fallible operations.
- `glib::ExitCode` for the `main` function.

## Module Design

**Exports:**
- Modules are declared in `main.rs` or `mod.rs`.
- `pub mod domain`, `pub mod services`, `pub mod daemon`, `pub mod ipc` are public for cross-module usage.

**Barrel Files:**
- `mod.rs` is used for sub-packages (e.g., `src/ui/mod.rs`, `src/domain/mod.rs`).

---

*Convention analysis: 2025-02-11*
