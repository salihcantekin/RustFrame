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
| **Start capture** | ENTER |
| **Exit** | ESC |

## 📸 Typical Workflow

1. **Launch** → Two windows appear
2. **Position** → Drag overlay over content you want to share
3. **Resize** → Adjust overlay to frame exactly what you need
4. **Confirm** → Press ENTER to start capturing
5. **Share** → In Teams/Zoom, share "RustFrame - Captured Region" window
6. **Done** → Press ESC to exit

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
└── shader.wgsl       ← GPU shaders
```

## 🔍 Key Concepts

### Windows.Graphics.Capture (WGC)
- **NOT using GDI/BitBlt** (old, slow, CPU-bound)
- **Using WGC** (modern, fast, GPU-accelerated)
- Captures via Direct3D 11 textures

### Two Windows
1. **Overlay**: Transparent, borderless selector (what YOU see)
2. **Destination**: Normal window with captured content (what OTHERS see)

### Texture Flow
```
Screen → D3D11 Texture → Staging → CPU → wgpu → Window
        (WGC capture)   (GPU)    (copy) (upload) (render)
```

## 🎯 Usage Example

**Scenario**: You want to share a terminal window on Zoom without showing your entire screen.

1. Run `cargo run`
2. Drag the **overlay window** over your terminal
3. Resize it to fit the terminal perfectly
4. Press **ENTER**
5. In Zoom, click "Share Screen" → Select "RustFrame - Captured Region"
6. ✨ Only your terminal is visible to others!

## 🐛 Troubleshooting

### "Nothing is captured / black screen"
- Press ENTER to start capture (you might still be in selection mode)

### "Overlay window is invisible"
- It's transparent! Look for a subtle window frame
- TODO: We should add a colored border for visibility

### "Captures whole monitor, not just overlay region"
- Known limitation (cropping not yet implemented)
- The capture engine gets the full monitor, we need to add cropping

### "Performance is laggy"
- The CPU copy step adds latency
- For production: implement zero-copy D3D12 sharing

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
