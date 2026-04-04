# Coding Conventions

**Analysis Date:** 2024-05-24

## Naming Patterns

**Files:**
- snake_case (e.g., `src/domain/key_parser.rs`, `src/ui/components/vpn_key_row.rs`)

**Functions:**
- snake_case (e.g., `parse_vpn_key`, `setup_callbacks`)

**Variables:**
- snake_case (e.g., `query_params`, `active_connection_btn`)

**Types:**
- PascalCase for Structs, Enums, and Traits (e.g., `ParsedKey`, `VrxxWindow`, `MultiWriter`)

## Code Style

**Formatting:**
- Standard `rustfmt` formatting. No custom `rustfmt.toml` detected.
- 4-space indentation.

**Linting:**
- Standard `cargo clippy`. No explicit `clippy.toml` configuration.

## Import Organization

**Order:**
1. Standard library modules (`use std::collections::HashMap;`)
2. External crate dependencies (`use url::Url; use serde::{...};`)
3. Internal crate modules (`use crate::ui::pages::VrxxVpnPage;`)

**Path Aliases:**
- `crate::` prefix for internal imports (e.g., `crate::settings::SettingsManager`)
- `super::*` for nested `mod imp` blocks.

## Error Handling

**Patterns:**
- Extensive use of `Result<T, E>`.
- Internal logic often maps errors to `String` (e.g., `Result<ParsedKey, String>`) via `.map_err(|e| e.to_string())?`.
- `anyhow` and `thiserror` are included in `Cargo.toml` and likely used for more complex backend operations.
- Avoids `unwrap()` in fallible logic where possible, relying on `unwrap_or(...)` or early returns via `?` operator.

## Logging

**Framework:** `tracing` crate. GTK GLib logs are also bridged to `tracing` via `glib::log_set_writer_func`.

**Patterns:**
- Initialization in `src/main.rs`.
- `tracing::info!()`, `tracing::warn!()`, `tracing::error!()` used for standard app logging.
- Logs are written to `app.log` and `all.log` in `dirs::config_dir()/vrxx/logs`.

## Comments

**When to Comment:**
- Minimal inline comments. Used mainly to clarify business logic or complex data formats (e.g., `// vmess usually is base64 encoded JSON` in `src/domain/key_parser.rs`).
- Module/File level header comments used for licensing.

**JSDoc/TSDoc:**
- Not applicable (Rust). Standard `///` rustdoc comments are sparse, mainly relying on clear naming instead.

## Function Design

**Size:** Small to medium. Helper functions extract complex parsing logic (e.g., `parse_vmess` in `src/domain/key_parser.rs`).

**Parameters:** Prefers borrowed types like `&str` or `&ParsedKey` instead of taking ownership when mutating is not required.

**Return Values:** Typically `Result<T, E>` or `Option<T>` for fallible operations. Returns owned objects like `String` or `ParsedKey` when creating new data.

## Module Design

**Exports:**
- Defined via `mod.rs` files (e.g., `pub mod domain;`).
- GTK subclassing uses internal `mod imp` blocks inside the component file, exposing the outer wrapper class via `glib::wrapper!`.

**Barrel Files:**
- `src/domain/mod.rs` and `src/ui/pages/mod.rs` act similarly to barrel files by aggregating submodule exports.

---

*Convention analysis: 2024-05-24*
