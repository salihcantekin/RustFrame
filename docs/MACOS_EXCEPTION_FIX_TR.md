# macOS "Rust Cannot Catch Foreign Exceptions" Hatası - Çözüm Özeti

## Türkçe Özet

### Sorun
Proje macOS'ta derlendiğinde "Rust cannot catch foreign exceptions" hatası veriyordu. Bu hata, Objective-C API'lerinden fırlatılan NSException'ların Rust tarafından yakalanamamasından kaynaklanıyordu.

### Kök Neden
macOS için yazılmış kod parçaları (`src/destination_window/macos.rs` ve `src/hollow_border/macos.rs`), Cocoa framework'ünü ve Objective-C çalışma zamanını kullanıyor ancak gerekli bağımlılıklar `Cargo.toml` dosyasında tanımlanmamıştı.

Kullanılan ancak eksik olan bağımlılıklar:
- **`cocoa`**: NSWindow, NSView gibi Cocoa API'leri için bağlantılar
- **`objc`**: Objective-C çalışma zamanı ve mesaj gönderme (`msg_send!` makrosu)

### Çözüm
`Cargo.toml` dosyasına eksik bağımlılıklar eklendi:

```toml
# macOS-specific dependencies
[target.'cfg(target_os = "macos")'.dependencies]
core-graphics = "0.24"
cocoa = "0.26"
objc = { version = "0.2.7", features = ["exception"] }
```

### Kritik Nokta: `exception` Özelliği

`objc` crate'inin `exception` özelliği çok önemlidir çünkü:
- Objective-C'den fırlatılan NSException'ları FFI sınırında yakalar
- Bunları Rust panic'lerine dönüştürür
- Böylece `std::panic::catch_unwind()` ile yakalanabilir hale gelir
- Kaynakların düzgün temizlenmesini sağlar

Bu özellik olmadan:
- NSException'lar Rust'ın panic mekanizmasını atlar
- Tanımsız davranış (undefined behavior) oluşabilir
- Derleme hatası alınır

### NSException Ne Zaman Fırlatılır?

Modern Cocoa API'leri genellikle beklenen hatalar için `NSError` kullanır. Ancak NSException hala şu durumlarda fırlatılır:
- Programlama hataları (geçersiz parametreler, sözleşme ihlalleri)
- Bazı eski API'ler
- Grafik/pencere işlemlerindeki çalışma zamanı hataları
- Geçersiz durum erişimleri

### Test Etme

macOS'ta projeyi derlemek için:

```bash
# Temiz bir derleme yapın
cargo clean

# Projeyi derleyin
cargo build --release

# Uygulamayı çalıştırın
cargo run --release
```

### Detaylı Dokümantasyon

Daha fazla teknik detay için:
- İngilizce: [docs/MACOS_EXCEPTION_FIX.md](MACOS_EXCEPTION_FIX.md)
- Build talimatları: [../BUILD_INSTRUCTIONS.md](../BUILD_INSTRUCTIONS.md) (macOS bölümü eklendi)

### Değişiklik Özeti

1. ✅ `Cargo.toml` - Eksik macOS bağımlılıkları eklendi
2. ✅ `docs/MACOS_EXCEPTION_FIX.md` - Detaylı açıklama (İngilizce)
3. ✅ `docs/MACOS_EXCEPTION_FIX_TR.md` - Bu özet (Türkçe)
4. ✅ `BUILD_INSTRUCTIONS.md` - macOS build talimatları eklendi

### Teknik Akış

```
Objective-C Kodu → NSException fırlatılır
    ↓
objc crate'nin exception özelliği yakalar
    ↓
Rust panic'ine dönüştürülür
    ↓
std::panic::catch_unwind() ile yakalanabilir (kod içinde kullanılıyorsa)
    ↓
Kaynaklar düzgün temizlenir
```

### Projedeki Kullanım Alanları

NSException fırlatabilecek Objective-C çağrıları:
- Window oluşturma (`NSWindow::alloc`, `initWithContentRect...`)
- Ekran yakalama (`CGDisplay::image()`)
- Grafik context işlemleri (`CGContext::create_bitmap_context`)
- Custom NSView sınıfları (`objc::declare::ClassDecl`)
- `msg_send!` makrosu ile yapılan tüm çağrılar

Tüm bu çağrılar artık `objc` crate'inin exception özelliği sayesinde güvenli bir şekilde korunmaktadır.
