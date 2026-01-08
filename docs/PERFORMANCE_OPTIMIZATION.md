# RustFrame - CPU Performance Optimization Strategies

## Current Status (January 2026)

### Measured Performance (Release Build)
- **Normal Capture (GPU-only):** 7-8% CPU ✅
- **Click Highlight Active:** 30-35% CPU ⚠️
- **Border Movement:** 8-9% CPU ✅
- **Target for Click Highlight:** 10-15% CPU

### Root Cause Analysis

Click highlight causes high CPU usage because:
1. **BGRA→RGBA conversion happens every frame (60 FPS)** even in GPU-only mode
2. IOSurface (3600×2338 pixels) → Cropped RGBA buffer (1402×660) = ~3.7MB/frame
3. At 60 FPS: 222 MB/s memory bandwidth
4. Manual pixel processing even with optimized bulk copy

---

## Optimization Strategy Roadmap

### Phase 1: Conditional RGBA Conversion ⭐ (HIGHEST IMPACT)

**Goal:** Only convert BGRA→RGBA when CPU path is needed (clicks active)

**Current Problem:**
```rust
// In macos_sck.rs, SCK callback:
// This runs EVERY FRAME regardless of whether CPU path is needed!
let mut rgba = vec![0u8; (out_w * out_h * 4) as usize];
// ... conversion ...
state.set(rgba, out_w, out_h);
```

**Solution:**
1. Keep IOSurface pointer + crop info always
2. Only convert BGRA→RGBA on-demand when `has_clicks == true`
3. Use lazy evaluation pattern

**Implementation:**
```rust
pub struct OutputState {
    // Current: Always stores RGBA
    latest: Mutex<Option<(Vec<u8>, u32, u32)>>,
    
    // Proposed: Store IOSurface + conversion function
    latest_iosurface: Mutex<Option<IOSurfaceInfo>>,
    cached_rgba: Mutex<Option<(Vec<u8>, u32, u32, u64)>>, // Lazily populated
    needs_cpu_data: AtomicBool, // Signal from main thread
}

impl OutputState {
    pub fn get(&self) -> Option<(Vec<u8>, u32, u32, u64)> {
        // Check if we have cached RGBA for current frame
        if let Some(cached) = self.cached_rgba.lock().unwrap().as_ref() {
            return Some(cached.clone());
        }
        
        // Only convert if needed
        if !self.needs_cpu_data.load(Ordering::Relaxed) {
            return None; // GPU-only mode, no conversion
        }
        
        // Convert on-demand
        let rgba = self.convert_iosurface_to_rgba()?;
        *self.cached_rgba.lock().unwrap() = Some(rgba.clone());
        Some(rgba)
    }
}
```

**Expected Impact:** 
- **GPU-only: 7-8% → 3-5%** (no RGBA conversion)
- **Click highlight: 30-35% → 12-18%** (conversion only when needed)

---

### Phase 2: GPU-Based Click Rendering 🚀 (MEDIUM IMPACT, HIGH COMPLEXITY)

**Goal:** Draw click highlights on GPU using Metal shaders

**Approach:**
1. Create Metal shader for circle rendering
2. Composite click overlay on top of IOSurface
3. No CPU-side pixel manipulation

**Implementation Steps:**
```swift
// Metal shader (click_highlight.metal)
fragment float4 clickHighlightFragment(
    VertexOut in [[stage_in]],
    texture2d<float> baseTexture [[texture(0)]],
    constant ClickData *clicks [[buffer(0)]]
) {
    float4 baseColor = baseTexture.sample(sampler, in.texCoord);
    
    // Draw circles for each click
    for (int i = 0; i < numClicks; i++) {
        float2 clickPos = clicks[i].position;
        float distance = length(in.position - clickPos);
        if (distance < clicks[i].radius) {
            float alpha = clicks[i].alpha * (1.0 - distance / clicks[i].radius);
            baseColor = mix(baseColor, clicks[i].color, alpha);
        }
    }
    
    return baseColor;
}
```

**Rust Integration:**
```rust
pub struct MetalClickRenderer {
    device: metal::Device,
    pipeline: metal::RenderPipelineState,
    command_queue: metal::CommandQueue,
}

impl MetalClickRenderer {
    pub fn render_with_clicks(
        &self,
        iosurface: IOSurfaceRef,
        clicks: &[Click],
    ) -> IOSurfaceRef {
        // Create Metal texture from IOSurface
        // Apply click shader
        // Return composited IOSurface
    }
}
```

**Expected Impact:**
- **Click highlight: 30-35% → 5-8%** (all GPU)
- **Complexity:** High (Metal API integration)

---

### Phase 3: Optimized Memory Layout 📦 (LOW IMPACT)

**Goal:** Reduce memory allocations and copies

**Strategies:**

#### 3.1. Pre-allocated RGBA Buffer
```rust
pub struct OutputState {
    rgba_buffer: Mutex<Vec<u8>>,  // Pre-allocated, reused
    buffer_capacity: AtomicUsize,
}

impl OutputState {
    fn set(&self, new_data: &[u8], w: u32, h: u32) {
        let mut buffer = self.rgba_buffer.lock().unwrap();
        let required = (w * h * 4) as usize;
        
        // Reuse buffer if large enough
        if buffer.capacity() < required {
            buffer.reserve(required - buffer.len());
        }
        
        buffer.clear();
        buffer.extend_from_slice(new_data);
    }
}
```

#### 3.2. SIMD-Optimized Color Conversion
```rust
#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

unsafe fn bgra_to_rgba_simd(src: &[u8], dst: &mut [u8]) {
    // Process 16 pixels (64 bytes) at once with NEON
    for chunk in src.chunks_exact(64).zip(dst.chunks_exact_mut(64)) {
        let (src_chunk, dst_chunk) = chunk;
        
        // Load 64 bytes (16 pixels)
        let bgra = vld4q_u8(src_chunk.as_ptr());
        
        // Reorder: BGRA → RGBA
        let rgba = uint8x16x4_t {
            0: bgra.2, // R
            1: bgra.1, // G
            2: bgra.0, // B
            3: bgra.3, // A
        };
        
        // Store
        vst4q_u8(dst_chunk.as_mut_ptr(), rgba);
    }
}
```

**Expected Impact:**
- **Additional 3-5% reduction** in color conversion overhead
- **Complexity:** Medium (SIMD intrinsics)

---

### Phase 4: Separate Click Overlay Window 🪟 (ALTERNATIVE APPROACH)

**Goal:** Keep main capture window pure GPU, overlay clicks in transparent window

**Architecture:**
```
┌─────────────────────────┐
│  Main Preview Window    │  ← Pure GPU (IOSurface)
│  (GPU-only rendering)   │     No CPU path needed
└─────────────────────────┘
           ▲
           │ Same position/size
           │
┌─────────────────────────┐
│ Click Overlay Window    │  ← Transparent, click-through
│ (CPU rendering only)    │     Only active during clicks
└─────────────────────────┘
```

**Benefits:**
- Main window stays 100% GPU
- Click overlay only renders when needed
- No switching between GPU/CPU paths

**Implementation:**
```rust
pub struct ClickOverlayWindow {
    window: NSWindow,  // Transparent, always on top
    active: AtomicBool,
}

impl ClickOverlayWindow {
    pub fn show_clicks(&self, clicks: &[Click]) {
        if clicks.is_empty() {
            self.window.orderOut_(nil);
            return;
        }
        
        // Draw clicks on transparent window
        self.window.orderFront_(nil);
        self.render_clicks(clicks);
    }
}
```

**Expected Impact:**
- **Main window: Always 7-8%** (no CPU spikes)
- **Click overlay: 5-10%** (separate budget)
- **Total during clicks: 12-18%**
- **Complexity:** Medium

---

## Implementation Priority

### Immediate (Phase 1): Conditional RGBA Conversion
- **Effort:** 2-3 hours
- **Impact:** 30% → 15% (50% reduction)
- **Risk:** Low (isolated change)

### Near-term (Phase 4): Separate Overlay Window
- **Effort:** 4-6 hours
- **Impact:** Consistent 7-8% main, 5-10% overlay
- **Risk:** Low (separate component)

### Long-term (Phase 2): GPU Click Rendering
- **Effort:** 1-2 days
- **Impact:** 30% → 5-8% (80% reduction)
- **Risk:** Medium (Metal API learning curve)

### Optional (Phase 3): Memory Optimizations
- **Effort:** 3-4 hours
- **Impact:** Additional 3-5% reduction
- **Risk:** Low
- **Note:** Apply after Phase 1/2 for compounding benefits

---

## Benchmarking Methodology

### CPU Measurement
```bash
# macOS Activity Monitor
# Process: rustframe
# Column: % CPU (single-core normalized)
# Scenario: 885×800 capture region, 60 FPS
```

### Key Metrics
1. **Idle:** < 1%
2. **GPU-only capture:** 3-8%
3. **Click highlight:** < 15% (target)
4. **Border movement:** < 10%

### Test Cases
- [ ] No clicks, static window → Should be 3-8%
- [ ] 1 click, 300ms dissolve → Peak should be < 20%
- [ ] 5 clicks rapid succession → Should not exceed 25%
- [ ] Border drag during clicks → Should be < 30% combined

---

## Code References

### Files to Modify

**Phase 1 (Conditional Conversion):**
- `src/capture/macos_sck.rs`: Lines 350-400 (SCK callback)
- `src/capture/macos_sck.rs`: Lines 170-210 (OutputState::get)
- `src/main.rs`: Lines 1585-1610 (GPU/CPU path decision)

**Phase 2 (GPU Click Rendering):**
- Create `src/render/metal_click.rs`
- Modify `src/destination_window/macos.rs`
- Add Metal shader files in `shaders/`

**Phase 4 (Overlay Window):**
- Create `src/click_overlay/macos.rs`
- Modify `src/main.rs` to manage overlay lifecycle

---

## Performance Profiling Commands

```bash
# Instruments (macOS)
instruments -t "Time Profiler" target/release/rustframe

# Sample during click highlight
sample rustframe 5 -f profile.txt

# Memory allocations
leaks --atExit -- target/release/rustframe

# CPU usage monitoring
top -pid $(pgrep rustframe) -stats cpu,mem
```

---

## Notes

- Current implementation prioritizes code simplicity over performance
- RGBA conversion happens in SCK callback (60 FPS) regardless of need
- Click dissolve time: 300ms (18 frames at 60 FPS)
- IOSurface size: 3600×2338 pixels (full display)
- Crop region: ~1400×660 pixels (capture area)
- Memory bandwidth: 222 MB/s during CPU path

**Last Updated:** January 8, 2026
