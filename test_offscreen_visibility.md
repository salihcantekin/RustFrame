# macOS Off-Screen Window Visibility Test

## Problem
Google Meet'te Windows bölümünde preview penceresi görünmüyor (boş).
Preview window off-screen konumda: (-10000, -10000)

## Hipotez
macOS screen sharing picker'ları (CGWindowListCreateImage, ScreenCaptureKit) 
off-screen pencereleri görmüyor veya capture edemiyor olabilir.

## Test Senaryoları

### 1. Off-Screen Window Behavior
- `NSWindow.isVisible` durumu nedir?
- `orderOut()` vs off-screen positioning farkı

### 2. CGWindowList Filtering
CGWindowListCopyWindowInfo ile pencere listesi:
- kCGWindowListOptionOnScreenOnly (sadece ekranda görünenler)
- kCGWindowListOptionAll (tümü dahil off-screen)

### 3. Alternatif Çözümler

#### A. 1x1 Pixel Mini Window
- Pozisyon: (0, 0) veya ekran köşesi
- Boyut: 1x1 pixel
- Avantaj: Görünmez ama "on-screen"
- Dezavantaj: Teknik olarak görülebilir

#### B. Screen Edge Position
- Pozisyon: (-width+1, 0) → sadece 1px görünür
- Avantaj: CGWindowList'te var
- Dezavantaj: Küçük bir pixel görünebilir

#### C. NSWindow.alphaValue = 0.01
- Tamamen transparent ama on-screen
- Avantaj: Picker'da preview gösterilir
- Dezavantaj: Hafif opacity gerekebilir (CGWindowList için)

#### D. Off-Screen + Manual CGImage
- Preview için manuel CGWindowListCreateImage çağrısı
- Screen sharing app'lere custom preview sağla
- Dezavantaj: Complex implementation

## Apple Documentation Findings

### NSWindow.sharingType
- `.none` (0): Deprecated, artık kullanılmıyor
- `.readOnly` (1): Capture edilebilir ama kontrol edilemez
- `.readWrite` (2): Full access

### NSWindow.isVisible
> "A Boolean value that indicates whether the window is visible onscreen 
> (even when it's obscured by other windows)."

Key point: "onscreen" - off-screen pencereler `false` döner!

### CGWindowListCreateImage
Window Server sadece "on-screen" pencereleri render eder.
Off-screen windows için preview generate edilmez.

## Recommended Solution

### Yaklaşım 1: Mini Window (Preferred)
```rust
// Pencereyi ekranın en altına, 1x1 boyutunda koy
let screen_height = NSScreen::mainScreen().frame().size.height;
let frame = NSRect::new(
    NSPoint::new(0.0, 0.0),  // Sol alt köşe
    NSSize::new(1.0, 1.0)     // 1x1 pixel
);
```

Artıları:
- ✅ On-screen → CGWindowList'te görünür
- ✅ Meet/Zoom picker'da preview oluşturulur
- ✅ Kullanıcı görmez (1 pixel)
- ✅ Basit implementation

Eksileri:
- ⚠️ Teorik olarak 1 pixel görülebilir
- ⚠️ Screenshot'larda dahil olabilir

### Yaklaşım 2: Transparent On-Screen
```rust
// Normal boyutta ama tamamen transparent
window.setAlphaValue_(0.001);  // Minimal alpha
window.setIgnoresMouseEvents_(YES);
// Position: Normal location or off to side
```

Artıları:
- ✅ On-screen → CGWindowList OK
- ✅ Preview render edilir
- ✅ Görünmez (transparent)

Eksileri:
- ⚠️ Çok hafif bir opacity gerekebilir (0.0 picker'da sorun çıkarabilir)
- ⚠️ Screen üzerinde yer kaplar (capture edilebilir)

### Yaklaşım 3: Screen Edge (-width+1)
```rust
// Pencerenin sadece 1 pixel'i ekranda
let frame = NSRect::new(
    NSPoint::new(-(width as f64) + 1.0, 100.0),
    NSSize::new(width as f64, height as f64)
);
```

Artıları:
- ✅ Hemen hemen off-screen
- ✅ CGWindowList'te var

Eksileri:
- ⚠️ 1 pixel line görünür
- ⚠️ Çözünürlük değişikliklerinde problem olabilir

## Test Plan
1. 1x1 mini window test → Meet picker'da görünüyor mu?
2. Alpha = 0.001 test → Preview render ediliyor mu?
3. Screen edge test → 1px line visible mi?

## Implementation Strategy
1. Debug mode: Normal position (100, 100) ✅ Current
2. Release mode: 1x1 mini window at (0, 0)
3. Settings'te kullanıcı seçimi: "Keep preview visible" toggle?
