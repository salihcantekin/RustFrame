# RustFrame

[![Build and Release](https://github.com/salihcantekin/RustFrame/actions/workflows/release.yml/badge.svg?branch=dev)](https://github.com/salihcantekin/RustFrame/actions/workflows/release.yml)
[![Latest Release](https://img.shields.io/github/v/release/salihcantekin/RustFrame?include_prereleases&sort=semver)](https://github.com/salihcantekin/RustFrame/releases)
[![Downloads](https://img.shields.io/github/downloads/salihcantekin/RustFrame/total.svg)](https://github.com/salihcantekin/RustFrame/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**A cross-platform screen region capture tool for precise screen sharing in video calls.**

📦 **[Download Latest Release](https://github.com/salihcantekin/RustFrame/releases/latest)** | 📚 **[Documentation](docs/)** | 🚀 **[Quick Start](docs/user-guide/quick-start.md)**

RustFrame lets you capture and share a specific region of your screen in video calls (Google Meet, Zoom, Teams, Discord) without exposing your entire desktop. Perfect for privacy-conscious sharing, multi-monitor setups, or ultra-wide displays.

## ✨ Key Features

- 🎯 **Precise Region Capture** - Select any screen area with pixel-perfect control
- 🖥️ **Multi-Monitor Support** - Capture from any display, auto-detects when you drag between monitors
- ⚡ **GPU-Accelerated** - High-performance capture with minimal CPU usage (~8-10%)
- 🎨 **Customizable** - Click highlights, cursor visibility, border styling, FPS tuning
- 🔧 **Cross-Platform** - Windows, macOS, and Linux (experimental)
- 🎮 **Real-Time Adjustment** - Move and resize capture region while sharing

## 🚀 Quick Start

### Installation

**Windows**: Extract ZIP and run `RustFrame.exe`  
**macOS**: Drag to Applications folder  
**Linux**: Make AppImage executable and run  

→ See [Installation Guide](docs/user-guide/installation.md) for detailed instructions

### Usage (30 seconds)

1. **Launch RustFrame** → UI window opens
2. **Configure region** → Settings → Capture Region (use preview border)
3. **Start capture** → Click "Start Capture"
4. **Share** → In your video call, select "RustFrame Preview" window

→ See [Quick Start Guide](docs/user-guide/quick-start.md) for detailed walkthrough

## 📚 Documentation

### For Users
- **[User Guide](docs/user-guide/)** - Installation, usage, and troubleshooting
- **[Quick Start](docs/user-guide/quick-start.md)** - Get started in 30 seconds
- **[Features](docs/user-guide/features.md)** - Complete feature reference
- **[Troubleshooting](docs/user-guide/troubleshooting.md)** - Common issues and solutions
- **[FAQ](docs/user-guide/faq.md)** - Frequently asked questions

### For Developers
- **[Developer Guide](docs/developer/)** - Architecture and contributing
- **[Building Guide](docs/developer/building.md)** - Compile from source
- **[Technical Documentation](docs/technical/)** - Low-level implementation details

## 🎯 Features in Detail

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
- **Windows**: GPU acceleration temporarily disabled due to D3D11 device mismatch (will be fixed)
- **macOS**: Click highlights use CPU instead of GPU (optimization planned)
- **Linux**: PipeWire support experimental, may have compatibility issues

→ See [Known Issues](docs/developer/known-issues.md) for complete list

### Planned Features
- [ ] Window-based capture (capture specific application windows)
- [ ] Zero-copy GPU texture sharing (eliminate CPU copy)
- [ ] Global hotkeys for start/stop
- [ ] Region presets (save/load favorite regions)
- [ ] Annotation tools (draw on capture)

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
