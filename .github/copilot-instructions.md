# RustFrame - GitHub Copilot Instructions

> **Note**: These instructions apply to both GitHub Copilot and AI agents. All contributors (human and AI) must follow these guidelines to maintain code quality and consistency.

## Project Overview

**RustFrame** is a high-performance, cross-platform screen capture application built with Rust + Tauri 2 and React. The primary feature is creating a **Preview Window** that displays only a user-selected region of the screen, enabling users with wide-screen monitors to share specific areas in video conferencing apps (Discord, Google Meets, Zoom, Teams) instead of their entire screen.

### Core Purpose
- Capture and display a user-selected screen region in a preview window
- Enable selective screen sharing in video conferencing applications
- Support multiple capture methods optimized for each platform
- Maintain high performance with minimal memory footprint

### Non-Negotiable Core Features (Do Not Break)
- **Hollow Border Window**: A dedicated border window for region selection/moving/resizing.
- **Click-Through Capture Mode**: In capture mode, the border’s interior must remain click-through; only the border edges/corners should be interactive.
- **Preview Window**: A destination/preview window that displays the captured region and can be selected in screen sharing pickers (Meet/Zoom/Discord/Teams).
- **Settings as Source of Truth**: Settings must be persisted in the user config directory and loaded reliably on next startup; changes must not silently reset to defaults.
- **Last Region Memory**: When enabled, the last capture region must be saved and restored on startup.

If you make changes in this repo, you MUST verify that these core features still work on the affected platform(s). If preserving these invariants conflicts with a change, prefer the invariant and adjust the change.

---

## Cross-Platform Architecture

### Platform Structure
```
src/
├── platform/          # Platform-specific abstractions
│   ├── mod.rs        # Common traits and interfaces
│   ├── windows.rs    # Windows-specific implementations
│   ├── macos.rs      # macOS-specific implementations
│   └── linux.rs      # Linux-specific implementations
├── capture/          # Capture engine implementations
│   ├── windows/      # Windows Graphics Capture, GDI
│   ├── macos/        # ScreenCaptureKit, CGDisplayStream
│   └── linux/        # X11, Wayland
└── main.rs           # Platform-agnostic entry point
```

### Critical Rules

1. **Platform-Specific Code Must Be Isolated**
   - Windows-only code: Use `#[cfg(target_os = "windows")]`
   - macOS-only code: Use `#[cfg(target_os = "macos")]`
   - Linux-only code: Use `#[cfg(target_os = "linux")]`
   - Never mix platform-specific APIs in shared code

2. **Conditional Compilation**
   ```rust
   #[cfg(target_os = "windows")]
   use windows::Win32::Graphics::Gdi::*;
   
   #[cfg(target_os = "macos")]
   use core_graphics::display::*;
   ```

3. **Shared Abstractions**
   - When functionality is common across platforms, create traits in `platform/mod.rs`
   - Implement traits separately for each platform
   - Use enums or trait objects for runtime polymorphism

4. **Cargo Dependencies**
   ```toml
   [target.'cfg(windows)'.dependencies]
   windows = { version = "0.58", features = [...] }
   
   [target.'cfg(target_os = "macos")'.dependencies]
   cocoa = "0.26"
   core-graphics = "0.24"
   ```

5. **⚠️ CRITICAL: macOS Main Thread Requirement**
   - **ALL Cocoa/AppKit APIs MUST run on the main thread**
   - Calling NSWindow, NSView, NSColor, etc. from background threads causes NSException
   - Rust cannot catch NSException → immediate crash with "foreign exception" error
   - **Solution**: Use `dispatch_sync_f` with `_dispatch_main_q` to dispatch to main thread
   ```rust
   extern "C" {
       static _dispatch_main_q: std::ffi::c_void;
       fn dispatch_sync_f(
           queue: *const std::ffi::c_void,
           context: *mut std::ffi::c_void,
           work: extern "C" fn(*mut std::ffi::c_void),
       );
       fn pthread_main_np() -> i32; // Returns non-zero if on main thread
   }
   
   // Check thread and dispatch if needed
   let is_main = unsafe { pthread_main_np() } != 0;
   if !is_main {
       unsafe {
           dispatch_sync_f(&_dispatch_main_q, context_ptr, callback);
       }
   }
   ```
   - This applies to:
     - Window creation (HollowBorder, DestinationWindow)
     - Screen capture APIs (CGWindowListCreateImage, CGDisplayStream)
     - Any UI manipulation
   - Common error: `fatal runtime error: Rust cannot catch foreign exceptions, aborting`

---

## Rust Coding Standards

### Performance & Memory Optimization

1. **Zero-Copy When Possible**
   - Use references (`&T`) instead of cloning
   - Prefer `&str` over `String` for read-only text
   - Use `Cow<'_, T>` when conditional ownership is needed

2. **Memory Management**
   - Minimize heap allocations in hot paths
   - Use `Vec::with_capacity()` when size is known
   - Prefer stack allocation for small, fixed-size data
   - Use `Box<T>` for large structs to avoid stack overflow

3. **Concurrency**
   - Use `tokio` for async I/O operations
   - Prefer message passing (`mpsc`, `broadcast`) over shared state
   - Use `Arc<RwLock<T>>` sparingly, prefer `Arc<Mutex<T>>` for simplicity
   - Never block async tasks with synchronous operations

4. **Error Handling**
   ```rust
   // Use Result<T, E> for fallible operations
   fn capture_frame(&self) -> Result<Frame, CaptureError> {
       // Implementation
   }
   
   // Use anyhow for application errors
   use anyhow::{Context, Result};
   
   fn load_settings() -> Result<Settings> {
       let data = fs::read_to_string(path)
           .context("Failed to read settings file")?;
       Ok(serde_json::from_str(&data)?)
   }
   ```

5. **Lifetime Management**
   - Avoid `'static` unless truly necessary
   - Use explicit lifetimes for clarity in public APIs
   - Prefer owned types in struct fields to simplify lifetimes

### Code Style

1. **Naming Conventions**
   - Types: `PascalCase` (e.g., `CaptureEngine`, `WindowManager`)
   - Functions/methods: `snake_case` (e.g., `start_capture`, `get_settings`)
   - Constants: `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_FPS`, `MAX_RETRIES`)
   - Modules: `snake_case` (e.g., `capture_engine`, `window_manager`)

2. **Documentation**
   ```rust
   /// Captures a frame from the specified region.
   ///
   /// # Arguments
   /// * `region` - The screen region to capture (x, y, width, height)
   ///
   /// # Returns
   /// A `Result` containing the captured frame or an error
   ///
   /// # Errors
   /// Returns `CaptureError::DeviceLost` if the capture device is unavailable
   pub fn capture_region(&self, region: (i32, i32, u32, u32)) -> Result<Frame, CaptureError> {
       // Implementation
   }
   ```

3. **Modern Rust Features**
   - Use `const fn` for compile-time evaluation
   - Leverage `impl Trait` for return types
   - Use pattern matching extensively
   - Prefer `if let` and `while let` for simple matches

---

## React/TypeScript Standards

### Component Structure

1. **Functional Components with Hooks**
   ```tsx
   import { useState, useEffect, useCallback } from 'react';
   
   interface CaptureControlProps {
       onStart: () => void;
       onStop: () => void;
       isCapturing: boolean;
   }
   
   export function CaptureControl({ onStart, onStop, isCapturing }: CaptureControlProps) {
       // Implementation
   }
   ```

2. **State Management**
   - Use `useState` for local component state
   - Use `useReducer` for complex state logic
   - Keep state close to where it's used
   - Lift state only when necessary

3. **Performance Optimization**
   ```tsx
   // Memoize expensive computations
   const filteredData = useMemo(() => {
       return data.filter(item => item.active);
   }, [data]);
   
   // Memoize callbacks to prevent re-renders
   const handleClick = useCallback(() => {
       invoke('start_capture', { region });
   }, [region]);
   
   // Memoize components that receive callbacks
   const MemoizedChild = memo(ChildComponent);
   ```

4. **Tauri Integration**
   ```tsx
   import { invoke } from '@tauri-apps/api/core';
   import { getCurrentWindow } from '@tauri-apps/api/window';
   
   // Type-safe Tauri commands
   interface Settings {
       capture_method: string;
       target_fps: number;
       show_cursor: boolean;
   }
   
   const settings = await invoke<Settings>('get_settings');
   ```

5. **TypeScript Best Practices**
   - Enable strict mode in `tsconfig.json`
   - Avoid `any`, use `unknown` if type is truly unknown
   - Use discriminated unions for state machines
   - Leverage utility types: `Partial<T>`, `Pick<T>`, `Omit<T>`

---

## UI/UX Standards

### TailwindCSS Usage

1. **Consistent Spacing**
   - Use Tailwind spacing scale: `p-2`, `px-4`, `gap-6`
   - Maintain 4px increments for consistency

2. **Color Palette**
   - Background: `bg-gray-800`, `bg-gray-900`
   - Borders: `border-gray-700`, `border-gray-600`
   - Text: `text-white`, `text-gray-300`, `text-gray-400`
   - Accents: `text-blue-400`, `text-green-400`, `text-red-500`

3. **Responsive Design**
   ```tsx
   <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
       {/* Content */}
   </div>
   ```

4. **Custom Titlebar**
   - Use `data-tauri-drag-region` for draggable areas
   - Implement window controls (minimize, maximize, close)
   - Handle double-click to maximize: `e.detail === 2`

---

## Capture Engine Guidelines

### Windows
- **Primary**: Windows Graphics Capture API (WGC) - hardware accelerated
- **Fallback**: BitBlt/GDI - compatibility mode
- Use Direct3D 11 for texture handling
- Handle monitor DPI scaling correctly

### macOS
- **Primary**: ScreenCaptureKit (macOS 12.3+) - modern, efficient
- **Fallback**: CGDisplayStream - legacy support
- Request screen recording permission on first run
- Handle Retina displays with proper scaling

### Linux
- **Primary**: PipeWire (modern) - Wayland support
- **Fallback**: X11 - traditional X server
- Detect compositor (Wayland/X11) at runtime
- Handle multiple displays correctly

---

## Common Patterns

### 1. Settings Management
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub capture_method: String,
    pub target_fps: u32,
    pub show_cursor: bool,
    pub show_border: bool,
    pub border_width: u32,
    pub remember_last_region: bool,
}
```

### 2. Region Selection
- Use hollow border window to show capture region
- Store region as `(x, y, width, height)` tuple
- Persist last region if `remember_last_region` is enabled

### 3. Preview Window
- Use `wgpu` for cross-platform GPU rendering
- Implement custom shader for efficient texture display
- Support window resizing while maintaining aspect ratio

---

## Testing Requirements

1. **Unit Tests**
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;
       
       #[test]
       fn test_region_validation() {
           let region = (0, 0, 1920, 1080);
           assert!(is_valid_region(region));
       }
   }
   ```

2. **Integration Tests**
   - Test platform-specific implementations separately
   - Mock system APIs when possible
   - Use CI/CD for cross-platform testing

3. **Performance Benchmarks**
   ```rust
   #[bench]
   fn bench_frame_capture(b: &mut Bencher) {
       let engine = CaptureEngine::new().unwrap();
       b.iter(|| engine.capture_frame());
   }
   ```

---

## Build & Deployment

### Cargo.toml Structure
```toml
[package]
name = "rustframe"
version = "1.1.0"
edition = "2021"

[dependencies]
# Cross-platform
tauri = { version = "2.9", features = ["..." ] }
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }

# Platform-specific (see earlier example)

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

### Build Commands
```bash
# Development
cargo tauri dev

# Release build for current platform
cargo tauri build

# Cross-platform builds (use GitHub Actions)
```

---

## Key Performance Metrics

- **Startup Time**: < 500ms
- **Memory Usage**: < 100MB idle, < 200MB capturing
- **CPU Usage**: < 5% idle, < 15% capturing at 60 FPS
- **Frame Latency**: < 16ms for 60 FPS capture

---

## Important Files to Consider

### Configuration Files
- `tauri.conf.json` - Tauri configuration
- `capabilities/default.json` - Security permissions
- `ui/src/config.ts` - UI configuration constants

### Platform Modules
- `src/platform/` - Cross-platform abstractions
- `src/capture/` - Capture engine implementations
- `src/hollow_border/` - Region selection UI
- `src/destination_window/` - Preview window

### UI Components
- `ui/src/App.tsx` - Main application UI
- `ui/src/components/SettingsDialog.tsx` - Settings modal

---

## Development Workflow

### User Approval Requirements

**CRITICAL**: Always request user approval before proceeding in these scenarios:

1. **Major Platform API Changes**
   - Using low-level OS APIs (Win32, Cocoa, X11, etc.)
   - Modifying capture engine implementations
   - Changing window management or rendering logic
   - Adding new platform-specific dependencies

2. **Large-Scale Refactoring**
   - Restructuring more than 5 files
   - Changing core architectural patterns
   - Modifying build configuration or dependencies
   - Altering cross-platform abstraction layers

3. **Breaking Changes**
   - Changes that affect settings file format
   - Modifications to public API signatures
   - Updates that require data migration
   - Changes that impact backward compatibility

4. **Small Feature with Many Changes**
   - If a seemingly simple feature requires modifications to 10+ files
   - When adding a feature impacts multiple platform implementations
   - If changes affect both backend and frontend extensively

**Approval Process**:
```
1. Explain what needs to be changed and why
2. List all files that will be modified
3. Highlight potential risks or breaking changes
4. Wait for explicit user confirmation before proceeding
5. If user declines, propose alternative approaches
```

**Example Approval Request**:
```
I need to implement [feature]. This requires:
- Modifying Windows capture API in src/capture/windows/wgc.rs
- Adding new dependency: windows-capture v0.2.0
- Updating 3 platform-specific files

Risks: May affect capture performance on older Windows versions.
Shall I proceed?
```

---

## Anti-Patterns to Avoid

1. ❌ Don't use platform-specific APIs in shared code
2. ❌ Don't block async operations with `std::thread::sleep`
3. ❌ Don't use `unwrap()` in production code paths
4. ❌ Don't clone large data structures unnecessarily
5. ❌ Don't ignore compiler warnings
6. ❌ Don't use `Arc<Mutex<T>>` when `&T` suffices
7. ❌ Don't create new threads without using thread pools
8. ❌ Don't use global mutable state

---

## Code Review Checklist

- [ ] Platform-specific code properly isolated with `#[cfg(...)]`
- [ ] Error handling uses `Result<T, E>` appropriately
- [ ] No memory leaks or resource leaks
- [ ] Performance-critical paths optimized
- [ ] Code documented with examples
- [ ] Tests added for new functionality
- [ ] UI responsive and accessible
- [ ] Cross-platform compatibility verified

---

## Resources

- [Tauri Documentation](https://tauri.app/v2/guide/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [React Documentation](https://react.dev/)
- [wgpu Documentation](https://wgpu.rs/)

---

**Remember**: Every code change should maintain cross-platform compatibility, follow performance best practices, and use modern, idiomatic patterns for both Rust and React.
