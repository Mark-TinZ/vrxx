# Testing Patterns

**Analysis Date:** 2024-05-24

## Test Framework

**Runner:**
- Standard Rust test runner (`cargo test`)
- Config: None (standard Cargo setup)

**Assertion Library:**
- Standard library macros (`assert!`, `assert_eq!`)

**Run Commands:**
```bash
cargo test              # Run all tests
```

## Test File Organization

**Location:**
- Co-located tests inside `#[cfg(test)] mod tests { ... }` blocks at the bottom of the source file (e.g., `src/domain/key_parser.rs`, `src/domain/singbox_config.rs`).
- Specialized UI tests exist in `src/ui/tests.rs`.
- Standalone integration-like test scripts exist in the root directory (`test_sub.rs`, `test_subprocess.rs`) but appear to be ad-hoc experiments with `fn main()` rather than traditional cargo integration tests.

**Naming:**
- Modules: `mod tests`
- Functions: `fn test_[behavior_or_function_name]`

**Structure:**
```
src/
└── domain/
    └── key_parser.rs
        ... logic ...
        #[cfg(test)]
        mod tests {
            use super::*;
            #[test]
            fn test_parse_vless_reality_url() { ... }
        }
```

## Test Structure

**Suite Organization:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_specific_scenario() {
        // Arrange
        let url = "vmess://...";
        // Act
        let result = parse_vpn_key(url);
        // Assert
        assert!(result.is_ok());
    }
}
```

**Patterns:**
- **Setup pattern:** Initialize string literals or structs directly inside the test function.
- **Teardown pattern:** None explicitly needed due to Rust's memory management and lack of persistent side-effects in current unit tests.
- **Assertion pattern:** Use of `.expect()` for unwrapping `Result` inside tests, followed by multiple `assert_eq!` calls to verify field values.

## Mocking

**Framework:** None detected natively (`mockall` or similar are missing from `Cargo.toml`).

**Patterns:**
```rust
// No formal mocking libraries. External systems (like networking or shell commands) 
// are usually either skipped in unit tests or tested directly in ad-hoc binaries.
```

**What to Mock:**
- Guidelines: Currently, the codebase tests pure functions (like parsing keys or building config JSONs) which do not require mocking.

**What NOT to Mock:**
- Guidelines: Domain logic and format parsing should rely on concrete structs.

## Fixtures and Factories

**Test Data:**
```rust
let valid_base64 = "vmess://eyJhZGQiOiIxMjcuMC4wLjEiLCJwb3J0Ijo0NDMsImlkIjoibXktdXVpZCIsInBzIjoiVGVzdEtleSJ9";
```

**Location:**
- Test data (like sample VPN URIs) is hardcoded directly inside the test functions as string literals.

## Coverage

**Requirements:** None enforced.

**View Coverage:**
```bash
# Standard Rust coverage tools can be used (e.g., tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin
```

## Test Types

**Unit Tests:**
- High focus on unit testing domain logic: configuration generators (`xray_config.rs`, `singbox_config.rs`) and parsers (`key_parser.rs`).

**Integration Tests:**
- Not formally established in `tests/` folder. Ad-hoc `.rs` files in root are used for manual integration checks.

**E2E Tests:**
- Not used.

## Common Patterns

**UI Testing:**
```rust
// From src/ui/tests.rs
fn init_gtk() {
    let _ = gtk::init();
    let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
    if let Ok(res) = gio::Resource::from_data(&glib::Bytes::from(res_data)) {
        gio::resources_register(&res);
    }
}

#[test]
fn test_ui_components_init() {
    init_gtk();
    let _log_window = crate::ui::components::log_window::VrxxLogWindow::new();
}
```
Requires calling `gtk::init()` and manually registering GTK resources before instantiating UI components to prevent panics during test execution.

**Error Testing:**
```rust
#[test]
fn test_parse_invalid_base64() {
    let res = parse_vmess("vmess://!!!invalid&&&");
    assert!(res.is_err(), "Parser should return Err on invalid Base64");
}
```

---

*Testing analysis: 2024-05-24*
