
## 2024-05-18 - Caching D-Bus System Connection

**Learning:** Recreating `zbus::Connection::system().await` per D-Bus proxy call introduces significant and unnecessary IPC overhead (DBus handshake + socket creation), degrading performance especially in UI applications that poll statuses or interact frequently with system daemons.
**Action:** Use `tokio::sync::OnceCell` to instantiate the global D-Bus connection once during startup and clone it for subsequent proxy object initializations. Always verify that system/IPC connections are reused appropriately, preventing connection bloat and reducing latency.
## 2024-05-30 - Optimize D-Bus Connection Caching
**Learning:** Re-establishing the D-Bus connection for `is_running` polling and other backend operations introduces unnecessary IPC overhead.
**Action:** The backend connection `zbus::Connection` can be cached instead of recreating it upon every `get_proxy()` request.
## 2024-05-30 - Optimize D-Bus Proxy Caching
**Learning:** Re-establishing the D-Bus `DaemonProxy` per operation (e.g. `is_running`) even when using a cached connection still introduces significant runtime overhead due to proxy object recreation.
**Action:** The backend connection `DaemonProxy` itself can be cached entirely via `tokio::sync::OnceCell` in `CoreBackend`, completely eliminating repeated creation overhead for standard IPC calls.
## 2024-05-15 - reqwest::Client connection pool dropping overhead in DaemonClient
**Learning:** Frequent polling or initialization (e.g., UI components making API calls) using `DaemonClient::new()` was previously recreating `reqwest::Client` on every call. This drops the internal connection pool and introduces significant overhead due to constant TCP connection setup and teardown, as well as allocation overhead.
**Action:** When a client (`reqwest::Client`) handles frequent requests or SSE polling to a backend (like the privileged daemon in this app), always globally cache it (e.g., via `std::sync::OnceLock`) inside the client wrapper constructor to preserve the connection pool. `reqwest::Client` uses `Arc` internally and is cheap to clone.
