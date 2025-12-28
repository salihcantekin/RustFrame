# RustFrame

**A modern Windows screen region capture tool built with Rust, using Windows.Graphics.Capture API**

RustFrame allows you to select a region of your screen and mirror it to a separate window, perfect for sharing specific content on Teams, Zoom, Google Meet or Discord without exposing your entire screen.

**Project Links:** [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md) · [Code of Conduct](CODE_OF_CONDUCT.md) · [Security](SECURITY.md) · [License](LICENSE)

## 🎯 Features

- ✅ **Modern Capture API**: Uses Windows.Graphics.Capture (not GDI/BitBlt) for GPU-accelerated capture
- ✅ **Multi-Monitor Support**: Capture works on any connected monitor, not just primary
- ✅ **Transparent Overlay**: Frameless, transparent selection window with visual border
- ✅ **Real-time Mirroring**: Captured region displayed in a shareable window
- ✅ **Drag-to-Move**: Click and drag the overlay window to reposition
- ✅ **Resizable Selection**: Resize the overlay to select your desired region
- ✅ **GPU Rendering**: wgpu-based rendering pipeline with Direct3D 12 backend
- ✅ **Keyboard Shortcuts**: Quick adjustments with hotkeys (C, B, E, S, H, +/-)
- ✅ **Real-time Settings Display**: Live status indicators in overlay (color-coded)
- ✅ **Settings Dialog**: Customize cursor visibility and border width
- ✅ **System Tray**: Minimize to tray with quick access menu and custom app icon
- ✅ **Smart ESC Behavior**: ESC stops capture first, then exits (prevents accidental closure)
- ✅ **Production Mode**: Off-screen destination window for clean video sharing
- ✅ **Help Overlay**: On-screen keyboard shortcut reference (H key)

## 🏗️ Architecture

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

2. **Two windows appear:**
   - **Overlay Window** (transparent, borderless): This is your selection tool
   - **Destination Window** (normal): This is what you share on Teams/Zoom

3. **Position the overlay:**
   - **Click and drag** to move the overlay window
   - **Resize** using window edges (standard Windows resize)
   - Position it over the content you want to share

4. **Start capturing:**
   - Press **ENTER** or **Numpad Enter** to start real-time capture
   - The destination window will display the selected region

5. **Keyboard Shortcuts (during selection):**
   - **C**: Toggle cursor visibility in capture
   - **B**: Toggle border visibility
   - **E**: Toggle exclude from capture mode
   - **S**: Open settings dialog
   - **H**: Toggle help overlay
   - **+/-**: Adjust border width

6. **Share on Teams/Zoom/Google Meet:**
   - Select "RustFrame Output" window in your screen sharing dialog
   - Only the captured region will be visible to participants

7. **Exit:**
   - Press **ESC** once to stop capture (returns to selection mode)
   - Press **ESC** again to close the application
   - Or right-click tray icon and select Exit

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
