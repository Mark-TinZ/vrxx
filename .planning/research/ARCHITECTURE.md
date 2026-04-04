# Architecture Patterns

**Domain:** Linux VPN client using Rust, GTK4/Libadwaita, Xray, Sing-box
**Researched:** 2024-05-24

## Recommended Architecture

A modern Linux VPN client leveraging Rust, GTK4, and underlying proxy cores (Xray/Sing-box) should follow a **Privilege-Separated Client-Core Architecture**. This ensures security by isolating the unprivileged graphical interface from the privileged networking operations (like managing TUN devices and routing tables).

The architecture consists of three main layers: the UI Frontend, the Privileged Backend Daemon, and the Proxy Cores.

### Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| **Frontend (GUI)** | Unprivileged user interface (Rust, GTK4, Libadwaita). Handles user interaction, configuration management (Profiles, Subscriptions), and status visualization. | Backend Daemon (via D-Bus), User |
| **Backend (Daemon)** | Privileged service (Rust, Tokio). Manages the VPN lifecycle, applies network configurations (`ip route`, `TUN`), and orchestrates the proxy cores. Uses Polkit for authorization. | Frontend (via D-Bus), Proxy Cores, OS Network Stack |
| **Proxy Cores** | Pre-compiled binaries (`xray-core`, `sing-box`). Handle actual data tunneling, encryption, and protocol-specific logic (VLESS, Trojan, etc.). | Backend Daemon (via Subprocess/stdin/stdout), Remote Servers |
| **IPC Layer** | Inter-Process Communication (D-Bus via `zbus`). Defines methods (Connect, Disconnect) and signals (StatusChanged, TrafficStats). | Frontend, Backend Daemon |

### Data Flow

1. **User Input:** User adds a subscription or profile in the Frontend.
2. **Domain Translation:** The Frontend stores this in its local database/config file.
3. **Connection Request:** User clicks "Connect". The Frontend sends a `Connect(profile_id)` command to the Backend via D-Bus.
4. **Configuration Generation:** The Backend reads the profile, generates the complex JSON (for Xray) or JSON/HCL (for Sing-box) configuration required by the core.
5. **Core Execution:** The Backend spawns the proxy core process (`xray` or `sing-box`) as a subprocess, passing the generated config.
6. **Network Routing:** The Backend configures the OS (`TUN` device creation, routing table modification, or `iptables`/`nftables` rules for transparent proxying).
7. **Telemetry:** The core reports traffic stats to the Backend (via API or stdout). The Backend emits D-Bus signals (`BytesTransferred`). The Frontend updates the UI dynamically.

## Patterns to Follow

### Pattern 1: Privilege Separation (Daemon-UI)
**What:** Splitting the app into an unprivileged GUI and a privileged daemon service.
**When:** Always, since Linux requires `CAP_NET_ADMIN` to create TUN interfaces and modify routing, which a GUI should not have.
**Example:**
```rust
// zbus D-Bus interface definition in a shared crate
#[dbus_interface(name = "org.vpn.Daemon")]
trait VpnDaemon {
    async fn connect(&self, profile_id: String) -> zbus::fdo::Result<()>;
    async fn disconnect(&self) -> zbus::fdo::Result<()>;
    #[dbus_interface(signal)]
    async fn status_changed(ctxt: &SignalContext<'_>, status: VpnStatus) -> zbus::Result<()>;
}
```

### Pattern 2: Domain-to-Core Configuration Translation
**What:** The application manages high-level domain entities (Profiles, Subscriptions, Routing Rules) and dynamically generates the low-level JSON/HCL required by Xray/Sing-box at connection time.
**When:** Managing complex proxy cores.
**Example Domain Models:**
- **Profile:** Protocol (VLESS, VMess), Address, Port, TLS settings, UUID.
- **Routing Rule:** Traffic flow logic (Direct, Proxy, Block).
- **Subscription:** Remote URL for profile synchronization.

### Pattern 3: MVC with GTK4 `gio::ListModel`
**What:** Using GTK4's robust list models to bind internal data structures to UI elements (like server lists).
**When:** Displaying dynamic lists of profiles or subscriptions.
**Example:** Wrapping Rust structs in `glib::Object` to use `gio::ListStore` and `gtk::ListView` for efficient UI rendering.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Monolithic Privilege (Running GUI as Root)
**What:** Requiring the user to run the entire GTK application via `sudo` or Polkit `pkexec` to allow it to manage network interfaces.
**Why bad:** Huge security risk. A vulnerability in the complex GUI framework (GTK/WebKit) could lead to full system compromise.
**Instead:** Use the Daemon-UI separation pattern with D-Bus.

### Anti-Pattern 2: Blocking the GTK Main Thread
**What:** Performing core process spawning, network requests (fetching subscriptions), or heavy JSON serialization on the UI thread.
**Why bad:** Freezes the UI, leading to a poor user experience and potential OS "Application Not Responding" warnings.
**Instead:** Offload all heavy lifting to the Toko async runtime or separate threads, and communicate back to the UI via `glib::MainContext::channel`.

### Anti-Pattern 3: Exposing Raw JSON to the User
**What:** Forcing the user to manually edit Xray or Sing-box JSON configuration files.
**Why bad:** High barrier to entry, prone to syntax errors.
**Instead:** Build a comprehensive GUI that abstracts the complexity into domain models (Forms, Toggles, Dropdowns) and generates the JSON automatically.

## Scalability Considerations

| Concern | 10 Profiles | 1,000 Profiles (Large Subscription) |
|---------|-------------|-------------------------------------|
| **UI Rendering** | Simple `gtk::Box` iteration works. | Must use `gtk::ListView` + `gio::ListStore` for widget recycling. |
| **Config Generation** | Instantaneous. | Pre-caching domain models; avoid blocking UI during parse/generation. |
| **Subscription Update** | Single HTTP request. | Concurrent fetching, differential updates to avoid UI stutter. |

## Sources

- [Linux Daemon Architecture (zbus/D-Bus)](https://dbus2.github.io/zbus/) - HIGH confidence
- [GTK4 Rust Bindings & Architecture](https://gtk-rs.org/gtk4-rs/stable/latest/book/) - HIGH confidence
- [Project V / Xray-core Architecture](https://xtls.github.io/en/development/architecture.html) - HIGH confidence
- [Sing-box Design & Architecture](https://sing-box.sagernet.org/design/) - HIGH confidence
