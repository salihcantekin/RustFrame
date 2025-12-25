// build.rs - Build script for RustFrame
//
// This embeds the Windows manifest file, icon, and metadata into the executable
// which enables modern visual styles for Win32 controls and proper file properties.

fn main() {
    #[cfg(windows)]
    {
        // Embed Windows resources
        let mut res = winres::WindowsResource::new();
        
        // Embed the manifest for modern visual styles
        res.set_manifest_file("RustFrame.exe.manifest");
        
        // Set application icon (will be shown in Explorer and taskbar)
        // The icon.ico file should contain multiple sizes: 16x16, 32x32, 48x48, 64x64, 128x128, 256x256
        if std::path::Path::new("icon.ico").exists() {
            res.set_icon("icon.ico");
        }
        
        // File metadata (shown in Properties > Details)
        res.set("FileDescription", "RustFrame - Screen Region Capture Tool");
        res.set("ProductName", "RustFrame");
        res.set("OriginalFilename", "RustFrame.exe");
        res.set("LegalCopyright", "Copyright © 2024-2025 Salih Cantekin");
        res.set("CompanyName", "Salih Cantekin");
        res.set("Comments", "GPU-accelerated screen capture using Windows.Graphics.Capture API");
        
        // Version info (e.g., 1.0.0.0)
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        
        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to compile Windows resources: {}", e);
        }
    }
}
