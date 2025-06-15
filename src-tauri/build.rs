use std::fs::File;
use std::io::Stderr;
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=LICENCE-3rdparty.csv");

    let status = Command::new("dd-rust-license-tool")
        .args(&["check"])
        .status();

    match status {
        Ok(code) => {
            if !code.success() {
                println!("cargo::error=\"LICENSE-3rdparty.csv is not up to date. Rerun `dd-rust-license-tool write` and verify it.\"");
            }
        }
        Err(e) => {
            println!("cargo::error=Failed to run dd-rust-license-tool: {}", e);
        }
    }

    tauri_build::build()
}
