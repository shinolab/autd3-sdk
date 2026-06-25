use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Subcommand;

use crate::util::run;

const SOLUTION: &str = "AUTD3.slnx";
const SOEM_CRATE: &str = "autd3-ffi-link-soem";

#[derive(Subcommand)]
pub enum CsCmd {
    Build {
        #[arg(long)]
        debug: bool,
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
        CsCmd::Test => {
            let native = build_ffi(root)?;
            let mut cmd = Command::new("dotnet");
            cmd.args(["test", SOLUTION, "-c", "Debug"])
                .current_dir(&dir);
            set_native_lib_path(&mut cmd, &native);
            spawn(cmd, "dotnet")
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

fn spawn(mut cmd: Command, program: &str) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn `{program}`"))?;
    if !status.success() {
        bail!("`{program}` exited with {status}");
    }
    Ok(())
}
