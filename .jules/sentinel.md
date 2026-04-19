## 2026-05-22 - [Secure File Permissions]
**Vulnerability:** Application settings (`settings.json`) and logs (`app.log`, `daemon.log`, `core.log`) were created with default system permissions (0664), allowing other users on the system to read sensitive data like VPN UUIDs, passwords, and proxy logs.
**Learning:** `fs::File::create` or `fs::write` in Rust use default umask. On many Linux systems, this defaults to 0644 or 0664. `OpenOptions::mode` only applies at creation time; pre-existing files retain their old permissions unless explicitly changed.
**Prevention:** Use `std::os::unix::fs::OpenOptionsExt` to set `mode(0o600)` during creation AND `std::os::unix::fs::PermissionsExt` with `set_permissions` to ensure 0600 even if the file already exists.
