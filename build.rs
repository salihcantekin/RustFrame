use std::fs;
use std::path::Path;

fn main() {
    tauri_build::build();
    
    // On first run, copy default profiles and settings to user's config directory
    // These will only be copied if they don't exist - allowing user customization
    copy_default_resources();
}

fn copy_default_resources() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    
    // Source directories
    let resources_src = Path::new(&manifest_dir).join("resources");
    
    // Get user's config directory
    let Some(config_dir) = dirs::config_dir() else {
        println!("cargo:warning=Could not determine config directory");
        return;
    };
    
    let rustframe_config = config_dir.join("RustFrame");
    
    // Ensure config directory exists
    if let Err(e) = fs::create_dir_all(&rustframe_config) {
        println!("cargo:warning=Failed to create config directory: {}", e);
        return;
    }
    
    // Copy default settings if it doesn't exist
    let default_settings_src = resources_src.join("default_settings.json");
    let settings_dst = rustframe_config.join("settings.json");
    if !settings_dst.exists() && default_settings_src.exists() {
        if let Err(e) = fs::copy(&default_settings_src, &settings_dst) {
            println!("cargo:warning=Failed to copy settings: {}", e);
        } else {
            println!("cargo:warning=Copied default settings to {:?}", settings_dst);
        }
    }
    
    // Copy profiles directory structure
    let profiles_src = resources_src.join("profiles");
    let profiles_dst = rustframe_config.join("Profiles");
    
    if profiles_src.exists() {
        if let Err(e) = fs::create_dir_all(&profiles_dst) {
            println!("cargo:warning=Failed to create Profiles directory: {}", e);
            return;
        }
        
        // Copy all OS directories (windows, macos, linux)
        for os_name in &["windows", "macos", "linux"] {
            let os_profiles_src = profiles_src.join(os_name);
            let os_profiles_dst = profiles_dst.join(os_name);
            
            if os_profiles_src.exists() {
                if let Err(e) = copy_profiles_if_not_exist(&os_profiles_src, &os_profiles_dst) {
                    println!("cargo:warning=Failed to copy {} profiles: {}", os_name, e);
                } else {
                    println!("cargo:warning=Profiles directory ready for {}", os_name);
                }
            }
        }
    }
    
    println!("cargo:rerun-if-changed=resources/");
}

fn copy_profiles_if_not_exist(src: &Path, dst: &Path) -> std::io::Result<()> {
    // Create destination directory if it doesn't exist
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    
    // Copy each profile file only if it doesn't exist in destination
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);
            
            // Only copy if destination doesn't exist (preserve user customizations)
            if !dst_path.exists() {
                fs::copy(&path, &dst_path)?;
            }
        }
    }
    
    Ok(())
}

