use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{on_path, run};

const PROJECT_NAME: &str = "autd3-fpga";

#[derive(Subcommand)]
pub enum FpgaCmd {
    Project,

    Build {
        #[arg(long)]
        force: bool,
    },

    Flash {
        #[arg(long)]
        force: bool,
    },

    Clean,
}

pub fn run_fpga(root: &Path, cmd: &FpgaCmd) -> Result<()> {
    let fpga_dir = root.join("firmware/fpga");
    match cmd {
        FpgaCmd::Project => fpga_project(&fpga_dir),
        FpgaCmd::Build { force } => fpga_build(root, *force).map(|_| ()),
        FpgaCmd::Flash { force } => fpga_flash(root, *force),
        FpgaCmd::Clean => fpga_clean(&fpga_dir),
    }
}

fn fpga_project(fpga_dir: &Path) -> Result<()> {
    let vivado = resolve_vivado()?;
    run(
        &vivado,
        ["-mode", "batch", "-source", "proj_gen.tcl"],
        fpga_dir,
    )
}

pub fn fpga_build(root: &Path, force: bool) -> Result<PathBuf> {
    let fpga_dir = root.join("firmware/fpga");
    let vivado = resolve_vivado()?;

    if !fpga_dir.join(format!("{PROJECT_NAME}.xpr")).exists() {
        fpga_project(&fpga_dir)?;
    }

    let jobs = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut args = vec![
        "-mode".to_string(),
        "batch".to_string(),
        "-source".to_string(),
        "build.tcl".to_string(),
        "-tclargs".to_string(),
        jobs.to_string(),
    ];
    if force {
        args.push("force".to_string());
    }
    run(&vivado, args, &fpga_dir)?;

    let mcs = fpga_dir.join(format!("{PROJECT_NAME}.mcs"));
    if !mcs.is_file() {
        bail!("Vivado finished but {} was not created", mcs.display());
    }
    println!("fpga built: {}", mcs.display());
    Ok(mcs)
}

fn fpga_flash(root: &Path, force: bool) -> Result<()> {
    let mcs = fpga_build(root, force)?;
    let vivado = resolve_vivado()?;
    let fpga_dir = root.join("firmware/fpga");
    println!("Make sure the configuration cable is connected and the AUTD3 power is on.");
    run(
        &vivado,
        ["-mode", "batch", "-source", "configuration.tcl"],
        &fpga_dir,
    )
    .context("Vivado failed. Make sure the AUTD3 is connected and powered on.")?;
    println!("flash complete: {}", mcs.display());
    Ok(())
}

pub fn resolve_vivado() -> Result<String> {
    if on_path("vivado") {
        return Ok("vivado".to_string());
    }
    #[cfg(windows)]
    if let Some(path) = find_vivado_windows() {
        return Ok(path);
    }
    bail!(
        "Vivado not found. Put `vivado` on PATH (source Vivado's `settings64` script) \
         or install it so it can be auto-detected."
    );
}

#[cfg(windows)]
fn find_vivado_windows() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    const NEEDLES: [&str; 4] = [
        "Vivado",
        "Vitis",
        "Xilinx Design Tools FPGAs",
        "AMDDesignTools",
    ];

    let uninstall = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall")
        .ok()?;

    let mut install_location: Option<String> = None;
    for subkey_name in uninstall.enum_keys().flatten() {
        let Ok(subkey) = uninstall.open_subkey(&subkey_name) else {
            continue;
        };
        let display_name: String = subkey.get_value("DisplayName").unwrap_or_default();
        if NEEDLES.iter().any(|n| display_name.contains(n)) {
            if let Ok(loc) = subkey.get_value::<String, _>("InstallLocation") {
                install_location = Some(loc);
            }
        }
    }

    let install = PathBuf::from(install_location?);
    let vivado_dir = find_vivado_2025(&install).or_else(|| find_vivado_2024(&install))?;
    Some(
        vivado_dir
            .join("bin")
            .join("vivado.bat")
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(windows)]
fn find_vivado_2024(install: &Path) -> Option<PathBuf> {
    let vivado = install.join("Vivado");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(vivado)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.pop()
}

#[cfg(windows)]
fn find_vivado_2025(install: &Path) -> Option<PathBuf> {
    fn recurse(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > 2 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case("vivado"))
                {
                    out.push(path.clone());
                }
                recurse(&path, depth + 1, out);
            }
        }
    }
    let mut found = Vec::new();
    recurse(install, 0, &mut found);
    found.sort();
    found.pop()
}

fn fpga_clean(fpga_dir: &Path) -> Result<()> {
    const FILE_EXTS: [&str; 8] = ["jou", "log", "zip", "prm", "str", "pb", "mcs", "xpr"];

    let entries =
        std::fs::read_dir(fpga_dir).with_context(|| format!("reading {}", fpga_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let remove = if path.is_dir() {
            name == ".Xil" || name.starts_with("autd3-fpga.")
        } else {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| FILE_EXTS.contains(&e))
        };
        if remove {
            if path.is_dir() {
                std::fs::remove_dir_all(&path)
                    .with_context(|| format!("removing {}", path.display()))?;
            } else {
                std::fs::remove_file(&path)
                    .with_context(|| format!("removing {}", path.display()))?;
            }
            println!("removed {}", path.display());
        }
    }
    Ok(())
}
