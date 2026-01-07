# macOS Off-Screen Window & Screen Sharing Research

## Problem
Preview penceresi off-screen olduğunda Google Meet picker'da siyah görünüyor.
Alpha < 255 olduğunda da CGWindowList içeriği capture edemiyor.

## Test Sonuçları
- ✅ Debug mode (alpha=255, on-screen): İçerik görünüyor
- ✅ Release mode (alpha=255, on-screen): İçerik görünüyor  
- ❌ Release mode (alpha=1-10, on-screen): Siyah görünüyor
- ❌ Release mode (off-screen -10000): Preview yok

## macOS Window Visibility Hierarchy

### 1. NSWindow.isVisible
- Off-screen windows: `false`
- Alpha=0: `false`
- Alpha>0 + on-screen: `true`

### 2. CGWindowList Options
```objc
kCGWindowListOptionAll              // Tüm pencereler
kCGWindowListOptionOnScreenOnly     // Sadece on-screen (Meet/Zoom kullanıyor)
kCGWindowListExcludeDesktopElements // Desktop öğelerini hariç tut
```

**Meet/Zoom/Discord picker davranışı:**
- `kCGWindowListOptionOnScreenOnly` kullanıyorlar
- Off-screen windows listelenmez
- Alpha < threshold windows içeriği render edilmez

### 3. NSWindowCollectionBehavior
```objc
NSWindowCollectionBehaviorCanJoinAllSpaces      // Tüm Space'lerde
NSWindowCollectionBehaviorMoveToActiveSpace     // Aktif Space'e taşı
NSWindowCollectionBehaviorManaged               // Mission Control'de görün
NSWindowCollectionBehaviorTransient             // Geçici pencere
NSWindowCollectionBehaviorStationary            // Sabit konumda kal
NSWindowCollectionBehaviorParticipatesInCycle   // Cmd+Tab'da görün
NSWindowCollectionBehaviorIgnoresCycle          // Cmd+Tab'da görünme
NSWindowCollectionBehaviorFullScreenAuxiliary   // Fullscreen yanında olabilir
```

## Alternatif Çözümler

### ❌ Seçenek 1: Off-Screen + Full Alpha
```rust
position: (-10000, -10000)
alpha: 255
```
**Sonuç:** Meet picker'da pencere listede yok (kCGWindowListOptionOnScreenOnly filtresi)

### ❌ Seçenek 2: On-Screen + Low Alpha
```rust
position: (100, 100)
alpha: 1-10
```
**Sonuç:** Pencere listede ama içerik siyah (CGWindowList render eşiği)

### ✅ Seçenek 3: On-Screen + Full Alpha + Off-Screen Position
```rust
position: (-width+1, 100)  // 1 pixel on-screen
alpha: 255
```
**Artıları:** 
- CGWindowList "on-screen" sayıyor
- İçerik düzgün render ediliyor
- Kullanıcı sadece 1px görüyor

**Eksileri:**
- Ekranın sol kenarında 1px çizgi görünür

### ✅ Seçenek 4: On-Screen + Full Alpha + Hidden Position
```rust
position: Screen dışında ama "technically on-screen"
alpha: 255
collection_behavior: IgnoresCycle + Transient
```
**Artıları:**
- Pencere Dock/Cmd+Tab'da görünmüyor
- CGWindowList erişebiliyor
- İçerik render ediliyor

**Eksileri:**
- Ekranda bir yerlerde görünür olması gerekiyor

### 🔥 Seçenek 5: OrderBack + Full Alpha + IgnoresCycle
```rust
position: (0, 0) behind all windows
alpha: 255
level: NSNormalWindowLevel
collection_behavior: IgnoresCycle | Stationary | Transient
orderBack() // En arkada
```
**Artıları:**
- Pencere tüm pencerelerin arkasında (masaüstü seviyesinde)
- CGWindowList erişebiliyor
- Kullanıcı görmez (üstte her zaman başka pencereler var)
- Dock/Cmd+Tab'da görünmüyor

**Eksileri:**
- Desktop'ta hiç pencere yoksa görünebilir
- Show Desktop (F11) basılırsa görünür

### 🌟 Seçenek 6: Minimized Window + Custom Preview
```rust
// Window'u minimize et
window.miniaturize()
// Ama CGWindowList hala erişebiliyor mu?
```
**Apple Docs:** Minimized windows are still in CGWindowList!

### 🎯 Seçenek 7 (ÖNERİLEN): NSWindow setStyleMask Hidden
```objc
NSWindowStyleMask kombinasyonu:
- Borderless ✓
- NonActivating ✓
- HUDWindow (heads-up display) ?
```

## Apple API Deep Dive

### NSWindowSharingType (Zaten kullanıyoruz)
- `.readOnly`: Screen capture edilebilir ✓
- Bu yeterli ama window visible olmalı

### Window Level Hiyerarşi
```
NSScreenSaverWindowLevel    = 1000
NSFloatingWindowLevel       = 3
NSNormalWindowLevel         = 0
NSDesktopWindowLevel        = -1 (arkada)
```

**Test:** `NSDesktopWindowLevel` kullan?
- Masaüstü arka planının hemen önünde
- Kullanıcı görmez (tüm pencereler önde)
- CGWindowList erişir mi?

## Önerilen Çözüm: Window Level -1 (Behind Desktop Icons)

```rust
window.setLevel_(-1);  // Desktop icon'larının arkası
position: (100, 100)
alpha: 255
collection_behavior: IgnoresCycle | Stationary
```

**Neden çalışır:**
1. Technically on-screen → CGWindowList YES
2. Desktop seviyesinden düşük → Kullanıcı görmez
3. Full alpha → İçerik render ediliyor
4. IgnoresCycle → Cmd+Tab'da yok

**Test gerekli:** macOS'ta window level -1 veya daha düşük izin veriyor mu?

## Alternatif: Dock Hidden + Behind All
```rust
// Önce tüm pencerelerin arkasına gönder
window.orderBack()
// Sonra hidden mode
NSApp.setActivationPolicy(.accessory)  // Dock'ta görünme
```

Bu uygulama seviyesinde değişiklik gerektirir.

## Son Karar

Test edilecek sıralama:
1. **Window Level = -1** (Desktop'ın arkası)
2. **OrderBack + IgnoresCycle + Stationary**
3. **Partially Off-Screen** (-width+1 position)
4. **Settings Toggle:** "Keep preview visible" (kullanıcı seçsin)
