# macOS Permissions Guide

RustFrame requires specific system permissions to function properly on macOS. This guide explains what permissions are needed and why.

---

## Required Permissions

### 1. Screen Recording Permission ⚠️ **CRITICAL**

**Why it's needed:** RustFrame captures your screen region to display it in the preview window.

**How to grant:**
1. macOS will prompt you automatically on first launch
2. Or manually: **System Settings** → **Privacy & Security** → **Screen Recording**
3. Enable checkbox for **RustFrame** (or **Terminal/VS Code** if running via `cargo`)

**What happens without it:**
- Preview window shows black screen
- Application logs: "Screen capture permission denied"

---

### 2. Accessibility Permission (Optional)

**Why it's needed:** For advanced mouse tracking and click highlighting features.

**How to grant:**
1. **System Settings** → **Privacy & Security** → **Accessibility**
2. Enable checkbox for **RustFrame**

**What happens without it:**
- Basic features work fine
- Advanced click effects may not work

---

### 3. Automation Permission (Single Instance)

**Why it's needed:** When you launch RustFrame while it's already running, the app uses AppleScript to bring the existing window to front.

**How to grant:**
1. macOS will prompt you when launching second instance
2. Or manually: **System Settings** → **Privacy & Security** → **Automation**
3. Enable **RustFrame** → **System Events** (or **VS Code/Terminal** → **System Events** when running via cargo)

**What happens without it:**
- First instance works normally
- Second instance shows "RustFrame is already running!" but window doesn't come to front
- You'll need to manually click the window in Mission Control/Expose

---

## First Launch Checklist

When you launch RustFrame for the first time:

1. ✅ **Screen Recording prompt** → Click "Allow" (required)
2. ✅ **Automation prompt** (when launching 2nd instance) → Click "OK" (recommended)
3. ⚠️ If you accidentally clicked "Don't Allow", follow manual grant steps above

---

## Running from Terminal vs App Bundle

### Running via `cargo run` or Terminal

Permissions will be requested for **Terminal.app** or **VS Code.app** (the parent process).

**Pros:**
- Easy for development
- Quick testing

**Cons:**
- Terminal must keep running
- Permissions tied to Terminal app
- Icon shows as generic Terminal icon in Dock

### Running as App Bundle (Production)

Permissions will be requested for **RustFrame.app** itself.

**Pros:**
- Clean app experience
- Proper app icon in Dock
- Independent from Terminal
- Recommended for end users

**Cons:**
- Requires building `.app` bundle: `cargo tauri build`

---

## Troubleshooting

### Preview window shows black screen

**Cause:** Screen Recording permission not granted

**Solution:**
1. Open **System Settings** → **Privacy & Security** → **Screen Recording**
2. Enable **RustFrame** (or Terminal if running via cargo)
3. **Restart RustFrame** (permission changes require app restart)

### Second instance doesn't bring window to front

**Cause:** Automation permission not granted

**Solution:**
1. Open **System Settings** → **Privacy & Security** → **Automation**
2. Expand **RustFrame** (or Terminal/VS Code)
3. Enable **System Events**
4. Try launching second instance again

### "Visual Studio Code wants access to control System Events"

**Cause:** Running via `cargo run` from VS Code terminal

**Solution:**
- This is normal! Click "OK" to allow
- Permission is for VS Code, not RustFrame itself
- When using app bundle, permission will be for RustFrame.app

---

## Security & Privacy

### Why does RustFrame need these permissions?

- **Screen Recording:** Core feature - capturing screen region
- **Automation:** User experience - bringing window to front when already running
- **Accessibility:** Optional - enhanced mouse tracking (not required)

### What data is collected?

**Nothing.** RustFrame:
- Does NOT send data to internet
- Does NOT record or save screenshots
- Only captures screen content **while preview window is open**
- All capture happens **locally on your Mac**

### Can I revoke permissions?

Yes, at any time in **System Settings** → **Privacy & Security**. The app will stop working but no data is retained.

---

## Version-Specific Notes

### macOS 14 Sonoma and later

Screen Recording permission is **mandatory** and strictly enforced. The app cannot function without it.

### macOS 13 Ventura

Same as Sonoma.

### macOS 12 Monterey

Screen Recording permission works the same, but uses legacy ScreenCaptureKit API.

### macOS 11 Big Sur and earlier

**Not officially supported.** ScreenCaptureKit requires macOS 12.3+. The app may fall back to older APIs but functionality is limited.

---

## Developer Notes

### Testing Permissions in Development

When running via `cargo run`:
- Permissions apply to **Terminal.app** process
- To test app bundle permissions: `cargo tauri build` → run `.app` bundle
- Reset permissions: `tccutil reset All com.salihcantekin.rustframe` (requires SIP disabled)

### Bundle Identifier

Production app uses: `com.salihcantekin.rustframe` (from `tauri.conf.json`)

This identifier is used for:
- Permission storage in TCC database
- Single instance lock file: `~/.config/RustFrame/.rustframe.lock`
- App identification in NSWorkspace

---

## Related Documentation

- [Installation Guide](installation.md) - How to install RustFrame
- [Quick Start](quick-start.md) - Getting started guide
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
- [FAQ](faq.md) - Frequently asked questions

---

**Last Updated:** January 9, 2026  
**RustFrame Version:** 1.1.0+
