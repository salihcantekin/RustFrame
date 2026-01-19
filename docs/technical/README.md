# Technical Documentation

This section contains low-level technical details, platform-specific implementation notes, and performance analysis for RustFrame.

## Contents

- [GPU Optimization](gpu-optimization.md)
- [Zero-Copy Strategy](zero-copy-strategy.md)
- [Multi-Monitor Support](multi-monitor.md)
- [Color Format Handling](color-formats.md)
- [Coordinate Systems](coordinate-system-architecture.md)
- [Platform-Specific Details](#platform-specific-details)

---

## Platform-Specific Details

| Platform      | Capture Engine                | Rendering         | Windowing/Notes                |
|--------------|-------------------------------|-------------------|-------------------------------|
| Windows      | Windows Graphics Capture (WGC)| DirectX 11 (GPU)  | Full GPU pipeline, best perf  |
| macOS 12.3+  | ScreenCaptureKit              | Metal (GPU)       | Window exclusion, full GPU    |
| macOS <12.3  | CoreGraphics                  | CPU fallback      | No window exclusion           |
| Linux        | X11/Wayland                   | wgpu              | Experimental, limited support |

- See each technical doc for in-depth analysis and code references.
- For historical/experimental approaches, see [../archive/](../archive/).

---

## Libraries & Dependencies

- Rust, Tauri, React, wgpu, DirectX 11, Metal, ScreenCaptureKit, CoreGraphics, X11, Wayland, and more.
- See Cargo.toml and package.json for full lists.
