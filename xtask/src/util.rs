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

pub fn on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&paths).any(|dir| {
        exts.iter()
            .any(|ext| dir.join(format!("{name}{ext}")).is_file())
    })
}

pub fn run_built_bin(bin: &Path, args: &[String], no_sudo: bool, cwd: &Path) -> Result<()> {
    let bin_str = bin.to_string_lossy().into_owned();
    let use_sudo = !no_sudo && cfg!(target_os = "linux");
    if use_sudo {
        let mut sudo_args: Vec<String> = Vec::with_capacity(args.len() + 2);
        if let Ok(log) = std::env::var("RUST_LOG") {
            sudo_args.push(format!("RUST_LOG={log}"));
        }
        sudo_args.push(bin_str);
        sudo_args.extend(args.iter().cloned());
        run("sudo", sudo_args.iter().map(String::as_str), cwd)
    } else {
        run(&bin_str, args.iter().map(String::as_str), cwd)
    }
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

pub fn workspace_member_packages(workspace_dir: &Path) -> Result<Vec<String>> {
    let manifest = workspace_dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest.display()))?;
    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(toml_edit::Item::as_array)
        .with_context(|| format!("no [workspace] members in {}", manifest.display()))?;
    let mut names = Vec::new();
    for member in members {
        let member = member
            .as_str()
            .with_context(|| format!("non-string member in {}", manifest.display()))?;
        let member_manifest = workspace_dir.join(member).join("Cargo.toml");
        let member_text = std::fs::read_to_string(&member_manifest)
            .with_context(|| format!("failed to read {}", member_manifest.display()))?;
        let member_doc = member_text
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("failed to parse {}", member_manifest.display()))?;
        let name = member_doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml_edit::Item::as_str)
            .with_context(|| format!("no [package] name in {}", member_manifest.display()))?;
        names.push(name.to_string());
    }
    Ok(names)
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

pub fn macos_soem_excludes(crates: &[&'static str]) -> Vec<&'static str> {
    if cfg!(target_os = "macos") {
        crates.iter().flat_map(|c| ["--exclude", *c]).collect()
    } else {
        Vec::new()
    }
}
