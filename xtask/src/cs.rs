use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::run;

const SOLUTION: &str = "AUTD3.slnx";
const SOEM_CRATE: &str = "autd3-ffi-link-soem";

const CS_NATIVE: &[(&str, &str)] = &[
    ("AUTD3.Core", "autd3_core"),
    ("AUTD3", "autd3capi"),
    ("AUTD3.Pattern", "autd3_pattern"),
    ("AUTD3.Pattern.Holo", "autd3_pattern_holo"),
    ("AUTD3.Modulation", "autd3_modulation"),
    ("AUTD3.Link.Ethercrab", "autd3_link_ethercrab"),
    ("AUTD3.Link.Remote", "autd3_link_remote"),
    ("AUTD3.Link.Twincat", "autd3_link_twincat"),
    ("AUTD3.Link.Nop", "autd3_link_nop"),
    ("AUTD3.Link.Soem", "autd3_link_soem"),
];

const RIDS: &[&str] = &["win-x64", "linux-x64", "osx-arm64"];

#[derive(Subcommand)]
pub enum CsCmd {
    Build {
        #[arg(long)]
        debug: bool,
    },
    Pack {
        #[arg(long)]
        native_dir: Option<PathBuf>,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    Test,
    Format {
        #[arg(long)]
        fix: bool,
    },
    Example {
        name: String,
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        no_sudo: bool,
    },
}

pub fn run_cs(root: &Path, cmd: CsCmd) -> Result<()> {
    let dir = root.join("bindings").join("csharp");
    match cmd {
        CsCmd::Build { debug } => {
            let config = if debug { "Debug" } else { "Release" };
            run("dotnet", ["build", SOLUTION, "-c", config], &dir)
        }
        CsCmd::Pack { native_dir, out } => pack(root, native_dir, out),
        CsCmd::Test => {
            let native = build_ffi(root)?;
            if cfg!(target_os = "windows") {
                run("dotnet", ["build", SOLUTION, "-c", "Debug"], &dir)?;
                stage_native_libs(&native, &dir)?;
                run(
                    "dotnet",
                    ["test", SOLUTION, "-c", "Debug", "--no-build"],
                    &dir,
                )
            } else {
                let mut cmd = Command::new("dotnet");
                cmd.args(["test", SOLUTION, "-c", "Debug"])
                    .current_dir(&dir);
                set_native_lib_path(&mut cmd, &native);
                spawn(cmd, "dotnet")
            }
        }
        CsCmd::Format { fix } => {
            let mut args = vec!["format", SOLUTION];
            if !fix {
                args.push("--verify-no-changes");
            }
            run("dotnet", args, &dir)
        }
        CsCmd::Example {
            name,
            debug,
            no_sudo,
        } => {
            let native = build_ffi(root)?;
            let config = if debug { "Debug" } else { "Release" };
            let project_dir = dir.join("examples").join(&name);
            let project = project_dir.join(format!("{name}.csproj"));
            if !project.is_file() {
                bail!("example not found: {}", project.display());
            }
            run(
                "dotnet",
                ["build", &project.to_string_lossy(), "-c", config],
                &dir,
            )?;
            let exe = find_example_exe(&project_dir, config, &name)?;
            run_example(&exe, &native, no_sudo, &dir)
        }
    }
}

fn pack(root: &Path, native_dir: Option<PathBuf>, out: Option<PathBuf>) -> Result<()> {
    let dir = root.join("bindings").join("csharp");
    let ffi = root.join("bindings").join("ffi");
    let native_root = native_dir.unwrap_or_else(|| ffi.join("target").join("native"));

    let present: Vec<&str> = RIDS
        .iter()
        .copied()
        .filter(|rid| native_root.join(rid).is_dir())
        .collect();
    let host = if present.is_empty() {
        run("cargo", ["build", "--workspace", "--release"], &ffi)?;
        Some((host_rid()?, ffi.join("target").join("release")))
    } else {
        None
    };

    let out = out.unwrap_or_else(|| dir.join("dist"));
    std::fs::create_dir_all(&out)?;
    let src = dir.join("src");

    for (pkg, lib) in CS_NATIVE {
        let pkg_dir = src.join(pkg);
        let runtimes = pkg_dir.join("runtimes");
        if runtimes.exists() {
            std::fs::remove_dir_all(&runtimes)
                .with_context(|| format!("clearing {}", runtimes.display()))?;
        }
        match &host {
            Some((rid, from)) => stage_native(from, rid, lib, &runtimes)?,
            None => {
                for rid in &present {
                    stage_native(&native_root.join(rid), rid, lib, &runtimes)?;
                }
            }
        }
        let proj = pkg_dir.join(format!("{pkg}.csproj"));
        run(
            "dotnet",
            [
                "pack",
                &proj.to_string_lossy(),
                "-c",
                "Release",
                "-o",
                &out.to_string_lossy(),
            ],
            &dir,
        )?;
    }
    println!("cs pack: nupkg written to {}", out.display());
    Ok(())
}

fn stage_native(from: &Path, rid: &str, lib: &str, runtimes: &Path) -> Result<()> {
    let (prefix, ext) = rid_affix(rid);
    let file = format!("{prefix}{lib}.{ext}");
    let src = from.join(&file);
    if !src.is_file() {
        bail!("native lib not found: {}", src.display());
    }
    let dst_dir = runtimes.join(rid).join("native");
    std::fs::create_dir_all(&dst_dir)?;
    std::fs::copy(&src, dst_dir.join(&file))
        .with_context(|| format!("staging {} -> {}", src.display(), dst_dir.display()))?;
    Ok(())
}

fn rid_affix(rid: &str) -> (&'static str, &'static str) {
    if rid.starts_with("win") {
        ("", "dll")
    } else if rid.starts_with("osx") {
        ("lib", "dylib")
    } else {
        ("lib", "so")
    }
}

fn host_rid() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("windows", "x86_64") => "win-x64",
        ("macos", "aarch64") => "osx-arm64",
        ("macos", "x86_64") => "osx-x64",
        (os, arch) => bail!("unsupported host {os}/{arch} for `cs pack`"),
    })
}

fn build_ffi(root: &Path) -> Result<PathBuf> {
    let ffi = root.join("bindings").join("ffi");
    run(
        "cargo",
        ["build", "--workspace", "--exclude", SOEM_CRATE, "--release"],
        &ffi,
    )?;
    Ok(ffi.join("target").join("release"))
}

fn find_example_exe(project_dir: &Path, config: &str, name: &str) -> Result<PathBuf> {
    let bin = project_dir.join("bin").join(config);
    if let Ok(entries) = std::fs::read_dir(&bin) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    bail!("built example executable not found under {}", bin.display());
}

fn run_example(exe: &Path, native: &Path, no_sudo: bool, cwd: &Path) -> Result<()> {
    let exe = exe.to_string_lossy().into_owned();
    let native = native.to_string_lossy().into_owned();
    if !no_sudo && cfg!(target_os = "linux") {
        let args = [format!("LD_LIBRARY_PATH={native}"), exe];
        run("sudo", args.iter().map(String::as_str), cwd)
    } else {
        let mut cmd = Command::new(&exe);
        cmd.current_dir(cwd).env("LD_LIBRARY_PATH", &native);
        spawn(cmd, "example")
    }
}

fn set_native_lib_path(cmd: &mut Command, native: &Path) {
    if cfg!(target_os = "windows") {
        let existing = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{};{existing}", native.display()));
    } else if cfg!(target_os = "macos") {
        cmd.env("DYLD_LIBRARY_PATH", native);
    } else {
        cmd.env("LD_LIBRARY_PATH", native);
    }
}

fn stage_native_libs(native: &Path, csharp_dir: &Path) -> Result<()> {
    let test_bin = csharp_dir.join("tests/AUTD3.Tests/bin/Debug");
    let ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let mut staged = 0;
    for tfm in std::fs::read_dir(&test_bin)
        .with_context(|| format!("reading test output dir {}", test_bin.display()))?
    {
        let tfm = tfm?.path();
        if !tfm.is_dir() {
            continue;
        }
        for lib in std::fs::read_dir(native)? {
            let lib = lib?.path();
            if lib.extension().and_then(|e| e.to_str()) == Some(ext) {
                std::fs::copy(&lib, tfm.join(lib.file_name().unwrap()))?;
                staged += 1;
            }
        }
    }
    if staged == 0 {
        bail!(
            "no native libraries staged from {} into {}",
            native.display(),
            test_bin.display()
        );
    }
    Ok(())
}

fn spawn(mut cmd: Command, program: &str) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn `{program}`"))?;
    if !status.success() {
        bail!("`{program}` exited with {status}");
    }
    Ok(())
}
