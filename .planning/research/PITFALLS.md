# Domain Pitfalls

**Domain:** Linux VPN client using Rust, GTK4/Libadwaita, Xray, Sing-box
**Researched:** Current

## Critical Pitfalls

Mistakes that cause rewrites or major security issues.

### Pitfall 1: Monolithic Privilege Escalation (Running GUI as Root)
**What goes wrong:** The application requires `sudo` to launch in order to manage network routing and TUN interfaces.
**Why it happens:** Sing-box/Xray require root privileges to establish a TUN interface for global VPN routing. The path of least resistance is running the entire GTK app as root.
**Consequences:** Massive security vulnerability. GTK, Wayland/X11, and image parsing libraries are not designed to be run as root. A bug in the UI layer could lead to complete system compromise.
**Prevention:** Adopt a split architecture (Daemon/Client). Run the GTK GUI as an unprivileged user. Create a minimal Rust backend service (managed via systemd or polkit) that runs as root exclusively to configure the TUN interface, modify routing tables, and launch the proxy core.
**Detection:** The `.desktop` file uses `pkexec` for the main binary, or the app throws permission errors when run as a normal user.

### Pitfall 2: DNS Leaks and systemd-resolved Conflicts
**What goes wrong:** Traffic is tunneled, but DNS queries leak to the ISP, or internet connectivity breaks entirely when the VPN connects.
**Why it happens:** Linux DNS resolution is heavily fragmented (systemd-resolved, NetworkManager, bare `/etc/resolv.conf`). Xray/Sing-box might not perfectly hijack local DNS automatically, or they conflict with existing stub resolvers.
**Consequences:** Complete loss of privacy (ISP sees all visited domains) or a broken user experience (no internet).
**Prevention:** 
- Explicitly configure `fakedns` or DNS hijacking rules within Xray/Sing-box `inbounds`.
- For global routing, use DBus to cleanly integrate with `systemd-resolved` (e.g., setting the TUN interface as the default routing domain for DNS) rather than forcefully overwriting `/etc/resolv.conf`.
**Detection:** DNS leak test websites show the ISP's DNS servers instead of the proxy's DNS.

### Pitfall 3: Zombie Proxy Processes and Port Conflicts
**What goes wrong:** The user closes the Rust GUI, but the `xray` or `sing-box` child process continues running in the background invisibly.
**Why it happens:** Rust `std::process::Command` does not automatically kill child processes when the parent drops. If the Rust app panics or is killed (SIGKILL), the child process becomes orphaned.
**Consequences:** The proxy remains active, consuming resources. Relaunching the app fails with "Address already in use" (e.g., port 1080/2080 is bound).
**Prevention:** 
- Use process groups (`setsid` / `setpgid`) and catch `SIGINT`/`SIGTERM` to gracefully shut down the proxy core.
- Alternatively, offload proxy lifecycle management to a systemd transient service, which guarantees cleanup.
**Detection:** `ps aux | grep sing-box` shows the process still running after closing the app.

### Pitfall 4: The GObject "Boilerplate Wall" and State Desync
**What goes wrong:** The UI codebase becomes an unmaintainable tangle of `glib::clone!`, `Rc<RefCell<T>>`, and reference cycles.
**Why it happens:** Rust's strict ownership model clashes violently with GTK's object-oriented, reference-counted GObject model. Trying to write complex UI logic in "pure" Rust without an abstraction layer leads to massive boilerplate.
**Consequences:** UI freezes, memory leaks from un-dropped widget references, and massive technical debt that slows feature development.
**Prevention:** Use a reactive component framework like **Relm4**. It abstracts the GObject event loops and state synchronization into an idiomatic Rust message-passing architecture (similar to Elm).
**Detection:** UI files contain deeply nested closures with 5+ cloned variables.

## Moderate Pitfalls

### Pitfall 1: Libadwaita Namespace Shadowing
**What goes wrong:** Missing Libadwaita features (like automatic dark mode or adaptive layouts) or runtime panics when adding children to a window.
**Prevention:** Never mix `gtk::ApplicationWindow` and `adw::ApplicationWindow`. Always use the `adw::` namespace for top-level windows and header bars to ensure correct styling and behavior.

### Pitfall 2: Proxy Core Supply Chain Risks
**What goes wrong:** Integrating backdoored or severely outdated versions of Xray or Sing-box.
**Prevention:** Do not use random third-party bash scripts for installation. Download pre-compiled binaries directly from the official GitHub releases (`SagerNet/sing-box` or `XTLS/Xray-core`), verify their checksums in the Rust build script or download manager, and pin specific stable versions.

### Pitfall 3: Flatpak & GSettings Crashes
**What goes wrong:** App crashes instantly on startup with "Schema not found" during local development.
**Prevention:** GTK apps relying on GSettings need the schema compiled and accessible. Ensure your build system (e.g., meson or a justfile) compiles `glib-compile-schemas` and sets the `XDG_DATA_DIRS` environment variable correctly during local testing.

## Minor Pitfalls

### Pitfall 1: Hardcoded UI Definitions
**What goes wrong:** Defining complex visual layouts programmatically in Rust code makes it hard to visualize and maintain.
**Prevention:** Use XML `.ui` files or **Blueprint** for layout definitions. Load them at runtime or compile them into the binary using `gtk::Builder` and `gio::Resource`.

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| **Architecture / Setup** | Root-level GUI | Design a polkit/DBus daemon from day one. Do not defer this. |
| **Core Integration** | Zombie processes | Implement strict child process lifecycle tests before building the UI. |
| **UI Development** | State management hell | Adopt Relm4 or a strict MVC pattern early. |
| **Routing / TUN** | DNS Leaks | Test exclusively with systemd-resolved, as it is the default on modern Linux (Ubuntu/Fedora). |

## Sources

- [HIGH] GTK/Rust Official Docs (glib::clone!, GObject model)
- [HIGH] Relm4 Documentation (State management solutions)
- [HIGH] Xray/Sing-box GitHub Repositories (Binary sources, TUN configurations)
- [MEDIUM] Cybersecurity reports on Proxy Abuse (Supply chain risks)
- [MEDIUM] Linux network stack documentation (systemd-resolved integration)
