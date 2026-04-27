## 2024-03-24 - D-Bus Connection Caching
**Learning:** `zbus::Connection` in zbus crate is internally reference-counted (via `Arc`), making it very cheap and safe to clone. To avoid severe IPC overhead during frequent polling (e.g., UI updates), D-Bus connections should be cached globally via `tokio::sync::OnceCell` rather than recreated on every call.
**Action:** Implement global connection caching in `src/ipc.rs` and refactor the application to use this cached connection instead of `Connection::system().await` repeatedly.
