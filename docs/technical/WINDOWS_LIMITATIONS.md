# Windows Platform Limitations - Window Filtering

## Summary

⚠️ **Window filtering (include/exclude apps/windows) is NOT available on Windows** due to operating system API limitations.  
✅ **This feature is ONLY available on macOS 12.3+**

## Why Windows Cannot Support Native Window Exclusion

### The Problem

RustFrame captures an **arbitrary screen region** (e.g., 800×600 pixels at position 100,100). Unlike traditional screen capture that targets a specific window, we need to:

1. Capture monitor pixels in a rectangular region
2. Exclude certain application windows from that capture
3. Do this in real-time (60 FPS) without performance degradation
4. **Without visual artifacts** (no black rectangles in video calls)

### What We Tried

#### ❌ Approach 1: CPU Masking (Black Rectangles)

```
Capture frame → Enumerate excluded windows → Paint black over them
```

**Problem:** Video conferencing apps (Meet/Zoom/Teams) show **black rectangles** where excluded windows were.

**Why it fails:**
- Meet/Zoom encode in RGB format (no transparency/alpha support)
- Black masking creates visible artifacts in shared screen
- CPU overhead for pixel manipulation (5-15% additional CPU usage)
- User experience: "Why are there black boxes in my meeting?"

**Example:**
```
User captures region with Chrome + Slack windows
Slack is excluded → Black rectangle appears in Chrome's position
Meet attendees see: ⬛ (black box) instead of desktop background
```

#### ❌ Approach 2: Alpha Channel Transparency

```
Set excluded pixels to alpha=0 (fully transparent)
```

**Problem:** Video codecs **strip alpha channel** during encoding.

**Why it fails:**
- WebRTC MediaStream uses RGB/YUV formats (no alpha channel)
- Electron `desktopCapturer` API provides RGB only
- VP8/VP9/H.264 codecs don't preserve transparency
- Transparent pixels → Black pixels during encode

**Technical Details:**
```rust
// Even if we set alpha to 0:
data[pixel_offset + 3] = 0;  // BGRA alpha channel

// Video encoder converts to RGB:
RGB(0, 0, 0) = Black  // Transparency lost
```

#### ❌ Approach 3: Z-Order Manipulation

```rust
SetWindowPos(excluded_hwnd, HWND_BOTTOM, ...)  
// Move window behind capture region
```

**Problems:**
- Changes user's window order (breaks workflow)
- Doesn't work with fullscreen/maximized windows
- Windows DWM (Desktop Window Manager) still composites all layers
- Complex restoration logic required
- **Still captures the window** (just moved to back)

**Why it fails:**
- Monitor capture includes ALL layers in DWM composition
- Z-order only affects visual stacking, not capture
- Users complain: "Why did my windows rearrange?"

#### ❌ Approach 4: Windows Graphics Capture API

Microsoft's WGC API provides:

```rust
session.SetIncludeSecondaryWindows(false);  // ✅ EXISTS
```

**Problem:** Only works for **child/owned windows** of the captured item.

**Missing API:**
```rust
// This API does NOT exist in Windows:
session.SetExcludedWindows(&[hwnd1, hwnd2, hwnd3]);  // ❌ NOT AVAILABLE
session.ExcludeWindowsByProcessName(&["zoom.exe", "slack.exe"]);  // ❌ NOT AVAILABLE
```

**What `SetIncludeSecondaryWindows` actually does:**
- Applies ONLY to window-level capture (not monitor/region capture)
- Controls child/owned windows of THE SAME APPLICATION
- Example: Main window + popup dialogs from same app
- **Cannot exclude arbitrary windows from other applications**

### What macOS Has (That Windows Doesn't)

macOS **ScreenCaptureKit** (introduced in macOS 12.3 / 2022) provides **native GPU-accelerated exclusion**:

```swift
let filter = SCContentFilter(
    display: display,
    excluding: [window1, window2, window3]  // ✅ Native API
)

let stream = SCStream(
    filter: filter,
    configuration: config,
    delegate: self
)
```

**Advantages:**
- ✅ GPU-accelerated masking (zero CPU overhead)
- ✅ No visual artifacts (excluded windows never enter capture pipeline)
- ✅ Hardware-level filtering BEFORE encoding
- ✅ Works with any window from any application
- ✅ Maintains display background where windows were excluded

**Windows equivalent:** **DOES NOT EXIST**

Microsoft has not provided equivalent functionality in Windows Graphics Capture API as of Windows 11 24H2 (January 2026).

### Why OBS/Other Software Don't Have This Either

**OBS Studio** uses a different capture model:

1. **Window Capture**: Captures ONE specific window (not a region)
   - `GraphicsCaptureItem::CreateForWindow(hwnd)`
   - Excludes everything except that window automatically
   
2. **Display Capture**: Captures ENTIRE monitor
   - Users manually hide/minimize OBS window
   - Or use separate monitor for OBS controls

**OBS does NOT support:**
- ❌ Arbitrary region capture (our use case)
- ❌ Excluding specific windows from region/display capture
- ❌ Dynamic window filtering during capture

**Workarounds OBS users do:**
- Use "Window Capture" source (captures 1 window only)
- Use multiple monitors (OBS on one, content on another)
- Manually minimize windows they don't want captured
- Use "Projector" window for preview (separate from controls)

**Other software status:**
- **ShareX**: No window exclusion for region capture
- **Snagit**: No window exclusion (static capture only)
- **Loom**: Window capture OR full screen (no hybrid)
- **Discord**: Window capture OR screen share (no exclusions)

### Why This Matters

**Without native window exclusion, users see:**

```
┌─────────────────────────────────┐
│  Chrome Browser                 │
│                                 │
│  ⬛⬛⬛⬛⬛  ← Black box      │
│  ⬛ Slack ⬛  where excluded   │
│  ⬛⬛⬛⬛⬛  window should be  │
│                                 │
└─────────────────────────────────┘
```

**In video calls, attendees ask:**
- "What's that black rectangle?"
- "Can you share your screen properly?"
- "I can't see the content behind the black box"

**This is why we MUST disable the feature on Windows.**

## Technical Deep Dive

### Windows Graphics Capture Architecture

```
User selects region: (100, 100, 800, 600)
          ↓
Monitor-level capture via WGC:
  GraphicsCaptureItem::CreateForMonitor(monitor_handle)
          ↓
D3D11 texture (full monitor resolution):
  - Contains ALL windows visible on monitor
  - DWM compositor output
  - No per-window control
          ↓
Manual crop to region:
  - GPU shader extracts (100, 100, 800, 600) rectangle
  - Still includes ALL windows within that region
          ↓
No API to exclude windows at this stage
```

**Why region capture is fundamentally different:**

| Type | API | Window Control |
|------|-----|----------------|
| **Window Capture** | `CreateForWindow(hwnd)` | ✅ Captures only that window |
| **Monitor Capture** | `CreateForMonitor(monitor)` | ❌ Captures all windows |
| **Region Capture** | Monitor → Manual Crop | ❌ No exclusion API |

### macOS SCK Architecture (For Comparison)

```
User selects region: (100, 100, 800, 600)
          ↓
SCContentFilter with exclusions:
  SCContentFilter(
    display: display,
    excluding: [Slack.app, Discord.app]
  )
          ↓
ScreenCaptureKit engine (GPU):
  - Filters windows BEFORE composition
  - Only includes non-excluded windows
  - Maintains desktop background
          ↓
IOSurface (Metal texture):
  - Already has exclusions applied
  - Zero CPU overhead
  - No artifacts
```

**Key difference:** macOS filters at **capture source**, Windows filters at **destination** (too late).

## Alternative Solutions (Why They Don't Work)

### 1. Desktop Duplication API (DXGI)

```rust
IDXGIOutputDuplication::AcquireNextFrame()
```

**Limitations:**
- Full monitor capture only (no region selection)
- Single-monitor restriction
- Same window exclusion problem
- **Deprecated** in favor of WGC

### 2. PrintWindow API

```rust
PrintWindow(hwnd, hdc, PW_RENDERFULLCONTENT)
```

**Limitations:**
- Window-by-window capture (can't capture regions)
- Doesn't capture all window types (e.g., UWP apps)
- Slow performance (CPU-based)
- Can't exclude windows (captures what you target)

### 3. Custom DWM Hook

Hypothetically intercept Desktop Window Manager composition...

**Why it's impossible:**
- DWM composition happens in protected kernel space
- No public API for composition pipeline access
- Would require kernel-mode driver (security risk)
- Microsoft explicitly prohibits this

### 4. Chroma Key (Green Screen)

Paint excluded windows green, then filter in OBS/vMix...

**Why it's impractical:**
- Requires external software (OBS Virtual Camera)
- Complex user setup
- Performance overhead (extra encoding pass)
- False positives (real green content gets filtered)
- Still shows GREEN boxes in raw capture

## Platform Comparison Table

| Feature | macOS 12.3+ | Windows 10/11 | Linux (Wayland) |
|---------|-------------|---------------|-----------------|
| **Native Window Exclusion** | ✅ SCContentFilter | ❌ Not Available | ❌ Not Available |
| **GPU Acceleration** | ✅ Metal + IOSurface | ✅ WGC + D3D11 | ✅ PipeWire + DMA-BUF |
| **Region Capture** | ✅ Yes | ✅ Yes | ✅ Yes |
| **Arbitrary Window Exclusion** | ✅ Yes | ❌ **NO** | ❌ **NO** |
| **CPU Masking Workaround** | N/A (native support) | ⚠️ Creates black boxes | ⚠️ Creates black boxes |

## Decision: Feature Gating

Based on this analysis, **RustFrame disables window filtering on Windows and Linux**:

```rust
// src/config.rs
#[cfg(target_os = "macos")]
pub const SUPPORTS_WINDOW_FILTERING: bool = true;

#[cfg(not(target_os = "macos"))]
pub const SUPPORTS_WINDOW_FILTERING: bool = false;
```

### User Communication

**Windows users will see:**
- Share Content tab: ❌ Hidden
- Window filter summary: ❌ Hidden
- README note: "Window filtering requires macOS 12.3+"

**macOS users will see:**
- Share Content tab: ✅ Available
- Full include/exclude functionality
- Native GPU performance

## Future Possibilities

### If Microsoft Adds Native Exclusion API

Hypothetical future Windows API:

```rust
// If Microsoft adds this API:
session.SetExcludedWindows(&excluded_hwnd_list);
```

**Requirements for RustFrame to support:**
1. API must work with monitor-level capture
2. Must not create visual artifacts
3. Must be GPU-accelerated
4. Must be available on Windows 10+ (not just 11+)

**Timeline:** Unknown. Microsoft has not announced such plans.

### Linux (Wayland) Future

PipeWire (Linux screencasting) might add exclusion support:

```c
// Future PipeWire API (hypothetical):
pw_stream_connect(stream,
    PW_DIRECTION_INPUT,
    PW_ID_ANY,
    PW_STREAM_FLAG_AUTOCONNECT |
    PW_STREAM_FLAG_EXCLUDE_WINDOWS,
    params, n_params);
```

**Status:** Not available as of PipeWire 1.0.x (January 2026)

## References

- [Windows Graphics Capture API Documentation](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture)
- [macOS ScreenCaptureKit Documentation](https://developer.apple.com/documentation/screencapturekit)
- [OBS Studio Source Code - Window Capture Plugin](https://github.com/obsproject/obs-studio/tree/master/plugins/win-capture)
- [PipeWire Screencasting API](https://docs.pipewire.org/)

## Related Documentation

- [macOS Window Exclusion Implementation](./macos-window-exclusion.md) - Reference for how it SHOULD work
- [GPU Optimization](./gpu-optimization.md) - WGC and D3D11 architecture
- [User Guide - Platform Support](../user-guide/platform-support.md) - User-facing explanation
- [FAQ - Why can't I exclude windows on Windows?](../user-guide/faq.md#windows-window-exclusion)

---

**Last Updated:** January 11, 2026  
**Status:** Windows window exclusion is NOT SUPPORTED due to OS API limitations  
**Alternative:** Use macOS 12.3+ for full window filtering functionality
