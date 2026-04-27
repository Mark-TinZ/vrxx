## 2024-05-30 - Optimize D-Bus Connection Caching
**Learning:** Re-establishing the D-Bus connection for `is_running` polling and other backend operations introduces unnecessary IPC overhead.
**Action:** The backend connection `zbus::Connection` can be cached instead of recreating it upon every `get_proxy()` request.
