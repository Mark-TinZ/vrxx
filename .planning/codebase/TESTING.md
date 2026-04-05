# Testing Patterns

**Analysis Date:** 2025-02-11

## Test Framework

**Runner:**
- `cargo test`

**Assertion Library:**
- Standard Rust assertions (`assert_eq!`, `assert!`).
- `expect()` for unwrapping and descriptive errors in tests.

**Run Commands:**
```bash
cargo test              # Run all unit and integration tests
cargo test -- --nocapture # Run tests and show stdout/stderr
```

## Test File Organization

**Location:**
- Unit tests are co-located within source files using `#[cfg(test)] mod tests`.
- UI-specific tests are in `src/ui/tests.rs`.
- Experimental/playground scripts like `test_subprocess.rs` are at the project root (not part of the standard test suite).

**Naming:**
- Follows module naming for co-located tests.

**Structure:**
```
src/
├── domain/
│   ├── key_parser.rs (with mod tests)
│   ├── xray_config.rs (with mod tests)
│   └── singbox_config.rs (with mod tests)
└── ui/
    └── tests.rs
```

## Test Structure

**Suite Organization:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_name() {
        // ... test logic
    }
}
```

**Patterns:**
- **UI Initialization:** In `src/ui/tests.rs`, `init_gtk()` is used to initialize GTK and load GResources before testing widget instantiation.
- **External Dependency Checks:** In `src/domain/xray_config.rs` and `src/domain/singbox_config.rs`, tests check for tool availability (e.g., `xray version`) and skip if the tool is not found.

## Mocking

**Framework:** None detected.

**Patterns:**
- No explicit mocking library (e.g., `mockall`) is used.
- Manual mocking is achieved by creating sample data structures (e.g., `ParsedKey` in config tests).

**What to Mock:**
- VPN keys for configuration generation tests.
- System configurations (simulated using `AppSettings`).

## Fixtures and Factories

**Test Data:**
- Hardcoded URLs and JSON strings for key parsing and config building.
```rust
let url = "vless://a3482e88-6860-4a1c-914c-4b4ea5c49f87@1.2.3.4:443?security=reality&...";
```

**Location:**
- Within test functions or local helper modules.

## Coverage

**Requirements:** None enforced.

**View Coverage:**
- Not explicitly configured (e.g., no `cargo-tarpaulin` setup).

## Test Types

**Unit Tests:**
- `src/domain/key_parser.rs`: Tests VPN key URL parsing for various protocols (VLESS, VMess).
- `src/domain/xray_config.rs`, `src/domain/singbox_config.rs`: Tests JSON configuration generation for VPN cores.

**Integration Tests:**
- `src/ui/tests.rs`: Tests instantiation of UI components and pages using GTK.

**E2E Tests:**
- Not used.

## Common Patterns

**Async Testing:**
- `tokio::runtime::Runtime` is used manually in production code. No `#[tokio::test]` was found in the test suite yet.
- For async-heavy code, the pattern is to use `block_on` from a manually created runtime.

**Error Testing:**
- Explicitly testing for `is_err()` on invalid input:
```rust
let res = parse_vmess("vmess://!!!invalid&&&");
assert!(res.is_err(), "Parser should return Err on invalid Base64");
```

---

*Testing analysis: 2025-02-11*
