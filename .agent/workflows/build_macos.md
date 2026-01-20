---
description: Build and Run on macOS
---

# System Requirements

*   **macOS Version**: macOS 12.3 (Monterey) or later.
    *   *Reason*: Required for **ScreenCaptureKit** framework support.
*   **Permissions**: Screen Recording permission must be granted to the terminal/IDE or the built application.
*   **Xcode**: Xcode Command Line Tools (`xcode-select --install`).
*   **Graphics**: Metal-compatible GPU (required for `wgpu` rendering).

# Build Steps

1. **Check for Tauri Changes**
   If you have modified frontend files (`ui/`) or `tauri.conf.json`:
   
   ```bash
   // turbo
   cargo tauri build
   ```
   
   *Tip: Use `cargo tauri dev` for hot-reloading during development.*

2. **Rust-Only Changes**
   If you strictly modified only Rust files (`src/**/*.rs`) and want a faster build:
   
   ```bash
   // turbo
   cargo build
   ```

3. **Verify**
   Run the application to verify changes:
   ```bash
   // turbo
   cargo run
   ```
