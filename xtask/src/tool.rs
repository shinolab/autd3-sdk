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

    Doctor,

    InstallEsi,
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

    match cmd {
        TwincatCmd::InstallEsi => install_autd_xml(&dir, true),
        TwincatCmd::Doctor => diagnose_vbs(),
        TwincatCmd::Run { debug, args } => {
            let exe = ensure_built(&dir, debug)?;

            if let Err(e) = install_autd_xml(&dir, false) {
                eprintln!("warning: {e:#}");
            }
            run_cli(&exe, &dir, "run", &args)
        }
        TwincatCmd::Open { debug, args } => {
            let exe = ensure_built(&dir, debug)?;
            run_cli(&exe, &dir, "open", &args)
        }
    }
}

fn run_cli(exe: &Path, dir: &Path, sub: &str, args: &[String]) -> Result<()> {
    let cli_args = std::iter::once(sub).chain(args.iter().map(String::as_str));
    run(&exe.to_string_lossy(), cli_args, dir)
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

fn diagnose_vbs() -> Result<()> {
    const SCRIPT: &str = "\
try { $vbs = (Get-CimInstance -Namespace root\\Microsoft\\Windows\\DeviceGuard -ClassName Win32_DeviceGuard -ErrorAction Stop).VirtualizationBasedSecurityStatus } catch { $vbs = '' }; \
'VBS=' + $vbs; \
try { $mi = (Get-ItemProperty 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\DeviceGuard\\Scenarios\\HypervisorEnforcedCodeIntegrity' -Name Enabled -ErrorAction Stop).Enabled } catch { $mi = '' }; \
'MemIntegrity=' + $mi; \
try { $hv = (Get-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V-Hypervisor -ErrorAction Stop).State } catch { $hv = '' }; \
'HyperV=' + $hv; \
try { $vmp = (Get-WindowsOptionalFeature -Online -FeatureName VirtualMachinePlatform -ErrorAction Stop).State } catch { $vmp = '' }; \
'VMP=' + $vmp";

    let powershell = find_powershell();

    let output = Command::new(&powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .output()
        .with_context(|| format!("failed to spawn `{powershell}` (is PowerShell installed?)"))?;
    let out = String::from_utf8_lossy(&output.stdout);

    let mut values = std::collections::HashMap::new();
    for line in out.lines() {
        if let Some((k, v)) = line.split_once('=') {
            values.insert(k.trim(), v.trim().to_string());
        }
    }
    let get = |key: &str| values.get(key).map_or("", String::as_str);

    println!(
        "Virtualization-based security diagnosis (all should be OFF for TwinCAT real-time):\n"
    );

    report(
        "Virtualization-based security (VBS)",
        &match get("VBS") {
            "0" => Status::Off,
            "1" | "2" => Status::On,
            _ => Status::Unknown,
        },
    );

    report(
        "Core isolation / memory integrity (HVCI)",
        &match get("MemIntegrity") {
            "" | "0" => Status::Off,
            _ => Status::On,
        },
    );
    report("Hyper-V hypervisor", &feature_status(get("HyperV")));
    report("Virtual Machine Platform", &feature_status(get("VMP")));

    Ok(())
}

enum Status {
    Off,
    On,
    Unknown,
}

fn feature_status(state: &str) -> Status {
    match state {
        "Disabled" | "DisabledWithPayloadRemoved" => Status::Off,
        "" => Status::Unknown,
        _ => Status::On,
    }
}

fn report(label: &str, status: &Status) {
    let line = match status {
        Status::Off => format!("  OK       {label}: off"),
        Status::On => format!("  WARNING  {label}: enabled (disable it)"),
        Status::Unknown => format!("  ?        {label}: unknown (please run as admin)"),
    };
    println!("{line}");
}

fn install_autd_xml(dir: &Path, announce_noop: bool) -> Result<()> {
    let src = dir.join("AUTD.xml");
    if !src.is_file() {
        bail!("AUTD.xml not found next to twincat-cli: {}", src.display());
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(env_root) = std::env::var("TWINCAT3DIR")
        && !env_root.trim().is_empty()
    {
        roots.push(PathBuf::from(env_root));
    }
    roots.push(PathBuf::from(r"C:\TwinCAT\3.1"));
    roots.push(PathBuf::from(
        r"C:\Program Files (x86)\Beckhoff\TwinCAT\3.1",
    ));

    let mut dsts: Vec<PathBuf> = Vec::new();
    for root in roots {
        let dst = root.join(r"Config\Io\EtherCAT\AUTD.xml");
        if !dst.exists() && dst.parent().is_some_and(Path::exists) && !dsts.contains(&dst) {
            dsts.push(dst);
        }
    }

    if dsts.is_empty() {
        if announce_noop {
            println!("AUTD.xml already installed (or no TwinCAT EtherCAT config dir found)");
        }
        return Ok(());
    }

    let mut failed = false;
    for dst in &dsts {
        match std::fs::copy(&src, dst) {
            Ok(_) => println!("installed AUTD.xml -> {}", dst.display()),
            Err(e) => {
                failed = true;
                eprintln!("failed to copy AUTD.xml to {}: {e}", dst.display());
            }
        }
    }

    if failed {
        eprintln!(
            "\nre-run as Administrator, or manually copy AUTD.xml ({}) into:",
            src.display()
        );
        for dst in &dsts {
            if let Some(parent) = dst.parent() {
                eprintln!("    {}", parent.display());
            }
        }
        bail!("could not install AUTD.xml automatically (see manual steps above)");
    }
    Ok(())
}

fn find_powershell() -> String {
    if let Some(root) = std::env::var_os("SystemRoot") {
        let abs = Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        if abs.is_file() {
            return abs.to_string_lossy().into_owned();
        }
    }
    if on_path("powershell") {
        return "powershell".to_string();
    }
    if on_path("pwsh") {
        return "pwsh".to_string();
    }

    "powershell".to_string()
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
