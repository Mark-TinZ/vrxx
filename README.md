# vrxx

A modern, high-performance graphical interface for **Xray-core** and **Sing-box**, built with Rust and GTK4/Libadwaita. Designed with focus on privacy, security, and the GNOME Human Interface Guidelines (HIG).

## ✨ Key Features

- **🚀 Multiple Cores Support**: Switch between Xray-core and Sing-box seamlessly.
- **🛡️ Streamer Mode**: Mask sensitive information (IP addresses) with one click for safe screen sharing.
- **💾 SSD-Safe Logging**: High-performance asynchronous logging that batches writes to protect your SSD from excessive wear.
- **🌍 Geo-Intelligence**: Automatic resolution of server location and timezone.
- **📝 Advanced Routing**: Manage whitelists with wildcard support (e.g., `*.ru`, `*.google.com`) translated directly to core regex rules.
- **🎨 GNOME Native**: Fully compliant with Libadwaita standards, including dark mode support and adaptive UI.
- **⚡ Non-blocking UI**: Asynchronous backend management ensures the interface never freezes.

## 🛠️ Requirements

- **Rust** (Latest Stable)
- **GTK4** & **Libadwaita** development headers
- **Xray-core** or **Sing-box** binary installed in your PATH

## 📦 Building

```bash
# Compile and run
cargo run --release
```

## 🤝 Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## 📜 License

This project is licensed under the MPL-2.0 License.
