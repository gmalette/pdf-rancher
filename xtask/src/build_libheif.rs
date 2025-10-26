use anyhow::{anyhow, Context, Result};
use cargo_metadata::MetadataCommand;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug)]
pub enum HostTarget {
    MacAarch64,
    WindowsX86_64,
}

#[derive(Clone, Copy, Debug)]
enum Target {
    MacAarch64,
    WindowsX86_64Msvc,
    WindowsX86_64MinGW,
}

impl Target {
    fn triple(&self) -> &'static str {
        match self {
            Target::MacAarch64 => "aarch64-macos",
            Target::WindowsX86_64Msvc | Target::WindowsX86_64MinGW => "x86_64-windows",
        }
    }

    fn build_dir_suffix(&self) -> &'static str {
        match self {
            Target::MacAarch64 => "mac",
            Target::WindowsX86_64Msvc => "win",
            Target::WindowsX86_64MinGW => "win-x",
        }
    }

    fn is_windows(&self) -> bool {
        matches!(self, Target::WindowsX86_64Msvc | Target::WindowsX86_64MinGW)
    }

    fn shared_lib_extension(&self) -> &'static str {
        if self.is_windows() { "dll" } else { "dylib" }
    }
}

fn detect_host_target() -> Result<HostTarget> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    match (os, arch) {
        ("macos", "aarch64") => Ok(HostTarget::MacAarch64),
        ("windows", "x86_64") => Ok(HostTarget::WindowsX86_64),
        _ => Err(anyhow!(
            "Unsupported host: {}-{}. This xtask currently supports macOS aarch64 and Windows x86_64 only.",
            os, arch
        )),
    }
}

fn run_cmd(cmd: &mut Command) -> Result<()> {
    let display = format!("{:?}", cmd);
    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn: {}", display))?;
    if !status.success() {
        return Err(anyhow!("command failed ({}): {}", status, display));
    }
    Ok(())
}

fn ensure_dir(p: &Path) -> Result<()> {
    fs::create_dir_all(p).with_context(|| format!("create_dir_all {}", p.display()))
}

fn git_clone_or_fetch(repo: &str, dir: &Path, tag: &str) -> Result<()> {
    if dir.exists() {
        run_cmd(
            Command::new("git")
                .arg("-C")
                .arg(dir)
                .arg("fetch")
                .arg("--tags"),
        )?;
    } else {
        ensure_dir(dir.parent().unwrap_or(Path::new(".")))?;
        run_cmd(
            Command::new("git")
                .arg("clone")
                .arg("--depth")
                .arg("1")
                .arg("--branch")
                .arg(tag)
                .arg(repo)
                .arg(dir),
        )?;
    }
    run_cmd(
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .arg("checkout")
            .arg(tag),
    )?;
    Ok(())
}

fn cmake_build_release(
    src: &Path,
    build: &Path,
    generator: Option<&str>,
    arch_flag: Option<&str>,
    defs: &[(&str, &str)],
) -> Result<()> {
    ensure_dir(build)?;
    let mut cfg = Command::new("cmake");
    cfg.arg("-S")
        .arg(src)
        .arg("-B")
        .arg(build)
        .arg("-DCMAKE_POLICY_VERSION_MINIMUM=3.5");

    if let Some(gen) = generator {
        cfg.arg("-G").arg(gen);
    }
    if let Some(arch) = arch_flag {
        cfg.arg("-A").arg(arch);
    }

    for (k, v) in defs {
        cfg.arg(format!("-D{}={}", k, v));
    }
    run_cmd(&mut cfg)?;

    let mut build_cmd = Command::new("cmake");
    build_cmd
        .arg("--build")
        .arg(build)
        .arg("--config")
        .arg("Release")
        .arg("--");

    #[cfg(not(target_os = "windows"))]
    build_cmd.arg(format!("-j{}", num_cpus::get()));

    run_cmd(&mut build_cmd)
}

fn cmake_install(build: &Path) -> Result<()> {
    run_cmd(
        Command::new("cmake")
            .arg("--install")
            .arg(build)
            .arg("--config")
            .arg("Release"),
    )
}

fn workspace_root() -> Result<PathBuf> {
    let metadata = MetadataCommand::new().exec()?;
    Ok(metadata.workspace_root.as_std_path().to_path_buf())
}

struct BuildContext<'a> {
    workspace_root: &'a Path,
    build_root: &'a Path,
    src_libde265: &'a Path,
    src_libheif: &'a Path,
    libde265_tag: &'a str,
    common_de265_defs: &'a [(&'a str, &'a str)],
    common_heif_defs: &'a [(&'a str, &'a str)],
}

struct StagePaths {
    root: PathBuf,
    lib: PathBuf,
    include: PathBuf,
}

impl StagePaths {
    fn new(build_root: &Path, suffix: &str) -> Result<Self> {
        let root = build_root.join(format!("stage-{}", suffix));
        let lib = root.join("lib");
        let include = root.join("include");
        ensure_dir(&lib)?;
        ensure_dir(&include)?;
        Ok(Self { root, lib, include })
    }
}

fn copy_files_with_extension(from_dir: &Path, to_dir: &Path, extension: &str) -> Result<bool> {
    let mut copied = false;
    if from_dir.exists() {
        for entry in fs::read_dir(from_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                let file_name = path.file_name()
                    .ok_or_else(|| anyhow!("Invalid file name"))?;
                fs::copy(&path, to_dir.join(file_name))?;
                copied = true;
            }
        }
    }
    Ok(copied)
}

fn find_and_copy_file_recursive(
    search_dir: &Path,
    to_dir: &Path,
    filename: &str,
) -> Result<bool> {
    for entry in walkdir::WalkDir::new(search_dir) {
        let path = entry?.path().to_path_buf();
        if path.file_name().and_then(|n| n.to_str()) == Some(filename) {
            fs::copy(&path, to_dir.join(filename))?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn find_and_copy_by_pattern<F>(
    search_dir: &Path,
    to_dir: &Path,
    predicate: F,
) -> Result<bool>
where
    F: Fn(&Path) -> bool,
{
    for entry in walkdir::WalkDir::new(search_dir) {
        let path = entry?.path().to_path_buf();
        if predicate(&path) {
            if let Some(file_name) = path.file_name() {
                fs::copy(&path, to_dir.join(file_name))?;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn ensure_version_header(stage_inc: &Path, build_dir: &Path, libde265_tag: &str) -> Result<()> {
    let header_dir = stage_inc.join("libde265");
    let header_path = header_dir.join("de265-version.h");

    if !header_path.exists() {
        ensure_dir(&header_dir)?;

        // Try to find and copy from build directory
        if !find_and_copy_file_recursive(build_dir, &header_dir, "de265-version.h")? {
            // Synthesize header if not found
            synthesize_version_header(&header_path, libde265_tag)?;
        }
    }
    Ok(())
}

fn synthesize_version_header(path: &Path, tag: &str) -> Result<()> {
    ensure_dir(path.parent().unwrap())?;
    let v = tag.trim_start_matches('v');
    let mut parts = v.split('.');
    let (maj, min, pat) = (
        parts.next().unwrap_or("0"),
        parts.next().unwrap_or("0"),
        parts.next().unwrap_or("0"),
    );

    let content = format!(
        "#ifndef DE265_VERSION_H\n\
         #define DE265_VERSION_H\n\
         #define LIBDE265_VERSION_MAJOR {}\n\
         #define LIBDE265_VERSION_MINOR {}\n\
         #define LIBDE265_VERSION_PATCH {}\n\
         #define LIBDE265_VERSION \"{}\"\n\
         #endif\n",
        maj, min, pat, v
    );

    fs::File::create(path)?.write_all(content.as_bytes())?;
    Ok(())
}

fn copy_headers(src_dir: &Path, dest_dir: &Path) -> Result<()> {
    if src_dir.exists() {
        ensure_dir(dest_dir)?;
        for entry in fs::read_dir(src_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("h") {
                if let Some(file_name) = path.file_name() {
                    let _ = fs::copy(&path, dest_dir.join(file_name));
                }
            }
        }
    }
    Ok(())
}

fn build_libde265(
    ctx: &BuildContext,
    target: Target,
    stage: &StagePaths,
    generator: Option<&str>,
    arch_flag: Option<&str>,
    extra_defs: Vec<(&str, &str)>,
) -> Result<()> {
    let build_dir = ctx.build_root.join(format!("build-libde265-{}", target.build_dir_suffix()));

    let mut defs = ctx.common_de265_defs.to_vec();
    defs.push(("CMAKE_INSTALL_PREFIX", stage.root.to_str().unwrap_or("")));
    defs.extend(extra_defs);

    println!("[xtask] Building libde265 for {}...", target.triple());
    cmake_build_release(ctx.src_libde265, &build_dir, generator, arch_flag, &defs)?;
    cmake_install(&build_dir)?;

    // Copy built libraries
    let lib_subdir = build_dir.join("libde265");
    let extension = target.shared_lib_extension();

    if !copy_files_with_extension(&lib_subdir, &stage.lib, extension)? {
        // Fallback: search recursively
        find_and_copy_by_pattern(&build_dir, &stage.lib, |p| {
            p.extension().and_then(|e| e.to_str()) == Some(extension)
        })?;
    }

    // Copy headers
    let inc_src = ctx.src_libde265.join("libde265");
    if inc_src.join("de265.h").exists() {
        fs::copy(&inc_src.join("de265.h"), stage.include.join("de265.h"))?;
    }
    copy_headers(&inc_src, &stage.include.join("libde265"))?;
    ensure_version_header(&stage.include, &build_dir, ctx.libde265_tag)?;

    Ok(())
}

fn build_libheif(
    ctx: &BuildContext,
    target: Target,
    stage: &StagePaths,
    generator: Option<&str>,
    arch_flag: Option<&str>,
    extra_defs: Vec<(&str, &str)>,
) -> Result<()> {
    let build_dir = ctx.build_root.join(format!("build-libheif-{}", target.build_dir_suffix()));

    // Clean build directory for cross-compilation to avoid cache issues
    if matches!(target, Target::WindowsX86_64MinGW) && build_dir.exists() {
        let _ = fs::remove_dir_all(&build_dir);
    }

    let mut defs = ctx.common_heif_defs.to_vec();
    defs.push(("CMAKE_INSTALL_PREFIX", stage.root.to_str().unwrap_or("")));
    defs.extend(extra_defs);

    println!("[xtask] Building libheif for {}...", target.triple());
    cmake_build_release(ctx.src_libheif, &build_dir, generator, arch_flag, &defs)?;

    // Copy built libraries
    let extension = target.shared_lib_extension();
    let found = find_and_copy_by_pattern(&build_dir, &stage.lib, |p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("libheif") && n.ends_with(extension))
            .unwrap_or(false)
    })?;

    if !found {
        return Err(anyhow!("Could not find built libheif {}", extension));
    }

    Ok(())
}

fn copy_stage_to_frameworks(
    workspace_root: &Path,
    stage: &StagePaths,
    target: Target,
) -> Result<()> {
    let dest_dir = workspace_root.join("src-tauri/frameworks").join(target.triple());
    fs::create_dir_all(&dest_dir)?;

    let extension = target.shared_lib_extension();
    let copied_files: Vec<_> = fs::read_dir(&stage.lib)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(extension))
                .unwrap_or(false)
        })
        .collect();

    if copied_files.is_empty() {
        return Err(anyhow!(
            "No runtime shared libraries were produced to copy for {}.",
            target.triple()
        ));
    }

    for path in copied_files {
        if let Some(file_name) = path.file_name() {
            let file_name_str = file_name.to_str().unwrap_or("");

            // Determine the destination filename with version number
            let dest_name = if file_name_str.contains("libheif") {
                if target.is_windows() {
                    "libheif.dll".to_string()
                } else {
                    "libheif.1.dylib".to_string()
                }
            } else if file_name_str.contains("libde265") {
                if target.is_windows() {
                    "libde265.dll".to_string()
                } else {
                    "libde265.dylib".to_string()
                }
            } else {
                file_name_str.to_string()
            };

            let dest = dest_dir.join(&dest_name);
            println!("[xtask] Copy {} -> {}", path.display(), dest.display());
            fs::copy(&path, &dest)?;
        }
    }

    println!(
        "[xtask] libheif build complete for {}. Artifacts placed in {}",
        target.triple(),
        dest_dir.display()
    );
    Ok(())
}

struct MinGWToolchain {
    cc_path: String,
    cxx_path: String,
    rc_path: String,
    make_program: String,
    prefix: Option<String>,
}

impl MinGWToolchain {
    fn detect() -> Self {
        let cc = env::var("MINGW_CC").unwrap_or_else(|_| "x86_64-w64-mingw32-gcc".to_string());
        let cxx = env::var("MINGW_CXX").unwrap_or_else(|_| "x86_64-w64-mingw32-g++".to_string());
        let rc = env::var("MINGW_RC").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".to_string());
        let prefix = env::var("MINGW_PREFIX").ok();

        let cc_path = which::which(&cc)
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or(cc);
        let cxx_path = which::which(&cxx)
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or(cxx);
        let rc_path = which::which(&rc)
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or(rc);
        let make_program = env::var("MAKE")
            .ok()
            .or_else(|| which::which("make").ok().and_then(|p| p.to_str().map(String::from)))
            .unwrap_or_else(|| "make".to_string());

        Self {
            cc_path,
            cxx_path,
            rc_path,
            make_program,
            prefix,
        }
    }

    fn cmake_defs(&self) -> Vec<(&'static str, &str)> {
        let mut defs = vec![
            ("CMAKE_SYSTEM_NAME", "Windows"),
            ("CMAKE_C_COMPILER", self.cc_path.as_str()),
            ("CMAKE_CXX_COMPILER", self.cxx_path.as_str()),
            ("CMAKE_RC_COMPILER", self.rc_path.as_str()),
            ("CMAKE_MAKE_PROGRAM", self.make_program.as_str()),
        ];

        if let Some(ref prefix) = self.prefix {
            defs.push(("CMAKE_FIND_ROOT_PATH", prefix.as_str()));
        }

        defs
    }
}

fn build_macos_native(ctx: &BuildContext) -> Result<()> {
    let target = Target::MacAarch64;
    let stage = StagePaths::new(ctx.build_root, target.build_dir_suffix())?;

    build_libde265(ctx, target, &stage, None, None, vec![])?;
    build_libheif(ctx, target, &stage, None, None, vec![])?;
    copy_stage_to_frameworks(ctx.workspace_root, &stage, target)?;

    Ok(())
}

fn build_windows_msvc(ctx: &BuildContext) -> Result<()> {
    let target = Target::WindowsX86_64Msvc;
    let stage = StagePaths::new(ctx.build_root, target.build_dir_suffix())?;

    let generator = Some("Visual Studio 17 2022");
    let arch = Some("x64");

    build_libde265(ctx, target, &stage, generator, arch, vec![])?;

    // Copy additional artifacts for MSVC
    let build_de265 = ctx.build_root.join(format!("build-libde265-{}", target.build_dir_suffix()));
    if let Ok(entries) = fs::read_dir(build_de265.join("Release")) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ["dll", "lib"].contains(&ext) {
                    if let Some(file_name) = path.file_name() {
                        let _ = fs::copy(&path, stage.lib.join(file_name));
                    }
                }
            }
        }
    }

    build_libheif(ctx, target, &stage, generator, arch, vec![])?;

    // Copy additional artifacts for MSVC
    let build_heif = ctx.build_root.join(format!("build-libheif-{}", target.build_dir_suffix()));
    let release_dir = build_heif.join("Release");
    if release_dir.exists() {
        for entry in fs::read_dir(&release_dir)?.filter_map(Result::ok) {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ["dll", "lib"].contains(&ext) {
                    if let Some(file_name) = path.file_name() {
                        fs::copy(&path, stage.lib.join(file_name))?;
                    }
                }
            }
        }
    } else {
        return Err(anyhow!("Could not find built libheif (dll/lib) in Release"));
    }

    copy_stage_to_frameworks(ctx.workspace_root, &stage, target)?;
    Ok(())
}

fn build_windows_mingw_cross(ctx: &BuildContext) -> Result<()> {
    let target = Target::WindowsX86_64MinGW;
    let stage = StagePaths::new(ctx.build_root, target.build_dir_suffix())?;
    let toolchain = MinGWToolchain::detect();

    let generator = Some("Unix Makefiles");
    let mut extra_defs = toolchain.cmake_defs();

    build_libde265(ctx, target, &stage, generator, None, extra_defs.clone())?;

    // Copy additional MinGW artifacts
    let build_de265 = ctx.build_root.join(format!("build-libde265-{}", target.build_dir_suffix()));
    for entry in walkdir::WalkDir::new(&build_de265) {
        if let Ok(entry) = entry {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ["dll", "a", "lib"].contains(&ext) {
                    if let Some(file_name) = path.file_name() {
                        let _ = fs::copy(path, stage.lib.join(file_name));
                    }
                }
            }
        }
    }

    // Add cross-compilation specific settings for libheif
    extra_defs.extend([
        ("CMAKE_PREFIX_PATH", stage.root.to_str().unwrap_or("")),
        ("CMAKE_FIND_USE_SYSTEM_ENVIRONMENT_PATH", "OFF"),
        ("CMAKE_FIND_ROOT_PATH_MODE_PACKAGE", "ONLY"),
        ("CMAKE_FIND_ROOT_PATH_MODE_LIBRARY", "ONLY"),
        ("CMAKE_FIND_ROOT_PATH_MODE_INCLUDE", "ONLY"),
    ]);

    build_libheif(ctx, target, &stage, generator, None, extra_defs)?;
    copy_stage_to_frameworks(ctx.workspace_root, &stage, target)?;

    Ok(())
}

pub fn run(version: Option<String>, all_targets: bool) -> Result<()> {
    let workspace_root = workspace_root()?;
    env::set_current_dir(&workspace_root)?;

    let host = detect_host_target()?;
    let build_root = workspace_root.join("target/libheif-build");
    ensure_dir(&build_root)?;

    let libheif_tag = version.unwrap_or_else(|| "v1.17.6".to_string());
    let libde265_tag = env::var("LIBDE265_TAG").unwrap_or_else(|_| "v1.0.15".to_string());

    let src_libde265 = build_root.join("libde265");
    let src_libheif = build_root.join("libheif");

    println!("[xtask] Fetching libde265 ({})...", libde265_tag);
    git_clone_or_fetch(
        "https://github.com/strukturag/libde265.git",
        &src_libde265,
        &libde265_tag,
    )?;

    println!("[xtask] Fetching libheif ({})...", libheif_tag);
    git_clone_or_fetch(
        "https://github.com/strukturag/libheif.git",
        &src_libheif,
        &libheif_tag,
    )?;

    let common_de265_defs = vec![
        ("CMAKE_BUILD_TYPE", "Release"),
        ("BUILD_SHARED_LIBS", "ON"),
        ("DISABLE_SSE", "OFF"),
        ("ENABLE_DEC265", "OFF"),
        ("BUILD_DEC265", "OFF"),
        ("ENABLE_TOOLS", "OFF"),
        ("BUILD_TOOLS", "OFF"),
        ("WITH_SDL", "OFF"),
        ("WITH_SDL2", "OFF"),
        ("ENABLE_SDL", "OFF"),
        ("BUILD_TESTING", "OFF"),
    ];

    let common_heif_defs = vec![
        ("CMAKE_BUILD_TYPE", "Release"),
        ("BUILD_SHARED_LIBS", "ON"),
        ("WITH_DAV1D", "OFF"),
        ("WITH_AOM", "OFF"),
        ("WITH_X265", "OFF"),
        ("WITH_RAV1E", "OFF"),
        ("WITH_SvtEnc", "OFF"),
        ("WITH_GDK_PIXBUF", "OFF"),
        ("WITH_LIBDE265", "ON"),
        ("CMAKE_DISABLE_FIND_PACKAGE_AOM", "ON"),
        ("CMAKE_DISABLE_FIND_PACKAGE_LibAOM", "ON"),
        ("CMAKE_DISABLE_FIND_PACKAGE_SHARPYUV", "ON"),
        ("CMAKE_DISABLE_FIND_PACKAGE_sharpyuv", "ON"),
        ("WITH_AOM_DECODER", "OFF"),
        ("WITH_AOM_ENCODER", "OFF"),
        ("WITH_SHARPYUV", "OFF"),
        ("WITH_LIBSHARPYUV", "OFF"),
        ("WITH_JPEG_DECODER", "OFF"),
        ("WITH_JPEG_ENCODER", "OFF"),
        ("CMAKE_DISABLE_FIND_PACKAGE_JPEG", "ON"),
        ("WITH_PNG_DECODER", "OFF"),
        ("WITH_PNG_ENCODER", "OFF"),
        ("CMAKE_DISABLE_FIND_PACKAGE_PNG", "ON"),
        ("WITH_TIFF_DECODER", "OFF"),
        ("WITH_TIFF_ENCODER", "OFF"),
        ("CMAKE_DISABLE_FIND_PACKAGE_TIFF", "ON"),
        ("WITH_OPENJPEG_DECODER", "OFF"),
        ("WITH_OPENJPEG_ENCODER", "OFF"),
        ("CMAKE_DISABLE_FIND_PACKAGE_OpenJPEG", "ON"),
        ("BUILD_EXAMPLES", "OFF"),
        ("BUILD_TOOLS", "OFF"),
        ("BUILD_TESTING", "OFF"),
        ("CMAKE_DISABLE_FIND_PACKAGE_SDL2", "ON"),
        ("HEIF_ENABLE_EXAMPLES", "OFF"),
        ("HEIF_BUILD_EXAMPLES", "OFF"),
        ("LIBHEIF_BUILD_EXAMPLES", "OFF"),
        ("HEIF_BUILD_TOOLS", "OFF"),
        ("LIBHEIF_BUILD_TOOLS", "OFF"),
        ("HEIF_BUILD_TESTS", "OFF"),
        ("LIBHEIF_BUILD_TESTS", "OFF"),
    ];

    let ctx = BuildContext {
        workspace_root: &workspace_root,
        build_root: &build_root,
        src_libde265: &src_libde265,
        src_libheif: &src_libheif,
        libde265_tag: &libde265_tag,
        common_de265_defs: &common_de265_defs,
        common_heif_defs: &common_heif_defs,
    };

    // Build for the host's native target
    match host {
        HostTarget::MacAarch64 => build_macos_native(&ctx)?,
        HostTarget::WindowsX86_64 => build_windows_msvc(&ctx)?,
    }

    // If requested, build for additional targets
    if all_targets {
        match host {
            HostTarget::MacAarch64 => build_windows_mingw_cross(&ctx)?,
            HostTarget::WindowsX86_64 => {
                println!("[xtask] --all-targets requested but cross-compiling to macOS from Windows is not supported by this xtask. Skipping macOS build.");
            }
        }
    }

    Ok(())
}
