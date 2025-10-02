use anyhow::{anyhow, Result};
use cargo_metadata::MetadataCommand;
use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io::copy;
use std::path::PathBuf;
use tar::Archive;

pub fn run(version: &str) -> Result<()> {
    // Ensure we are at the workspace root
    let metadata = MetadataCommand::new().exec()?;
    let workspace_root = metadata.workspace_root.as_std_path();
    std::env::set_current_dir(workspace_root)?;

    let tag = version;
    println!("Using pdfium-binaries tag: {}", tag);

    let targets = [
        (
            "pdfium-mac-arm64",
            "aarch64-macos",
            "lib/libpdfium.dylib",
            "libpdfium.dylib",
        ),
        (
            "pdfium-win-x64",
            "x86_64-windows",
            "bin/pdfium.dll",
            "pdfium.dll",
        ),
        (
            "pdfium-win-arm64",
            "aarch64-windows",
            "bin/pdfium.dll",
            "pdfium.dll",
        ),
    ];

    let download_dir = PathBuf::from("target/pdfium-downloads");
    fs::create_dir_all(&download_dir)?;

    for (release_tag, target_arch, archive_path, final_name) in targets.iter() {
        let url = format!(
            "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/{}/{release_tag}.tgz",
            tag, release_tag = release_tag
        );
        println!("Downloading from {}", url);

        let response = reqwest::blocking::get(&url)?;
        if !response.status().is_success() {
            return Err(anyhow!("Failed to download {}: {}", url, response.status()));
        }

        let tgz_filename = download_dir.join(format!("{}.tgz", release_tag));
        let mut dest = File::create(&tgz_filename)?;
        let content = response.bytes()?;
        copy(&mut content.as_ref(), &mut dest)?;

        println!("Extracting {}", tgz_filename.display());
        let tgz_file = File::open(&tgz_filename)?;
        let tar = GzDecoder::new(tgz_file);
        let mut archive = Archive::new(tar);

        let mut found = false;
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_string_lossy().to_string();

            if path == *archive_path {
                let framework_dir = workspace_root
                    .join("src-tauri/frameworks")
                    .join(target_arch);
                fs::create_dir_all(&framework_dir)?;
                let final_path = framework_dir.join(final_name);
                entry.unpack(&final_path)?;
                println!("Copied {} to {}", archive_path, final_path.display());
                found = true;
                break;
            }
        }
        if !found {
            return Err(anyhow!(
                "Could not find {} in {}",
                archive_path,
                tgz_filename.display()
            ));
        }
    }

    println!("Successfully updated all pdfium binaries.");
    Ok(())
}
