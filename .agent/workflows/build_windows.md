---
description: Build and Run on Windows
---

# System Requirements

*   **OS Version**: Windows 10 Version 1803 (Build 17134) or later.
    *   *Reason*: Required for **Windows.Graphics.Capture** API.
*   **Build Tools**: Visual Studio Build Tools with "Desktop development with C++" workload.
*   **SDK**: Windows 10 SDK (10.0.17134.0) or newer (installed via VS Build Tools).
*   **Runtime**: [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (usually pre-installed on Win 11).
*   **Graphics**: GPU with DirectX 11.1+ support (for `wgpu` and `Windows.Graphics.Capture`).

# Build Steps

1. **Prerequisites Check**
   Ensure you have:
   - "Desktop Development with C++" workload in Visual Studio Build Tools.
   - Windows 10/11 SDK.

2. **Check for Tauri Changes**
   If you have modified frontend files or configuration:
   
   ```bash
   // turbo
   cargo tauri build
   ```

3. **Rust-Only Changes**
   For faster iteration on backend logic:
   
   ```bash
   // turbo
   cargo build
   ```

4. **Verify**
   ```bash
   // turbo
   cargo run
   ```
