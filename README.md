# RustFrame

[![Build and Release](https://github.com/salihcantekin/RustFrame/actions/workflows/release.yml/badge.svg?branch=dev)](https://github.com/salihcantekin/RustFrame/actions/workflows/release.yml)
[![Latest Release](https://img.shields.io/github/v/release/salihcantekin/RustFrame?include_prereleases&sort=semver)](https://github.com/salihcantekin/RustFrame/releases)
[![Downloads](https://img.shields.io/github/downloads/salihcantekin/RustFrame/total.svg)](https://github.com/salihcantekin/RustFrame/releases)

**A modern Windows screen region capture tool built with Rust, using Windows.Graphics.Capture API**

➜ Download the latest release: https://github.com/salihcantekin/RustFrame/releases/latest

RustFrame allows you to select a region of your screen and mirror it to a separate window, perfect for sharing specific content on Teams, Zoom, Google Meet or Discord without exposing your entire screen.

**Project Links:** [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md) · [Code of Conduct](CODE_OF_CONDUCT.md) · [Security](SECURITY.md) · [License](LICENSE)

## 🎯 Features

- ✅ **Modern Capture API**: Uses Windows.Graphics.Capture (not GDI/BitBlt) for GPU-accelerated capture
- ✅ **Multi-Monitor Support**: Capture works on any connected monitor, not just primary
- ✅ **Real-time Mirroring**: Captured region displayed in a shareable window
- ✅ **Settings UI**: Configure cursor, border, performance, and region selection
- ✅ **Invisible Share Window (Release)**: The output window can be fully transparent + click-through while still being shareable

## 🏗️ Architecture

> Note: This repository contains older/experimental modules (winit/wgpu overlay, etc.).
> The current app entry point is Tauri-based ([src/main.rs](src/main.rs)) and uses a WinAPI GDI output window.

### Core Modules

#### `main.rs` - Application Orchestrator
- Event loop management (winit-based)
- Window lifecycle coordination
- Mouse/keyboard input handling
- Drag functionality implementation

#### `capture.rs` - Windows.Graphics.Capture Implementation
- **Direct3D 11 device creation** with BGRA support
- **WinRT interop** between Win32 D3D11 and WinRT APIs
- **GraphicsCaptureItem** for monitor/window capture
- **Frame pool management** with double-buffering
- **Event-driven frame capture** using TypedEventHandler
- Thread-safe frame access with Arc<Mutex<>>

#### `window_manager.rs` - Window Management
- **OverlayWindow**: Transparent, borderless, always-on-top selector
  - Win32 `SetLayeredWindowAttributes` for true transparency
  - `WS_EX_LAYERED` extended window style
  - Drag-to-move functionality
- **DestinationWindow**: Standard shareable window with title bar

#### `renderer.rs` - wgpu Rendering Pipeline
- **D3D11 → wgpu texture bridge** with staging texture
- **CPU-side texture copying** (map → copy → unmap)
- **Full-screen quad rendering** with texture sampling
- **WGSL shaders** for GPU processing
- Automatic resize handling

#### `shader.wgsl` - GPU Shaders
- Vertex shader: NDC to clip space transformation
- Fragment shader: Texture sampling and output

## 🚀 Usage

### Building

See [BUILD_INSTRUCTIONS.md](BUILD_INSTRUCTIONS.md) for detailed build setup.

**macOS Users**: If you encounter a "Rust cannot catch foreign exceptions" error, see [docs/MACOS_EXCEPTION_FIX.md](docs/MACOS_EXCEPTION_FIX.md) or [docs/MACOS_EXCEPTION_FIX_TR.md](docs/MACOS_EXCEPTION_FIX_TR.md) (Turkish) for details. This has been fixed in the latest version by adding proper Objective-C exception handling dependencies.

**Quick start with RustRover:**
1. Open project in RustRover
2. Press `Ctrl+F9` to build
3. Press `Shift+F10` to run

**Command line (requires proper MSVC setup):**
```bash
cargo build --release
cargo run --release
```

### Running

1. **Launch RustFrame**
   ```bash
   cargo run
   ```

2. **Open Settings**
   - Click **Settings** in the app UI
   - Use the **Capture Region** tab to position/size your region (Preview Border helps)

3. **Start capturing**
   - Click **Start Capture**
   - RustFrame will mirror the selected region into a separate shareable window

4. **Share on Teams/Zoom/Google Meet:**
   - Select the RustFrame output window (titled **"RustFrame Preview"**) in your screen sharing dialog
   - Only the captured region will be visible to participants

5. **Stop / Exit:**
   - Click **Stop Capture** to stop mirroring
   - Close the app to exit

### Advanced (Hidden) Settings

You can add these keys manually to `settings.json` (Settings → Advanced → Open Settings Folder):

- `winapi_destination_alpha`: 0..255 (default: 0 in release builds)
- `winapi_destination_topmost`: true/false (default: true)

If a meeting app shows black or stops updating, try setting `winapi_destination_alpha` to `1` or `255` for diagnostics.

## 🛠️ Technical Details

### Why Windows.Graphics.Capture?

Traditional screen capture methods (GDI's `BitBlt`) have significant limitations:
- **CPU-bound**: Involves CPU-side memory copies
- **Poor performance**: Can't capture modern DWM-composited content efficiently
- **Missing features**: No support for HDR, multi-GPU, or proper DPI scaling

**Windows.Graphics.Capture (WGC) solves these:**
- **GPU-accelerated**: Zero-copy capture using Direct3D 11 textures
- **Modern**: Supports DWM, HDR, and multi-monitor setups
- **Efficient**: Lower latency and CPU usage
- **Future-proof**: Microsoft's recommended API for Windows 10/11

### Texture Pipeline

```
Screen (DWM)
    ↓ (GPU, Windows.Graphics.Capture)
D3D11 Texture
    ↓ (GPU-to-GPU copy)
Staging Texture (CPU-readable)
    ↓ (Map + CPU copy)
CPU Memory Buffer
    ↓ (Upload to GPU)
wgpu Texture
    ↓ (GPU rendering)
Swapchain → Window
```

**Performance note:** The CPU copy step (staging texture) adds ~2-5ms latency. For production use, implement Direct3D 12 resource sharing for zero-copy interop.

### COM Object Safety

This project uses many Windows COM objects (`ID3D11Device`, `GraphicsCaptureSession`, etc.). Key safety considerations:

1. **COM Initialization**: `CoInitializeEx` called with `COINIT_MULTITHREADED`
2. **Reference Counting**: COM objects use automatic reference counting
3. **Thread Safety**: `Send`/`Sync` implemented for COM wrappers after verification
4. **Explicit Cleanup**: `Drop` implementations for proper resource cleanup

### Transparency Implementation

```rust
// Step 1: Enable layered window
SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED);

// Step 2: Set alpha transparency
SetLayeredWindowAttributes(hwnd, COLORREF(0), 200, LWA_ALPHA);
//                                           ↑     ↑    ↑
//                                    color key  alpha  mode
```

- **WS_EX_LAYERED**: Enables per-window alpha blending
- **LWA_ALPHA**: Use alpha channel for transparency
- **200/255 opacity**: Slightly transparent (adjust as needed)

## 📋 Dependencies

### Core Libraries
- **`winit`**: Cross-platform window creation and event handling
- **`wgpu`**: Modern GPU graphics API (WebGPU for Rust)
- **`windows`**: Official Microsoft Windows API bindings

### Key Features Used
- `Graphics_Capture`: Windows.Graphics.Capture API
- `Graphics_DirectX_Direct3D11`: D3D11 texture interop
- `Win32_Graphics_Dxgi`: DirectX Graphics Infrastructure
- `Win32_System_WinRT`: WinRT-to-Win32 bridges

See [Cargo.toml](Cargo.toml) for complete dependency list with explanations.

## 🔧 Known Limitations & Future Plans

### Current Limitations

1. **CPU-side texture copying** (not zero-copy)
   - Uses staging texture with Map/Unmap
   - Adds 2-5ms latency per frame
   - **Future**: Implement Direct3D 12 resource sharing

### Future Enhancements

- [x] ~~Support multi-monitor selection~~ ✅ Implemented in v0.2.0
- [ ] Add window picker (capture specific window instead of monitor)
- [ ] Implement zero-copy D3D12 texture sharing
- [ ] Save/load region presets
- [ ] Add framerate control settings
- [ ] Global hotkey support for starting/stopping capture

## 📚 Learning Resources

This project is designed as a learning resource. Key concepts demonstrated:

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
