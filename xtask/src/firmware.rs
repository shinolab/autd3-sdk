use std::io::{Read, Write};
#[cfg(windows)]
use std::path::PathBuf as WinPathBuf;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};

use crate::fpga::resolve_vivado;
use crate::util::{on_path, run};

#[derive(Subcommand)]
pub enum FirmwareCmd {
    Write(WriteArgs),
}

#[derive(clap::Args)]
pub struct WriteArgs {
    #[arg(long)]
    version: String,

    #[arg(long, value_enum)]
    target: Option<Target>,

    #[arg(long)]
    force_download: bool,
}

#[derive(Copy, Clone, ValueEnum)]
enum Target {
    Both,
    Fpga,
    Cpu,
}

pub fn run_firmware(_root: &Path, cmd: FirmwareCmd) -> Result<()> {
    match cmd {
        FirmwareCmd::Write(args) => write(&args),
    }
}

fn write(args: &WriteArgs) -> Result<()> {
    let version = args.version.trim_start_matches('v');

    let dir = download_and_extract(version, args.force_download)?;
    let (cpu, fpga) = find_firmwares(&dir)?;

    println!("Found firmwares:");
    match &cpu {
        Some(p) => println!("  CPU : {}", p.display()),
        None => println!("  CPU : (none)"),
    }
    match &fpga {
        Some(p) => println!("  FPGA: {}", p.display()),
        None => println!("  FPGA: (none)"),
    }
    println!("Make sure the configuration cables are connected and the AUTD3 power is on.");

    let target = match args.target {
        Some(t) => t,
        None => prompt_target()?,
    };

    match target {
        Target::Both => {
            update_cpu(cpu.as_deref())?;
            update_fpga(fpga.as_deref())?;
        }
        Target::Fpga => update_fpga(fpga.as_deref())?,
        Target::Cpu => update_cpu(cpu.as_deref())?,
    }

    println!("Done. Power-cycle the AUTD3 to load the new firmware.");
    Ok(())
}

fn download_and_extract(version: &str, force: bool) -> Result<PathBuf> {
    let dest = std::env::temp_dir().join(format!("autd3-sdk-firmware-v{version}"));
    if dest.is_dir() && !force {
        println!("Using cached firmware at {}", dest.display());
        return Ok(dest);
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest)
            .with_context(|| format!("removing stale cache {}", dest.display()))?;
    }

    let url = format!(
        "https://github.com/shinolab/autd3-sdk/releases/download/firmware-v{version}/autd3-sdk-firmware-v{version}.zip"
    );
    println!("Downloading {url}");
    let resp = ureq::get(&url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let mut bytes = Vec::new();
    resp.into_body()
        .into_reader()
        .read_to_end(&mut bytes)
        .context("reading firmware bundle")?;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .context("opening firmware zip (was the version correct?)")?;
    archive
        .extract(&dest)
        .with_context(|| format!("extracting firmware to {}", dest.display()))?;
    Ok(dest)
}

fn find_firmwares(dir: &Path) -> Result<(Option<PathBuf>, Option<PathBuf>)> {
    let mut files = Vec::new();
    collect_files(dir, &mut files)?;

    let mut cpu = None;
    let mut fpga = None;
    for f in files {
        match f.extension().and_then(|e| e.to_str()) {
            Some("bin") => cpu = Some(f),
            Some("mcs") => fpga = Some(f),
            _ => {}
        }
    }
    if cpu.is_none() && fpga.is_none() {
        bail!(
            "no firmware images (*.bin / *.mcs) found in {}",
            dir.display()
        );
    }
    Ok((cpu, fpga))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

fn prompt_target() -> Result<Target> {
    println!("Select which firmware to update:");
    println!("  [0]: Both (default)");
    println!("  [1]: FPGA");
    println!("  [2]: CPU");
    loop {
        print!("Select: ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            bail!("no input (use --target to run non-interactively)");
        }
        match line.trim() {
            "" | "0" => return Ok(Target::Both),
            "1" => return Ok(Target::Fpga),
            "2" => return Ok(Target::Cpu),
            other => println!("invalid selection: {other:?}"),
        }
    }
}

fn update_cpu(firmware: Option<&Path>) -> Result<()> {
    let firmware = firmware.context("CPU firmware (*.bin) not found in the bundle")?;
    let jlink = resolve_jlink()?;

    let script = jlink_script(firmware);
    let script_path = std::env::temp_dir().join("autd3-cpu-flash.jlink");
    std::fs::write(&script_path, script)
        .with_context(|| format!("writing {}", script_path.display()))?;

    println!("Flashing CPU via {jlink}...");
    run(
        &jlink,
        [
            "-device",
            "R7S910018_R4F",
            "-if",
            "JTAG",
            "-speed",
            "4000",
            "-jtagconf",
            "-1,-1",
            "-autoconnect",
            "1",
            "-ExitOnError",
            "1",
            "-CommanderScript",
            &script_path.to_string_lossy(),
        ],
        &std::env::temp_dir(),
    )
    .context("J-Link failed. Make sure the AUTD3 is connected and powered on.")?;
    println!("CPU update done.");
    Ok(())
}

fn update_fpga(firmware: Option<&Path>) -> Result<()> {
    let firmware = firmware.context("FPGA firmware (*.mcs) not found in the bundle")?;
    let vivado = if on_path("vivado_lab") {
        "vivado_lab".to_string()
    } else {
        resolve_vivado()?
    };

    let script = fpga_script(firmware);
    let script_path = std::env::temp_dir().join("autd3-fpga-configuration.tcl");
    std::fs::write(&script_path, script)
        .with_context(|| format!("writing {}", script_path.display()))?;

    println!("Flashing FPGA via {vivado}...");
    run(
        &vivado,
        [
            "-mode",
            "batch",
            "-nojournal",
            "-nolog",
            "-notrace",
            "-source",
            &script_path.to_string_lossy(),
        ],
        &std::env::temp_dir(),
    )
    .context("Vivado failed. Make sure the AUTD3 is connected and powered on.")?;
    println!("FPGA update done.");
    Ok(())
}

fn jlink_script(bin: &Path) -> String {
    format!("r\nloadfile {} 0x30000000\nq\n", tcl_path(bin))
}

fn fpga_script(mcs: &Path) -> String {
    const DEV: &str = "xc7a200t_0";
    let mcs = tcl_path(mcs);
    format!(
        "open_hw_manager
connect_hw_server -allow_non_jtag
open_hw_target
current_hw_device [get_hw_devices {DEV}]
refresh_hw_device -update_hw_probes false [lindex [get_hw_devices {DEV}] 0]
create_hw_cfgmem -hw_device [lindex [get_hw_devices {DEV}] 0] [lindex [get_cfgmem_parts {{mt25ql128-spi-x1_x2_x4}}] 0]
set_property PROGRAM.BLANK_CHECK 0 [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
set_property PROGRAM.ERASE 1 [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
set_property PROGRAM.CFG_PROGRAM 1 [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
set_property PROGRAM.VERIFY 1 [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
set_property PROGRAM.CHECKSUM 0 [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
refresh_hw_device [lindex [get_hw_devices {DEV}] 0]
set_property PROGRAM.ADDRESS_RANGE {{use_file}} [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
set_property PROGRAM.FILES [list {{{mcs}}}] [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
set_property PROGRAM.PRM_FILE {{}} [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
set_property PROGRAM.UNUSED_PIN_TERMINATION {{pull-none}} [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
startgroup
create_hw_bitstream -hw_device [lindex [get_hw_devices {DEV}] 0] [get_property PROGRAM.HW_CFGMEM_BITFILE [lindex [get_hw_devices {DEV}] 0]]; program_hw_devices [lindex [get_hw_devices {DEV}] 0]; refresh_hw_device [lindex [get_hw_devices {DEV}] 0];
program_hw_cfgmem -hw_cfgmem [get_property PROGRAM.HW_CFGMEM [lindex [get_hw_devices {DEV}] 0]]
endgroup
close_hw_manager
"
    )
}

fn tcl_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn resolve_jlink() -> Result<String> {
    let candidates = if cfg!(windows) {
        ["JLink", "jlink"].as_slice()
    } else {
        ["JLinkExe"].as_slice()
    };
    for name in candidates {
        if on_path(name) {
            return Ok((*name).to_string());
        }
    }
    #[cfg(windows)]
    if let Some(path) = find_jlink_windows() {
        return Ok(path);
    }
    bail!(
        "J-Link not found. Install SEGGER J-Link and put its commander \
         (`JLinkExe` / `JLink.exe`) on PATH."
    );
}

#[cfg(windows)]
fn find_jlink_windows() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let uninstall = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall")
        .ok()?;
    for subkey_name in uninstall.enum_keys().flatten() {
        let Ok(subkey) = uninstall.open_subkey(&subkey_name) else {
            continue;
        };
        let display_name: String = subkey.get_value("DisplayName").unwrap_or_default();
        if display_name.contains("J-Link") {
            if let Ok(loc) = subkey.get_value::<String, _>("InstallLocation") {
                let exe = WinPathBuf::from(loc).join("JLink.exe");
                if exe.is_file() {
                    return Some(exe.to_string_lossy().into_owned());
                }
            }
        }
    }
    None
}
