# Features Guide

Complete reference for all RustFrame features.

## Table of Contents

- [Capture Region](#capture-region)
- [Mouse & Cursor](#mouse--cursor)
- [Click Highlights](#click-highlights)
- [Border Customization](#border-customization)
- [Performance Settings](#performance-settings)
- [Capture Methods](#capture-methods)
- [Preview Modes](#preview-modes)
- [Multi-Monitor Support](#multi-monitor-support)
- [Recording Indicator](#recording-indicator)
- [Capture Profiles](#capture-profiles)
- [Advanced Settings](#advanced-settings)

---

## Capture Region

Control which part of your screen is captured.

### Configuration

**Location**: Settings → Capture Region

| Setting | Description | Default |
|---------|-------------|---------|
| **Monitor** | Which display to capture from | Primary |
| **X Position** | Horizontal offset in pixels | 100 |
| **Y Position** | Vertical offset in pixels | 100 |
| **Width** | Capture width in pixels | 800 |
| **Height** | Capture height in pixels | 600 |
| **Remember Last Region** | Save position/size between sessions | Enabled |

### Interactive Adjustment

**Preview Border**: Visual representation of capture region

- **Enable**: Settings → Capture Region → Toggle "Preview Border"
- **Move**: Click and drag from inside the border
- **Resize**: Drag corners or edges
- **Precision**: Use arrow keys for 1-pixel adjustments (when supported)

### Technical Details

- **Minimum Size**: 1×1 pixel (enforced)
- **Maximum Size**: Limited by screen resolution
- **Border Offset**: Capture region excludes border thickness (prevents border from appearing in capture)
- **Coordinate System**: Screen coordinates (multi-monitor aware)

### Use Cases

✅ **Single Application**: Frame exactly one window  
✅ **Partial Window**: Exclude toolbars, hide personal info  
✅ **Multi-Window**: Capture parts of multiple applications  
✅ **Custom Aspect Ratio**: Any width/height combination  

---

## Mouse & Cursor

Control cursor visibility in the capture.

### Show Cursor

**Path**: Settings → Mouse & Clicks → Show Cursor

| State | Behavior | Best For |
|-------|----------|----------|
| **Enabled** | Your cursor appears in capture | Tutorials, pointing |
| **Disabled** | Cursor is hidden | Presentations |

**Default**: Disabled (prevents "double cursor" in screen sharing)

### Technical Notes

- **Platform Capture**: 
  - Windows: Uses `show_cursor` parameter in WGC
  - macOS: ScreenCaptureKit includes cursor automatically when enabled
  - Linux: X11/Wayland cursor overlay
- **Performance**: No impact on FPS
- **Compatibility**: Works with all capture methods

---

## Click Highlights

Visual feedback for mouse clicks—great for tutorials!

### Configuration

**Path**: Settings → Mouse & Clicks

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| **Capture Clicks** | Enable click detection | On/Off | On |
| **Highlight Color** | Circle color (RGBA) | Any color | Red (255,0,0,255) |
| **Radius** | Circle size in pixels | 10-100px | 20px |
| **Dissolve Time** | Fade-out duration | 100-2000ms | 300ms |

### Visual Effect

```
Mouse Click → Colored Circle Appears → Fades Out
     ↓                ↓                    ↓
  Instant         [Radius]           [Dissolve Time]
```

### Color Customization

**Preset Colors**:
- Red: `[255, 0, 0, 255]` (default)
- Blue: `[0, 120, 255, 255]`
- Green: `[0, 255, 120, 255]`
- Yellow: `[255, 255, 0, 255]`

**Custom**: Edit RGB values manually in Settings

### Performance Impact

| Configuration | CPU Usage | Notes |
|---------------|-----------|-------|
| Disabled | 0% overhead | No click detection |
| Enabled | ~2-5% extra | Minimal impact |

**Optimization**: 
- On macOS: GPU-accelerated (Metal shader)
- On Windows: CPU drawing (optimized)

### Platform Differences

#### Windows
- Global mouse hook captures clicks system-wide
- Works even when RustFrame not focused
- Left/right/middle buttons supported

#### macOS  
- CGEventTap monitors clicks
- Requires Accessibility permission (prompted)
- Left/right buttons only

#### Linux
- X11: XQueryPointer + XGrabButton
- Wayland: Portal API required
- Button support varies by compositor

---

## Border Customization

Customize the hollow border appearance.

### Settings

**Path**: Settings → Border

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| **Show Border** | Display border during capture | On/Off | On |
| **Width** | Border thickness | 1-20px | 3px |
| **Color** | Border color (RGB) | Any color | Red |

### Border Behavior

**Capture Mode** (when capturing):
- Border shows capture region
- Interior is click-through
- Only edges/corners are interactive
- Prevents accidental clicks inside

**Preview Mode** (before capture):
- Border is fully draggable
- Interior responds to mouse
- Used for positioning

### Technical Details

- **Corner Thickening**: Corners are ~20px for easier resizing
- **Capture Exclusion**: Border pixels NOT included in capture
- **Z-Order**: Always on top (configurable)
- **Transparency**: Color key transparency (Windows) or window compositing (macOS/Linux)

### Use Cases

✅ **Visible Border**: See exactly what's being captured  
✅ **Hidden Border**: Disable for cleaner look (border won't show in preview anyway)  
✅ **Thin Border**: Minimal visual impact (1-2px)  
✅ **Thick Border**: Easier to grab and resize (10-15px)  

---

## Performance Settings

Tune RustFrame for your workflow.

### Target FPS

**Path**: Settings → Performance → Target FPS

| FPS | Use Case | CPU Usage | Quality |
|-----|----------|-----------|---------|
| **15** | Slides, static content | Lowest (~2%) | Occasional stutter |
| **30** | Presentations, talks | Low (~5%) | Smooth |
| **60** | Demos, tutorials | Medium (~10%) | Very smooth |
| **144** | Gaming, fast motion | High (~20%) | Buttery |

**Default**: 60 FPS

**Recommendation**: 
- 30 FPS for most use cases
- 60 FPS for smooth demos
- 144 FPS only if needed (high-refresh displays)

### GPU Acceleration

**Path**: Settings → Performance → GPU Acceleration

| State | Behavior | Performance |
|-------|----------|-------------|
| **Enabled** | GPU-accelerated capture (WGC/SCK) | Best |
| **Disabled** | CPU-based capture (GDI/CG) | Fallback |

**Default**: Enabled

**When to Disable**:
- Compatibility issues with old GPUs
- Driver problems
- Specific video conferencing app bugs

### Performance Metrics

**Windows (WGC + GPU)**:
- Idle: ~3-5% CPU
- Capturing: ~7-10% CPU
- With Clicks: ~12-15% CPU

**macOS (SCK + Metal)**:
- Idle: ~2-4% CPU
- Capturing (GPU): ~5-8% CPU
- With Clicks (CPU): ~30-35% CPU (optimization planned)

**Memory Usage**:
- Idle: ~50-80 MB
- Capturing: ~100-200 MB (depends on region size)

---

## Capture Methods

Platform-specific capture engines.

### Windows

**Path**: Settings → Performance → Capture Method

#### Windows Graphics Capture (WGC) ⭐ Recommended
- **Technology**: DirectX 11 interop
- **Performance**: GPU-accelerated, ~8% CPU
- **Requirements**: Windows 10 1803+
- **Features**: Modern, DWM-compatible, HDR-ready

#### GDI Copy (Fallback)
- **Technology**: BitBlt to memory
- **Performance**: CPU-bound, ~15-20% CPU
- **Requirements**: Any Windows version
- **Features**: Compatible with old hardware

### macOS

**Path**: Settings → Performance → Capture Method

#### ScreenCaptureKit (SCK) ⭐ Recommended
- **Technology**: IOSurface + Metal
- **Performance**: GPU-accelerated, ~5-8% CPU
- **Requirements**: macOS 12.3+
- **Features**: Zero-copy, modern API

#### CoreGraphics (CG)
- **Technology**: CGDisplayCreateImage
- **Performance**: CPU-based, ~10-15% CPU
- **Requirements**: macOS 10.15+
- **Features**: Legacy support, stable

### Linux

**Path**: Settings → Performance → Capture Method

#### PipeWire (Wayland)
- **Technology**: Portal API
- **Performance**: GPU-accelerated (compositor-dependent)
- **Requirements**: Wayland + PipeWire
- **Features**: Modern, secure

#### X11
- **Technology**: XGetImage / Xlib
- **Performance**: CPU-based
- **Requirements**: X11 server
- **Features**: Classic, widely compatible

---

## Preview Modes

Control how the preview window is rendered.

### Available Modes

#### Windows

##### WinAPI GDI ⭐ Recommended
- **Rendering**: CPU-based GDI BitBlt
- **Compatibility**: Best for Google Meet, Teams, Zoom
- **Performance**: ~10% CPU overhead
- **Features**: Reliable, always works

##### Tauri Canvas (Experimental)
- **Rendering**: WebView canvas element
- **Compatibility**: Works with most apps
- **Performance**: GPU-accelerated (browser engine)
- **Features**: Modern, flexible

#### macOS

##### CALayer + Metal ⭐ Default
- **Rendering**: GPU-accelerated via CALayer
- **Compatibility**: Works with all apps
- **Performance**: Zero-copy when possible
- **Features**: Native, optimized

#### Linux

##### wgpu
- **Rendering**: Vulkan/OpenGL via wgpu
- **Compatibility**: Depends on compositor
- **Performance**: GPU-accelerated
- **Features**: Cross-platform

---

## Multi-Monitor Support

Capture from any connected display.

### Features

✅ **Monitor Selection**: Choose source display  
✅ **Auto-Detection**: Drag border to switch monitors  
✅ **DPI Awareness**: Handles different scale factors  
✅ **Coordinate Translation**: Automatic origin adjustment  

### Configuration

**Path**: Settings → Capture Region → Monitor

Displays list of connected monitors:
```
Monitor 1 (Primary): 1920×1080 @ 0,0
Monitor 2: 2560×1440 @ 1920,0
Monitor 3: 1920×1080 @ 4480,0
```

### Dynamic Switching

While capturing, drag the border to another monitor:

1. Border detects new monitor center point
2. Logs monitor change
3. Stops current capture
4. Restarts capture on new monitor
5. Preserves capture settings

**Platforms**:
- ✅ Windows: Uses `MonitorFromPoint` + `GetDpiForMonitor`
- ✅ macOS: Uses `CGGetDisplaysWithPoint`
- 🚧 Linux: X11 screen detection

### Technical Details

- **Detection**: Center point of border determines monitor
- **Restart**: Seamless capture restart (< 100ms)
- **DPI Handling**: Coordinates adjusted for scale factor
- **Origin Offset**: Each monitor has unique origin (x, y)

---

## Recording Indicator

Visual "REC" badge during capture.

### Configuration

**Path**: Settings → Display → Show Recording Indicator

| State | Behavior |
|-------|----------|
| **Enabled** | Red "REC" badge appears near border |
| **Disabled** | No indicator shown |

### Positioning

- **Default**: Top-right corner of border
- **Follows Border**: Moves/resizes with border
- **Always Visible**: On top of all windows

### Customization

- **Color**: Red background, white text (hardcoded)
- **Size**: Scales with border size
- **Position**: Fixed offset from border corner

---

## Capture Profiles

Pre-configured settings for different apps.

### What are Profiles?

JSON files with platform-specific overrides for compatibility.

**Location**: 
- Windows: `%APPDATA%\RustFrame\profiles\`
- macOS: `~/Library/Application Support/RustFrame/profiles/`
- Linux: `~/.config/RustFrame/profiles/`

### Built-in Profiles

#### Discord Profile
**File**: `profile_discord.json`
```json
{
  "winapi_destination_toolwindow": false,
  "winapi_destination_noactivate": false,
  "winapi_destination_click_through": false,
  "hide_taskbar_after_ms": 2000
}
```

**Why**: Discord requires window in taskbar to detect it

### Creating Custom Profiles

1. Create file: `profile_<name>.json`
2. Add overrides (only keys you want to change):
   ```json
   {
     "target_fps": 30,
     "show_cursor": true,
     "border_width": 1
   }
   ```
3. Restart RustFrame
4. Select profile from dropdown

### Profile Hints

Special metadata for UI behavior:

```json
{
  "hide_taskbar_after_ms": 2000
}
```

Tells RustFrame to auto-hide preview window from taskbar after 2 seconds.

---

## Advanced Settings

Hidden settings for troubleshooting.

### Windows-Specific

Manual editing of `settings.json`:

```json
{
  "winapi_destination_alpha": 255,
  "winapi_destination_topmost": true,
  "winapi_destination_click_through": false,
  "winapi_destination_toolwindow": false,
  "winapi_destination_layered": true,
  "winapi_destination_appwindow": true,
  "winapi_destination_noactivate": false,
  "winapi_destination_overlapped": false
}
```

| Setting | Default | Description |
|---------|---------|-------------|
| `alpha` | 255 | Window opacity (0-255) |
| `topmost` | true | Always on top |
| `click_through` | false | Mouse clicks pass through |
| `toolwindow` | false | Hide from Alt+Tab |
| `layered` | true | Layered window for transparency |
| `appwindow` | true | Show in taskbar |
| `noactivate` | false | Don't activate on creation |

**Troubleshooting Use**:
- Black preview → Set `alpha: 1` or `255`
- Not shareable → Ensure `appwindow: true`
- Too many windows → Set `toolwindow: true`

### macOS-Specific

```json
{
  "preview_mode_macos_level": -1
}
```

| Setting | Default | Description |
|---------|---------|-------------|
| `macos_level` | 0 | NSWindowLevel (-1 = behind desktop) |

---

## Keyboard Shortcuts

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Open Settings | `Ctrl+,` | `Cmd+,` |
| Start/Stop Capture | `Ctrl+S` | `Cmd+S` |
| Hide/Show Border | `Ctrl+B` | `Cmd+B` |
| Quit Application | `Ctrl+Q` | `Cmd+Q` |

*(Some shortcuts may vary by platform or be unavailable)*

---

## Tips & Best Practices

### 🎯 Precision Region Selection
- Use keyboard arrows for 1-pixel adjustments
- Enable grid overlay for alignment
- Test with screenshot before sharing

### 🚀 Performance Optimization
- Start with 30 FPS, increase if needed
- Disable click highlights when not presenting
- Use GPU acceleration when available

### 🖱️ Cursor Management
- Hide cursor for slides/presentations
- Show cursor for tutorials/demos
- Test in actual screen share to verify

### 📊 Monitoring
- Check logs if issues occur (Settings → Open Logs)
- CPU usage visible in Task Manager/Activity Monitor
- Lower FPS if performance is poor

### 💡 Remember
- "Remember Last Region" saves time
- Profiles avoid reconfiguring for different apps
- Hollow border won't appear in capture (automatic)

---

**Previous**: [Quick Start](quick-start.md) | **Next**: [Troubleshooting](troubleshooting.md) →
