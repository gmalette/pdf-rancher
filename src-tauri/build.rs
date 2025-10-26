fn main() {
    // Link to pre-compiled libheif and libde265 from frameworks folder
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let frameworks_dir = std::path::Path::new(&manifest_dir).join("frameworks");

    #[cfg(target_os = "macos")]
    {
        let lib_dir = frameworks_dir.join("aarch64-macos");

        // Copy libraries to target directory for tests and development
        if let Ok(out_dir) = std::env::var("OUT_DIR") {
            let target_dir = std::path::PathBuf::from(&out_dir)
                .ancestors()
                .nth(3)
                .unwrap()
                .to_path_buf();
            let target_frameworks = target_dir.join("Frameworks");

            // Create target Frameworks directory
            let _ = std::fs::create_dir_all(&target_frameworks);

            // Copy versioned libraries to target/Frameworks for runtime loading
            for lib_name in &["libheif.1.dylib", "libde265.dylib"] {
                let src = lib_dir.join(lib_name);
                let dest = target_frameworks.join(lib_name);
                if src.exists() {
                    let _ = std::fs::copy(&src, &dest);
                }
            }
        }

        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=dylib=heif");
        println!("cargo:rustc-link-lib=dylib=de265");
    }

    #[cfg(target_os = "windows")]
    {
        let lib_dir = frameworks_dir.join("x86_64-windows");
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=dylib=heif");
        println!("cargo:rustc-link-lib=dylib=de265");
    }

    tauri_build::build()
}
