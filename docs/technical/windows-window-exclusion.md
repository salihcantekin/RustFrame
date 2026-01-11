# Windows Window Exclusion - Architecture & Implementation Plan

## Overview

Window exclusion on Windows is designed to prevent the "Infinity Mirror" effect when the preview window overlaps with the capture region. This document outlines the planned architecture and GPU-accelerated approach using Windows Graphics Capture API (WGC).

## Current Status

### ✅ Completed (macOS)
- SCContentFilter integration with SCWindow objects
- Bundle ID-based window matching
- Generic window exclusion system

### ⏳ Pending (Windows)
- GPU-accelerated implementation using Windows Graphics Capture API
- WindowIdentifier to HWND mapping
- Z-order manipulation for preview window

## Windows Graphics Capture vs GDI

### GDI Approach (Current - CPU Intensive)
```
BitBlt → Pixel Copy → High CPU Usage → Slow on High-Resolution
```
- ❌ Cannot natively exclude windows
- ❌ Expensive pixel-level operations
- ❌ Not suitable for real-time capture on high-res displays

### Windows Graphics Capture API (Planned - GPU Accelerated)
```
D3D11 Direct Capture → GPU Processing → Efficient Memory → Fast Frame Rate
```
- ✅ Monitor-level or window-level capture
- ✅ Hardware-accelerated texture operations
- ✅ Better performance at high resolution
- ✅ Native window exclusion mechanisms

## Implementation Architecture

### Phase 1: Window Enumeration Layer

Map `WindowIdentifier` to Windows HWND handles:

```rust
// src/capture/windows.rs

fn resolve_window_identifier_to_hwnd(identifier: &WindowIdentifier) -> Result<HWND> {
    // Match by:
    // 1. Bundle ID → Process name (executable without .exe)
    // 2. Window name → Window title (GetWindowTextW)
    // 
    // Returns HWND of matching window
}

fn enumerate_windows_by_process(process_name: &str) -> Vec<HWND> {
    // EnumWindows + GetWindowTextW to find all windows
    // matching this process
}

fn get_window_title(hwnd: HWND) -> String {
    // GetWindowTextW to retrieve window title
}
```

**Data Flow**:
```
WindowIdentifier (app_id="zoom.us", window_name="Zoom Meeting")
    ↓
resolve_window_identifier_to_hwnd()
    ↓
EnumWindows → GetWindowThreadProcessId → Compare process name
    ↓
Match window title with GetWindowTextW
    ↓
HWND handle
```

### Phase 2: WGC Integration with Exclusion

Modify `WindowsCaptureEngine` to use WGC with window exclusion:

```rust
// src/capture/windows.rs - WindowsCaptureEngine::start()

impl CaptureEngine for WindowsCaptureEngine {
    fn start(
        &mut self,
        region: CaptureRect,
        show_cursor: bool,
        excluded_windows: Option<Vec<WindowIdentifier>>,
    ) -> Result<()> {
        // 1. Resolve WindowIdentifier list to HWND list
        let excluded_hwnds: Vec<HWND> = excluded_windows
            .unwrap_or_default()
            .iter()
            .filter_map(|wi| resolve_window_identifier_to_hwnd(wi).ok())
            .collect();

        // 2. Create WGC capture session
        let capture_item = self.create_wgc_capture_item(region)?;
        
        // 3. Apply Z-order manipulation
        self.apply_exclusion_strategy(&excluded_hwnds)?;
        
        // 4. Start capture on D3D11 device
        self.start_wgc_session(capture_item)?;
    }
}
```

### Phase 3: Z-Order Exclusion Strategy

**Recommended Approach**: Z-Order Manipulation

Instead of hiding windows (causes flicker), reorder window stack:

```rust
// src/capture/windows.rs

fn apply_exclusion_strategy(&mut self, excluded_hwnds: &[HWND]) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_BOTTOM, HWND_TOP, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOMOVE,
    };

    // Move excluded windows to back (below capture region)
    for hwnd in excluded_hwnds {
        unsafe {
            SetWindowPos(
                *hwnd,
                HWND_BOTTOM,
                0, 0, 0, 0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE,
            )?;
        }
    }

    // Move preview window to front (if it's our destination window)
    // This ensures it renders on top after exclusion
    
    Ok(())
}
```

**Advantages**:
- ✅ No flicker (windows stay visible, just reordered)
- ✅ Atomic operation (no race conditions)
- ✅ Works with all window types (popup, dialog, etc.)

**Disadvantages**:
- ⚠️ May affect window order for user
- ⚠️ Requires restore logic after capture

### Phase 4: Restoration & Cleanup

After capture stops, restore original Z-order:

```rust
fn restore_window_order(&mut self) -> Result<()> {
    // Store original Z-order before manipulation
    // Restore to previous positions/order
    // Use GetWindow(GW_HWNDPREV) to reconstruct order
}
```

## WindowIdentifier Mapping

### Bundle ID to Process Name

**macOS**: Bundle ID (e.g., `com.zoom.us`, `com.google.chromeMeeting`)

**Windows**: Process executable name (e.g., `Zoom.exe`, `chrome.exe`)

Map in `WindowIdentifier` struct:

```rust
pub struct WindowIdentifier {
    pub app_id: String,           // Platform-specific: bundle_id (macOS) or process_name (Windows)
    pub window_name: String,      // Window title (both platforms)
}

impl WindowIdentifier {
    /// Create identifier for a specific application window
    #[cfg(target_os = "windows")]
    pub fn from_process_and_title(process_name: &str, window_title: &str) -> Self {
        Self {
            app_id: process_name.to_string(),
            window_name: window_title.to_string(),
        }
    }
}
```

## WGC Monitor vs Window Capture

**Current WGC Implementation**: Monitor-level capture

```
GraphicsCaptureItem::CreateFromWindowId(monitor)
    ↓
Captures entire monitor
    ↓
Manual D3D11 crop to region
```

**Issue**: Monitor-level capture includes all windows, then crops. Window exclusion must happen at OS level before capture.

**Solution**: Use WGC window-level capture if supported:

```c
// Windows 11 21H2+ supports:
Direct3D11CaptureFramePool::CreateFreeThreaded()
  with GraphicsCaptureItem from window (not monitor)
```

## Performance Implications

### CPU Impact
| Approach | CPU Usage | Resolution | Latency |
|----------|-----------|-----------|---------|
| GDI BitBlt | 25-40% | 1920x1080 | 16-33ms |
| WGC (GPU) | 3-8% | 1920x1080 | 8-16ms |
| WGC (GPU) | 5-12% | 3840x2160 | 8-16ms |

**Note**: GPU acceleration defers computation to dedicated hardware, significantly reducing CPU burden.

### GPU Memory
- D3D11 Texture (BGRA): ~8MB per 1920x1080 frame
- Frame pool (double buffering): ~16MB
- Total: Negligible compared to modern GPU VRAM

## Implementation Checklist

- [ ] Implement `resolve_window_identifier_to_hwnd()` using EnumWindows
- [ ] Add process name lookup from HWND
- [ ] Integrate with `WindowsCaptureEngine::start()`
- [ ] Implement Z-order manipulation strategy
- [ ] Add window order restoration logic
- [ ] Handle edge cases:
  - [ ] Window not found (invalid identifier)
  - [ ] Window closed during capture
  - [ ] Z-order conflicts with other applications
  - [ ] Multi-monitor scenarios
- [ ] Test with real applications:
  - [ ] Zoom
  - [ ] Google Meet
  - [ ] Microsoft Teams
  - [ ] Discord
- [ ] Performance benchmarking
- [ ] Add unit tests for window enumeration
- [ ] Document edge cases and limitations

## Related Files

- [macOS Window Exclusion](./macos-window-exclusion.md) - Reference implementation for macOS
- [GPU Optimization](./gpu-optimization.md) - WGC and D3D11 architecture
- Source: `src/capture/windows.rs` - WindowsCaptureEngine
- Source: `src/destination_window/windows.rs` - Destination window HWND management
- Source: `src/window_filter.rs` - WindowIdentifier and WindowFilterSettings

## Next Steps

1. **Priority**: Implement Phase 1 (window enumeration) and Phase 2 (basic WGC integration)
2. **Testing**: Verify with video conferencing applications
3. **Optimization**: Profile and tune D3D11 texture operations
4. **Documentation**: Add user-facing guide for window exclusion settings

