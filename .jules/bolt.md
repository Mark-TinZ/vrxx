
## 2024-05-18 - Caching D-Bus System Connection

**Learning:** Recreating `zbus::Connection::system().await` per D-Bus proxy call introduces significant and unnecessary IPC overhead (DBus handshake + socket creation), degrading performance especially in UI applications that poll statuses or interact frequently with system daemons.
**Action:** Use `tokio::sync::OnceCell` to instantiate the global D-Bus connection once during startup and clone it for subsequent proxy object initializations. Always verify that system/IPC connections are reused appropriately, preventing connection bloat and reducing latency.
## 2024-05-30 - Optimize D-Bus Connection Caching
**Learning:** Re-establishing the D-Bus connection for `is_running` polling and other backend operations introduces unnecessary IPC overhead.
**Action:** The backend connection `zbus::Connection` can be cached instead of recreating it upon every `get_proxy()` request.
## 2024-05-30 - Optimize D-Bus Proxy Caching
**Learning:** Re-establishing the D-Bus `DaemonProxy` per operation (e.g. `is_running`) even when using a cached connection still introduces significant runtime overhead due to proxy object recreation.
**Action:** The backend connection `DaemonProxy` itself can be cached entirely via `tokio::sync::OnceCell` in `CoreBackend`, completely eliminating repeated creation overhead for standard IPC calls.
## 2024-05-30 - Fix layout thrashing in bulk log insertion
**Learning:** In GTK4 `GtkTextView` manipulations, performing layout-triggering operations (like `scroll_to_mark`) inside a bulk insertion loop causes severe O(n²) layout thrashing. Such UI operations must be deferred until after the text insertion loop completes.
**Action:** Extract log insertion from scrolling behavior to allow for `append_log_no_scroll` during loops, only calling `scroll_to_bottom` once the load loop finishes.
