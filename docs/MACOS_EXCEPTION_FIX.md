# macOS "Rust Cannot Catch Foreign Exceptions" Fix

## Problem Description

When compiling RustFrame on macOS, the project was failing with the error:
```
Rust cannot catch foreign exceptions
```

This error occurred because the codebase was using Objective-C APIs (through `cocoa` and `objc` crates) without properly declaring these dependencies.

## Root Cause

The macOS-specific implementation files were importing and using Cocoa/Objective-C APIs:

- **`src/destination_window/macos.rs`**: Uses `NSWindow`, `NSColor`, `NSBackingStoreType` from `cocoa::appkit`
- **`src/hollow_border/macos.rs`**: Uses `NSWindow`, `NSView`, custom Objective-C class declarations
- **Both files**: Use `objc::msg_send!` macro for Objective-C method calls

However, `Cargo.toml` only declared `core-graphics = "0.24"` for macOS, missing:
- `cocoa` - Bindings to Cocoa framework for macOS
- `objc` - Objective-C runtime bindings with exception handling

### Why "Rust Cannot Catch Foreign Exceptions"?

When Objective-C code throws an `NSException`, Rust's panic handling mechanism cannot catch it by default. 

**Important Note**: Most modern Cocoa APIs use `NSError` for expected error conditions. However, `NSException` is still thrown for:
- Programming errors (invalid parameters, contract violations)
- Some legacy APIs
- Runtime errors in graphics/window operations
- Invalid state access

The `objc` crate provides an `exception` feature that:

1. Intercepts Objective-C exceptions at the FFI boundary
2. Converts them to Rust panics
3. Allows proper unwinding and cleanup

Without this feature enabled, any `NSException` thrown by Cocoa APIs would:
- Bypass Rust's panic handling
- Potentially cause undefined behavior
- Fail to properly clean up resources
- Trigger the "cannot catch foreign exceptions" error during compilation

This is a compile-time safety check - Rust refuses to compile code that might call exception-throwing foreign code without proper handling infrastructure.

## Solution

Added the missing dependencies to `Cargo.toml`:

```toml
# macOS-specific dependencies
[target.'cfg(target_os = "macos")'.dependencies]
core-graphics = "0.24"
cocoa = "0.26"
objc = { version = "0.2.7", features = ["exception"] }
```

### Key Points

1. **`cocoa = "0.26"`**
   - Provides safe Rust bindings to macOS Cocoa APIs
   - Includes `NSWindow`, `NSView`, `NSColor`, etc.
   - Compatible with `core-graphics = "0.24"`

2. **`objc = { version = "0.2.7", features = ["exception"] }`**
   - Enables Objective-C runtime interop
   - **`exception` feature**: Critical for exception handling
   - Converts `NSException` to catchable Rust panics
   - Enables safe use of `msg_send!` macro

## Technical Details

### Exception Handling Flow

Without the fix:
```
Objective-C Code → NSException thrown → Undefined behavior in Rust
```

With the fix:
```
Objective-C Code → NSException thrown → objc exception feature intercepts 
→ Converted to Rust panic → Caught by std::panic::catch_unwind() (if used)
```

### Existing Panic Handling

The codebase already has defensive panic handling in `src/capture/macos.rs`:

```rust
// Create image from display - this can panic on some systems
let screen_image = match std::panic::catch_unwind(|| {
    display.image()
}) {
    Ok(Some(img)) => {
        log::info!("Screen image created successfully: {}x{}", img.width(), img.height());
        img
    },
    Ok(None) => {
        log::error!("Failed to capture screen image - display.image() returned None");
        return Err(anyhow!("Failed to capture screen image"));
    },
    Err(e) => {
        log::error!("Panic during screen capture: {:?}", e);
        return Err(anyhow!("Panic during screen capture"));
    }
};
```

With the `objc` exception feature enabled, this code will now properly catch NSExceptions that occur during `display.image()` calls.

## Testing

To verify the fix on macOS:

```bash
# Clean build to ensure dependencies are properly resolved
cargo clean

# Build the project
cargo build --release

# Run the application
cargo run --release
```

The application should now compile and run without the "foreign exceptions" error.

## Potential Exception Sources

The following macOS API calls could potentially throw NSExceptions (typically for programming errors):

1. **Window Creation**
   - `NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(...)`
   - NSException may be thrown for: Invalid style mask combinations, invalid backing store type, or nil dereference

2. **Display Capture**
   - `CGDisplay::image()` - Screen capture operations
   - May throw if: Display is invalid, insufficient permissions, or graphics context errors
   - `CGDisplay::active_displays()` - Display enumeration
   - May throw if: Display system is unavailable

3. **Graphics Context**
   - `CGContext::create_bitmap_context(...)` - Invalid context parameters
   - NSException if: Invalid dimensions, null data pointer with zero bytes, or invalid color space
   - `CGContext::draw_image(...)` - Drawing operations
   - NSException if: Invalid rect dimensions or null image

4. **Custom NSView Classes**
   - Runtime class registration with `objc::declare::ClassDecl`
   - NSException if: Class name conflicts, invalid superclass, or duplicate method registration
   - Method invocations via `msg_send!`
   - NSException if: Selector doesn't exist, wrong parameter types, or object is deallocated

**Note**: Most of these exceptions indicate programming errors rather than recoverable runtime errors. The exception feature ensures these are properly caught and converted to Rust panics for debugging.

## References

- **objc crate**: https://crates.io/crates/objc
- **objc exception feature**: Enables `objc_exception` dependency for NSException handling
- **cocoa crate**: https://crates.io/crates/cocoa
- **Apple Objective-C Documentation**: https://developer.apple.com/documentation/objectivec

## Related Issues

This fix addresses the macOS compilation error reported in the issue where the project couldn't compile due to missing Objective-C exception handling infrastructure.
