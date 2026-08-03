use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate lives one directory below the workspace root")
        .to_path_buf()
}

pub fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub fn dist_target() -> Option<String> {
    std::env::var("CARGO_DIST_TARGET")
        .ok()
        .filter(|target| !target.is_empty())
}

pub fn ensure_rust_target(target: &str) -> Result<()> {
    run("rustup", ["target", "add", target], &workspace_root())
}

pub fn cargo_bin(workspace: &Path, target: Option<&str>, debug: bool, name: &str) -> PathBuf {
    let mut dir = workspace.join("target");
    if let Some(target) = target {
        dir = dir.join(target);
    }
    dir.join(if debug { "debug" } else { "release" })
        .join(exe_name(name))
}

pub fn run_cargo<I, S>(args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("cargo");
    command.args(args).current_dir(cwd);
    for var in ["CC", "CXX"] {
        if std::env::var(var).as_deref() == Ok("cl.exe") {
            command.env_remove(var);
        }
    }
    let status = command
        .status()
        .context("failed to spawn `cargo` (is it installed and on PATH?)")?;
    if !status.success() {
        bail!("`cargo` exited with {status}");
    }
    Ok(())
}

pub fn cargo_build_args<'a>(
    package: &'a str,
    target: Option<&'a str>,
    debug: bool,
) -> Vec<&'a str> {
    let mut args = vec!["build", "-p", package];
    if !debug {
        args.push("--release");
    }
    if let Some(target) = target {
        args.push("--target");
        args.push(target);
    }
    args
}

pub fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)
        .with_context(|| format!("copying {} -> {}", src.display(), dst.display()))?;
    Ok(())
}

pub fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            copy_file(&path, &target)?;
        }
    }
    Ok(())
}

pub fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&paths).find_map(|dir| {
        exts.iter()
            .map(|ext| dir.join(format!("{name}{ext}")))
            .find(|path| path.is_file())
    })
}

pub fn on_path(name: &str) -> bool {
    which(name).is_some()
}

#[cfg(target_os = "linux")]
fn setcap_program() -> Option<String> {
    if on_path("setcap") {
        return Some("setcap".to_owned());
    }
    ["/usr/bin/setcap", "/usr/sbin/setcap", "/sbin/setcap"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .map(str::to_owned)
}

const RUN_CAPABILITIES: &str = "cap_net_raw,cap_net_admin,cap_sys_nice+ep";

#[cfg(target_os = "linux")]
fn grant_capabilities(bin: &Path) -> bool {
    let Some(setcap) = setcap_program() else {
        return false;
    };
    Command::new("sudo")
        .args(["-n", &setcap, RUN_CAPABILITIES])
        .arg(bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(target_os = "linux"))]
fn grant_capabilities(_bin: &Path) -> bool {
    false
}

pub fn run_built_bin(bin: &Path, args: &[String], no_sudo: bool, cwd: &Path) -> Result<()> {
    let bin_str = bin.to_string_lossy().into_owned();
    if no_sudo || !cfg!(unix) {
        return run(&bin_str, args.iter().map(String::as_str), cwd);
    }
    if grant_capabilities(bin) {
        println!("granted {RUN_CAPABILITIES} to {bin_str}; running without sudo");
        return run(&bin_str, args.iter().map(String::as_str), cwd);
    }
    let mut sudo_args: Vec<String> = Vec::with_capacity(args.len() + 2);
    if let Ok(log) = std::env::var("RUST_LOG") {
        sudo_args.push(format!("RUST_LOG={log}"));
    }
    sudo_args.push(bin_str);
    sudo_args.extend(args.iter().cloned());
    run("sudo", sudo_args.iter().map(String::as_str), cwd)
}

pub fn capture(program: &str, args: &[&str], cwd: &Path) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to spawn `{program}` (is it installed and on PATH?)"))?;
    if !output.status.success() {
        bail!("`{program}` exited with {}", output.status);
    }
    let stdout = String::from_utf8(output.stdout)
        .with_context(|| format!("`{program}` produced non-UTF-8 output"))?;
    Ok(stdout.trim().to_string())
}

pub fn capture_lenient(program: &str, args: &[&str], cwd: &Path) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to spawn `{program}` (is it installed and on PATH?)"))?;
    let stdout = String::from_utf8(output.stdout)
        .with_context(|| format!("`{program}` produced non-UTF-8 output"))?;
    Ok(stdout.trim().to_string())
}

struct MemberPackage {
    name: String,
    version: String,
    publishable: bool,
}

fn read_member_package(
    member_manifest: &Path,
    workspace_manifest: &Path,
    inherited_version: Option<&str>,
) -> Result<MemberPackage> {
    let member_text = std::fs::read_to_string(member_manifest)
        .with_context(|| format!("failed to read {}", member_manifest.display()))?;
    let member_doc = member_text
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("failed to parse {}", member_manifest.display()))?;
    let package = member_doc
        .get("package")
        .with_context(|| format!("no [package] in {}", member_manifest.display()))?;
    let name = package
        .get("name")
        .and_then(toml_edit::Item::as_str)
        .with_context(|| format!("no [package] name in {}", member_manifest.display()))?;
    let version = match package.get("version").and_then(toml_edit::Item::as_str) {
        Some(version) => version.to_string(),
        None => inherited_version
            .with_context(|| {
                format!(
                    "{} inherits its version but {} has no [workspace.package] version",
                    member_manifest.display(),
                    workspace_manifest.display()
                )
            })?
            .to_string(),
    };
    let publishable = match package.get("publish") {
        None => true,
        Some(item) if item.as_bool() == Some(true) => true,
        Some(item) => item
            .as_array()
            .is_some_and(|r| r.iter().any(|v| v.as_str() == Some("crates-io"))),
    };
    Ok(MemberPackage {
        name: name.to_string(),
        version,
        publishable,
    })
}

fn workspace_members(workspace_dir: &Path) -> Result<Vec<MemberPackage>> {
    let manifest = workspace_dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest.display()))?;
    let inherited_version = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(toml_edit::Item::as_str)
        .map(str::to_string);
    let Some(members) = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(toml_edit::Item::as_array)
    else {
        return Ok(vec![read_member_package(
            &manifest,
            &manifest,
            inherited_version.as_deref(),
        )?]);
    };
    let mut packages = Vec::new();
    if doc.contains_key("package") {
        packages.push(read_member_package(
            &manifest,
            &manifest,
            inherited_version.as_deref(),
        )?);
    }
    for member in members {
        let member = member
            .as_str()
            .with_context(|| format!("non-string member in {}", manifest.display()))?;
        let member_manifest = workspace_dir.join(member).join("Cargo.toml");
        packages.push(read_member_package(
            &member_manifest,
            &manifest,
            inherited_version.as_deref(),
        )?);
    }
    Ok(packages)
}

fn workspace_member_packages(workspace_dir: &Path) -> Result<Vec<String>> {
    Ok(workspace_members(workspace_dir)?
        .into_iter()
        .map(|package| package.name)
        .collect())
}

fn is_published(package: &MemberPackage, cwd: &Path) -> Result<bool> {
    let spec = format!("{}@{}", package.name, package.version);
    let status = Command::new("cargo")
        .args(["info", &spec, "--registry", "crates-io"])
        .current_dir(cwd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to spawn `cargo` (is it installed and on PATH?)")?;
    Ok(status.success())
}

pub fn publish_workspace(workspace_dir: &Path, dry_run: bool) -> Result<()> {
    let mut args = vec![
        "publish".to_string(),
        "--workspace".to_string(),
        "--no-verify".to_string(),
    ];
    let mut pending = Vec::new();
    for package in workspace_members(workspace_dir)?
        .iter()
        .filter(|package| package.publishable)
    {
        if is_published(package, workspace_dir)? {
            println!(
                "skipping {} v{} (already on crates.io)",
                package.name, package.version
            );
            args.push("--exclude".to_string());
            args.push(package.name.clone());
        } else {
            pending.push(format!("{} v{}", package.name, package.version));
        }
    }
    if pending.is_empty() {
        println!("nothing to publish; every publishable package is already on crates.io");
        return Ok(());
    }
    println!("publishing {}", pending.join(", "));
    if dry_run {
        args.push("--dry-run".to_string());
    }
    run("cargo", args, workspace_dir)
}

pub fn cargo_fmt_packages(workspace_dir: &Path, fix: bool) -> Result<()> {
    let packages = workspace_member_packages(workspace_dir)?;
    let mut args = vec!["fmt".to_string()];
    for package in &packages {
        args.push("-p".to_string());
        args.push(package.clone());
    }
    if !fix {
        args.push("--".to_string());
        args.push("--check".to_string());
    }
    run("cargo", args, workspace_dir)
}

pub fn run<I, S>(program: &str, args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to spawn `{program}` (is it installed and on PATH?)"))?;
    if !status.success() {
        bail!("`{program}` exited with {status}");
    }
    Ok(())
}

pub fn run_tool<I, S>(program: &str, args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if cfg!(windows) {
        let mut full: Vec<std::ffi::OsString> = vec!["/C".into(), program.into()];
        full.extend(args.into_iter().map(|a| a.as_ref().to_os_string()));
        run("cmd", full, cwd)
    } else {
        run(program, args, cwd)
    }
}
