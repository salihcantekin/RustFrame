# Windows Profile Parameters

Platform-specific window configuration parameters for Windows preview window customization.

## Window Style Parameters

### `winapi_destination_overlapped`
**Type**: `boolean`  
**Default**: `false`  
**Windows API**: Controls window style (`WS_OVERLAPPEDWINDOW` vs `WS_POPUP`)

**Effect**:
- `true`: Creates a standard application window with title bar, borders, and system menu (WS_OVERLAPPEDWINDOW)
- `false`: Creates a borderless popup window without title bar (WS_POPUP)

**Use Cases**:
- `true`: Required for some legacy screen sharing applications that filter out popup windows
- `false`: Modern, clean look without title bar (recommended for most use cases)

**Windows API Documentation**:
- [Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-styles)
- [WS_OVERLAPPEDWINDOW](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-styles#ws_overlappedwindow)
- [WS_POPUP](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-styles#ws_popup)

---

## Extended Window Style Parameters

### `winapi_destination_toolwindow`
**Type**: `boolean`  
**Default**: `true`  
**Windows API**: `WS_EX_TOOLWINDOW` extended style

**Effect**:
- `true`: Window does not appear in taskbar or Alt-Tab switcher (behaves like a tool palette)
- `false`: Window appears in taskbar and Alt-Tab list

**Use Cases**:
- `true`: Default - keeps taskbar clean, preview window hidden from user
- `false`: Discord profile - required for Discord's window picker to detect the window

**Windows API Documentation**:
- [Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
- [WS_EX_TOOLWINDOW](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles#ws_ex_toolwindow)

---

### `winapi_destination_appwindow`
**Type**: `boolean`  
**Default**: `false`  
**Windows API**: `WS_EX_APPWINDOW` extended style

**Effect**:
- `true`: Forces window to appear in taskbar and window pickers (even if it's a tool window)
- `false`: Window follows default taskbar behavior based on other styles

**Use Cases**:
- `false`: Default - preview window hidden from taskbar
- `true`: Discord profile - ensures window appears in screen sharing pickers

**Windows API Documentation**:
- [WS_EX_APPWINDOW](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles#ws_ex_appwindow)
- [Managing Taskbar Buttons](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar)

---

### `winapi_destination_layered`
**Type**: `boolean`  
**Default**: `false`  
**Windows API**: `WS_EX_LAYERED` extended style

**Effect**:
- `true`: Enables alpha blending and transparency support via `SetLayeredWindowAttributes`
- `false`: Window is fully opaque, no transparency support

**Use Cases**:
- `true`: When you need transparency or alpha blending effects
- `false`: Default - better performance, no transparency needed

**Windows API Documentation**:
- [WS_EX_LAYERED](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles#ws_ex_layered)
- [SetLayeredWindowAttributes](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes)
- [Layered Windows](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#layered-windows)

---

### `winapi_destination_topmost`
**Type**: `boolean`  
**Default**: `false`  
**Windows API**: `WS_EX_TOPMOST` extended style

**Effect**:
- `true`: Window stays on top of all non-topmost windows (always visible)
- `false`: Window follows normal z-order rules

**Use Cases**:
- `true`: Debugging or when preview must always be visible
- `false`: Default - preview window positioned at HWND_BOTTOM (behind other windows)

**Windows API Documentation**:
- [WS_EX_TOPMOST](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles#ws_ex_topmost)
- [SetWindowPos (HWND_TOPMOST)](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)

---

### `winapi_destination_click_through`
**Type**: `boolean`  
**Default**: `false`  
**Windows API**: `WS_EX_TRANSPARENT` extended style

**Effect**:
- `true`: Window ignores mouse input, clicks pass through to window beneath
- `false`: Window is interactive and responds to mouse input

**Use Cases**:
- `true`: Advanced use cases where preview should be non-interactive
- `false`: Default - window can be moved/interacted with normally

**Windows API Documentation**:
- [WS_EX_TRANSPARENT](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles#ws_ex_transparent)

---

### `winapi_destination_noactivate`
**Type**: `boolean`  
**Default**: `false`  
**Windows API**: `WS_EX_NOACTIVATE` extended style

**Effect**:
- `true`: Window does not steal focus when created or clicked
- `false`: Window can receive focus normally

**Use Cases**:
- `true`: Prevents interrupting user's work when preview window is created
- `false`: Default - standard window focus behavior

**Windows API Documentation**:
- [WS_EX_NOACTIVATE](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles#ws_ex_noactivate)

---

## Visual Parameters

### `winapi_destination_alpha`
**Type**: `integer` (0-255)  
**Default**: `255`  
**Windows API**: `SetLayeredWindowAttributes` alpha parameter

**Effect**:
- `0`: Fully transparent (invisible)
- `255`: Fully opaque (solid)
- `1-254`: Partial transparency

**Requirements**:
- Requires `winapi_destination_layered: true` to work

**Use Cases**:
- `255`: Default - fully visible preview window
- `0-254`: Semi-transparent overlay effects

**Windows API Documentation**:
- [SetLayeredWindowAttributes](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes)
- [Alpha Blending](https://learn.microsoft.com/en-us/windows/win32/gdi/alpha-blending)

---

## Behavior Parameters

### `winapi_destination_hide_taskbar_after_ms`
**Type**: `integer` (milliseconds) or `null`  
**Default**: `null`  
**Implementation**: Custom RustFrame feature

**Effect**:
- `null`: Window maintains initial taskbar visibility state
- `> 0`: After specified delay, automatically adds `WS_EX_TOOLWINDOW` to hide from taskbar

**Use Cases**:
- `null`: Default - preview stays hidden from taskbar
- `15000`: Discord profile - visible for 15 seconds to allow user selection, then auto-hides

**Implementation Details**:
- Window is initially created with `appwindow: true` to appear in taskbar/pickers
- After delay, `SetWindowLong(GWL_EXSTYLE)` is used to add `WS_EX_TOOLWINDOW`
- Once screen sharing app has selected the window, it continues sharing even after taskbar visibility changes

**Windows API Documentation**:
- [SetWindowLong](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowlongw)
- [GetWindowLong](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowlongw)

---

## Common Profile Configurations

### Default Profile (Meet/Zoom/Teams)
```json
{
  "winapi_destination_overlapped": false,
  "winapi_destination_toolwindow": true,
  "winapi_destination_appwindow": false,
  "winapi_destination_layered": false,
  "winapi_destination_alpha": 255,
  "winapi_destination_topmost": false,
  "winapi_destination_click_through": false,
  "winapi_destination_noactivate": false,
  "winapi_destination_hide_taskbar_after_ms": null
}
```
**Characteristics**: Borderless, hidden from taskbar, always behind other windows

---

### Discord Profile
```json
{
  "winapi_destination_overlapped": false,
  "winapi_destination_toolwindow": false,
  "winapi_destination_appwindow": true,
  "winapi_destination_layered": false,
  "winapi_destination_alpha": 255,
  "winapi_destination_topmost": false,
  "winapi_destination_click_through": false,
  "winapi_destination_noactivate": false,
  "winapi_destination_hide_taskbar_after_ms": 15000
}
```
**Characteristics**: Borderless, visible in taskbar for 15s (for Discord picker), then auto-hides

---

## Windows API Context

### Window Creation Flow
1. **Register Window Class**: `RegisterClassExW` with custom window procedure
2. **Create Window**: `CreateWindowExW` with combined styles
3. **Set Properties**: `SetLayeredWindowAttributes` for alpha/transparency
4. **Position Window**: `SetWindowPos` for z-order (HWND_BOTTOM)
5. **Message Loop**: Dedicated thread processes WM_PAINT, WM_SIZE, etc.

### Z-Order Management
RustFrame uses `HWND_BOTTOM` to position the preview window behind all other windows while keeping it visible:
```cpp
SetWindowPos(hwnd, HWND_BOTTOM, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
```

**Windows API Documentation**:
- [CreateWindowExW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowexw)
- [SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)
- [Window Messages](https://learn.microsoft.com/en-us/windows/win32/winmsg/windowing)

---

## Screen Sharing Compatibility

### How Screen Sharing Apps Filter Windows

Most screen sharing applications use `EnumWindows` with filters:

**Common Filters**:
1. **Visibility**: `IsWindowVisible` must return TRUE
2. **Cloaked**: Windows with `DWM_CLOAKED` attribute are excluded
3. **Tool Windows**: Many apps exclude `WS_EX_TOOLWINDOW`
4. **Alpha**: Fully transparent windows (alpha=0) are often filtered
5. **Size**: Very small windows may be excluded

**Platform-Specific Behavior**:
- **Discord**: Requires `WS_EX_APPWINDOW` or standard window in taskbar
- **Google Meet/Zoom**: More permissive, accepts most visible windows
- **Microsoft Teams**: Similar to Meet/Zoom
- **OBS Studio**: Captures all visible windows

**Windows API Documentation**:
- [EnumWindows](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumwindows)
- [IsWindowVisible](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindowvisible)
- [DwmGetWindowAttribute (DWM_CLOAKED)](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmgetwindowattribute)

---

## Performance Considerations

### Style Impact on Performance

| Style | Performance Impact | Notes |
|-------|-------------------|-------|
| `WS_POPUP` | ✅ Fast | No system drawing of title bar/borders |
| `WS_OVERLAPPEDWINDOW` | ⚠️ Slower | System must draw title bar, borders, buttons |
| `WS_EX_LAYERED` | ⚠️ Slower | Requires alpha composition, use only if needed |
| `WS_EX_TRANSPARENT` | ✅ Minimal | Simple flag, no rendering overhead |
| `WS_EX_TOPMOST` | ✅ Minimal | Z-order management only |

**Recommendation**: Use `WS_POPUP` without `WS_EX_LAYERED` for best performance.

---

## Related Documentation

- [RustFrame Settings Guide](../user-guide/settings.md)
- [Windows Capture Engine](../technical/windows-capture.md)
- [Profile System](../user-guide/features.md#profiles)

---

## External Resources

### Microsoft Documentation
- [Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-styles)
- [Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
- [Window Features](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features)
- [About Windows](https://learn.microsoft.com/en-us/windows/win32/winmsg/about-windows)
- [Desktop Window Manager](https://learn.microsoft.com/en-us/windows/win32/dwm/dwm-overview)

### Related Win32 APIs
- [CreateWindowExW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowexw)
- [SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)
- [SetWindowLong](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowlongw)
- [SetLayeredWindowAttributes](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes)
- [EnumWindows](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumwindows)
