# Troubleshooting Guide

Common issues and their solutions.

## Table of Contents

- [Installation Issues](#installation-issues)
- [Black Preview Window](#black-preview-window)
- [Permission Problems](#permission-problems)
- [Performance Issues](#performance-issues)
- [Capture Problems](#capture-problems)
- [Screen Sharing Issues](#screen-sharing-issues)
- [Multi-Monitor Problems](#multi-monitor-problems)
- [Platform-Specific](#platform-specific)

---

## Installation Issues

### Windows: Missing DLL Errors

**Error**: `VCRUNTIME140.dll` or `MSVCP140.dll` is missing

**Solution**:
```
1. Download Visual C++ Redistributable:
   https://aka.ms/vs/17/release/vc_redist.x64.exe
2. Install it
3. Restart RustFrame
```

**Error**: `link.exe` failed (building from source)

**Solution**: See [Building Guide](../developer/building.md#link-exe-conflict)

### macOS: "App is damaged"

**Error**: "RustFrame.app is damaged and can't be opened"

**Solution**:
```bash
# Remove Gatekeeper quarantine
xattr -cr /Applications/RustFrame.app

# If still doesn't work, try:
sudo spctl --master-disable  # Disable Gatekeeper temporarily
# Open RustFrame
sudo spctl --master-enable   # Re-enable Gatekeeper
```

### macOS: "Apple could not verify" or Malware Warning

**Error**: "Apple could not verify 'RustFrame' is free of malware that may harm your Mac or compromise your privacy"

**Why this happens**: RustFrame is not code-signed with an Apple Developer ID certificate. This is a macOS Gatekeeper security feature that blocks unsigned applications.

**Solution 1: Remove quarantine flag (Recommended)**
```bash
# Navigate to where you extracted the app
cd ~/Downloads  # or wherever RustFrame.app is located

# Remove quarantine attribute
xattr -cr RustFrame.app

# Now you can run it normally
open RustFrame.app
```

**Solution 2: Right-click + Open**
```
1. Right-click (or Control + click) on RustFrame.app
2. Select "Open" from the menu
3. Click "Open" in the security dialog
4. This creates a permanent exception
```

**Solution 3: System Settings**
```
1. Try to open RustFrame.app (it will be blocked)
2. Go to System Settings > Privacy & Security
3. Scroll down to find "RustFrame was blocked"
4. Click "Open Anyway"
5. Enter your password
6. Try opening RustFrame again
```

**Note for developers**: To avoid this warning for end users, you need to:
- Enroll in Apple Developer Program ($99/year)
- Code sign the app with a Developer ID Application certificate
- Notarize the app through Apple's notarization service
- See [Developer Guide - Code Signing](../developer/code-signing.md) for details

### Linux: AppImage Won't Run

**Error**: `fusermount: failed to open /etc/fuse.conf`

**Solution**:
```bash
# Install FUSE
sudo apt install libfuse2      # Ubuntu/Debian
sudo pacman -S fuse2           # Arch
sudo dnf install fuse-libs     # Fedora
```

**Error**: Missing libraries

**Solution**:
```bash
# Install GTK and WebKit dependencies
sudo apt install libgtk-3-0 libwebkit2gtk-4.0-37 libayatana-appindicator3-1
```

---

## Black Preview Window

Most common issue! Multiple causes and solutions.

### Cause 1: GPU Acceleration Conflict

**Symptoms**:
- Preview window is black
- Main UI works fine
- Capture logs show "GPU render successful"

**Solution Windows**:
```
1. Open Settings → Performance
2. Set Capture Method to "GDI Copy"
3. Restart capture
```

**Solution macOS**:
```
1. Ensure Screen Recording permission granted
2. System Settings → Privacy → Screen Recording
3. Remove and re-add RustFrame
4. Restart RustFrame
```

### Cause 2: Graphics Driver Issues

**Symptoms**:
- Black window on specific GPU
- Works on integrated graphics

**Solution**:
```
1. Update graphics drivers:
   - NVIDIA: GeForce Experience or nvidia.com
   - AMD: AMD Adrenalin or amd.com
   - Intel: Intel Driver & Support Assistant

2. After update, restart computer
3. Try RustFrame again
```

### Cause 3: D3D11 Device Mismatch (Windows)

**Symptoms**:
- Application crashes with "foreign exception"
- Exit code 0xc000041d

**Temporary Solution**:
GPU acceleration is temporarily disabled in current builds. Will be fixed in future update.

### Cause 4: Off-Screen Window (macOS)

**Symptoms**:
- Preview window exists but shows black in screen sharing picker

**Solution**:
This is a known macOS limitation. Ensure:
```
1. Preview window is on-screen (not minimized)
2. Window has non-zero alpha (Settings → check alpha value)
3. Screen Recording permission granted
```

### Quick Diagnostic

Run this test:
```
1. Start capture
2. Take a screenshot (Snipping Tool / Cmd+Shift+4)
3. If preview window shows content in screenshot but not in video call:
   → Screen sharing compatibility issue (see below)
4. If preview window is black everywhere:
   → GPU/driver issue
```

---

## Permission Problems

### macOS: Screen Recording

**Error**: "RustFrame does not have screen recording permission"

**Solution**:
```
1. System Settings → Privacy & Security → Screen Recording
2. Enable RustFrame checkbox
3. If already enabled:
   a. Disable it
   b. Quit RustFrame completely
   c. Re-enable permission
   d. Restart RustFrame
```

**Error**: Permission prompt doesn't appear

**Solution**:
```bash
# Reset TCC database (requires admin)
tccutil reset ScreenCapture com.rustframe.app

# Or manually:
sudo sqlite3 ~/Library/Application\ Support/com.apple.TCC/TCC.db \
  "DELETE FROM access WHERE service='kTCCServiceScreenCapture' AND client='com.rustframe.app';"

# Restart RustFrame - prompt should appear
```

### macOS: Accessibility Permission (Click Highlights)

**Error**: Click highlights don't appear

**Solution**:
```
1. System Settings → Privacy & Security → Accessibility
2. Enable RustFrame
3. Restart RustFrame
```

### Linux: Wayland Capture Permission

**Error**: "Failed to start PipeWire capture"

**Solution**:
```bash
# Ensure PipeWire is running
systemctl --user status pipewire

# If not running:
systemctl --user start pipewire

# Grant portal permission (varies by DE)
# KDE: System Settings → Applications → Screen Capture
# GNOME: Settings → Privacy → Screen Sharing
```

---

## Performance Issues

### High CPU Usage

**Problem**: CPU usage > 30%

**Diagnosis**:
```
1. Check if click highlights are enabled → Disable if not needed
2. Check target FPS → Lower to 30 FPS
3. Check capture method → Try CPU-based method
4. Check region size → Smaller region = less CPU
```

**Solutions by Platform**:

**Windows**:
```
- Use WGC (not GDI) for better GPU utilization
- Lower FPS to 30
- Disable click highlights
- Reduce capture region size
```

**macOS**:
```
- Ensure using ScreenCaptureKit (not CoreGraphics)
- Disable click highlights (uses CPU)
- Lower FPS to 30
- Future update will add GPU click rendering
```

### Frame Drops / Stuttering

**Problem**: Preview window updates slowly or stutters

**Possible Causes**:
1. **FPS too low**: Increase to 60 FPS
2. **GPU overloaded**: Close other GPU-intensive apps
3. **Thermal throttling**: Check laptop cooling

**Solution**:
```
1. Settings → Performance → Target FPS
2. Set to 60 FPS
3. Ensure GPU acceleration enabled
4. Close unnecessary applications
```

### High Memory Usage

**Problem**: Memory usage > 500 MB

**Normal Ranges**:
- Idle: 50-100 MB
- Capturing 1080p: 100-200 MB
- Capturing 4K: 300-500 MB

**If Excessive**:
```
1. Check for memory leaks (restart RustFrame)
2. Reduce capture region size
3. Lower FPS
4. Report bug with logs
```

---

## Capture Problems

### Border Not Appearing

**Problem**: Start capture but no hollow border shows

**Solution**:
```
1. Check if Settings → Border → Show Border is enabled
2. Border might be off-screen:
   - Settings → Capture Region
   - Reset to safe coordinates (e.g., 100, 100)
3. Check logs for errors
```

### Can't Resize Border

**Problem**: Border corners don't respond to mouse

**Solution**:
```
In Capture Mode:
- Interior is click-through (intentional)
- Only EDGES and CORNERS are interactive
- Drag from the actual border line, not inside

In Preview Mode (Settings open):
- Full border is draggable
```

### Region Resets on Restart

**Problem**: Capture region position forgotten

**Solution**:
```
1. Settings → Advanced
2. Enable "Remember Last Region"
3. Settings → Save
```

### Border Corners Visible in Capture

**This should NOT happen** - border is automatically excluded!

If you see border corners:
```
1. Report bug with screenshot
2. Workaround: Adjust region slightly inward manually
```

---

## Screen Sharing Issues

### Can't Find Preview Window in Google Meet

**Problem**: "RustFrame Preview" not in window list

**Solutions**:

**Windows**:
```
1. Ensure capture is active (Start Capture button)
2. Look for "RustFrame Preview" or "Destination Window"
3. If using profile, wait for hide_taskbar delay
4. Try: Settings → Advanced → set winapi_destination_appwindow: true
```

**macOS**:
```
1. Ensure preview window is on-screen (not minimized)
2. Grant Screen Recording permission
3. Preview window must have alpha > 0
```

### Preview Shows Black in Zoom/Teams

**Diagnosis**:
```
1. Take screenshot of preview window
2. If screenshot shows content but Zoom doesn't:
   → Platform compatibility issue
3. If screenshot also black:
   → GPU rendering issue (see Black Preview Window above)
```

**Solutions**:

**Windows**:
```
Settings → Performance
- Capture Method: Windows Graphics Capture
- Preview Mode: WinAPI GDI (most compatible)
```

**macOS**:
```
Ensure:
- Screen Recording permission granted
- Preview window visible on screen
- Using ScreenCaptureKit (if available)
```

### Discord Can't Detect Window

**Problem**: Discord screen share doesn't list RustFrame window

**Solution**:
```
1. Use "Discord" capture profile:
   - Download from profiles/ folder or create manually
2. Restart capture
3. Preview window will appear in taskbar briefly
4. Select it in Discord quickly
5. Window auto-hides after 2 seconds
```

**Manual Fix** (edit settings.json):
```json
{
  "winapi_destination_toolwindow": false,
  "winapi_destination_appwindow": true,
  "winapi_destination_noactivate": false
}
```

---

## Multi-Monitor Problems

### Border Doesn't Move to Second Monitor

**Problem**: Drag border to Monitor 2, but it stays on Monitor 1

**Solution**:
```
While Capturing:
- Drag border so CENTER point is on target monitor
- RustFrame detects monitor by center point
- Capture restarts on new monitor

Before Capture:
- Settings → Capture Region → Monitor dropdown
- Select monitor manually
```

### DPI/Scaling Issues

**Problem**: Capture region wrong size on high-DPI display

**Windows**:
```
1. Ensure "Use Windows DPI scaling" enabled in app manifest
2. Check: Settings → Display → Scale (Windows settings)
3. RustFrame auto-detects scale factor
4. If issues persist, report bug with DPI values from logs
```

**macOS**:
```
Retina displays handled automatically by Core Graphics.
If issues, check Console.app for scale factor logs.
```

### Monitor Not Detected

**Problem**: Monitor 2/3 not in dropdown

**Windows**:
```
1. Ensure monitor connected and enabled in Windows Display Settings
2. Restart RustFrame
3. Check logs for EnumDisplayMonitors output
```

**macOS**:
```
1. System Settings → Displays → Arrange
2. Ensure displays not mirrored
3. Restart RustFrame
```

---

## Platform-Specific

### Windows

#### "Application failed to start (0xc000007b)"

**Solution**:
```
1. Ensure you downloaded x64 version (not x86)
2. Install .NET Desktop Runtime:
   https://dotnet.microsoft.com/download/dotnet/6.0
```

#### Windows Defender Blocks App

**Solution**:
```
1. Windows Security → Virus & threat protection
2. Protection history
3. Allow RustFrame.exe
4. Add to exclusions if needed
```

### macOS

#### Crash on Launch (M1/M2 Mac)

**Solution**:
```
1. Ensure using Universal binary (supports Apple Silicon)
2. Check: file /Applications/RustFrame.app/Contents/MacOS/RustFrame
   Should show: Mach-O universal binary with 2 architectures
3. If Intel-only, re-download latest release
```

#### Preview Window Behind Desktop Icons

**This is intentional** on macOS for certain configurations.

To change:
```json
// settings.json
{
  "preview_mode_macos_level": 0  // Default: normal window level
}
```

### Linux

#### Wayland: No Capture Output

**Solution**:
```bash
# Check PipeWire
systemctl --user status pipewire pipewire-media-session

# Restart if needed
systemctl --user restart pipewire

# Check portal
ls /usr/libexec/xdg-desktop-portal*
```

#### X11: BadWindow Errors

**Solution**:
```bash
# Check X server
echo $DISPLAY  # Should output :0 or similar

# Test X permissions
xhost +local:

# Run RustFrame from terminal to see errors
./RustFrame*.AppImage 2>&1 | tee rustframe.log
```

---

## Getting Further Help

### Gather Diagnostic Info

Before reporting bugs:

1. **Check Logs**:
   ```
   Settings → Open Logs Folder
   → Latest file: rustframe-YYYY-MM-DD.log
   ```

2. **Record Details**:
   - OS version (Windows 11, macOS 14.2, Ubuntu 24.04)
   - Graphics card (NVIDIA RTX 3060, Intel UHD 620)
   - Monitor setup (single 1080p, dual 4K, etc.)
   - Video conferencing app (Google Meet, Zoom, Teams)

3. **Screenshot**:
   - Main UI with error
   - Settings dialog
   - Preview window (if visible)

### Report Bug

**GitHub Issues**: https://github.com/salihcantekin/RustFrame/issues

**Include**:
- [ ] OS and version
- [ ] RustFrame version
- [ ] Steps to reproduce
- [ ] Expected vs actual behavior
- [ ] Relevant logs
- [ ] Screenshots

### Community Support

**Discussions**: https://github.com/salihcantekin/RustFrame/discussions

### Known Limitations

See [Known Issues](../developer/known-issues.md) for current limitations and planned fixes.

---

**Previous**: [Features Guide](features.md) | **Next**: [FAQ](faq.md) →
