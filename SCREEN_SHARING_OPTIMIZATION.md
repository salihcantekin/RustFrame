# Screen Sharing Optimization - Technical Summary

## 🎯 Problem

Windows'ta Discord, Teams, Zoom gibi uygulamaların preview window'u görmeme sorunu.

**Sebep**: Preview window özellikleri screen sharing uygulamalarının `EnumWindows` filtrelerine takılıyordu:
- Alpha = 0 (tamamen transparan) → "invisible window" olarak filtreleniyordu
- WS_EX_TOOLWINDOW = true → Alt-Tab/taskbar'dan gizli → bazı picker'lar bunu dışlıyordu
- WS_EX_TRANSPARENT = true → "Ghost window" kategorisinde görülüyordu

## ✅ Çözüm

**Separation layer GEREKSIZ!** Preview window'u optimize ettik.

### Değişiklikler

| Özellik | Eski Değer | Yeni Değer | Neden |
|---------|------------|------------|-------|
| **alpha** | 0 (transparan) | 255 (opak) | Screen sharing detection için kritik |
| **click_through** | true | false | Normal window gibi davransın |
| **toolwindow** | true | false | Window picker'larda görünsün |
| **appwindow** | false | true | Regular app davranışı |
| **Window Title** | "RustFrame Preview" | "RustFrame - Share This Window" | Açık, bulunabilir |
| **Position** | Değişken | Sağ alt köşe | On-screen, DWM composed |
| **Z-Order** | - | HWND_BOTTOM | Otomatik, en arkada ama visible |

### Yeni Özellikler

```rust
// PreviewWindow trait'e eklendi:
fn send_to_back(&self);      // HWND_BOTTOM'a gönder
fn bring_to_front(&self);    // HWND_TOP'a gönder (debug)
fn get_rect(&self) -> Option<(i32, i32, i32, i32)>; // Position/size
```

**Platform implementasyonları**:
- **Windows**: `SetWindowPos` with `HWND_BOTTOM`/`HWND_TOP`
- **macOS**: `orderBack:`/`orderFront:`
- **Linux**: (placeholder)

## 🏗️ Mimari

### RegionToShare vs RustFrame

**RegionToShare** (3 katman):
```
MainWindow (Control)
   ↓ capture screen
RecordingWindow (Hollow Frame)
   ↓ copy content
Separation Layer (Share Target) ← Screen sharing bunu görür
```

**RustFrame** (2 katman):
```
Hollow Border (Region Selection)
   ↓ capture region
Preview Window (Share Target) ← Direkt paylaşılır!
```

**Fark**: RustFrame'de preview window **ZATEN** paylaşılacak pencere. Separation layer'a gerek yok çünkü screen capture API'den gelen veri direkt preview'e render ediliyor.

## 📊 Z-Order Stratejisi

```
┌─────────────────────────────┐
│  User Apps (Chrome, etc.)   │ ← Z-order: Ortada
├─────────────────────────────┤
│  Hollow Border (Frame)      │ ← Z-order: TOPMOST (WS_EX_TOPMOST)
└─────────────────────────────┘
         ↓
┌─────────────────────────────┐
│  Desktop / Wallpaper        │
├─────────────────────────────┤
│  Preview Window             │ ← Z-order: HWND_BOTTOM (en arkada)
│  - Alpha: 255 (opak)        │    AMA:
│  - Visible: true            │    - EnumWindows'da görünür
│  - On-screen: true          │    - DWM compose ediyor
│  - Title: "...Share This"   │    - Screen sharing bulur
└─────────────────────────────┘
```

### Screen Sharing Nasıl Görür?

`EnumWindows` API taraması:
```c
EnumWindows([](HWND hwnd, LPARAM) {
    // ✅ RustFrame preview window şu filtreleri geçer:
    if (!IsWindowVisible(hwnd)) return true;        // ✅ Visible
    
    DWORD exStyle = GetWindowLong(hwnd, GWL_EXSTYLE);
    if (exStyle & WS_EX_TOOLWINDOW) return true;    // ✅ Toolwindow değil
    
    char title[256];
    GetWindowText(hwnd, title, 256);
    if (strlen(title) == 0) return true;            // ✅ Title var
    
    DWORD alpha;
    GetLayeredWindowAttributes(hwnd, NULL, &alpha, NULL);
    if (alpha < 128) return true;                   // ✅ Alpha=255
    
    // ✅ Preview window listeye eklenir!
    AddToWindowList(hwnd, title);
    return true;
});
```

## 🔧 Implementation Details

### Windows (src/destination_window/windows.rs)

```rust
// Release mode defaults
let ex_style = {
    let layered = true;           // Alpha blending için
    let click_through = false;    // Normal window (DEĞİŞTİ!)
    let toolwindow = false;       // Window picker'da görünsün (DEĞİŞTİ!)
    let appwindow = true;         // Regular app (DEĞİŞTİ!)
    
    let mut style = WS_EX_NOACTIVATE; // Focus çalmaz
    if layered { style |= WS_EX_LAYERED; }
    if toolwindow { style |= WS_EX_TOOLWINDOW; }
    else if appwindow { style |= WS_EX_APPWINDOW; }
    if topmost { style |= WS_EX_TOPMOST; }
    // click_through KALDIRILDI (WS_EX_TRANSPARENT yok)
    style
};

// Window creation
CreateWindowExW(
    ex_style,
    w!("RustFrameDestination"),
    w!("RustFrame - Share This Window"), // DEĞİŞTİ!
    window_style,
    x_pos, y_pos,  // Sağ alt köşe
    width, height,
    //...
);

// Alpha setting
let alpha = 255; // DEĞİŞTİ! (eskiden 0)
SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);

// Z-order: HWND_BOTTOM'a gönder
SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, 
             SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
```

### Profiles (resources/profiles/windows/*.json)

**Tüm profillerde**:
```json
{
  "winapi_destination_click_through": false,  // DEĞİŞTİ! (true → false)
  "winapi_destination_toolwindow": false,     // Zaten false
  "winapi_destination_appwindow": true,       // Zaten true (Meet/Teams/Zoom)
  "winapi_destination_alpha": 255             // Zaten 255
}
```

**Discord özel**:
```json
{
  "winapi_destination_overlapped": true,      // WS_OVERLAPPEDWINDOW kullan
  "winapi_destination_appwindow": false       // overlapped ile birlikte
}
```

### macOS (src/destination_window/macos.rs)

```rust
// Z-order methods
pub fn send_to_back(&self) {
    unsafe {
        let _: () = msg_send![self.window, orderBack: nil];
    }
}

pub fn bring_to_front(&self) {
    unsafe {
        let _: () = msg_send![self.window, orderFront: nil];
    }
}

// Position/size getter
pub fn get_rect(&self) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let frame: NSRect = msg_send![self.window, frame];
        // Convert bottom-left to top-left origin
        // ...
        Some((x, y, width, height))
    }
}
```

## 📋 Test Checklist

### Ön Kontroller (Debug Mode)
- [ ] Preview window sağ alt köşede görünür
- [ ] Alt+Tab listesinde "RustFrame - Share This Window" var
- [ ] Taskbar'da icon görünür (profil ayarına göre)

### Screen Sharing Testleri (Release Mode)
- [ ] **Discord**: Window list'te görünür, paylaşım çalışır
- [ ] **Google Meet**: Chrome tab picker'da görünür, paylaşım çalışır
- [ ] **Teams**: Window picker'da görünür, paylaşım çalışır
- [ ] **Zoom**: Application window list'te görünür, paylaşım çalışır

### Debug Tools
- [ ] **Spy++**: Window styles doğru (toolwindow=false, appwindow=true)
- [ ] **Logs**: "Window alpha set" ve "sent to back" mesajları var
- [ ] **EnumWindows**: PowerShell test ile pencere bulunuyor

## 🐛 Troubleshooting

| Sorun | Olası Sebep | Çözüm |
|-------|-------------|-------|
| Preview görünmüyor | Off-screen position | Spy++ ile rect kontrol |
| Alt+Tab'da yok | WS_EX_TOOLWINDOW | Profil ayarı kontrol |
| Transparan görünür | Alpha != 255 | Log kontrol, default_settings.json |
| Click-through çalışıyor | WS_EX_TRANSPARENT | click_through=false olmalı |
| Discord görmüyor | overlapped=false | discord.json profil aktif mi? |

## 📚 Referanslar

### Karşılaştırma: RegionToShare
- **Repo**: https://github.com/tom-englert/RegionToShare
- **Fark**: 3 katmanlı (main + recording + separation), bizde 2 katman
- **Benzerlik**: Z-order management (HWND_BOTTOM), on-screen positioning

### Windows API
- `EnumWindows`: Window enumeration
- `GetWindowLong(GWL_EXSTYLE)`: Extended styles query
- `SetWindowPos(HWND_BOTTOM)`: Z-order management
- `SetLayeredWindowAttributes`: Alpha blending

### Commit History
- `5900772`: feat(windows): optimize preview window for screen sharing
- `5a43812`: docs: add Windows test guide for screen sharing optimization

## 🚀 Next Steps

1. **Windows Test**: Build ve test et (WINDOWS_TEST_GUIDE.md)
2. **Feedback**: Test sonuçlarını GitHub'da paylaş
3. **Iteration**: Gerekirse profil ayarlarını tweak et

---

**Status**: ✅ macOS build OK, Windows test bekliyor
**Branch**: master
**Last Update**: 2026-01-12
