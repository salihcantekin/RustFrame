# Developer Guide

Welcome to RustFrame development! This guide covers architecture, building, and contributing.

## 📚 Table of Contents

### Getting Started
- [Building from Source](building.md) - Compilation instructions
- [Development Setup](building.md#development-setup) - IDE configuration

### Architecture
- [Architecture Overview](#architecture-overview) - High-level design
- [Platform Abstractions](platform-specific.md) - Cross-platform code
- [Capture Engines](capture-engines.md) - Screen capture implementations
- [Rendering Pipeline](rendering-pipeline.md) - GPU/CPU rendering
- [Window Management](platform-specific.md#window-management) - Border and preview windows

### Technical Deep Dives
- [Performance Optimization](performance.md) - CPU/GPU tuning
- [Known Issues](known-issues.md) - Current limitations
- [Zero-Copy Strategy](../technical/zero-copy-strategy.md) - Memory optimization
- [Multi-Monitor Support](../technical/multi-monitor.md) - Display detection
- [Color Format Handling](../technical/color-formats.md) - BGRA vs RGBA

---

## Architecture Overview

RustFrame follows a modular, platform-abstracted design.

### High-Level Architecture

```
┌─────────────────────────────────────────────────────┐
│                    UI Layer (Tauri)                 │
│         React + TypeScript + TailwindCSS            │
└───────────────────┬─────────────────────────────────┘
                    │ IPC (Commands)
┌───────────────────┴─────────────────────────────────┐
│              Application Core (main.rs)             │
│  • Event Loop     • Settings Manager                │
│  • State Management • Window Coordinator            │
└───────┬───────────────────────────────┬─────────────┘
        │                               │
┌───────┴────────────┐         ┌────────┴─────────────┐
│  Capture Engine    │         │  Window Management   │
│  (trait-based)     │         │  (trait-based)       │
└───────┬────────────┘         └────────┬─────────────┘
        │                               │
┌───────┴─────────────────────┬─────────┴──────────────┐
│    Platform Modules         │  Platform Modules      │
│  • windows.rs               │  • hollow_border/      │
│  • macos.rs                 │  • destination_window/ │
│  • linux.rs                 │  • rec_indicator.rs    │
└─────────────────────────────┴────────────────────────┘
```

### Module Responsibilities

#### Core Modules

##### `src/main.rs`
- **Purpose**: Application entry point and orchestration
- **Responsibilities**:
  - Tauri app initialization
  - IPC command handlers
  - State management (Arc<Mutex<T>>)
  - Event loop coordination
  - Settings persistence
- **Key Types**:
  - `AppState`: Global application state
  - `Settings`: User configuration
  - Tauri command handlers

##### `src/lib.rs`
- **Purpose**: Library exports and re-exports
- **Responsibilities**:
  - Public API surface
  - Cross-module type definitions
  - Platform feature flags

#### Capture Module (`src/capture/`)

**Purpose**: Screen capture implementations

```
src/capture/
├── mod.rs         # Trait definitions, common types
├── windows.rs     # Windows Graphics Capture (WGC)
├── macos.rs       # ScreenCaptureKit + CoreGraphics
└── linux.rs       # X11 + PipeWire
```

**Key Trait**:
```rust
pub trait CaptureEngine: Send {
    fn start(&mut self, region: CaptureRect, show_cursor: bool) -> Result<()>;
    fn stop(&mut self);
    fn update_region(&mut self, region: CaptureRect) -> Result<()>;
    fn capture_frame(&mut self) -> Result<Option<CaptureFrame>>;
    fn get_region(&self) -> Option<CaptureRect>;
    fn as_any(&self) -> &dyn std::any::Any; // For downcasting
}
```

**Platform Implementations**:
- `WindowsCaptureEngine` - Windows Graphics Capture API
- `MacOSCaptureEngine` - ScreenCaptureKit or CoreGraphics
- `LinuxCaptureEngine` - PipeWire or X11

#### Window Management

##### Hollow Border (`src/hollow_border/`)
**Purpose**: Visual capture region indicator

```rust
pub trait BorderWindow: Send + Sync {
    fn new(...) -> Option<Self>;
    fn get_rect(&self) -> (i32, i32, i32, i32);
    fn update_rect(&self, x: i32, y: i32, width: i32, height: i32);
    fn update_style(&self, width: i32, color: u32);
    fn set_capture_mode(&mut self); // Interior click-through
    fn set_preview_mode(&mut self); // Interior draggable
    fn stop(&mut self);
}
```

**Platform Implementations**:
- `windows.rs` - Win32 layered window with color key transparency
- `macos.rs` - NSWindow with custom content view
- `linux.rs` - X11 shaped window or Wayland surface

##### Destination Window (`src/destination_window/`)
**Purpose**: Shareable preview output

**Rendering Strategies**:
- **Windows**: WinAPI GDI BitBlt (CPU) or D3D11 (GPU)
- **macOS**: CALayer + Metal (GPU)
- **Linux**: wgpu (GPU)

##### Recording Indicator (`src/rec_indicator.rs`)
**Purpose**: "REC" badge overlay

**Implementation**: Platform-specific top-level window

#### Platform Abstractions (`src/platform/`)

Common platform-specific utilities:
- `display_info.rs` - Monitor enumeration
- `input.rs` - Mouse/keyboard hooks
- `platform_info.rs` - OS detection and capabilities

### Data Flow

#### Capture Pipeline

```
1. User clicks "Start Capture"
   ↓
2. main.rs → start_capture() command
   ↓
3. Create HollowBorder (visual indicator)
   ↓
4. Create CaptureEngine (platform-specific)
   ↓
5. CaptureEngine.start(region)
   ↓
6. [Platform-specific capture begins]
   ↓
7. Render thread loop:
   while !stop_flag {
       frame = engine.capture_frame()
       destination_window.render(frame)
       sleep(1/target_fps)
   }
```

#### Frame Rendering

**Windows (WGC + GDI)**:
```
WGC API → D3D11 Texture → Staging Texture (GPU→CPU copy)
          ↓
       CPU Buffer (BGRA)
          ↓
   GDI BitBlt to Window DC
```

**macOS (SCK + Metal)**:
```
ScreenCaptureKit → CVPixelBuffer → IOSurface (GPU)
                                      ↓
                              CALayer.contents = IOSurface
                                      ↓
                             Metal Compositing (zero-copy!)
```

**Linux (X11 + wgpu)**:
```
XGetImage → CPU Buffer → wgpu Texture Upload
                              ↓
                        wgpu Render Pipeline
```

### Concurrency Model

#### Threading Strategy

1. **Main Thread** (Tauri event loop)
   - UI rendering (WebView)
   - IPC command handling
   - Window management (macOS requires main thread!)

2. **Capture Thread** (render loop)
   - Frame capture
   - Frame rendering
   - FPS regulation
   - Owned by render_thread_handle in AppState

3. **Platform Callbacks** (varies)
   - Windows: WGC frame callbacks (background thread)
   - macOS: SCK callbacks (dispatch queue)
   - Linux: X11 events (main thread)

#### Synchronization

```rust
// Shared state protected by Arc<Mutex<T>>
pub struct AppState {
    capture_engine: Arc<Mutex<Option<Box<dyn CaptureEngine>>>>,
    settings: Arc<Mutex<Settings>>,
    is_capturing: Arc<Mutex<bool>>,
    render_thread_stop: Arc<Mutex<bool>>,
}
```

**Critical Sections**:
- Settings access: Lock → read/write → unlock
- Capture engine: Lock for start/stop/update
- Window management: Platform-specific (avoid deadlocks)

#### macOS Main Thread Requirement

⚠️ **CRITICAL**: All Cocoa/AppKit APIs MUST run on main thread!

```rust
extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    fn dispatch_sync_f(...);
    fn pthread_main_np() -> i32; // Check if on main thread
}

// Always wrap Cocoa calls:
if unsafe { pthread_main_np() } == 0 {
    // Not on main thread - dispatch
    dispatch_sync_f(&_dispatch_main_q, context, callback);
} else {
    // Already on main thread - call directly
    callback();
}
```

**Why**: Calling NSWindow, NSView, etc. from background threads causes:
```
fatal runtime error: Rust cannot catch foreign exceptions
```

### Configuration Management

#### Settings Storage

**Location**:
- Windows: `%APPDATA%\RustFrame\settings.json`
- macOS: `~/Library/Application Support/RustFrame/settings.json`
- Linux: `~/.config/RustFrame/settings.json`

**Format**: JSON with serde serialization

**Profiles**: Optional override files in `profiles/` subdirectory

#### Settings Structure

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    // Mouse & Cursor
    pub show_cursor: bool,
    pub capture_clicks: bool,
    pub click_highlight_color: [u8; 4],
    pub click_dissolve_ms: u32,
    
    // Border
    pub show_border: bool,
    pub border_width: u32,
    
    // Performance
    pub target_fps: u32,
    pub gpu_acceleration: bool,
    pub capture_method: CaptureMethod,
    
    // Platform-specific overrides
    pub winapi_destination_alpha: Option<u8>,
    pub winapi_destination_topmost: Option<bool>,
    // ... more
}
```

### Error Handling

#### Result Types

```rust
use anyhow::{Result, Context};

fn capture_frame(&mut self) -> Result<CaptureFrame> {
    let frame = self.do_capture()
        .context("Failed to capture frame")?;
    Ok(frame)
}
```

#### Logging

```rust
use tracing::{info, warn, error, debug};

info!("Starting capture at ({}, {})", x, y);
warn!("GPU acceleration disabled, using CPU fallback");
error!("Failed to initialize D3D11 device: {}", e);
```

**Log Levels**:
- `error`: Critical failures
- `warn`: Degraded functionality
- `info`: Important state changes
- `debug`: Detailed diagnostics
- `trace`: Verbose internal state

### Testing Strategy

#### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_region_validation() {
        let region = CaptureRect::new(0, 0, 1920, 1080);
        assert!(is_valid_region(region));
    }
}
```

#### Integration Tests

Platform-specific tests in `src/{platform}/tests/`

#### Manual Testing Checklist

See [Testing Procedures](building.md#testing) for comprehensive checklist.

---

## Development Workflow

### 1. Local Development

```bash
# Clone repository
git clone https://github.com/salihcantekin/RustFrame
cd RustFrame

# Install dependencies (see building.md)

# Run in development mode
cargo tauri dev
```

### 2. Making Changes

```bash
# Create feature branch
git checkout -b feature/my-feature

# Make changes
# Test thoroughly on your platform

# Commit with descriptive message
git commit -m "feat: add multi-monitor support for Linux"
```

### 3. Cross-Platform Testing

**Ideal**: Test on all platforms before PR

**Minimum**: Test on your platform + document expected behavior

**CI/CD**: GitHub Actions tests on Windows/macOS/Linux

### 4. Submitting PR

1. Push to your fork
2. Create Pull Request
3. Fill out PR template
4. Link related issues
5. Wait for review

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for detailed guidelines.

---

## Code Style Guidelines

### Rust

```rust
// Use descriptive names
pub fn create_capture_engine(...) -> Result<Box<dyn CaptureEngine>> {
    // Implementation
}

// Document public APIs
/// Creates a new capture engine for the current platform.
///
/// # Arguments
/// * `region` - The screen region to capture
/// * `show_cursor` - Whether to include cursor in capture
///
/// # Returns
/// Platform-specific capture engine or error
///
/// # Errors
/// Returns error if platform doesn't support capture
pub fn create_engine(...) -> Result<...> { }

// Use platform-specific code with cfg
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::*;

#[cfg(target_os = "macos")]
use cocoa::appkit::*;
```

### TypeScript/React

```tsx
// Use functional components with TypeScript
interface SettingsDialogProps {
  settings: Settings;
  onSave: (settings: Settings) => void;
  onClose: () => void;
}

export function SettingsDialog({ 
  settings, 
  onSave, 
  onClose 
}: SettingsDialogProps) {
  // Use hooks
  const [localSettings, setLocalSettings] = useState(settings);
  
  // Type-safe Tauri invoke
  const handleSave = async () => {
    await invoke<void>("save_settings", { settings: localSettings });
    onSave(localSettings);
  };
  
  return (/* JSX */);
}
```

---

## Documentation Standards

### Code Comments

```rust
// Use inline comments for non-obvious logic
let border_offset = border_width as i32; // Exclude border from capture

// Use doc comments for public APIs (see above)

// Explain platform-specific workarounds
#[cfg(target_os = "macos")]
{
    // CRITICAL: All Cocoa APIs MUST run on main thread
    // to avoid NSException crashes
    dispatch_to_main_thread(|| {
        // Cocoa calls here
    });
}
```

### Documentation Files

- **User-facing**: Markdown in `docs/user-guide/`
- **Technical**: Markdown in `docs/developer/`
- **API docs**: Rust doc comments (`cargo doc`)

---

## Debugging Tips

### Enable Verbose Logging

```rust
// In main.rs, increase log level
env::set_var("RUST_LOG", "debug"); // or "trace"
```

### Platform-Specific Debugging

**Windows**:
```powershell
# Run with console output
cargo run 2>&1 | Tee-Object -FilePath debug.log
```

**macOS**:
```bash
# Check system logs
log stream --predicate 'process == "RustFrame"' --level debug

# Console.app for GUI log viewing
```

**Linux**:
```bash
# Run with X11 sync (catch X errors immediately)
RUST_LOG=debug ./rustframe 2>&1 | tee debug.log
```

---

## Next Steps

- **Build the Project** → [Building Guide](building.md)
- **Understand Capture** → [Capture Engines](capture-engines.md)
- **Optimize Performance** → [Performance Guide](performance.md)
- **Fix Issues** → [Known Issues](known-issues.md)

---

**Contributing**: See [CONTRIBUTING.md](../../CONTRIBUTING.md)  
**Code of Conduct**: See [CODE_OF_CONDUCT.md](../../CODE_OF_CONDUCT.md)  
**License**: [MIT License](../../LICENSE)
