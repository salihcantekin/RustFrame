# RustFrame

[![Build and Release](https://github.com/salihcantekin/RustFrame/actions/workflows/release.yml/badge.svg?branch=dev)](https://github.com/salihcantekin/RustFrame/actions/workflows/release.yml)
[![Latest Release](https://img.shields.io/github/v/release/salihcantekin/RustFrame?include_prereleases&sort=semver)](https://github.com/salihcantekin/RustFrame/releases)
[![Downloads](https://img.shields.io/github/downloads/salihcantekin/RustFrame/total.svg)](https://github.com/salihcantekin/RustFrame/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)



**RustFrame** is a privacy-focused, cross-platform screen region sharing tool. It lets you select any area of your screen and share only that region in video calls—without exposing your entire desktop.

**What does RustFrame do?**

- Lets you share a specific part of your screen (not the whole desktop)
- Works with all major video conferencing apps (Google Meet, Zoom, Teams, Discord)
- Supports multi-monitor setups and ultra-wide screens
- Keeps your private info hidden—share only what you want
- Fast and lightweight: minimal CPU/memory usage, instant region adjustment
- No account, no cloud, no telemetry—everything runs locally

**Key Features:**
- Pixel-perfect region selection with a draggable border
- Real-time preview window (“RustFrame - Share this window”) for sharing
- Move/resize the capture region live—even while sharing
- GPU-accelerated for smooth performance
- Customizable: border, highlights, cursor, FPS, and more
- Windows, macOS, and Linux support (Linux experimental)

See below for quick links and platform details. For more, see the [User Guide](docs/user-guide/).

---

## 🚀 Quick Links

- **[Download Latest Release](https://github.com/salihcantekin/RustFrame/releases/latest)**
- **[User Guide](docs/user-guide/)** – Installation, usage, troubleshooting
- **[Quick Start](docs/user-guide/quick-start.md)** – Get started in 30 seconds
- **[Developer Guide](docs/developer/)** – Architecture, building, contributing
- **[Technical Docs](docs/technical/)** – Platform details, performance, internals
- **[Profiles & Platform Settings](docs/profiles/)** – Platform-specific configuration
- **[Changelog](docs/changelog/)** – Version history
- **[Archive](docs/archive/)** – Historical/experimental docs

---

## 🎯 Project Overview

RustFrame is designed for:
- **Precise region capture** with pixel-perfect control
- **Multi-monitor support** and auto-detection
- **Content filtering** (include/exclude apps/windows)
- **GPU-accelerated performance**
- **Customizable UI** (borders, highlights, cursor, FPS)
- **Cross-platform**: Windows, macOS, Linux (experimental)

See [docs/README.md](docs/README.md) for full documentation structure and navigation.

---

## 📚 Documentation Structure

All detailed documentation is under the [docs/](docs/) directory:

- **User Guide**: [docs/user-guide/](docs/user-guide/)
- **Developer Guide**: [docs/developer/](docs/developer/)
- **Technical Docs**: [docs/technical/](docs/technical/)
- **Profiles & Platform Settings**: [docs/profiles/](docs/profiles/)
- **Changelog**: [docs/changelog/](docs/changelog/)
- **Archive**: [docs/archive/](docs/archive/)

Each section contains platform-specific details and comparison tables where relevant.

---

## 🖥️ Supported Platforms (Summary)

| Platform      | Status         | Capture Method                | Rendering         |
|--------------|----------------|-------------------------------|-------------------|
| Windows 10/11| ✅ Full Support | Windows Graphics Capture (WGC)| DirectX 11 (GPU)  |
| macOS 12.3+  | ✅ Full Support | ScreenCaptureKit              | Metal (GPU)       |
| macOS 10.15+ | ✅ Supported    | CoreGraphics                  | CPU fallback      |
| Linux        | 🚧 Experimental| X11/Wayland                   | wgpu              |

For details, see [docs/technical/](docs/technical/) and [docs/user-guide/](docs/user-guide/).

---

## 🗂️ Archive & Historical Docs

Past experiments, failed approaches, and technical lessons are preserved in [docs/archive/](docs/archive/). See archive README for details.

---

## 🛠️ Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/developer/](docs/developer/) for guidelines.

### Capture Features
- **Region Selection**: Pixel-perfect control via draggable/resizable border
- **Multi-Monitor**: Auto-detects when you drag border to different display
- **Live Adjustment**: Move/resize region while capturing (no restart needed)
- **Multiple Capture Methods**:
  - Windows: GPU (WGC) or CPU (GDI) fallback
  - macOS: ScreenCaptureKit (GPU) or CoreGraphics (CPU)
  - Linux: PipeWire (Wayland) or X11

### Interaction Features  
- **Cursor Control**: Show/hide your cursor in capture
- **Click Highlights**: Visual feedback with customizable colors and dissolve effects
- **Recording Indicator**: "REC" badge shows when capturing is active

### Share Content (Include/Exclude)
- **Modes**: Capture All, Exclude (hide selected windows/apps), Include Only (capture only selected)
- **Manual Refresh**: Load running apps/windows on demand; no auto-polling
- **Search & View**: Filter by text; switch between application or window list views
- **Selection UX**: Multi-select and add/remove; current picks visible at the top; clear-all control
- **Preview Safety**: Auto-exclude preview window toggle to avoid mirror loops
- **Platform Coverage**: macOS (CG + NSWorkspace) and Windows (EnumWindows, visible/non-cloaked, non-tool windows); Linux planned

### Customization
- **Border Styling**: Adjustable width, color, and visibility
- **Performance Tuning**: FPS control (15-144 FPS), GPU acceleration toggle
- **Capture Profiles**: Pre-configured settings for different apps (Discord, Meet, etc.)
- **Remember Region**: Automatically restore last capture area

## 🖥️ Platform Support

| Platform | Status | Capture Method | Performance |
|----------|--------|----------------|-------------|
| **Windows 10/11** | ✅ Stable | Windows Graphics Capture (WGC) | ~8-10% CPU |
| **macOS 12.3+** | ✅ Stable | ScreenCaptureKit + Metal | ~5-8% CPU |
| **macOS 10.15-12.2** | ✅ Supported | CoreGraphics | ~10-15% CPU |
| **Linux** | 🚧 Experimental | PipeWire / X11 + wgpu | Varies |

→ See [Platform-Specific Documentation](docs/developer/platform-specific.md) for technical details

## 🏗️ Architecture

RustFrame uses a modular, cross-platform architecture:

```
UI Layer (Tauri + React)
    ↓
Application Core (Rust)
    ↓
Platform Abstractions (Traits)
    ↓
Platform-Specific Implementations
```

### Key Components

- **Capture Engines** - Platform-specific screen capture (WGC, SCK, X11)
- **Window Management** - Hollow border and preview windows
- **Rendering Pipeline** - GPU or CPU rendering based on platform
- **Settings Management** - Persistent configuration with JSON

→ See [Architecture Overview](docs/developer/README.md#architecture-overview) for details

## 🛠️ Building from Source

### Prerequisites

- **Rust** (latest stable)
- **Node.js** (v18+) for UI
- **Platform Tools**:
  - Windows: Visual Studio Build Tools (MSVC)
  - macOS: Xcode Command Line Tools
  - Linux: GCC, GTK3, WebKit2GTK

### Build Steps

```bash
# Clone repository
git clone https://github.com/salihcantekin/RustFrame
cd RustFrame

# Development mode
cargo tauri dev

# Release build
cargo tauri build
```

→ See [Building Guide](docs/developer/building.md) for detailed instructions

## 🤝 Contributing

We welcome contributions! Here's how to get started:

1. **Read the Guides**
   - [CONTRIBUTING.md](CONTRIBUTING.md) - Contribution guidelines
   - [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) - Community standards
   - [Developer Guide](docs/developer/) - Technical documentation

2. **Find an Issue**
   - Check [GitHub Issues](https://github.com/salihcantekin/RustFrame/issues)
   - Look for `good-first-issue` label

3. **Submit a PR**
   - Fork the repository
   - Create a feature branch
   - Make your changes
   - Submit a pull request

## 📝 Known Issues & Roadmap

### Known Issues
- **macOS**: Click highlights use CPU instead of GPU (optimization planned)
- **Linux**: PipeWire support experimental, may have compatibility issues

→ See [Known Issues](docs/developer/known-issues.md) for complete list

### Planned Features
- [ ] Zero-copy GPU texture sharing (eliminate CPU copy)
- [ ] Global hotkeys for start/stop
- [ ] Region presets (save/load favorite regions)

## 📚 Technical Resources

### Documentation
- **[Complete Documentation Index](docs/)** - All documentation
- **[GPU Optimization Details](docs/technical/gpu-optimization.md)** - Platform GPU strategies
- **[Multi-Monitor Implementation](docs/technical/multi-monitor.md)** - Display detection
- **[Color Format Handling](docs/technical/color-formats.md)** - BGRA vs RGBA

### API References
- [Windows Graphics Capture API](https://docs.microsoft.com/en-us/uwp/api/windows.graphics.capture)
- [macOS ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)
- [Tauri Framework](https://tauri.app/)

## 📄 License

MIT License - see [LICENSE](LICENSE) for details

## 🔗 Links

- **Repository**: https://github.com/salihcantekin/RustFrame
- **Releases**: https://github.com/salihcantekin/RustFrame/releases
- **Issues**: https://github.com/salihcantekin/RustFrame/issues
- **Discussions**: https://github.com/salihcantekin/RustFrame/discussions
- **Changelog**: [CHANGELOG.md](CHANGELOG.md)
- **Security**: [SECURITY.md](SECURITY.md)

---

**Made with ❤️ using Rust + Tauri + React**

**Star ⭐ this repo if RustFrame helps you!**


### Windows Graphics APIs
- **COM Programming**: Creating and managing COM objects in Rust
- **Windows.Graphics.Capture**: Modern screen capture API
- **Direct3D 11**: GPU device creation, texture management
- **DXGI**: DirectX Graphics Infrastructure and swapchains
- **WinRT Interop**: Bridging Win32 and WinRT APIs

### Rust Systems Programming
- **Unsafe Code**: Proper use of `unsafe` with justification
- **FFI**: Calling Windows APIs through `windows` crate
- **Resource Management**: RAII, Drop implementations
- **Thread Safety**: Arc, Mutex, Send/Sync

### Graphics Programming
- **GPU Rendering**: wgpu render pipelines
- **Shader Programming**: WGSL shaders
- **Texture Management**: Staging, mapping, uploading
- **Swapchain Presentation**: Frame synchronization

## 🙏 Acknowledgments

- **Microsoft**: Windows.Graphics.Capture API documentation
- **wgpu Community**: Excellent graphics API and examples
- **windows-rs**: Official Rust bindings for Windows

## 📄 License

MIT License - See LICENSE file for details.

---

**Developed by [Salih Cantekin](https://github.com/salihcantekin)**

Built with ❤️ and Rust 🦀 for the Windows platform
