# Phase 1, Plan 03 Summary: Refactor backend to connect to DBus proxy

The refactoring of the application's backend to communicate with the privileged D-Bus daemon is complete. This change removes the use of `pkexec` from the GTK UI and establishes a safe way to delegate privileged tasks.

## Changes Made:

### `src/ipc.rs`
- Added a `#[proxy]` trait for the `Daemon` interface. This allows `zbus` to automatically generate the `DaemonProxy` struct used by the backend client.
- The proxy includes `ping()`, `start_proxy()`, and `stop_proxy()` methods.

### `src/backend.rs`
- Removed all logic that utilized `pkexec setcap` for TUN mode permissions.
- Replaced it with an informative error message directing users to the `vrxx-daemon` or providing the manual `setcap` command if the daemon is unavailable.
- Integrated `zbus` to connect to the system bus.
- Added asynchronous `ping()` calls to the D-Bus daemon in both `CoreBackend::new()` (on initialization) and `CoreBackend::start()` (when connecting).
- The `ping()` calls are executed in a separate background thread with a dedicated `tokio` runtime to ensure they do not block the main GTK UI thread or rely on a global async runtime.
- Added graceful handling of daemon unavailability: failures to connect to D-Bus or the daemon are logged as warnings rather than crashing the application.

## Verification:

- **Cargo Check:** `cargo check` passes with no errors related to the changes.
- **`pkexec` Removal:** Verified that no `pkexec` strings remain in `src/backend.rs`.
- **Architectural Integrity:** The UI now correctly acts as a low-privilege process communicating with a privileged service over D-Bus.

## Next Steps:

- **Phase 2:** Implement the actual logic in the daemon to perform privileged operations like setting capabilities and managing the VPN core process.
- **Phase 1, Plan 04:** Begin any remaining UI foundation work as scheduled.
