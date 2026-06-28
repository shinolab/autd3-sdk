use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{on_path, run, run_built_bin};

#[derive(Subcommand)]
pub enum ToolCmd {
    Perftest {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        no_sudo: bool,
        #[arg(long)]
        mem_profile: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Patternsoak {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        no_sudo: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    Synctune {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        no_sudo: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    Twincat {
        #[command(subcommand)]
        cmd: TwincatCmd,
    },
}

#[derive(Subcommand)]
pub enum TwincatCmd {
    Run {
        #[arg(long)]
        debug: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    Open {
        #[arg(long)]
        debug: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    Doctor {
        #[arg(long)]
        debug: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    InstallEsi {
        #[arg(long)]
        debug: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn run_tool(root: &Path, cmd: ToolCmd) -> Result<()> {
    match cmd {
        ToolCmd::Perftest {
            debug,
            no_sudo,
            mem_profile,
            args,
        } => {
            let features: &[&str] = if mem_profile { &["mem-profile"] } else { &[] };
            run_bin(root, "autd3-rs-perftest", debug, no_sudo, features, &args)
        }
        ToolCmd::Patternsoak {
            debug,
            no_sudo,
            args,
        } => run_bin(root, "autd3-rs-patternsoak", debug, no_sudo, &[], &args),
        ToolCmd::Synctune {
            debug,
            no_sudo,
            args,
        } => run_bin(root, "autd3-rs-synctune", debug, no_sudo, &[], &args),
        ToolCmd::Twincat { cmd } => run_twincat(root, cmd),
    }
}

fn run_twincat(root: &Path, cmd: TwincatCmd) -> Result<()> {
    if !cfg!(target_os = "windows") {
        bail!(
            "`tool twincat` is Windows-only: twincat-cli targets .NET Framework 4.8 and \
             drives the TwinCAT XAE Shell through the DTE COM API"
        );
    }

    let dir = root.join("tools").join("twincat-cli");

    let (sub, debug, args) = match cmd {
        TwincatCmd::Run { debug, args } => ("run", debug, args),
        TwincatCmd::Open { debug, args } => ("open", debug, args),
        TwincatCmd::Doctor { debug, args } => ("doctor", debug, args),
        TwincatCmd::InstallEsi { debug, args } => ("install-esi", debug, args),
    };

    let exe = ensure_built(&dir, debug)?;
    run_cli(&exe, &dir, sub, &args)
}

fn run_cli(exe: &Path, dir: &Path, sub: &str, args: &[String]) -> Result<()> {
    let cli_args = std::iter::once(sub).chain(args.iter().map(String::as_str));
    run(&exe.to_string_lossy(), cli_args, dir)
}

pub fn build_twincat_cli(root: &Path, debug: bool) -> Result<PathBuf> {
    let exe = ensure_built(&root.join("tools").join("twincat-cli"), debug)?;
    if !exe.is_file() {
        bail!(
            "twincat-cli build did not produce {} (MSBuild ran but the merged exe is missing; \
             check the ILRepack step and that TwinCAT XAE is installed for the TCatSysManagerLib \
             COM reference)",
            exe.display()
        );
    }
    Ok(exe)
}

fn ensure_built(dir: &Path, debug: bool) -> Result<PathBuf> {
    let config = if debug { "Debug" } else { "Release" };
    let exe = dir
        .join("bin")
        .join(config)
        .join("net48")
        .join("dist")
        .join("twincat-cli.exe");

    if exe.is_file() && !is_stale(dir, &exe)? {
        return Ok(exe);
    }

    let msbuild = find_msbuild().context(
        "could not locate MSBuild.exe; install Visual Studio or Build Tools with the \
         \"MSBuild\" component (the TwinCAT XAE Shell install includes it)",
    )?;
    let msbuild = msbuild.to_string_lossy().into_owned();

    let config_arg = format!("-p:Configuration={config}");
    run(
        &msbuild,
        ["twincat-cli.csproj", "-nologo", "-restore", &config_arg],
        dir,
    )?;
    Ok(exe)
}

fn is_stale(dir: &Path, exe: &Path) -> Result<bool> {
    let exe_mtime = exe.metadata()?.modified()?;
    Ok(newest_source_mtime(dir)?.is_some_and(|m| m > exe_mtime))
}

fn newest_source_mtime(dir: &Path) -> Result<Option<std::time::SystemTime>> {
    let mut newest: Option<std::time::SystemTime> = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name();
            if matches!(name.to_str(), Some("bin" | "obj" | ".vs")) {
                continue;
            }
            if let Some(m) = newest_source_mtime(&path)? {
                newest = Some(newest.map_or(m, |n| n.max(m)));
            }
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("cs" | "csproj" | "config" | "xml" | "sln")
        ) {
            let m = entry.metadata()?.modified()?;
            newest = Some(newest.map_or(m, |n| n.max(m)));
        }
    }
    Ok(newest)
}

fn find_msbuild() -> Option<PathBuf> {
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        let vswhere = Path::new(&pf86).join(r"Microsoft Visual Studio\Installer\vswhere.exe");
        if vswhere.is_file()
            && let Ok(out) = Command::new(&vswhere)
                .args([
                    "-latest",
                    "-products",
                    "*",
                    "-requires",
                    "Microsoft.Component.MSBuild",
                    "-find",
                    r"MSBuild\**\Bin\MSBuild.exe",
                ])
                .output()
            && let Some(line) = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
        {
            let p = PathBuf::from(line);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    on_path("msbuild").then(|| PathBuf::from("msbuild"))
}

fn run_bin(
    root: &Path,
    pkg: &str,
    debug: bool,
    no_sudo: bool,
    features: &[&str],
    args: &[String],
) -> Result<()> {
    let mut build_args: Vec<&str> = vec!["build", "-p", pkg];
    if !debug {
        build_args.push("--release");
    }
    let features_arg = features.join(",");
    if !features.is_empty() {
        build_args.push("--features");
        build_args.push(&features_arg);
    }
    run("cargo", build_args, root)?;

    let profile = if debug { "debug" } else { "release" };
    let bin = root.join("target").join(profile).join(pkg);
    run_built_bin(&bin, args, no_sudo, root)
}
