use anyhow::{anyhow, Result};
use cargo_metadata::MetadataCommand;
use colored::*;
use dotenv;
use git2::Repository;
use regex::Regex;
use reqwest::blocking::Client;
use rpassword::prompt_password;
use serde_json::json;
use std::env;
use std::fs;
use std::fs::create_dir_all;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

fn get_env_var_or_prompt(var_name: &str, prompt: &str, is_password: bool) -> Result<String> {
    match env::var(var_name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => {
            println!("{} not found in environment.", var_name.yellow().bold());
            let value = if is_password {
                prompt_password(prompt)?
            } else {
                print!("{}", prompt);
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                input.trim().to_string()
            };

            // Only save non-password values to .env
            if !is_password {
                save_to_env_file(var_name, &value)?;
            }

            Ok(value)
        }
    }
}

fn save_to_env_file(var_name: &str, value: &str) -> Result<()> {
    let dotenv_path = Path::new(".env");
    let mut dotenv_content = if dotenv_path.exists() {
        fs::read_to_string(dotenv_path)?
    } else {
        String::new()
    };

    if !dotenv_content.ends_with('\n') && !dotenv_content.is_empty() {
        dotenv_content.push('\n');
    }
    dotenv_content.push_str(&format!("{}={}\n", var_name, value));
    fs::write(dotenv_path, dotenv_content)?;
    println!("{}", format!(".env updated with {}", var_name).green());
    Ok(())
}

pub struct BuildTarget {
    pub platform_key: &'static str,
    pub rust_target: &'static str,
}

impl BuildTarget {
    #[inline]
    pub fn is_windows(&self) -> bool {
        self.rust_target.contains("pc-windows")
    }
    #[inline]
    pub fn is_macos(&self) -> bool {
        self.rust_target.contains("apple-darwin")
    }
    #[inline]
    pub fn is_aarch64(&self) -> bool {
        self.rust_target.starts_with("aarch64") || self.rust_target.contains("arm64")
    }

    fn relative_artifacts(&self, version: &str) -> Vec<PathBuf> {
        // Paths relative to target/<triple>/release
        if self.is_macos() {
            vec![
                Path::new("bundle/macos").join("PDF Rancher.app.tar.gz"),
                Path::new("bundle/macos").join("PDF Rancher.app.tar.gz.sig"),
                Path::new("bundle/dmg").join(format!("PDF Rancher_{}_aarch64.dmg", version)),
            ]
        } else if self.is_windows() {
            let arch_suffix = if self.rust_target.starts_with("x86_64") {
                "x64"
            } else if self.rust_target.starts_with("aarch64") {
                "arm64"
            } else {
                "unknown"
            };
            vec![
                Path::new("bundle/nsis")
                    .join(format!("PDF Rancher_{}_{}-setup.exe", version, arch_suffix)),
                Path::new("bundle/nsis").join(format!(
                    "PDF Rancher_{}_{}-setup.exe.sig",
                    version, arch_suffix
                )),
            ]
        } else {
            vec![]
        }
    }

    pub fn artifacts(&self, version: &str) -> Vec<PathBuf> {
        let base = Path::new("target").join(self.rust_target).join("release");
        self.relative_artifacts(version)
            .into_iter()
            .map(|p| base.join(p))
            .collect()
    }

    pub fn updater_paths(&self, version: &str) -> Option<(PathBuf, PathBuf)> {
        let rels = self.relative_artifacts(version);
        if self.is_macos() {
            let mut updater: Option<PathBuf> = None;
            let mut sig: Option<PathBuf> = None;
            for rel in &rels {
                if let Some(name) = rel.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".app.tar.gz") {
                        updater = Some(rel.clone());
                    } else if name.ends_with(".app.tar.gz.sig") {
                        sig = Some(rel.clone());
                    }
                }
            }
            match (updater, sig) {
                (Some(u), Some(s)) => Some((u, s)),
                _ => None,
            }
        } else if self.is_windows() {
            let mut exe: Option<PathBuf> = None;
            let mut sig: Option<PathBuf> = None;
            for rel in &rels {
                if let Some(name) = rel.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with("-setup.exe") {
                        exe = Some(rel.clone());
                    } else if name.ends_with("-setup.exe.sig") {
                        sig = Some(rel.clone());
                    }
                }
                if exe.is_some() && sig.is_some() {
                    break;
                }
            }
            match (exe, sig) {
                (Some(e), Some(s)) => Some((e, s)),
                _ => None,
            }
        } else {
            None
        }
    }
}

pub const BUILD_TARGETS: &[BuildTarget] = &[
    BuildTarget {
        platform_key: "darwin-aarch64",
        rust_target: "aarch64-apple-darwin",
    },
    BuildTarget {
        platform_key: "windows-x86_64",
        rust_target: "x86_64-pc-windows-msvc",
    },
];

pub fn run(allow_dirty: bool) -> Result<()> {
    dotenv::dotenv().ok();

    // Use cargo_metadata to find the workspace root
    let metadata = MetadataCommand::new().exec()?;
    let workspace_root = std::path::Path::new(metadata.workspace_root.as_str());
    env::set_current_dir(workspace_root)?;

    // 1. Generate LICENSE-3rdparty.csv using dd-rust-license-tool write (in src-tauri)
    let cwd = std::env::current_dir().unwrap();
    println!(
        "[xtask] Running dd-rust-license-tool write from: {}",
        cwd.display()
    );
    let mut cmd = Command::new("dd-rust-license-tool");
    cmd.arg("write").current_dir("src-tauri");
    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "[xtask] dd-rust-license-tool write failed with status: {}",
            output.status
        );
        eprintln!("[xtask] stderr: {}", stderr);
        return Err(anyhow!(
            "Failed to generate LICENSE-3rdparty.csv in src-tauri"
        ));
    }
    println!(
        "{}",
        "LICENSE-3rdparty.csv generated in src-tauri."
            .green()
            .bold()
    );

    // 1b. Check that the license file is up to date
    println!(
        "[xtask] Running dd-rust-license-tool check from: {}",
        cwd.display()
    );
    let mut cmd = Command::new("dd-rust-license-tool");
    cmd.arg("check").current_dir("src-tauri");
    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "[xtask] dd-rust-license-tool check failed with status: {}",
            output.status
        );
        eprintln!("[xtask] stderr: {}", stderr);
        return Err(anyhow!("LICENSE-3rdparty.csv is not up to date. Rerun `dd-rust-license-tool write` and verify it."));
    }
    println!("{}", "LICENSE-3rdparty.csv is up to date.".green().bold());

    // 2. Commit LICENSE-3rdparty.csv as a separate commit, but only if it changed
    let repo = Repository::open(".")?;
    let license_path = Path::new("src-tauri/LICENSE-3rdparty.csv");
    let changed = repo
        .diff_index_to_workdir(None, None)?
        .deltas()
        .into_iter()
        .any(|f| f.new_file().path() == Some(license_path));

    if changed {
        let mut index = repo.index()?;
        index.add_path(license_path)?;
        index.write()?;
        let oid = index.write_tree()?;
        let signature = repo.signature()?;
        let parent_commit = repo.head()?.peel_to_commit()?;
        let tree = repo.find_tree(oid)?;
        let msg = "Update LICENSE-3rdparty.csv";
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &msg,
            &tree,
            &[&parent_commit],
        )?;
        println!(
            "{}",
            "Committed LICENSE-3rdparty.csv as a separate commit."
                .green()
                .bold()
        );
    } else {
        println!(
            "{}",
            "LICENSE-3rdparty.csv unchanged, skipping commit."
                .yellow()
                .bold()
        );
    }

    // 3. Check for uncommitted changes
    if !allow_dirty {
        let statuses = repo.statuses(None)?;
        if statuses.iter().any(|e| {
            let s = e.status();
            s.is_index_new()
                || s.is_index_modified()
                || s.is_wt_new()
                || s.is_wt_modified()
                || s.is_wt_deleted()
                || s.is_conflicted()
        }) {
            eprintln!(
                "{}",
                "Uncommitted changes found. Please commit or stash them before releasing."
                    .red()
                    .bold()
            );
            return Err(anyhow!(
                "Please commit or stash your changes before running the release script."
            ));
        }
        println!(
            "{}",
            "No uncommitted changes. Proceeding with release..."
                .green()
                .bold()
        );
    } else {
        println!(
            "{}",
            "Warning: Skipping uncommitted changes check (--allow-dirty set)"
                .yellow()
                .bold()
        );
    }

    // --- Save previous version for rollback ---
    let cargo_toml_path = Path::new("src-tauri/Cargo.toml");
    let content = fs::read_to_string(cargo_toml_path)?;
    let re = Regex::new(r#"version\s*=\s*"(\d+)\.(\d+)\.(\d+)""#).unwrap();
    let caps = re
        .captures(&content)
        .ok_or_else(|| anyhow!("Could not find version in Cargo.toml"))?;

    // 4. Bump version in src-tauri/Cargo.toml (patch bump)
    let new_version = format!(
        "{}.{}.{}",
        caps[1].parse::<u64>()?,
        caps[2].parse::<u64>()?,
        caps[3].parse::<u64>()? + 1
    );
    let new_content = re.replace(&content, format!("version = \"{}\"", new_version));
    fs::write(cargo_toml_path, new_content.as_bytes())?;
    println!(
        "{} {}",
        "Bumped version to".cyan(),
        new_version.cyan().bold()
    );

    // 4b. Keep tauri.conf.json version in sync (file required)
    let tauri_conf_path = Path::new("src-tauri/tauri.conf.json");
    if !tauri_conf_path.exists() {
        return Err(anyhow!("Required file src-tauri/tauri.conf.json not found"));
    }
    let tauri_conf_raw = fs::read_to_string(tauri_conf_path)?;
    let mut v: serde_json::Value = serde_json::from_str(&tauri_conf_raw)
        .map_err(|e| anyhow!("Failed to parse tauri.conf.json: {e}"))?;
    let old = v
        .get("version")
        .and_then(|x| x.as_str())
        .unwrap_or("<none>")
        .to_string();
    if old != new_version {
        v["version"] = serde_json::Value::String(new_version.clone());
        fs::write(tauri_conf_path, serde_json::to_string_pretty(&v)? + "\n")?;
        println!(
            "{} {} -> {}",
            "Updated tauri.conf.json version".green(),
            old,
            new_version
        );
    } else {
        println!("{}", "tauri.conf.json already up to date".green());
    }

    let mut index = repo.index()?;
    index.add_path(tauri_conf_path)?;
    index.add_path(Path::new("src-tauri/Cargo.toml"))?;
    index.add_path(Path::new("Cargo.lock"))?;
    index.write()?;
    let oid = index.write_tree()?;
    let signature = repo.signature()?;
    let parent_commit = repo.head()?.peel_to_commit()?;
    let tree = repo.find_tree(oid)?;
    let msg = format!("Create release for version={}", new_version);
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &msg,
        &tree,
        &[&parent_commit],
    )?;
    repo.tag(
        &format!("v{}", new_version),
        repo.head()?.peel_to_commit()?.as_object(),
        &signature,
        &msg,
        false,
    )?;
    println!(
        "{} v{}",
        "Committed and tagged".green(),
        new_version.green().bold()
    );

    // 5b. Set Apple environment variables for notarization
    println!(
        "{}",
        "Setting Apple environment variables for notarization..."
            .cyan()
            .bold()
    );
    let apple_id = get_env_var_or_prompt("APPLE_ID", "Enter your Apple ID: ", false)?;

    let apple_password = get_env_var_or_prompt(
        "APPLE_PASSWORD",
        "Enter your Apple specific password: ",
        true,
    )?;

    let apple_team_id =
        get_env_var_or_prompt("APPLE_TEAM_ID", "Enter your Apple Team ID: ", false)?;

    println!("Apple environment variables set for notarization:");
    println!("  APPLE_ID: {}", apple_id);
    println!("  APPLE_PASSWORD: {}", "*".repeat(apple_password.len()));
    println!("  APPLE_TEAM_ID: {}", apple_team_id);

    // 6. Build for all targets
    for target in BUILD_TARGETS {
        let mut cmd = Command::new("cargo");
        cmd.arg("tauri").arg("build");
        if target.is_windows() {
            cmd.arg("--runner").arg("cargo-xwin");
            if target.is_aarch64() {
                cmd.arg("--cross-compiler").arg("clang");
            }
        }
        cmd.arg("--target").arg(target.rust_target);
        // Add Apple env vars for macOS notarization
        if target.is_macos() {
            cmd.env("APPLE_ID", &apple_id)
                .env("APPLE_PASSWORD", &apple_password)
                .env("APPLE_TEAM_ID", &apple_team_id);
        }
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow!("Build failed for target {}", target.rust_target));
        }
        println!(
            "{} {}",
            "Build succeeded for target".green(),
            target.rust_target.green().bold()
        );
    }

    // 7. Collect artifacts
    let release_dir = format!("release/{}", new_version);
    create_dir_all(&release_dir)?;

    for target in BUILD_TARGETS {
        for p in target.artifacts(&new_version) {
            if p.exists() {
                let dest = Path::new(&release_dir).join(p.file_name().unwrap());
                fs::copy(&p, &dest)?;
                println!("{} {}", "Collected".cyan(), dest.display());
            } else {
                println!("{} {}", "Missing (skip)".yellow(), p.display());
            }
        }
    }

    // After building artifacts and before uploading
    let version = format!("v{}", new_version);
    println!("Enter release notes (end with Ctrl+D):");
    let mut notes = String::new();
    io::stdin().read_to_string(&mut notes)?;
    let pub_date = chrono::Utc::now().to_rfc3339();

    // Collect updater files and generate update.json (paths relative to each target triple bundle root)
    let mut platform_json = serde_json::Map::new();
    for target in BUILD_TARGETS {
        if let Some((rel_updater, rel_sig)) = target.updater_paths(&new_version) {
            let base = Path::new("target").join(target.rust_target).join("release");
            let updater_file = base.join(&rel_updater);
            let sig_file = base.join(&rel_sig);
            if updater_file.exists() && sig_file.exists() {
                let signature = fs::read_to_string(&sig_file)?.trim().to_string();
                let url = format!(
                    "https://github.com/gmalette/pdf-rancher/releases/download/v{}/{}",
                    new_version,
                    updater_file.file_name().unwrap().to_string_lossy()
                );
                let mut entry = serde_json::Map::new();
                entry.insert(
                    "signature".to_string(),
                    serde_json::Value::String(signature),
                );
                entry.insert("url".to_string(), serde_json::Value::String(url));
                platform_json.insert(
                    target.platform_key.to_string(),
                    serde_json::Value::Object(entry),
                );
            } else {
                println!(
                    "{} {} (missing updater or signature: {} / {})",
                    "Skipping update.json entry".yellow(),
                    target.platform_key,
                    updater_file.display(),
                    sig_file.display()
                );
            }
        }
    }
    let update_json = serde_json::json!({
        "version": version,
        "notes": notes.trim(),
        "pub_date": pub_date,
        "platforms": platform_json
    });
    let update_json_path = Path::new("target/release/update.json");
    create_dir_all(update_json_path.parent().unwrap())?;
    fs::write(
        update_json_path,
        serde_json::to_string_pretty(&update_json)?,
    )?;
    println!(
        "{} {}",
        "Generated updater manifest at".green(),
        update_json_path.display()
    );

    // 8. Create draft release on GitHub
    let github_token = get_env_var_or_prompt(
        "GITHUB_TOKEN",
        "Create a new token at: https://github.com/settings/tokens/new?scopes=repo&description=pdf-rancher-release\nEnter your GitHub token: ",
        true  // This is a sensitive token, so don't save to .env
    )?;

    let client = Client::new();
    let release_resp = client
        .post("https://api.github.com/repos/gmalette/pdf-rancher/releases")
        .bearer_auth(&github_token)
        .header("User-Agent", "xtask-release-script")
        .json(&json!({
            "tag_name": format!("v{}", new_version),
            "name": format!("v{}", new_version),
            "body": format!("Draft release for v{}", new_version),
            "draft": true
        }))
        .send()?;
    let release_json: serde_json::Value = release_resp.json()?;
    let upload_url = release_json["upload_url"]
        .as_str()
        .ok_or_else(|| anyhow!("No upload_url in GitHub response"))?
        .replace("{?name,label}", "");

    for entry in fs::read_dir(&release_dir)? {
        let path = entry?.path();
        if path.is_file() {
            let fname = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace(" ", ".");
            let url = format!("{}?name={}", upload_url, fname);
            let file_bytes = fs::read(&path)?;
            let resp = client
                .post(&url)
                .bearer_auth(&github_token)
                .header("Content-Type", "application/octet-stream")
                .header("User-Agent", "xtask-release-script")
                .body(file_bytes)
                .send()?;
            if !resp.status().is_success() {
                println!(
                    "{} {}: {}",
                    "Failed to upload".red(),
                    fname.red().bold(),
                    resp.text()?
                );
            } else {
                println!("{} {}", "Uploaded artifact:".green(), fname.green().bold());
            }
        }
    }

    // Upload update.json as part of the release
    if update_json_path.exists() {
        let fname = update_json_path.file_name().unwrap().to_string_lossy();
        let url = format!("{}?name={}", upload_url, fname);
        let file_bytes = fs::read(update_json_path)?;
        let resp = client
            .post(&url)
            .bearer_auth(&github_token)
            .header("Content-Type", "application/octet-stream")
            .header("User-Agent", "xtask-release-script")
            .body(file_bytes)
            .send()?;
        if !resp.status().is_success() {
            println!(
                "{} {}: {}",
                "Failed to upload".red(),
                fname.red().bold(),
                resp.text()?
            );
        } else {
            println!("{} {}", "Uploaded artifact:".green(), fname.green().bold());
        }
    }

    println!(
        "{}",
        "Draft release created and artifacts uploaded."
            .green()
            .bold()
    );
    Ok(())
}
