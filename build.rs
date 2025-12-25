// build.rs - Build script for RustFrame
//
// This embeds the Windows manifest file into the executable
// which enables modern visual styles for Win32 controls.

fn main() {
    #[cfg(windows)]
    {
        // Embed the manifest for modern visual styles
        let mut res = winres::WindowsResource::new();
        res.set_manifest_file("RustFrame.exe.manifest");
        
        // Optional: Set icon
        // res.set_icon("icon.ico");
        
        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to compile Windows resources: {}", e);
        }
    }
}
