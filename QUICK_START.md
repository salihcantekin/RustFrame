# RustFrame - Quick Start Guide

## 🎬 30-Second Start

```bash
# In RustRover:
1. Open project
2. Press Ctrl+F9 (Build)
3. Press Shift+F10 (Run)

# Or command line (if MSVC configured):
cargo run
```

## 🎮 Controls

| Action | Key/Mouse |
|--------|-----------|
| **Move overlay** | Click and drag |
| **Resize overlay** | Drag window edges |
| **Start capture** | ENTER / Numpad Enter |
| **Toggle cursor** | C |
| **Open settings** | S |
| **Toggle help** | H |
| **Adjust border** | + / - |
| **Exit** | ESC |

## 📸 Typical Workflow

1. **Launch** → Transparent overlay window appears
2. **Position** → Drag overlay over content you want to share
3. **Resize** → Adjust overlay to frame exactly what you need
4. **Configure** → Press S for settings, C to toggle cursor, H for help
5. **Confirm** → Press ENTER to start capturing
6. **Share** → In Teams/Zoom/Google Meet, share "RustFrame Output" window
7. **Done** → Press ESC to exit

## 🏗️ Build Issues?

**Error: `link.exe` failed**
- Solution: Use RustRover's build system (it handles this automatically)
- Or see [BUILD_INSTRUCTIONS.md](BUILD_INSTRUCTIONS.md)

**Error: `dlltool.exe` not found**
- You're using GNU toolchain, need MSVC
- Solution: Use RustRover or install Visual Studio Build Tools

## 📁 Project Structure

```
src/
├── main.rs           ← Application entry point
├── capture.rs        ← Windows.Graphics.Capture (WGC) API
├── window_manager.rs ← Transparent overlay + destination window
├── renderer.rs       ← wgpu rendering pipeline
├── shader.wgsl       ← GPU shaders
├── settings_dialog.rs← Settings window
├── constants.rs      ← Centralized constants
├── utils.rs          ← Shared utilities
└── bitmap_font.rs    ← Pixel font rendering
```

## 🔍 Key Concepts

### Windows.Graphics.Capture (WGC)
- **NOT using GDI/BitBlt** (old, slow, CPU-bound)
- **Using WGC** (modern, fast, GPU-accelerated)
- Captures via Direct3D 11 textures

### Production Mode
- Overlay window appears on screen for selection
- Destination window is positioned off-screen
- Share the "RustFrame Output" window in video calls
- No infinite mirror effect!

### Texture Flow
```
Screen → D3D11 Texture → Staging → CPU → wgpu → Window
        (WGC capture)   (GPU)    (copy) (upload) (render)
```

## 🎯 Usage Example

**Scenario**: You want to share a terminal window on Zoom without showing your entire screen.

1. Run RustFrame
2. Drag the **overlay window** over your terminal
3. Resize it to fit the terminal perfectly
4. Press **ENTER**
5. In Zoom, click "Share Screen" → Select "RustFrame Output"
6. ✨ Only your terminal is visible to others!

## 🐛 Troubleshooting

### "Nothing is captured / black screen"
- Press ENTER to start capture (you might still be in selection mode)

### "Overlay window is hard to see"
- Press H to show help overlay with visual indicators
- The overlay has a subtle colored border

### "Performance is laggy"
- The CPU copy step adds some latency
- This is normal for the current implementation

## 🚀 Next Steps

1. **Read** [README.md](README.md) for full architecture details
2. **Explore** the code - every module has extensive comments
3. **Customize** - adjust transparency, add borders, implement cropping
4. **Learn** - this project demonstrates COM, WGC, D3D11, wgpu, and more!

## 💡 Pro Tips

- **Use RustRover**: It handles all the MSVC linker complexity
- **Read the comments**: Every `unsafe` block explains WHY it's safe
- **Start with `main.rs`**: Follow the flow from there
- **Check TODOs**: Look for `TODO:` comments for improvement ideas

## 📞 Need Help?

- Build issues? → [BUILD_INSTRUCTIONS.md](BUILD_INSTRUCTIONS.md)
- Architecture questions? → [README.md](README.md)
- Code questions? → Read the inline comments (they're extensive!)

---

**Happy capturing! 🎥**
