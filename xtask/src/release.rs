use anyhow::{anyhow, Result};
use git2::Repository;
use std::fs;
use std::fs::create_dir_all;
use std::fs::copy;
use std::path::Path;
use regex::Regex;
use std::process::Command;
use glob::glob;
use reqwest::blocking::Client;
use serde_json::json;
use std::env;
use colored::*;
use std::io::{self, Write};
use cargo_metadata::MetadataCommand;

pub fn run(allow_dirty: bool) -> Result<()> {
    // Use cargo_metadata to find the workspace root
    let metadata = MetadataCommand::new().exec()?;
    let workspace_root = std::path::Path::new(metadata.workspace_root.as_str());
    std::env::set_current_dir(workspace_root)?;

    // 1. Generate LICENSE-3rdparty.csv using dd-rust-license-tool write (in src-tauri)
    let cwd = std::env::current_dir().unwrap();
    println!("[xtask] Running dd-rust-license-tool write from: {}", cwd.display());
    let mut cmd = Command::new("dd-rust-license-tool");
    cmd.arg("write").current_dir("src-tauri");
    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[xtask] dd-rust-license-tool write failed with status: {}", output.status);
        eprintln!("[xtask] stderr: {}", stderr);
        return Err(anyhow!("Failed to generate LICENSE-3rdparty.csv in src-tauri"));
    }
    println!("{}", "LICENSE-3rdparty.csv generated in src-tauri.".green().bold());

    // 1b. Check that the license file is up to date
    println!("[xtask] Running dd-rust-license-tool check from: {}", cwd.display());
    let mut cmd = Command::new("dd-rust-license-tool");
    cmd.arg("check").current_dir("src-tauri");
    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[xtask] dd-rust-license-tool check failed with status: {}", output.status);
        eprintln!("[xtask] stderr: {}", stderr);
        return Err(anyhow!("LICENSE-3rdparty.csv is not up to date. Rerun `dd-rust-license-tool write` and verify it."));
    }
    println!("{}", "LICENSE-3rdparty.csv is up to date.".green().bold());

    // 2. Commit LICENSE-3rdparty.csv as a separate commit, but only if it changed
    let repo = Repository::open(".")?;
    let license_path = Path::new("src-tauri/LICENSE-3rdparty.csv");
    let diff = repo.diff_index_to_workdir(None, None)?;
    let mut changed = false;
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            if path == license_path {
                changed = true;
                break;
            }
        }
    }
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
        println!("{}", "Committed LICENSE-3rdparty.csv as a separate commit.".green().bold());
    } else {
        println!("{}", "LICENSE-3rdparty.csv unchanged, skipping commit.".yellow().bold());
    }

    // 3. Check for uncommitted changes
    if !allow_dirty {
        let statuses = repo.statuses(None)?;
        if statuses.iter().any(|e| {
            let s = e.status();
            s.is_index_new() || s.is_index_modified() || s.is_wt_new() || s.is_wt_modified() || s.is_wt_deleted() || s.is_conflicted()
        }) {
            eprintln!("{}", "Uncommitted changes found. Please commit or stash them before releasing.".red().bold());
            return Err(anyhow!("Please commit or stash your changes before running the release script."));
        }
        println!("{}", "No uncommitted changes. Proceeding with release...".green().bold());
    } else {
        println!("{}", "Warning: Skipping uncommitted changes check (--allow-dirty set)".yellow().bold());
    }

    // --- Save previous version for rollback ---
    let cargo_toml_path = Path::new("src-tauri/Cargo.toml");
    let content = fs::read_to_string(cargo_toml_path)?;
    let re = Regex::new(r#"version\s*=\s*"(\d+)\.(\d+)\.(\d+)""#).unwrap();
    let caps = re.captures(&content).ok_or_else(|| anyhow!("Could not find version in Cargo.toml"))?;

    // 4. Bump version in src-tauri/Cargo.toml (patch bump)
    let new_version = format!("{}.{}.{}", caps[1].parse::<u64>()?, caps[2].parse::<u64>()?, caps[3].parse::<u64>()? + 1);
    let new_content = re.replace(&content, format!("version = \"{}\"", new_version));
    fs::write(cargo_toml_path, new_content.as_bytes())?;
    println!("{} {}", "Bumped version to".cyan(), new_version.cyan().bold());

    // 5. Commit and tag
    let mut index = repo.index()?;
    index.add_path(Path::new("src-tauri/Cargo.toml"))?;
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
    repo.tag(&format!("v{}", new_version), repo.head()?.peel_to_commit()?.as_object(), &signature, &msg, false)?;
    println!("{} v{}", "Committed and tagged".green(), new_version.green().bold());

    // 6. Build for all targets
    let targets = [
        ("x86_64-pc-windows-msvc", Some("cargo-xwin")),
        ("aarch64-pc-windows-msvc", Some("cargo-xwin")),
        ("aarch64-apple-darwin", None),
    ];
    for (target, runner) in targets.iter() {
        let mut cmd = Command::new("cargo");
        cmd.arg("tauri").arg("build");
        if let Some(runner) = runner {
            cmd.arg("--runner").arg(runner);
        }
        cmd.arg("--target").arg(target);
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow!("Build failed for target {}", target));
        }
        println!("{} {}", "Build succeeded for target".green(), target.green().bold());
    }

    // 7. Collect artifacts
    let release_dir = format!("release/{}", new_version);
    create_dir_all(&release_dir)?;
    let patterns = [
        format!("target/x86_64-pc-windows-msvc/release/*{}*", new_version),
        format!("target/aarch64-pc-windows-msvc/release/*{}*", new_version),
        format!("target/aarch64-apple-darwin/release/*{}*", new_version),
    ];
    for pattern in &patterns {
        for entry in glob(pattern)? {
            let path = entry?;
            if path.is_file() {
                let fname = path.file_name().unwrap();
                let dest = Path::new(&release_dir).join(fname);
                copy(&path, &dest)?;
                println!("{} {}", "Copied artifact:".cyan(), dest.display());
            }
        }
    }

    // 8. Create draft release on GitHub
    let dotenv_path = Path::new(".env");
    let mut github_token = None;
    if dotenv_path.exists() {
        let dotenv_content = fs::read_to_string(dotenv_path)?;
        for line in dotenv_content.lines() {
            if let Some(rest) = line.strip_prefix("GITHUB_TOKEN=") {
                if !rest.trim().is_empty() {
                    github_token = Some(rest.trim().to_string());
                    break;
                }
            }
        }
    }
    let github_token = match github_token {
        Some(token) => token,
        None => {
            println!("{}", "GITHUB_TOKEN not found in .env".yellow().bold());
            println!("Create a new token at: https://github.com/settings/tokens/new?scopes=repo&description=pdf-rancher-release");
            print!("Enter your GitHub token: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let token = input.trim().to_string();
            let mut dotenv_content = if dotenv_path.exists() {
                fs::read_to_string(dotenv_path)?
            } else {
                String::new()
            };
            if !dotenv_content.ends_with('\n') && !dotenv_content.is_empty() {
                dotenv_content.push('\n');
            }
            dotenv_content.push_str(&format!("GITHUB_TOKEN={}\n", token));
            fs::write(dotenv_path, dotenv_content)?;
            println!("{}", ".env updated with GITHUB_TOKEN".green());
            token
        }
    };
    let repo = env::var("GITHUB_REPO").expect("GITHUB_REPO env var not set (e.g. username/repo)");
    let client = Client::new();
    let release_resp = client.post(&format!("https://api.github.com/repos/{}/releases", repo))
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
    let upload_url = release_json["upload_url"].as_str().ok_or_else(|| anyhow!("No upload_url in GitHub response"))?.replace("{?name,label}", "");
    for entry in fs::read_dir(&release_dir)? {
        let path = entry?.path();
        if path.is_file() {
            let fname = path.file_name().unwrap().to_string_lossy();
            let url = format!("{}?name={}", upload_url, fname);
            let file_bytes = fs::read(&path)?;
            let resp = client.post(&url)
                .bearer_auth(&github_token)
                .header("Content-Type", "application/octet-stream")
                .header("User-Agent", "xtask-release-script")
                .body(file_bytes)
                .send()?;
            if !resp.status().is_success() {
                println!("{} {}: {}", "Failed to upload".red(), fname.red().bold(), resp.text()?);
            } else {
                println!("{} {}", "Uploaded artifact:".green(), fname.green().bold());
            }
        }
    }
    println!("{}", "Draft release created and artifacts uploaded.".green().bold());
    Ok(())
}
