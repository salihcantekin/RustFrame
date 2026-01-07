# Window Visibility Tests for Screen Sharing Compatibility

**Date:** January 7, 2026  
**Objective:** Make preview window visible in Google Meet/Zoom screen sharing pickers while keeping it hidden from user's desktop  
**Platform:** macOS with CGWindowList API

## Background

The preview window needs to appear in screen sharing application pickers (Google Meet, Zoom, Teams, Discord) but should remain invisible to the user during normal operation. macOS's CGWindowList API with `kCGWindowListOptionOnScreenOnly` filter creates conflicting requirements.

## Test Methodologies

### Test Environment
- macOS (Sonoma/Sequoia)
- Google Meet screen sharing picker
- CGWindowList capture behavior analysis
- Window appearance verification

### Testing Approach
1. Modify window properties in `src/destination_window/macos.rs`
2. Build release version: `cargo build --release`
3. Launch application and start capture
4. Open Google Meet screen sharing picker
5. Verify window appearance (visible content vs black screen vs missing)
6. Check desktop visibility to user

---

## Experiment Results

### Strategy 1: Mini Window (1×1 pixel)
**Implementation:**
```rust
let (x_pos, y_pos) = (0.0, 0.0);
let (width, height) = (1, 1);
```

**Results:**
- ❌ **FAILED**: Frame buffer not rendered at tiny size
- Meet picker: Window appears but completely black
- Desktop: Not visible (too small)
- **Root cause**: Rendering engine skips buffer updates for 1×1 windows

---

### Strategy 2: Transparent Window (Low Alpha)
**Implementation:**
```rust
window.setOpaque_(NO);
window.setBackgroundColor_(clearColor);
window.setAlphaValue_(10.0 / 255.0); // Alpha = 10
```

**Results:**
- ❌ **FAILED**: CGWindowList captures as black screen
- Meet picker: Window visible but preview completely black
- Desktop: Nearly invisible (alpha ~4%)
- **Root cause**: `setOpaque_(NO)` + `clearColor` causes CGWindowList to capture black

---

### Strategy 3: Transparent + Opaque Background
**Implementation:**
```rust
window.setOpaque_(YES);
window.setBackgroundColor_(blackColor);
window.setAlphaValue_(10.0 / 255.0);
```

**Results:**
- ❌ **FAILED**: Still renders black in screen capture
- Meet picker: Window present but black preview
- Desktop: Barely visible
- **Root cause**: Alpha < 255 insufficient for proper CGWindowList capture

---

### Strategy 4: Window Level Below Desktop (Level -1)
**Implementation:**
```rust
window.setLevel_(-1); // Below NS_NORMAL_WINDOW_LEVEL
```

**Results:**
- ❌ **FAILED**: Excluded from CGWindowList entirely
- Meet picker: Window not present at all
- Desktop: Not visible
- **Root cause**: Level < 0 filtered out by screen capture APIs

---

### Strategy 5: Partially Off-Screen
**Implementation:**
```rust
let x_pos = -width + 1.0; // 1 pixel visible edge
let y_pos = 100.0;
```

**Results:**
- ❌ **FAILED**: Only visible region captured
- Meet picker: Window shows but content empty/incomplete
- Desktop: 1px edge visible on left border
- **Root cause**: CGWindowList only captures on-screen portion

---

### Strategy 6: orderBack() Behind All Windows
**Implementation:**
```rust
window.setLevel_(NS_NORMAL_WINDOW_LEVEL); // Level 0
let _: () = msg_send![window, orderBack: nil];
```

**Results:**
- ✅ **WORKS**: Content captured successfully
- ❌ **REJECTED**: Desktop visibility unacceptable
- Meet picker: Full preview with correct content
- Desktop: Window visible on desktop (behind others)
- **User feedback**: "masaüstünde görünüyor ekran, bunu kabul edemem"

---

### Strategy 7: Off-Screen Positioning (FINAL)
**Implementation:**
```rust
let (x_pos, y_pos) = if cfg!(debug_assertions) {
    (100.0, 100.0)  // Visible in debug mode
} else {
    (-10000.0, -10000.0)  // Off-screen in release
};
window.setLevel_(NS_NORMAL_WINDOW_LEVEL); // Level 0
window.setAlphaValue_(1.0); // Full opacity
window.setOpaque_(YES);
```

**Results:**
- ✅ **ACCEPTED**: User priority satisfied
- ❌ **TRADE-OFF**: Not in screen sharing pickers
- Meet picker: Window not visible (kCGWindowListOptionOnScreenOnly filter)
- Desktop: Completely invisible to user
- **Decision rationale**: User invisibility prioritized over screen sharing preview

---

## Technical Findings

### CGWindowList API Constraints
1. **On-Screen Requirement**: `kCGWindowListOptionOnScreenOnly` filters out off-screen windows
2. **Alpha Threshold**: Low alpha values (<255) render as black in capture
3. **Window Level Filter**: Level < 0 (desktop level) excluded from capture
4. **Partial Visibility**: Only on-screen portion captured, not full window

### macOS Window Behavior
1. **Opaque Flag Impact**: `setOpaque_(NO)` + transparent color → black capture
2. **Background Color Requirement**: Must use solid color (blackColor) for proper rendering
3. **Layer Opaque Setting**: CALayer must be opaque for content rendering
4. **Window Ordering**: `orderBack()` provides proper capture but desktop visibility

---

## Final Implementation

**File:** `src/destination_window/macos.rs` (Lines 120-205)

```rust
// Position: Off-screen in release, visible in debug
let (x_pos, y_pos) = if cfg!(debug_assertions) {
    (100.0, 100.0)
} else {
    (-10000.0, -10000.0)
};

// Properties: Full opacity, opaque window, black background
window.setOpaque_(YES);
let black_color: id = msg_send![class!(NSColor), blackColor];
window.setBackgroundColor_(black_color);
window.setAlphaValue_(1.0); // 255/255

// Level: Normal (not floating, not desktop)
window.setLevel_(NS_NORMAL_WINDOW_LEVEL); // 0

// Ordering: Front order (not back)
let _: () = msg_send![window, orderFront: nil];
```

---

## Conclusion

**Decision:** Off-screen positioning accepted as final solution despite screen sharing limitation.

**Rationale:**
- User experience priority: Window must never be visible on desktop
- Trade-off acceptable: Screen sharing preview feature less important than invisibility
- Alternative workflows: Users can share entire screen or specific application window
- Future enhancement: Settings toggle for "Show Preview Window" could switch positioning

**Known Limitations:**
- Preview window not visible in Google Meet/Zoom/Teams screen sharing pickers
- macOS CGWindowList API design constraint (kCGWindowListOptionOnScreenOnly)
- No workaround without desktop visibility

**Performance Impact:** None - off-screen rendering performs identically to on-screen

**Documentation:** Research findings documented in `OFFSCREEN_WINDOW_RESEARCH.md`
