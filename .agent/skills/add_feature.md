---
description: Guide for adding new features to RustFrame
---

# Adding a New Feature

Follow this checklist to add a new feature while maintaining project standards.

## 1. Plan & Isolation
- [ ] Determine if the feature is **Platform Specific** or **Shared**.
    - If **Platform Specific**:
        - Use `#[cfg(target_os = "macos")]` or `#[cfg(target_os = "windows")]`.
        - Implement standard trait (e.g., in `src/traits.rs`) if applicable.
    - If **Shared**:
        - Implement in `src/main.rs` or a shared module.

## 2. Implementation
- [ ] Create/Modify files.
- [ ] **Crucial**: If adding a new module, register it in `main.rs`.
- [ ] **Crucial**: Ensure `DirectoryWindow` or rendering logic handles the new feature if it's visual.

## 3. Configuration
- [ ] Does this need a user setting?
    - Add field to `Settings` struct in `main.rs`.
    - Update `Default` impl.
    - (Optional) Update Settings UI if applicable.

## 4. Verification
- [ ] Run `cargo check`.
- [ ] Run `cargo clippy`.
- [ ] Verify formatting with `cargo fmt`.
