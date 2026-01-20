---
description: RustFrame CODING GUIDELINES & RULES
---

# User Rules

These rules are derived from `CONTRIBUTING.md` and project analysis. They must be followed for all code changes.

## 1. Coding Standards (Rust)

*   **Style**: Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
*   **Formatting**: ALWAYS run `cargo fmt` on modified files.
*   **Linting**: Code must pass `cargo clippy` without warnings.
*   **Documentation**:
    *   Public functions MUST have doc comments (`///`).
    *   Complex logic should have inline comments explaining *why*, not just *what*.

## 2. Safety Guidelines (Critical)

This project uses `unsafe` code for Windows/macOS APIs.

*   **Unsafe Blocks**: MUST be documented with a `// SAFETY:` comment explaining why it is safe.
*   **COM/Native Errors**: Handle platform errors properly (return `Result`, don't panic).
*   **Resource Management**: Ensure resources (COM pointers, CFTypes) are cleaned up in `Drop` implementations.

## 3. Commit Messages

Follow **Conventional Commits**:
`type(scope): description`

*   **Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`.
*   **Example**: `feat(macos): add preview window capture setting`

## 4. Project Structure

*   `src/main.rs`: Entry point & event loop.
*   `src/platform.rs`: Cross-platform abstractions.
*   `src/destination_window/`: Platform-specific window implementations (`macos.rs`, `windows.rs`).
*   `src/hollow_border/`: Border window implementations.

## 5. Multi-Platform Development

*   Use `#[cfg(target_os = "macos")]` and `#[cfg(target_os = "windows")]` to gate platform-specific code.
*   Do not leave Windows-only code (like `SEPARATION_LAYER`) exposed to macOS builds.

## 6. Platform Capabilities & Constraints

### Windows
*   **API**: `Windows.Graphics.Capture` (WinRT).
*   **Min Version**: Windows 10 1803 (Build 17134).
*   **DirectX**: Requires D3D11 device.

### macOS
*   **API**: `ScreenCaptureKit` (primary) / `CGDisplayStream` (legacy fallback).
*   **Min Version**: macOS 12.3+ (for SCK).
*   **Permissions**: Requires "Screen Recording" permission.
*   **Threading**: UI updates must happen on Main Thread.

