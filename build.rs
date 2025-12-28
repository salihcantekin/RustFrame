// build.rs - Build script for RustFrame
//
// This embeds the Windows manifest file, icon, and metadata into the executable
// which enables modern visual styles for Win32 controls and proper file properties.

use semver::Version;

fn main() {
    #[cfg(windows)]
    {
        let version_str = env!("CARGO_PKG_VERSION");
        let version = Version::parse(version_str).expect("Invalid version format in Cargo.toml");

        if let Err(e) = embed_resource::compile(
            "RustFrame.exe.rc",
            &[
                format!("CARGO_PKG_VERSION=\"{}\"", version),
                format!("CARGO_PKG_VERSION_MAJOR={}", version.major),
                format!("CARGO_PKG_VERSION_MINOR={}", version.minor),
                format!("CARGO_PKG_VERSION_PATCH={}", version.patch),
            ],
        )
        .manifest_required()
        {
            panic!("Failed to compile Windows resources: {}", e);
        }
    }
}
