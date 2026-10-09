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

pub fn files_under(dir: &Path, descend: impl Fn(&str) -> bool) -> Result<Vec<PathBuf>> {
    walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || descend(&entry.file_name().to_string_lossy())
        })
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_type().is_dir() => None,
            Ok(entry) => Some(Ok(entry.into_path())),
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<_, _>>()
        .with_context(|| format!("reading {}", dir.display()))
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

fn output(program: &str, args: &[&str], cwd: &Path) -> Result<std::process::Output> {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to spawn `{program}` (is it installed and on PATH?)"))
}

fn trimmed_stdout(program: &str, stdout: Vec<u8>) -> Result<String> {
    let stdout = String::from_utf8(stdout)
        .with_context(|| format!("`{program}` produced non-UTF-8 output"))?;
    Ok(stdout.trim().to_string())
}

pub fn capture(program: &str, args: &[&str], cwd: &Path) -> Result<String> {
    let output = output(program, args, cwd)?;
    if !output.status.success() {
        bail!("`{program}` exited with {}", output.status);
    }
    trimmed_stdout(program, output.stdout)
}

pub fn capture_lenient(program: &str, args: &[&str], cwd: &Path) -> Result<String> {
    trimmed_stdout(program, output(program, args, cwd)?.stdout)
}

pub fn package_version(manifest: &Path) -> Result<String> {
    let text = std::fs::read_to_string(manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    let doc: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("parsing {}", manifest.display()))?;
    doc.get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml_edit::Item::as_str)
        .map(str::to_string)
        .with_context(|| format!("no [package] version in {}", manifest.display()))
}

pub struct MemberPackage {
    name: String,
    version: String,
    publishable: bool,
    dir: PathBuf,
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
        dir: member_manifest
            .parent()
            .with_context(|| format!("{} has no parent", member_manifest.display()))?
            .to_path_buf(),
    })
}

pub fn publishable_members(workspace_dir: &Path) -> Result<Vec<MemberPackage>> {
    Ok(workspace_members(workspace_dir)?
        .into_iter()
        .filter(|package| package.publishable)
        .collect())
}

impl MemberPackage {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
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

pub fn cargo_fmt(dir: &Path, scope: &[&str], fix: bool) -> Result<()> {
    let mut args = vec!["fmt"];
    args.extend_from_slice(scope);
    if !fix {
        args.extend(["--", "--check"]);
    }
    run("cargo", args, dir)
}

pub fn cargo_fmt_packages(workspace_dir: &Path, fix: bool) -> Result<()> {
    let packages = workspace_member_packages(workspace_dir)?;
    let scope: Vec<&str> = packages
        .iter()
        .flat_map(|package| ["-p", package.as_str()])
        .collect();
    cargo_fmt(workspace_dir, &scope, fix)
}

pub fn cargo_clippy(dir: &Path, args: &[&str]) -> Result<()> {
    let mut full = vec!["clippy"];
    full.extend_from_slice(args);
    full.extend(["--", "-D", "warnings"]);
    run("cargo", full, dir)
}

pub fn run<I, S>(program: &str, args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_env(program, args, cwd, &[])
}

pub fn run_env<I, S>(program: &str, args: I, cwd: &Path, env: &[(&str, &OsStr)]) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    for (key, value) in env {
        command.env(key, value);
    }
    let status = command
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

pub fn host_triple() -> Result<String> {
    let verbose = capture("rustc", &["-vV"], &workspace_root())?;
    verbose
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_string)
        .context("`rustc -vV` reported no host triple")
}
