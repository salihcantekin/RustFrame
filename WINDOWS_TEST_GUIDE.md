# Windows Test Guide - Screen Sharing Optimization

## 🎯 Test Edilen Özellik

Preview window'un screen sharing uygulamaları (Discord, Teams, Zoom, Google Meet) tarafından görünür olması.

## 📋 Değişiklikler Özeti

### Preview Window Özellikleri
- **Alpha**: 255 (tam opak, eskiden 0 idi)
- **Window Title**: "RustFrame - Share This Window"
- **Position**: Sağ alt köşe (ekranda, görünür)
- **Z-Order**: HWND_BOTTOM (en arkada ama visible)
- **Click-through**: false (normal window gibi)
- **Toolwindow**: false (window picker'larda görünür)
- **AppWindow**: true (regular app davranışı)

## 🔧 Test Adımları

### 1. Build ve Çalıştır

```bash
# Windows'ta (PowerShell veya CMD)
cd RustFrame
cargo build --release
.\target\release\RustFrame.exe
```

### 2. Temel Fonksiyonalite Testi

1. **RustFrame başlat**
2. **Settings → Capture Region** → Region ayarla
3. **Start Capture** tıkla
4. **Preview window'u kontrol et**:
   - Sağ alt köşede görünüyor mu?
   - Başlık: "RustFrame - Share This Window" yazıyor mu?
   - Alt+Tab yaptığınızda listede görünüyor mu?

### 3. Screen Sharing Testleri

#### Discord
1. Discord'u aç
2. Bir kanal seç → Screen share başlat
3. "Select Window" seçeneğinde:
   - ✅ "RustFrame - Share This Window" görünüyor mu?
   - ✅ Seçtiğinde capture edilen region görünüyor mu?

**Profile**: `resources/profiles/windows/discord.json`
- overlapped: true
- appwindow: false
- click_through: false

#### Google Meet
1. Meet oturumu aç
2. Present → "A window" seç
3. Window listesinde:
   - ✅ "RustFrame - Share This Window" görünüyor mu?
   - ✅ Paylaşım doğru çalışıyor mu?

**Profile**: `resources/profiles/windows/googlemeet.json`
- overlapped: false
- appwindow: true
- click_through: false

#### Microsoft Teams
1. Teams meeting başlat
2. Share → Window
3. Listede:
   - ✅ RustFrame görünüyor mu?
   - ✅ Capture region paylaşılıyor mu?

**Profile**: `resources/profiles/windows/teams.json`
- overlapped: false
- appwindow: true
- click_through: false

#### Zoom
1. Zoom meeting
2. Share Screen → Advanced → "Portion of Screen"
3. VEYA: Share → "Application Window"
   - ✅ RustFrame window listede mi?
   - ✅ Paylaşım başarılı mı?

**Profile**: `resources/profiles/windows/zoom.json`
- overlapped: false
- appwindow: true
- click_through: false

### 4. Debug Kontrolleri

#### A. Window Properties (Spy++ ile)
```
Download: https://docs.microsoft.com/en-us/visualstudio/debugger/introducing-spy-increment

1. Spy++ başlat
2. Find Window (binoculars icon)
3. RustFrame preview window'u seç
4. Properties → Styles kontrol et:
   - WS_VISIBLE: ✓
   - WS_EX_TOOLWINDOW: ✗ (olmamalı)
   - WS_EX_APPWINDOW: ✓
   - WS_EX_TRANSPARENT: ✗ (olmamalı)
   - WS_EX_LAYERED: ✓
```

#### B. Z-Order Kontrolü
```powershell
# PowerShell'de EnumWindows simulation
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public class WindowHelper {
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    
    [DllImport("user32.dll")]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
    
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
}
"@

$windows = @()
$callback = {
    param($hwnd, $lparam)
    $text = New-Object System.Text.StringBuilder 256
    [WindowHelper]::GetWindowText($hwnd, $text, 256)
    if ($text.ToString() -like "*RustFrame*") {
        Write-Host "Found: $($text.ToString()) - HWND: $hwnd"
    }
    return $true
}

[WindowHelper]::EnumWindows($callback, [IntPtr]::Zero)
```

#### C. Transparency Kontrolü
1. RustFrame preview'i ekranda bul
2. Görünürlük:
   - **Debug mode**: Preview tam görünür olmalı
   - **Release mode**: Alpha=255 olduğu için tam opak

### 5. Logs Kontrolü

```bash
# Log dosyası lokasyonu:
# C:\Users\<YourName>\AppData\Local\RustFrame\logs\

# Son log'u oku:
Get-Content "$env:LOCALAPPDATA\RustFrame\logs\rustframe.log" -Tail 50

# Şunları ara:
# - "Window alpha set for screen sharing visibility"
# - "Preview window sent to back (HWND_BOTTOM)"
# - "Destination window created"
```

## ✅ Başarı Kriterleri

### Minimum Gereksinimler
- [ ] Preview window Alt+Tab listesinde görünüyor
- [ ] Window title "RustFrame - Share This Window"
- [ ] En az 2 screen sharing uygulamasında görünüyor

### İdeal Durum
- [ ] Discord, Teams, Zoom, Meet'in HEPSİNDE görünüyor
- [ ] Preview window kullanıcıya rahatsız edici değil (sağ alt köşe, arkada)
- [ ] Capture region doğru paylaşılıyor

## 🐛 Sorun Giderme

### Preview Window Görünmüyor
1. **Spy++ ile kontrol**:
   - Window var mı?
   - WS_VISIBLE flag set mi?
   - WS_EX_TOOLWINDOW yok mu?

2. **Log kontrol**:
   ```
   "Destination window created" mesajı var mı?
   "Window alpha set" mesajı var mı?
   ```

3. **Manuel test**:
   - Alt+Tab yap, listede var mı?
   - Task Manager → Details → RustFrame.exe çalışıyor mu?

### Discord Görmüyor (Diğerleri Görüyor)
- Discord profili aktif mi?
- Settings → Capture Region → "Profile: Discord" seçili mi?
- overlapped=true olduğundan emin ol

### Hiçbir Uygulama Göremiyor
1. **Release build mi?**:
   ```bash
   cargo build --release
   ```

2. **Alpha kontrolü**:
   - Log'da "Window alpha set for screen sharing visibility" var mı?
   - Alpha=255 olmalı (0 değil!)

3. **Position kontrolü**:
   - Spy++ ile window rect kontrol et
   - Ekran dışında mı? (off-screen olmamalı)

### Click-through Problemi
- WS_EX_TRANSPARENT flag kontrolü
- click_through=false olmalı (default_settings.json)

## 📊 Test Sonuçları

Test yaptıktan sonra sonuçları kaydet:

| Uygulama | Görünüyor? | Paylaşım Çalışıyor? | Notlar |
|----------|-----------|-------------------|--------|
| Discord | ✅/❌ | ✅/❌ | |
| Google Meet | ✅/❌ | ✅/❌ | |
| Teams | ✅/❌ | ✅/❌ | |
| Zoom | ✅/❌ | ✅/❌ | |

## 📝 Raporlama

Test sonuçlarını GitHub issue olarak rapor et:

```markdown
## Test Environment
- OS: Windows 10/11 [version]
- Build: Release/Debug
- Commit: [hash]

## Test Results
[Yukarıdaki tablo]

## Logs
[İlgili log çıktıları]

## Screenshots
[Ekran görüntüleri]
```

## 🔗 İlgili Dosyalar

- Implementation: `src/destination_window/windows.rs`
- Profiles: `resources/profiles/windows/*.json`
- Settings: `resources/default_settings.json`
- Commit: [GitHub link - en son commit]

---

**İyi testler! 🚀**
