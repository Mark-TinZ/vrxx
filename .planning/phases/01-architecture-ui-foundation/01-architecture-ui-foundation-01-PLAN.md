---
phase: 01-architecture-ui-foundation
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - data/ru.mark.vrxx.daemon.conf
  - data/ru.mark.vrxx.daemon.service.in
  - data/ru.mark.vrxx.policy
  - data/meson.build
autonomous: true
requirements:
  - CORE-03

must_haves:
  truths:
    - Cargo builds successfully with zbus and zbus_polkit dependencies
    - Meson correctly installs the system bus and polkit configuration files
  artifacts:
    - path: data/ru.mark.vrxx.daemon.conf
      provides: D-Bus system bus permissions for the daemon
      contains: <busconfig>
    - path: data/ru.mark.vrxx.daemon.service.in
      provides: D-Bus system activation for the daemon
      contains: Exec=@bindir@/vrxx --daemon
    - path: data/ru.mark.vrxx.policy
      provides: Polkit actions for privileged tasks
      contains: <action id="ru.mark.vrxx.daemon.start-proxy">
  key_links:
    - from: data/meson.build
      to: data/ru.mark.vrxx.daemon.conf
      via: install_data to datadir/dbus-1/system.d
      pattern: install_dir: get_option('datadir') / 'dbus-1' / 'system.d'
---

<objective>
Introduce the required dependencies (`zbus`, `zbus_polkit`) and create the system configuration files (D-Bus system bus, System activation service, and Polkit rules) for the new privilege-separated daemon.

Purpose: To transition the app from running as a monolithic, potentially privileged process, to a securely separated client-core model where an unprivileged UI speaks to a background root daemon.
Output: System policies installed so that the daemon can claim its D-Bus name and authenticate users.
</objective>

<execution_context>
@$HOME/.gemini/get-shit-done/workflows/execute-plan.md
@$HOME/.gemini/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@data/meson.build
@Cargo.toml
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add dependencies to Cargo.toml</name>
  <files>Cargo.toml</files>
  <action>Add `zbus = { version = "5", features = ["tokio"] }` and `zbus_polkit = "5"` to dependencies block. Keep existing tokio and async dependencies.</action>
  <verify>
    <automated>cargo check</automated>
  </verify>
  <done>Cargo.toml is updated and cargo resolves the dependencies successfully.</done>
</task>

<task type="auto">
  <name>Task 2: Create D-Bus and Polkit XML policies</name>
  <files>data/ru.mark.vrxx.daemon.conf, data/ru.mark.vrxx.daemon.service.in, data/ru.mark.vrxx.policy</files>
  <action>
    - Create `data/ru.mark.vrxx.daemon.conf`: A D-Bus system bus policy file allowing user `root` to own the name `ru.mark.vrxx.daemon` and allowing anyone to send method calls to it.
    - Create `data/ru.mark.vrxx.daemon.service.in`: A system bus activation service file with `Name=ru.mark.vrxx.daemon`, `Exec=@bindir@/vrxx --daemon`, and `User=root`.
    - Create `data/ru.mark.vrxx.policy`: A Polkit policy file defining the action `ru.mark.vrxx.daemon.start-proxy` requiring `auth_admin` to execute.
  </action>
  <verify>
    <automated>test -f data/ru.mark.vrxx.daemon.conf && test -f data/ru.mark.vrxx.policy</automated>
  </verify>
  <done>The three XML files are created with correct structural content.</done>
</task>

<task type="auto">
  <name>Task 3: Update Meson build rules</name>
  <files>data/meson.build</files>
  <action>
    Modify `data/meson.build` to install the new files:
    - Install `ru.mark.vrxx.daemon.conf` to `get_option('datadir') / 'dbus-1' / 'system.d'` (using `install_data`).
    - Use `configure_file` for `ru.mark.vrxx.daemon.service.in`, outputting `ru.mark.vrxx.daemon.service`, installed to `get_option('datadir') / 'dbus-1' / 'system-services'`.
    - Install `ru.mark.vrxx.policy` to `get_option('datadir') / 'polkit-1' / 'actions'` (using `install_data`).
  </action>
  <verify>
    <automated>meson setup builddir --reconfigure || meson setup builddir</automated>
  </verify>
  <done>Meson is configured to install D-Bus and Polkit system files correctly.</done>
</task>

</tasks>

<verification>
Ensure cargo check completes with zbus and polkit dependencies, and meson parses the build directives correctly.
</verification>

<success_criteria>
The project successfully compiles and the infrastructure code for the D-Bus daemon is available and properly hooked into the build system.
</success_criteria>

<output>
After completion, create `.planning/phases/01-architecture-ui-foundation/01-architecture-ui-foundation-01-SUMMARY.md`
</output>
