# Settings Reference

Complete guide to all RustFrame settings and their effects.

---

## Table of Contents

1. [Mouse & Cursor Settings](#mouse--cursor-settings)
2. [Click Highlights](#click-highlights)
3. [Border Settings](#border-settings)
4. [Performance Settings](#performance-settings)
5. [Capture Method](#capture-method)
6. [Recording Indicator](#recording-indicator)
7. [Region Memory](#region-memory)
8. [Preview Mode](#preview-mode)
9. [Logging Settings](#logging-settings)
10. [Advanced Windows Settings](#advanced-windows-settings)

---

## Mouse & Cursor Settings

### Show Cursor in Preview
**Setting**: `show_cursor`  
**Type**: Boolean  
**Default**: `false`  
**UI Location**: Settings → Mouse & Cursor

**Description**: Controls whether your mouse cursor is visible in the preview window during capture.

**Recommended Settings**:
- ✅ **Disabled (Default)**: Prevents double cursor when screen sharing (recommended for video calls)
- ⚠️ **Enabled**: Shows cursor in preview - useful for:
  - Recording tutorials where cursor position matters
  - Demonstrating clicks and mouse movements
  - Creating instructional videos

**Technical Notes**:
- When disabled, Windows/macOS removes cursor from capture automatically
- When enabled, cursor is captured as part of the screen image
- Does not affect click highlights (controlled separately)

---

### Capture Mouse Clicks
**Setting**: `capture_clicks`  
**Type**: Boolean  
**Default**: `true`  
**UI Location**: Settings → Mouse & Cursor

**Description**: Enables visual click highlights that appear when you click your mouse.

**When to Use**:
- ✅ **Enabled**: Video calls, presentations, tutorials (shows viewers where you're clicking)
- ❌ **Disabled**: Performance-critical tasks, gaming, when highlights are distracting

**Performance Impact**:
- **Disabled**: Full GPU rendering (~2-3ms frame time, ~4-7% CPU)
- **Enabled**: CPU fallback rendering (~7-10ms frame time, ~8-12% CPU)

⚠️ **Note**: Click highlights require CPU rendering, which temporarily disables GPU acceleration.

---

## Click Highlights

For configuration, color options, and troubleshooting, see the [Features Guide – Click Highlights](features.md#click-highlights).

---

### Click Dissolve Time
**Setting**: `click_dissolve_ms`  
**Type**: Milliseconds (integer)  
**Default**: `300` ms  
**Range**: `100` - `5000` ms  
**UI Location**: Settings → Click Highlights → Duration

**Description**: How long the click highlight animation lasts before fading out.

**Recommended Values**:
- **Fast (100-200ms)**: Subtle feedback, minimal distraction
- **Default (300ms)**: Good balance for most use cases
- **Medium (500-800ms)**: Emphasizes clicks for presentations
- **Slow (1000ms+)**: Tutorial mode, ensures viewers notice every click

**Use Cases**:
- **Video Calls**: 300ms (default) - quick feedback without clutter
- **Tutorials**: 500-800ms - gives viewers time to see the click
- **Gaming/Demos**: 200ms - fast feedback, doesn't obstruct view
- **Accessibility**: 1000ms+ - ensures clicks are highly visible

---

### Click Highlight Radius
**Setting**: `click_highlight_radius`  
**Type**: Points (integer)  
**Default**: `20` points  
**Range**: `10` - `50` points  
**UI Location**: Settings → Click Highlights → Size

**Description**: The radius of the circle that appears when you click, in logical points.

**Size Guide**:
- **Small (10-15)**: Subtle, for high-density UIs
- **Default (20)**: Visible without being obtrusive
- **Medium (25-30)**: More prominent, good for presentations
- **Large (35-50)**: Maximum visibility for large displays or accessibility

**Platform Scaling**:
- Automatically scales for high-DPI displays (Retina, 4K)
- 20 points = ~40 pixels on Retina display
- Maintains consistent visual size across different screen densities

---

## Border Settings

For all border options and technical details, see the [Features Guide – Border Customization](features.md#border-customization).

---

### Border Width
**Setting**: `border_width`  
**Type**: Pixels (integer)  
**Default**: `4` pixels  
**Range**: `1` - `20` pixels  
**UI Location**: Settings → Border → Width

**Description**: Thickness of the border line in physical pixels.

**Recommendations**:
- **Thin (1-2px)**: Minimal, less obtrusive
- **Default (4px)**: Good visibility without being distracting
- **Thick (6-10px)**: Easier to see and grab for resizing
- **Very Thick (12-20px)**: High visibility, accessibility mode

---

### Border Color
**Setting**: `border_color`  
**Type**: RGBA Color Array `[R, G, B, A]`  
**Default**: `[255, 0, 0, 255]` (Red, fully opaque)  
**UI Location**: Settings → Border → Color Picker

**Description**: Color and transparency of the capture region border.

**Preset Colors**:
- **Red** (Default): `[255, 0, 0, 255]` - High contrast, easy to see
- **Green**: `[0, 255, 0, 255]` - Success/active feel
- **Blue**: `[0, 150, 255, 255]` - Professional
- **Yellow**: `[255, 255, 0, 255]` - Maximum visibility
- **Purple**: `[200, 0, 255, 255]` - Distinctive

**Recommendations**:
- Use high-contrast colors that stand out against your typical background
- Full opacity (255) recommended for maximum visibility
- Test against your desktop wallpaper and common application backgrounds

---

## Performance Settings

For detailed performance metrics, CPU/GPU usage, and optimization tips, see the [Features Guide – Performance Settings](features.md#performance-settings).

---

### GPU Acceleration
**Setting**: `gpu_acceleration`  
**Type**: Boolean  
**Default**: `true`  
**UI Location**: Settings → Performance → GPU Acceleration

**Description**: Enables GPU-accelerated capture and rendering.

**Performance Comparison**:

| Platform | GPU Enabled | GPU Disabled |
|----------|-------------|--------------|
| **Windows** | ~2-3ms frame time<br>~4-7% CPU | ~15-20ms frame time<br>~25-35% CPU |
| **macOS** | ~0.5ms frame time<br>~2% CPU | ~5ms frame time<br>~10-15% CPU |

**When to Disable**:
- ❌ **GPU Compatibility Issues**: Rare graphics driver problems
- ❌ **Power Saving**: On battery power (minor savings)
- ❌ **Testing**: Diagnosing capture issues

**Recommendations**:
- ✅ **Always Keep Enabled** (default) - GPU path is faster and more efficient
- Only disable if you experience graphics-related crashes or glitches

**Technical Details**:
- **Windows**: Uses Windows Graphics Capture (WGC) + DirectX 11 SwapChain
- **macOS**: Uses ScreenCaptureKit + Metal rendering
- **Linux**: Currently CPU-only (GPU support planned)

---

## Capture Method

Platform-specific screen capture APIs.

### Windows Capture Methods

**Setting**: `capture_method`  
**Type**: Enum  
**Default**: `auto` (uses WGC)  
**UI Location**: Settings → Capture → Method

#### 1. WGC (Windows Graphics Capture) - Recommended ✅
**Value**: `Wgc`  
**Requirements**: Windows 10 1903+ / Windows 11  
**Performance**: ~1-2ms capture, GPU-accelerated  
**CPU Usage**: ~3-5%

**Advantages**:
- ✅ GPU-accelerated, fastest method
- ✅ Captures all windows, including hardware-accelerated content
- ✅ Official Microsoft API
- ✅ Respects DRM protection (Netflix, etc.)

**Use When**:
- Modern Windows 10/11 system
- Need maximum performance
- Capturing games or video content

#### 2. GDI (Graphics Device Interface)
**Value**: `GdiCopy`  
**Requirements**: All Windows versions  
**Performance**: ~5-15ms capture, CPU-based  
**CPU Usage**: ~15-25%

**Advantages**:
- ✅ Compatible with older Windows versions
- ✅ Works on systems without WGC support
- ✅ Reliable fallback

**Use When**:
- Windows 7/8 or older Windows 10 builds
- WGC has compatibility issues
- Troubleshooting capture problems

---

### macOS Capture Methods

**Setting**: `capture_method`  
**Type**: Enum (read-only)  
**Default**: `CoreGraphics` (auto)

**macOS always uses ScreenCaptureKit (macOS 12.3+) or CGDisplayStream (older versions)**

**Performance**:
- ScreenCaptureKit: ~10µs capture (GPU)
- CGDisplayStream: ~3ms capture (GPU)

**Requirements**:
- First launch requires **Screen Recording** permission
- System Preferences → Security & Privacy → Screen Recording

---

### Linux Capture Methods

**Setting**: `capture_method`  
**Type**: Enum  
**Default**: `auto`

**Status**: ⚠️ Experimental

**Supported**:
- X11 (XCB)
- Wayland (PipeWire, planned)

---

## Recording Indicator

Visual "REC" indicator showing capture is active.

### Show Recording Indicator
**Setting**: `show_rec_indicator`  
**Type**: Boolean  
**Default**: `true`  
**UI Location**: Settings → Recording Indicator → Enabled

**Description**: Shows a "REC" badge in the top-right corner of the capture region.

**Purpose**:
- ✅ Visual confirmation that capture is active
- ✅ Privacy awareness - clear indicator you're sharing
- ✅ Debugging aid - confirms capture is running

**Appearance**:
- Position: Top-right corner of capture region border
- Color: Red background with white "REC" text
- Size: Configurable (small/medium/large)

---

### Recording Indicator Size
**Setting**: `rec_indicator_size`  
**Type**: String  
**Default**: `"small"`  
**Options**: `"small"`, `"medium"`, `"large"`  
**UI Location**: Settings → Recording Indicator → Size

**Size Guide**:
- **Small** (Default): Subtle, ~24x48px
- **Medium**: Moderate visibility, ~32x64px
- **Large**: High visibility, ~40x80px

---

## Region Memory

Control whether RustFrame remembers your last capture region.

### Remember Last Region
**Setting**: `remember_last_region`  
**Type**: Boolean  
**Default**: `true`  
**UI Location**: Settings → Region Memory → Remember

**Description**: Saves the position and size of your capture region when you close the app.

**Behavior**:
- ✅ **Enabled (Default)**: On next startup, border appears at same position/size
- ❌ **Disabled**: Border resets to default position (100, 100, 600x400)

**Stored Data**:
```json
"last_region": [x, y, width, height]
```

**Use Cases**:
- ✅ **Enabled**: Consistent workflow, capture same area repeatedly
- ❌ **Disabled**: Privacy (don't save screen positions), fresh start each launch

---

## Preview Mode

**Setting**: `preview_mode`  
**Type**: Enum  
**Default**: Platform-specific  
**UI Location**: Settings → Preview Mode (Advanced)

### Options

#### WinApiGdi (Windows)
- Uses Windows GDI for preview rendering
- CPU-based, compatible with all Windows versions

#### Cocoa (macOS)
- Uses native Cocoa NSWindow
- GPU-accelerated with Metal

#### X11 (Linux)
- Uses X11 window system
- CPU-based

**⚠️ Advanced Setting**: Most users should leave at default. Only change if experiencing preview window issues.

---

## Logging Settings

Configure application logging for debugging and troubleshooting.

### Log Level
**Setting**: `log_level`  
**Type**: String  
**Default**: `"Error"`  
**Options**: `"Off"`, `"Error"`, `"Warn"`, `"Info"`, `"Debug"`, `"Trace"`  
**UI Location**: Settings → Logging → Log Level

**Level Guide**:

| Level | Output | Use Case |
|-------|--------|----------|
| **Off** | Nothing | Disable all logging |
| **Error** | Only errors | Default, minimal logging |
| **Warn** | Errors + warnings | Troubleshooting issues |
| **Info** | + informational | Understanding app behavior |
| **Debug** | + debug details | Diagnosing problems |
| **Trace** | Everything | Deep debugging (verbose) |

**Recommendations**:
- **Normal Use**: `Error` (default)
- **Performance Issues**: `Info` or `Debug`
- **Crash Debugging**: `Debug` or `Trace`
- **Development**: `Trace`

**Performance Impact**:
- **Error/Warn**: Negligible
- **Info/Debug**: Minimal (~1% CPU)
- **Trace**: Moderate (~3-5% CPU, slower I/O)

---

### Log to File
**Setting**: `log_to_file`  
**Type**: Boolean  
**Default**: `true`  
**UI Location**: Settings → Logging → Save to File

**Description**: Writes logs to a file in the user's config directory.

**Log Location**:
- **Windows**: `%APPDATA%\com.salihcantekin.rustframe\logs\`
- **macOS**: `~/Library/Application Support/com.salihcantekin.rustframe/logs/`
- **Linux**: `~/.config/rustframe/logs/`

**File Format**:
```
rustframe-YYYY-MM-DD.log
```

**Benefits**:
- ✅ Debugging after crashes
- ✅ Sharing logs with support/developers
- ✅ Performance analysis
- ✅ Historical troubleshooting

**Considerations**:
- Log files are rotated based on `log_retention_days`
- Old logs are automatically deleted
- Minimal performance impact

---

### Log Retention Days
**Setting**: `log_retention_days`  
**Type**: Integer  
**Default**: `30` days  
**Range**: `1` - `365` days  
**UI Location**: Settings → Logging → Keep For

**Description**: How many days to keep log files before automatic deletion.

**Recommendations**:
- **Short (7 days)**: Minimal disk usage
- **Default (30 days)**: Good balance for troubleshooting
- **Long (90+ days)**: Historical analysis, development

**Disk Space Estimate**:
- ~500KB - 2MB per day (depends on log level)
- 30 days @ Error level: ~15-30MB
- 30 days @ Debug level: ~50-100MB

---

## Advanced Windows Settings

⚠️ **Expert Settings**: These settings control low-level Windows API behavior for the destination (preview) window. Only modify if you understand Windows window management.

### Available Settings

All advanced settings are **optional** and default to system-appropriate values if not set.

#### Window Opacity
**Setting**: `winapi_destination_alpha`  
**Type**: Integer (0-255)  
**Default**: `255` (fully opaque)

Controls window transparency:
- `255`: Fully opaque (default)
- `128`: 50% transparent
- `0`: Fully transparent (invisible)

---

#### Always on Top
**Setting**: `winapi_destination_topmost`  
**Type**: Boolean  
**Default**: `true`

**Description**: Keeps preview window above other windows.

- `true` (Default): Window stays on top
- `false`: Window can be covered by other windows

---

#### Click Through
**Setting**: `winapi_destination_click_through`  
**Type**: Boolean  
**Default**: `true`

**Description**: Mouse clicks pass through preview window to windows behind it.

- `true` (Default): Can click through preview window
- `false`: Preview window captures mouse input

---

#### Tool Window
**Setting**: `winapi_destination_toolwindow`  
**Type**: Boolean  
**Default**: `false`

**Description**: Window appears as a tool palette (affects Alt+Tab behavior).

- `true`: Excluded from Alt+Tab
- `false` (Default): Normal window behavior

---

#### Layered Window
**Setting**: `winapi_destination_layered`  
**Type**: Boolean  
**Default**: `true`

**Description**: Enables transparency and alpha channel support.

- `true` (Default): Required for `alpha` setting
- `false`: Opaque window only

---

#### App Window
**Setting**: `winapi_destination_appwindow`  
**Type**: Boolean  
**Default**: `true`

**Description**: Window appears in screen sharing pickers (Meet, Zoom, Teams).

- `true` (Default): Appears in window lists
- `false`: Hidden from window lists

---

#### No Activate
**Setting**: `winapi_destination_noactivate`  
**Type**: Boolean  
**Default**: `true`

**Description**: Prevents window from taking focus when clicked.

- `true` (Default): Doesn't steal focus
- `false`: Normal focus behavior

---

#### Overlapped Window
**Setting**: `winapi_destination_overlapped`  
**Type**: Boolean  
**Default**: `false`

**Description**: Window has title bar and borders (standard window).

- `true`: Looks like normal application window
- `false` (Default): Borderless window

---

#### Hide Taskbar After Start
**Setting**: `winapi_destination_hide_taskbar_after_ms`  
**Type**: Integer (milliseconds) or `null`  
**Default**: `null` (disabled)

**Description**: After starting capture, automatically hides preview window from taskbar after specified delay.

**Usage**:
- `null` (Default): No auto-hide
- `1000`: Hide after 1 second
- `5000`: Hide after 5 seconds

**Use Case**: Want preview window in screen picker but not in taskbar.

---

## Configuration File

Settings are stored in:
- **Windows**: `%APPDATA%\com.salihcantekin.rustframe\settings.json`
- **macOS**: `~/Library/Application Support/com.salihcantekin.rustframe/settings.json`
- **Linux**: `~/.config/rustframe/settings.json`

### Manual Editing

You can manually edit `settings.json` while RustFrame is closed. Changes take effect on next launch.

**Example settings.json**:
```json
{
  "capture_method": "Wgc",
  "target_fps": 60,
  "gpu_acceleration": true,
  "show_cursor": false,
  "capture_clicks": true,
  "click_highlight_color": [255, 255, 0, 180],
  "click_dissolve_ms": 300,
  "click_highlight_radius": 20,
  "show_border": true,
  "border_width": 4,
  "border_color": [255, 0, 0, 255],
  "remember_last_region": true,
  "last_region": [100, 100, 1200, 800],
  "show_rec_indicator": true,
  "rec_indicator_size": "small",
  "log_level": "Error",
  "log_to_file": true,
  "log_retention_days": 30
}
```

### Reset to Defaults

To reset all settings:
1. Close RustFrame
2. Delete `settings.json` file
3. Restart RustFrame (will create new settings with defaults)

---

## Settings Profiles

RustFrame supports multiple configuration profiles for different use cases.

### Built-in Profiles

Location: `resources/profiles/[platform]/`

**Windows**:
- `performance.json` - Maximum FPS, GPU acceleration
- `compatibility.json` - GDI capture, CPU fallback
- `presentation.json` - Click highlights, smooth motion

**macOS**:
- `performance.json` - GPU accelerated, 60 FPS
- `battery-saver.json` - 30 FPS, minimal CPU

### Using Profiles

Profiles can be applied through the UI or by copying their contents to `settings.json`.

---

## Troubleshooting Settings

For troubleshooting settings, see the [Troubleshooting Guide](troubleshooting.md) and [Features Guide](features.md#performance-settings).

## Related Documentation

- [Features Guide](features.md) - Detailed feature descriptions
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
- [Performance Optimization](../technical/gpu-optimization.md) - Technical performance details
- [FAQ](faq.md) - Frequently asked questions
