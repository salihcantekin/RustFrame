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

| Action | Where |
|--------|-------|
| **Open settings** | Settings button in the app UI |
| **Preview/select region** | Settings → Capture Region (Preview Border) |
| **Start capture** | Start Capture button |
| **Stop capture** | Stop Capture button |

## 📸 Typical Workflow

1. **Launch** → RustFrame UI opens
2. **Configure** → Open **Settings**
3. **Select region** → Use **Capture Region** (Preview Border helps you move/resize)
4. **Start** → Click **Start Capture**
5. **Share** → In Teams/Zoom/Google Meet, share the window titled **"RustFrame Preview"**
6. **Done** → Click **Stop Capture** and close the app

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
├── destination_window.rs ← WinAPI (GDI) output window (share this)
├── hollow_border.rs  ← Border window used for region preview/capture
└── platform/         ← Windows-specific helpers (input, monitors, etc.)
```

## Capture Profiles (Windows)

RustFrame supports optional "Capture Profiles" to improve compatibility with different apps (e.g. Discord vs Google Meet).

- Profiles are JSON files stored next to `settings.json` in the RustFrame config folder.
- Naming convention: `profile_<name>.json` (example: `profile_discord.json`).
- A profile file contains ONLY the keys you want to override from the default behavior.

At startup, RustFrame scans for `profile_*.json` files and shows a "Capture Profile" selector on the main screen.
Selecting a profile changes how capture starts (effective on the next Start Capture).

Example `profile_discord.json`:

```json
{
        "winapi_destination_toolwindow": false,
        "winapi_destination_noactivate": false,
        "winapi_destination_click_through": false
}
```

## 🔍 Key Concepts

### Windows.Graphics.Capture (WGC)
- **NOT using GDI/BitBlt** (old, slow, CPU-bound)
- **Using WGC** (modern, fast, GPU-accelerated)
- Captures via Direct3D 11 textures

### Production Mode

In release builds, the output window can be fully transparent and click-through while still being shareable.
If you need to override this for troubleshooting, edit `settings.json`:

- `winapi_destination_alpha`: 0..255 (default: 0)
- `winapi_destination_topmost`: true/false (default: true)

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
- Use Settings → Capture Region → enable Preview Border
- Increase Border Width / adjust Border Color in Settings

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
