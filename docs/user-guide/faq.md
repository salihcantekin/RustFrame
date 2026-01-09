# Frequently Asked Questions

Common questions about RustFrame.

## General

### What is RustFrame?

RustFrame is a cross-platform screen region capture tool that lets you share specific parts of your screen in video calls (Google Meet, Zoom, Teams, Discord) without exposing your entire desktop.

### Is RustFrame free?

Yes! RustFrame is free and open-source software under the MIT License.

### Which platforms are supported?

- ✅ **Windows 10/11**: Full support with GPU acceleration
- ✅ **macOS 10.15+**: Full support (macOS 12.3+ recommended)
- 🚧 **Linux**: Experimental support (X11 and Wayland)

### How is this different from OBS/ShareX?

| Feature | RustFrame | OBS | ShareX |
|---------|-----------|-----|--------|
| **Purpose** | Live region sharing | Streaming/recording | Screenshot utility |
| **Target** | Video calls | Twitch/YouTube | Documentation |
| **Setup** | Instant (30 seconds) | Complex (scenes, sources) | Screenshot-focused |
| **Resource Usage** | ~10% CPU | ~20-30% CPU | Minimal |

---

## Installation & Setup

### Do I need to install anything?

**Windows/Linux**: No installation required—just extract and run.  
**macOS**: Drag to Applications folder (standard Mac app).

### Why does Windows Defender flag the app?

This is normal for applications without extensive download history. The app is safe (open-source, you can audit the code). Click "More info" → "Run anyway".

### Why does macOS say the app is damaged?

macOS Gatekeeper requires notarization. Remove the quarantine flag:
```bash
xattr -cr /Applications/RustFrame.app
```

### What permissions does RustFrame need?

- **macOS**: Screen Recording permission (System Settings → Privacy)
- **Windows**: None required
- **Linux**: Screen capture via PipeWire (Wayland) or X11

---

## Usage

### How do I share my capture in Google Meet?

1. Start capture in RustFrame
2. In Google Meet, click "Present now" → "A window"
3. Select "RustFrame Preview" or "Destination Window"
4. Click "Share"

### Can I move the capture region while sharing?

Yes! Drag the hollow border to reposition or resize. Changes apply immediately without restarting capture.

### Why can't I see my cursor in the capture?

**Settings → Mouse & Clicks → Show Cursor** is disabled by default to prevent "double cursor" in screen sharing. Enable it if needed.

### Can I capture from multiple monitors?

Yes! Either:
- **Settings → Capture Region → Monitor dropdown**
- Or drag the border to another monitor (auto-detects and switches)

### What's the recording indicator for?

The "REC" badge reminds you that capture is active. Toggle it in **Settings → Display → Show Recording Indicator**.

---

## Performance

### How much CPU does RustFrame use?

| Scenario | CPU Usage | Notes |
|----------|-----------|-------|
| **Idle** | ~3-5% | App open, not capturing |
| **Capturing** | ~8-12% | GPU-accelerated |
| **With Clicks** | ~15-20% | Click highlights add overhead |

### My CPU usage is very high (>30%). What's wrong?

1. Check if click highlights are enabled → Disable if not needed
2. Lower target FPS to 30 (Settings → Performance)
3. Ensure GPU acceleration is enabled
4. Try a different capture method

### Does RustFrame use my GPU?

Yes, when GPU acceleration is enabled:
- **Windows**: DirectX 11 (Windows Graphics Capture)
- **macOS**: Metal (ScreenCaptureKit + IOSurface)
- **Linux**: Vulkan/OpenGL (via wgpu)

Fallback to CPU capture if GPU unavailable.

### What FPS should I use?

| Use Case | Recommended FPS |
|----------|----------------|
| Presentations, slides | 15-30 FPS |
| Tutorials, demos | 30-60 FPS |
| Gaming, fast motion | 60-144 FPS |

**Default**: 60 FPS (good balance for most use cases)

---

## Troubleshooting

### The preview window is black!

See [Troubleshooting Guide](troubleshooting.md#black-preview-window) for detailed solutions.

**Quick fixes**:
1. Update graphics drivers
2. Try GDI capture method (Windows) or CoreGraphics (macOS)
3. Check Screen Recording permission (macOS)

### I can't find the preview window in Zoom/Meet!

Look for **"RustFrame Preview"** or **"Destination Window"** in the window selection dialog.

**If not listed**:
- Ensure capture is active (border showing)
- On macOS: Check Screen Recording permission
- On Windows with Discord: Use Discord capture profile

### The border doesn't appear when I start capture!

1. Check **Settings → Border → Show Border** is enabled
2. Border might be off-screen → Reset region to 100, 100
3. Check logs for errors (Settings → Open Logs)

### Click highlights show wrong colors!

This was a known issue (RGBA vs BGRA) and has been fixed. Update to the latest version.

### Capture region resets every time I restart!

Enable **Settings → Advanced → Remember Last Region**.

---

## Privacy & Security

### What data does RustFrame collect?

**None.** RustFrame runs entirely locally. No telemetry, no analytics, no network requests.

### Where are my settings stored?

- **Windows**: `%APPDATA%\RustFrame\settings.json`
- **macOS**: `~/Library/Application Support/RustFrame/settings.json`
- **Linux**: `~/.config/RustFrame/settings.json`

### Is the captured content sent anywhere?

No. Capture happens entirely on your machine. The preview window is just a regular window that your video conferencing app captures.

### Can RustFrame see my other applications?

RustFrame only captures the specific screen region you select. It cannot access application data, files, or interact with other programs beyond screen capture.

---

## Advanced

### Can I use RustFrame for recording?

RustFrame is designed for live sharing, not recording. Use OBS or similar tools for recording (you can share the RustFrame preview window in OBS).

### Can I capture specific application windows instead of regions?

Not yet, but this feature is planned. Current workaround: Position the border around your target application window.

### Does RustFrame support HDR capture?

Windows Graphics Capture API supports HDR, but full HDR pipeline is not yet implemented. SDR capture works correctly.

### Can I use RustFrame with virtual cameras (OBS Virtual Camera, etc.)?

Yes! Some video conferencing apps let you share windows to virtual cameras. However, RustFrame preview window → direct screen share is more efficient.

### How do I create custom capture profiles?

1. Navigate to settings folder (Settings → Open Settings Folder)
2. Create a file: `profile_<name>.json`
3. Add JSON overrides:
   ```json
   {
     "target_fps": 30,
     "show_cursor": true,
     "border_width": 1
   }
   ```
4. Restart RustFrame → Select profile from dropdown

---

## Development

### Is RustFrame open-source?

Yes! View the source code at [github.com/salihcantekin/RustFrame](https://github.com/salihcantekin/RustFrame).

### Can I contribute?

Absolutely! See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

### What technologies does RustFrame use?

- **Backend**: Rust (Tauri 2 framework)
- **Frontend**: React + TypeScript + TailwindCSS
- **Capture APIs**: Windows Graphics Capture, ScreenCaptureKit, X11/Wayland
- **Rendering**: Direct3D 11, Metal, wgpu

### How do I build from source?

See [Developer Guide → Building](../developer/building.md).

### Where can I report bugs or request features?

[GitHub Issues](https://github.com/salihcantekin/RustFrame/issues)

---

## Comparison to Other Tools

### RustFrame vs Screen Sharing Built-in Features

| Feature | RustFrame | Native Screen Share |
|---------|-----------|---------------------|
| **Region Capture** | ✅ Precise pixel control | ⚠️ Limited or none |
| **Multi-Monitor** | ✅ Any monitor | ✅ Usually supported |
| **Real-time Adjust** | ✅ Drag/resize while sharing | ❌ Must restart |
| **Performance** | ✅ ~10% CPU | ⚠️ ~15-20% CPU |
| **Privacy** | ✅ Exact region control | ⚠️ Full screen/window |

### RustFrame vs OBS Studio

| Feature | RustFrame | OBS |
|---------|-----------|-----|
| **Learning Curve** | ⭐ Easy (30 seconds) | ⭐⭐⭐ Steep |
| **Resource Usage** | ~10% CPU | ~20-30% CPU |
| **Live Streaming** | ❌ Not designed for this | ✅ Primary purpose |
| **Recording** | ⚠️ Via screen share | ✅ Built-in |
| **Region Capture** | ✅ Primary feature | ✅ Via scenes |
| **Setup Time** | <1 minute | 5-10 minutes |

**When to use RustFrame**: Quick screen sharing in video calls  
**When to use OBS**: Streaming, recording, complex scenes

---

## Getting More Help

### Where can I get support?

1. **Documentation**: [docs/user-guide/](../user-guide/)
2. **Troubleshooting**: [troubleshooting.md](troubleshooting.md)
3. **GitHub Discussions**: [Discussions](https://github.com/salihcantekin/RustFrame/discussions)
4. **GitHub Issues**: [Report bugs](https://github.com/salihcantekin/RustFrame/issues)

### How do I check my RustFrame version?

**Settings → About** or check logs folder name (includes version).

### Is there a user community or Discord?

Not yet! If there's interest, we may create one. For now, use GitHub Discussions.

---

**Previous**: [Troubleshooting](troubleshooting.md) | [Back to User Guide](README.md)
