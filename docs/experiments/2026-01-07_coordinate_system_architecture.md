# Coordinate System Architecture - Y-Axis Inversion Fix

**Date**: January 7, 2026  
**Issue**: Click highlights appearing at wrong Y coordinates after CGEventTap migration  
**Root Cause**: Manual coordinate conversion instead of using centralized display_info module  
**Status**: ✅ FIXED

---

## Problem

After migrating from polling-based mouse capture to event-driven CGEventTap, click highlights were appearing at inverted Y positions. This was a regression of a previously solved problem.

### Symptoms
- Clicking at top of screen → highlight appears at bottom
- Clicking at bottom → highlight appears at top
- X coordinates were correct
- Only Y axis was inverted

---

## Root Cause Analysis

### macOS Coordinate Systems

macOS uses **multiple coordinate systems** that must be converted correctly:

1. **CGEvent/NSEvent Coordinates**
   - Origin: **Bottom-left corner** (Cocoa convention)
   - Units: **Points** (logical units)
   - Example: (0, 0) = bottom-left, (1000, 800) = top-right on 1000x800pt display

2. **Screen Capture Coordinates**
   - Origin: **Top-left corner** (standard graphics convention)
   - Units: **Pixels** (physical units)
   - Example: (0, 0) = top-left, (2000, 1600) = bottom-right on 2000x1600px display (Retina)

3. **Retina Display Scaling**
   - Scale factor: 2.0x for Retina displays
   - Points to Pixels: multiply by scale_factor
   - Pixels to Points: divide by scale_factor

### What Went Wrong

In the CGEventTap implementation (`platform.rs`), we manually converted coordinates:

```rust
// ❌ OLD CODE - Manual conversion (BUGGY)
let screen_height = f64::from_bits(SCREEN_HEIGHT.load(Ordering::Relaxed));
let scale_factor = f64::from_bits(SCREEN_SCALE_FACTOR.load(Ordering::Relaxed));

// Convert coordinates: CGEvent uses bottom-left origin in points
let y_flipped = screen_height - location.y;
let x_pixels = (location.x * scale_factor) as i32;
let y_pixels = (y_flipped * scale_factor) as i32;
```

**Problems**:
1. Using atomic storage for screen info (outdated approach)
2. No centralized coordinate conversion
3. Easy to make mistakes in conversion logic
4. Duplicated code across multiple files

---

## Architectural Solution

### Centralized Display Info Module

We already had a `display_info.rs` module (created in previous fixes) that provides:

1. **Single Source of Truth**: All display properties in one place
2. **Consistent Conversions**: Centralized conversion functions
3. **Cross-Platform Support**: Works on macOS, Windows, Linux
4. **Type Safety**: Compile-time guarantees for coordinate systems

### Module Structure

```rust
// src/display_info.rs

pub struct DisplayInfo {
    pub scale_factor: f64,          // 2.0 for Retina, 1.0 for standard
    pub width_points: f64,           // Logical width
    pub height_points: f64,          // Logical height
    pub width_pixels: u32,           // Physical width
    pub height_pixels: u32,          // Physical height
    pub initialized: bool,
}
```

### Conversion Functions

```rust
impl DisplayInfo {
    /// Convert macOS CGEvent coordinates (bottom-left, points) 
    /// to screen capture coordinates (top-left, pixels)
    pub fn macos_event_to_screen_pixels(&self, x_points: f64, y_points: f64) -> (i32, i32) {
        // Step 1: Flip Y axis (bottom-left → top-left)
        let y_flipped_points = self.height_points - y_points;
        
        // Step 2: Convert to pixels
        let x_pixels = (x_points * self.scale_factor) as i32;
        let y_pixels = (y_flipped_points * self.scale_factor) as i32;
        
        (x_pixels, y_pixels)
    }
    
    /// Reverse conversion: screen pixels → macOS event coordinates
    pub fn screen_pixels_to_macos_event(&self, x_pixels: i32, y_pixels: i32) -> (f64, f64) {
        // Step 1: Convert to points
        let x_points = x_pixels as f64 / self.scale_factor;
        let y_points = y_pixels as f64 / self.scale_factor;
        
        // Step 2: Flip Y axis (top-left → bottom-left)
        let y_flipped_points = self.height_points - y_points;
        
        (x_points, y_flipped_points)
    }
}
```

---

## Implementation Fix

### Before (Buggy Manual Conversion)

```rust
// src/platform.rs - CGEventTap callback
extern "C" fn event_callback(...) -> *mut std::ffi::c_void {
    unsafe {
        let location = CGEventGetLocation(event);
        
        // ❌ Manual conversion - prone to errors
        let screen_height = f64::from_bits(SCREEN_HEIGHT.load(Ordering::Relaxed));
        let scale_factor = f64::from_bits(SCREEN_SCALE_FACTOR.load(Ordering::Relaxed));
        
        let y_flipped = screen_height - location.y;
        let x_pixels = (location.x * scale_factor) as i32;
        let y_pixels = (y_flipped * scale_factor) as i32;
        
        log_click(x_pixels, y_pixels, btn);
    }
    event
}
```

### After (Centralized Conversion)

```rust
// src/platform.rs - CGEventTap callback
extern "C" fn event_callback(...) -> *mut std::ffi::c_void {
    unsafe {
        let location = CGEventGetLocation(event);
        
        // ✅ Use centralized display_info for conversion
        let display_info = crate::display_info::get();
        let (x_pixels, y_pixels) = display_info.macos_event_to_screen_pixels(
            location.x, 
            location.y
        );
        
        log::debug!("[MACOS_CLICK] CGEvent: ({:.1}pt, {:.1}pt) -> ({}, {})px @ {:.1}x",
            location.x, location.y, x_pixels, y_pixels, display_info.scale_factor);
        
        log_click(x_pixels, y_pixels, btn);
    }
    event
}
```

---

## Benefits of Centralized Approach

### 1. Single Source of Truth
- All display information in one module
- Initialize once at startup
- Access anywhere in codebase
- No duplication or drift

### 2. Type Safety
- Clear function names indicate coordinate systems
- `macos_event_to_screen_pixels()` - explicit about conversion
- Compile-time guarantees

### 3. Maintainability
- Coordinate conversion logic in ONE place
- Easy to fix bugs (change once, fixes everywhere)
- Easy to add new platforms
- Self-documenting code

### 4. Testability
```rust
#[test]
fn test_coordinate_conversion() {
    let info = DisplayInfo {
        scale_factor: 2.0,
        height_points: 900.0,
        // ... other fields
    };
    
    // Bottom-left (0, 0) in macOS → Top-left (0, 1800) in pixels
    let (x, y) = info.macos_event_to_screen_pixels(0.0, 0.0);
    assert_eq!(x, 0);
    assert_eq!(y, 1800);
}
```

### 5. Cross-Platform Consistency
```rust
// src/display_info.rs

#[cfg(target_os = "macos")]
pub fn initialize() -> Result<()> {
    // macOS-specific initialization
}

#[cfg(target_os = "windows")]
pub fn initialize() -> Result<()> {
    // Windows-specific initialization
}

#[cfg(target_os = "linux")]
pub fn initialize() -> Result<()> {
    // Linux-specific initialization
}

// Same interface everywhere!
pub fn get() -> DisplayInfo { ... }
```

---

## Usage Guidelines

### DO ✅

```rust
// 1. Initialize once at startup
display_info::initialize()?;

// 2. Use centralized conversions
let info = display_info::get();
let (x_px, y_px) = info.macos_event_to_screen_pixels(x_pt, y_pt);

// 3. Access scale factor from display_info
let scale = info.scale_factor;

// 4. Use consistent coordinate system throughout module
// - Store coordinates in ONE system (prefer pixels for screen capture)
// - Convert at boundaries (input/output)
```

### DON'T ❌

```rust
// 1. Don't manually convert coordinates
let y_flipped = screen_height - y;  // ❌ Error-prone

// 2. Don't store display info in multiple places
static SCREEN_HEIGHT: AtomicU64 = ...;  // ❌ Duplication
static SCALE_FACTOR: AtomicU64 = ...;   // ❌ Can drift

// 3. Don't mix coordinate systems
let x_pixels = ...; 
let y_points = ...;  // ❌ Inconsistent units

// 4. Don't skip coordinate conversion
CGEventGetLocation(event) -> store directly  // ❌ Wrong origin
```

---

## Coordinate Flow Diagram

```
┌──────────────────────────────────────────────────────────────┐
│                  macOS Input (CGEvent)                       │
│  Origin: Bottom-left, Units: Points                          │
│  Example: (500pt, 700pt) on 1000x900pt display              │
└────────────────────┬─────────────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────────────┐
│            display_info.macos_event_to_screen_pixels()       │
│                                                              │
│  Step 1: Flip Y → 900 - 700 = 200pt                        │
│  Step 2: Scale → (500 * 2.0, 200 * 2.0) = (1000, 400)px   │
└────────────────────┬─────────────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────────────┐
│            Screen Capture Coordinates (Pixels)               │
│  Origin: Top-left, Units: Pixels                            │
│  Example: (1000px, 400px) = correct click position          │
│                                                              │
│  Used by:                                                    │
│  - Capture engine (region selection)                        │
│  - Click highlight rendering                                │
│  - Frame buffer coordinates                                 │
└──────────────────────────────────────────────────────────────┘
```

---

## Testing Checklist

When modifying coordinate conversion code, always test:

### Manual Tests
1. ✅ Click top-left corner → highlight appears at top-left
2. ✅ Click bottom-right corner → highlight appears at bottom-right
3. ✅ Click center → highlight appears at center
4. ✅ Test on Retina display (2.0x scale)
5. ✅ Test on non-Retina display (1.0x scale)

### Console Verification
```bash
# Look for these log messages:
[MACOS_CLICK] CGEvent: (X.Xpt, Y.Ypt) -> (Xpx, Ypx) @ 2.0x
[RENDER] Drawing click at screen (Xpx, Ypx) -> frame (Xf, Yf)
```

### Expected Conversions (Retina 2560x1600px = 1280x800pt)

| Click Location | macOS Event (pt) | Screen Pixels (px) | Notes |
|----------------|------------------|--------------------|-------|
| Top-left | (0, 800) | (0, 0) | Y flipped & scaled |
| Top-right | (1280, 800) | (2560, 0) | X scaled, Y flipped |
| Bottom-left | (0, 0) | (0, 1600) | Y flipped & scaled |
| Bottom-right | (1280, 0) | (2560, 1600) | Both scaled & Y flipped |
| Center | (640, 400) | (1280, 800) | Both scaled & Y flipped |

---

## Related Files

### Core Module
- [`src/display_info.rs`](../../src/display_info.rs) - Centralized display information and conversion functions

### Usage Locations
- [`src/platform.rs`](../../src/platform.rs) - CGEventTap mouse capture
- [`src/main.rs`](../../src/main.rs) - Click highlight rendering
- [`src/capture/macos.rs`](../../src/capture/macos.rs) - Screen capture region
- [`src/hollow_border/macos.rs`](../../src/hollow_border/macos.rs) - Border positioning

### Related Docs
- [Color Format Fix](./2026-01-07_color_format_fix.md) - Platform-specific pixel formats
- [Event-Driven Optimization](./2026-01-07_event_driven_optimization.md) - CGEventTap implementation

---

## Lessons Learned

### 1. Don't Reinvent Coordinate Conversion
If you've solved a problem once (coordinate conversion), **don't solve it again differently**. Use the existing solution.

### 2. Centralize Platform-Specific Logic
- One module per platform concern
- Clear interfaces
- Easy to maintain

### 3. Make Mistakes Obvious
- Function names should indicate what they do: `macos_event_to_screen_pixels()`
- Type system should prevent mixing units (consider newtype pattern for future)

### 4. Document Coordinate Systems
- Always document origin (top-left vs bottom-left)
- Always document units (points vs pixels)
- Add diagrams when helpful

### 5. Test Cross-Platform Code on All Platforms
- macOS: Bottom-left origin, Retina scaling
- Windows: Top-left origin, DPI scaling
- Linux: Depends on display server (X11/Wayland)

---

## Future Improvements

### Type-Safe Coordinates (Optional)
```rust
// Potential future enhancement
pub struct Points(f64);
pub struct Pixels(i32);
pub struct MacOSPoint { x: Points, y: Points }
pub struct ScreenPixel { x: Pixels, y: Pixels }

// Compiler prevents mixing:
let pt: MacOSPoint = ...;
let px: ScreenPixel = info.convert(pt);  // ✅ Type-safe
let mixed = pt.x + px.x;  // ❌ Compile error - can't mix Points and Pixels
```

### Performance Optimization
- Cache display info in thread-local storage for hot paths
- Pre-compute frequently used conversions

### Better Error Handling
```rust
pub fn macos_event_to_screen_pixels(&self, ...) -> Result<(i32, i32), ConversionError> {
    if !self.initialized {
        return Err(ConversionError::NotInitialized);
    }
    // ... conversion logic
}
```

---

## Conclusion

The Y-axis inversion bug was caused by **not using the centralized `display_info` module** that was specifically created to solve this class of problems. By consistently using the architectural solution we already had in place, we:

1. ✅ Fixed the bug (click highlights now appear at correct positions)
2. ✅ Reduced code duplication (removed manual conversions)
3. ✅ Made the code more maintainable (one place to fix bugs)
4. ✅ Prevented future regressions (clear API prevents mistakes)

**Key Takeaway**: When you've invested in an architectural solution, **use it consistently**. Don't bypass it with manual implementations that seem simpler at first but lead to bugs.
