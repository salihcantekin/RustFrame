
# RustFrame Cross-Platform Refactoring Plan
 
## Hedefler
1. Kod tekrarını azalt
2. Trait implementation'ları zorunlu kıl
3. Linux desteğini kolaylaştır
4. Generic kod yazılabilir hale getir

## Öncelik 1: Trait Implementation Eksikliklerini Gider

### 1.1 Windows HollowBorder için BorderWindow trait implement et

**Dosya**: `src/hollow_border/windows.rs`

**Değişiklik**: 
```rust
// Mevcut impl HollowBorder {...} metodları kalsın
impl HollowBorder {
    pub fn new(...) -> Option<Self> { ... }
    pub fn get_rect(&self) -> (i32, i32, i32, i32) { ... }
    // ... diğer metodlar
}

// YENİ: Trait implementation ekle
impl BorderWindow for HollowBorder {
    fn new(x: i32, y: i32, width: i32, height: i32, border_width: i32, border_color: u32) -> Option<Self> {
        HollowBorder::new(x, y, width, height, border_width, border_color)
    }
    
    fn get_rect(&self) -> (i32, i32, i32, i32) {
        HollowBorder::get_rect(self)
    }
    
    fn get_inner_rect(&self) -> (i32, i32, i32, i32) {
        HollowBorder::get_inner_rect(self)
    }
    
    // ... tüm trait metodlarını implement et
}
```

**Fayda**: Generic kod yazılabilir:
```rust
fn create_border<T: BorderWindow>(x: i32, y: i32, ...) -> Option<T> {
    T::new(x, y, ...)
}
```

---

### 1.2 RecIndicator için RecordingIndicator trait implement et

**Dosya**: `src/rec_indicator.rs`

**Değişiklik**:
```rust
impl RecordingIndicator for RecIndicator {
    fn new() -> Option<Self> {
        RecIndicator::new()
    }
    
    fn show(&self, x: i32, y: i32, region_width: i32, border_width: i32) {
        RecIndicator::show(self, x, y, region_width, border_width)
    }
    
    // ... diğer metodlar
}
```

**Fayda**: RecIndicator trait object olarak kullanılabilir.

---

## Öncelik 2: Config Struct Unification

### 2.1 Ortak Config Struct Oluştur

**Yeni Dosya**: `src/window_config.rs`

```rust
//! Cross-platform window configuration

/// Common configuration for all platforms
#[derive(Debug, Clone, Default)]
pub struct CommonWindowConfig {
    pub alpha: Option<u8>,
    pub topmost: Option<bool>,
    pub click_through: Option<bool>,
}

/// Windows-specific configuration
#[derive(Debug, Clone, Default)]
pub struct WindowsWindowConfig {
    pub toolwindow: Option<bool>,
    pub layered: Option<bool>,
    pub appwindow: Option<bool>,
    pub noactivate: Option<bool>,
    pub overlapped: Option<bool>,
}

/// macOS-specific configuration
#[derive(Debug, Clone, Default)]
pub struct MacOSWindowConfig {
    pub floating_level: Option<bool>,
    pub sharing_type: Option<u64>,
    pub collection_behavior: Option<u64>,
    pub participates_in_cycle: Option<bool>,
}

/// Linux-specific configuration (future)
#[derive(Debug, Clone, Default)]
pub struct LinuxWindowConfig {
    // X11/Wayland specific options
}

/// Unified window configuration
#[derive(Debug, Clone, Default)]
pub struct WindowConfig {
    pub common: CommonWindowConfig,
    
    #[cfg(target_os = "windows")]
    pub windows: WindowsWindowConfig,
    
    #[cfg(target_os = "macos")]
    pub macos: MacOSWindowConfig,
    
    #[cfg(target_os = "linux")]
    pub linux: LinuxWindowConfig,
}

impl WindowConfig {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_alpha(mut self, alpha: u8) -> Self {
        self.common.alpha = Some(alpha);
        self
    }
    
    pub fn with_topmost(mut self, topmost: bool) -> Self {
        self.common.topmost = Some(topmost);
        self
    }
    
    // Builder pattern for easy configuration
}
```

**Kullanım**:
```rust
let config = WindowConfig::new()
    .with_alpha(255)
    .with_topmost(true)
    .with_click_through(true);

#[cfg(target_os = "windows")]
let config = config.with_windows(|w| {
    w.toolwindow = Some(true);
    w.layered = Some(true);
});

let window = DestinationWindow::new(800, 600, config);
```

**Fayda**: 
- ✅ Tek config struct
- ✅ Platform-specific kısımlar compile-time'da seçiliyor
- ✅ Builder pattern ile kolay kullanım

---

## Öncelik 3: Global State Abstraction

### 3.1 Ortak State Pattern

**Yeni Dosya**: `src/border_state.rs`

```rust
//! Common border state management

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

/// Common border state (platform-agnostic)
pub struct BorderState {
    pub rect: Arc<Mutex<(i32, i32, i32, i32)>>,
    pub border_width: Arc<Mutex<i32>>,
    pub border_color: Arc<Mutex<u32>>,
    pub is_interacting: Arc<AtomicBool>,
}

impl BorderState {
    pub fn new() -> Self {
        Self {
            rect: Arc::new(Mutex::new((0, 0, 800, 600))),
            border_width: Arc::new(Mutex::new(4)),
            border_color: Arc::new(Mutex::new(0x4080FF)),
            is_interacting: Arc::new(AtomicBool::new(false)),
        }
    }
    
    pub fn get_rect(&self) -> (i32, i32, i32, i32) {
        *self.rect.lock().unwrap()
    }
    
    pub fn set_rect(&self, rect: (i32, i32, i32, i32)) {
        *self.rect.lock().unwrap() = rect;
    }
    
    pub fn is_interacting(&self) -> bool {
        self.is_interacting.load(Ordering::SeqCst)
    }
    
    pub fn set_interacting(&self, value: bool) {
        self.is_interacting.store(value, Ordering::SeqCst);
    }
}

/// Callback registry (platform-agnostic)
pub type BorderCallback = Box<dyn Fn(i32, i32, i32, i32) + Send + Sync>;

pub struct BorderCallbacks {
    pub interaction_complete: Arc<Mutex<Option<BorderCallback>>>,
    pub live_move: Arc<Mutex<Option<BorderCallback>>>,
}

impl BorderCallbacks {
    pub fn new() -> Self {
        Self {
            interaction_complete: Arc::new(Mutex::new(None)),
            live_move: Arc::new(Mutex::new(None)),
        }
    }
    
    pub fn set_interaction_complete<F>(&self, callback: F)
    where
        F: Fn(i32, i32, i32, i32) + Send + Sync + 'static,
    {
        *self.interaction_complete.lock().unwrap() = Some(Box::new(callback));
    }
    
    pub fn set_live_move<F>(&self, callback: F)
    where
        F: Fn(i32, i32, i32, i32) + Send + Sync + 'static,
    {
        *self.live_move.lock().unwrap() = Some(Box::new(callback));
    }
    
    pub fn trigger_interaction_complete(&self, x: i32, y: i32, w: i32, h: i32) {
        if let Some(ref cb) = *self.interaction_complete.lock().unwrap() {
            cb(x, y, w, h);
        }
    }
    
    pub fn trigger_live_move(&self, x: i32, y: i32, w: i32, h: i32) {
        if let Some(ref cb) = *self.live_move.lock().unwrap() {
            cb(x, y, w, h);
        }
    }
}
```

**Platform Implementation**:
```rust
// src/hollow_border/windows.rs
use crate::border_state::{BorderState, BorderCallbacks};

lazy_static! {
    static ref BORDER_STATE: BorderState = BorderState::new();
    static ref BORDER_CALLBACKS: BorderCallbacks = BorderCallbacks::new();
}

pub fn set_border_interaction_complete_callback<F>(callback: F)
where
    F: Fn(i32, i32, i32, i32) + Send + Sync + 'static,
{
    BORDER_CALLBACKS.set_interaction_complete(callback);
}

// Window message handler'da:
fn on_mouse_up() {
    let rect = BORDER_STATE.get_rect();
    BORDER_CALLBACKS.trigger_interaction_complete(rect.0, rect.1, rect.2, rect.3);
}
```

**Fayda**:
- ✅ Ortak state management
- ✅ Callback pattern standardize edildi
- ✅ Her platformda aynı API

---

## Öncelik 4: Platform Helper Functions

### 4.1 Ortak Utility Fonksiyonlar

**Yeni Dosya**: `src/platform_utils.rs`

```rust
//! Platform-agnostic utility functions

/// Convert BGR color to RGBA
pub fn bgr_to_rgba(bgr: u32) -> [u8; 4] {
    [
        ((bgr >> 16) & 0xFF) as u8,  // R
        ((bgr >> 8) & 0xFF) as u8,   // G
        (bgr & 0xFF) as u8,           // B
        255,                          // A
    ]
}

/// Convert RGBA to BGR
pub fn rgba_to_bgr(rgba: [u8; 4]) -> u32 {
    ((rgba[2] as u32) << 16) | ((rgba[1] as u32) << 8) | (rgba[0] as u32)
}

/// Calculate inner rect (excluding border)
pub fn calculate_inner_rect(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    border_width: i32,
) -> (i32, i32, i32, i32) {
    (
        x + border_width,
        y + border_width,
        width - 2 * border_width,
        height - 2 * border_width,
    )
}

/// Validate window dimensions
pub fn validate_window_size(width: i32, height: i32) -> Result<(), String> {
    if width < 50 || height < 50 {
        return Err(format!("Window too small: {}x{}", width, height));
    }
    if width > 7680 || height > 4320 {
        return Err(format!("Window too large: {}x{}", width, height));
    }
    Ok(())
}
```

**Fayda**: Ortak logic duplication yok.

---

## Öncelik 5: Linux Support Hazırlığı

### 5.1 Skeleton Implementation

**Dosya**: `src/hollow_border/linux.rs`

```rust
//! Hollow Border Window - Linux Implementation (X11/Wayland)

use crate::traits::BorderWindow;
use crate::border_state::{BorderState, BorderCallbacks};

pub struct HollowBorder {
    // X11/Wayland specific handles
    state: BorderState,
    callbacks: BorderCallbacks,
}

impl BorderWindow for HollowBorder {
    fn new(...) -> Option<Self> {
        // TODO: X11/Wayland implementation
        None
    }
    
    // ... other trait methods
}
```

**Fayda**: Trait zorunlu olduğu için Linux'a geçerken API tutarlılığı garantili.

---

## Implementation Sırası

### Phase 1: Foundation (1-2 gün)
1. ✅ `src/window_config.rs` oluştur
2. ✅ `src/border_state.rs` oluştur
3. ✅ `src/platform_utils.rs` oluştur
4. ✅ Tüm dosyaları `src/lib.rs`'e ekle

### Phase 2: Trait Compliance (2-3 gün)
1. ✅ Windows `HollowBorder` için `BorderWindow` trait impl
2. ✅ `RecIndicator` için `RecordingIndicator` trait impl
3. ✅ Compile-time enforcement test et

### Phase 3: Config Migration (1-2 gün)
1. ✅ `WindowConfig` kullanımına geç
2. ✅ Eski `DestinationWindowConfig` kaldır
3. ✅ `main.rs` güncellemelerini yap

### Phase 4: State Refactoring (2-3 gün)
1. ✅ Windows hollow_border'da `BorderState` kullan
2. ✅ macOS hollow_border'da `BorderState` kullan
3. ✅ Callback sistemini standardize et

### Phase 5: Linux Skeleton (1 gün)
1. ✅ `hollow_border/linux.rs` skeleton
2. ✅ `destination_window/linux.rs` skeleton
3. ✅ Trait compliance test et

### Phase 6: Testing & Validation (2 gün)
1. ✅ Windows testleri
2. ✅ macOS testleri
3. ✅ Generic kod örnekleri
4. ✅ Dokümantasyon

**Toplam Tahmini Süre**: 9-13 gün

---

## Risk Analizi

### Yüksek Risk
- ❌ macOS main thread dispatch pattern'i bozulabilir
- ❌ Windows message loop etkilenebilir

**Çözüm**: Küçük incrementaldeğişiklikler, her adımda test.

### Orta Risk
- ⚠️ Mevcut kod çalışmayı durdurmadan refactor zor

**Çözüm**: Yeni yapıları ekle, eski yapıları kademeli kaldır.

### Düşük Risk
- ✅ Trait implementation ekleme güvenli
- ✅ Config struct değişikliği compile-time hataları verir

---

## Başarı Kriterleri

### Fonksiyonel
- ✅ Windows ve macOS mevcut gibi çalışıyor
- ✅ Tüm trait'ler implement edilmiş
- ✅ Generic kod yazılabiliyor

### Kod Kalitesi
- ✅ Kod duplikasyonu %50+ azalmış
- ✅ Linux skeleton hazır
- ✅ Build warning'leri %30+ azalmış

### Maintainability
- ✅ Yeni platform eklemek kolay
- ✅ API consistency var
- ✅ Dokümantasyon güncel

---

## Sonraki Adımlar

1. **Bu planı onayla**
2. **Phase 1'den başla** (foundation dosyaları)
3. **Incrementally test et** (her commit test edilebilir)
4. **macOS'u bozmadan ilerle** (kritik requirement)
