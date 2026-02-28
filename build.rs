fn main() {
    // Embed the application manifest for DPI awareness via mt.exe or linker
    println!("cargo:rerun-if-changed=wintab.manifest");

    // Use the MSVC linker to embed the manifest
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        std::path::Path::new("wintab.manifest")
            .canonicalize()
            .unwrap()
            .display()
    );
}
