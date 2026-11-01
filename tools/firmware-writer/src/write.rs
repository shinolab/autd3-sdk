use std::io::{BufRead, Write};
#[cfg(windows)]
use std::path::PathBuf as WinPathBuf;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use autd3_firmware_writer::bundle::{self, Bundle};
use autd3_firmware_writer::image::CpuFirmwareImage;

use crate::Target;
use crate::util::{run, which};

pub(crate) fn write(version: &str, target: Option<Target>, force_download: bool) -> Result<()> {
    fn prompt_target() -> Result<Target> {
        eprintln!("Select which firmware to update:");
        eprintln!("  [0]: Both (default)");
        eprintln!("  [1]: FPGA");
        eprintln!("  [2]: CPU");
        loop {
            eprint!("Select: ");
            std::io::stderr().flush().ok();
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line)? == 0 {
                bail!("no input (use --target to run non-interactively)");
            }
            match line.trim() {
                "" | "0" => return Ok(Target::Both),
                "1" => return Ok(Target::Fpga),
                "2" => return Ok(Target::Cpu),
                other => eprintln!("invalid selection: {other:?}"),
            }
        }
    }

    let release = bundle::parse_release(version)?;

    let Bundle {
        cpu,
        fpga_mcs: fpga,
        ..
    } = bundle::fetch(release, force_download)?;

    let [major, minor, patch] = release;
    eprintln!("Found firmwares (autd3-sdk v{major}.{minor}.{patch}):");
    match &cpu {
        Some(p) => eprintln!("  CPU : {}", p.display()),
        None => eprintln!("  CPU : (none)"),
    }
    match &fpga {
        Some(p) => eprintln!("  FPGA: {}", p.display()),
        None => eprintln!("  FPGA: (none)"),
    }
    eprintln!("Make sure the configuration cables are connected and the AUTD3 power is on.");

    let target = match target {
        Some(t) => t,
        None => prompt_target()?,
    };

    let cpu = cpu.as_deref().context(CPU_NOT_IN_BUNDLE);
    let fpga = fpga.as_deref().context(FPGA_NOT_IN_BUNDLE);
    match target {
        Target::Both => write_images(Some(cpu?), Some(fpga?)),
        Target::Fpga => write_images(None, Some(fpga?)),
        Target::Cpu => write_images(Some(cpu?), None),
    }
}

const CPU_NOT_IN_BUNDLE: &str = "CPU firmware (*.bin) not found in the bundle";
const FPGA_NOT_IN_BUNDLE: &str = "FPGA firmware (*.mcs) not found in the bundle";

pub(crate) fn write_local(cpu: Option<&Path>, fpga: Option<&Path>) -> Result<()> {
    let absolute = |image: &Path| -> Result<PathBuf> {
        if !image.is_file() {
            bail!("{} not found", image.display());
        }
        std::path::absolute(image).with_context(|| format!("resolving {}", image.display()))
    };
    let cpu = cpu.map(absolute).transpose()?;
    let fpga = fpga.map(absolute).transpose()?;
    eprintln!("Make sure the configuration cables are connected and the AUTD3 power is on.");
    write_images(cpu.as_deref(), fpga.as_deref())
}

fn write_images(cpu: Option<&Path>, fpga: Option<&Path>) -> Result<()> {
    let read =
        |image: &Path| std::fs::read(image).with_context(|| format!("reading {}", image.display()));
    if let Some(cpu) = cpu {
        CpuFirmwareImage::from_flash_image(&read(cpu)?)
            .with_context(|| format!("{} is not a CPU flash image", cpu.display()))?;
    }
    if let Some(fpga) = fpga
        && !is_mcs(fpga, &read(fpga)?)
    {
        bail!(
            "{} is not an FPGA configuration image (expected an Intel HEX *.mcs ending with its EOF record)",
            fpga.display()
        );
    }

    if let Some(cpu) = cpu {
        update_cpu(cpu)?;
    }
    if let Some(fpga) = fpga {
        update_fpga(fpga)?;
    }
    eprintln!("Done. Power-cycle the AUTD3 to load the new firmware.");
    Ok(())
}

fn is_mcs(path: &Path, content: &[u8]) -> bool {
    let is_mcs_file = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mcs"));
    let text = String::from_utf8_lossy(content);
    let mut lines = text.trim_end().lines();
    is_mcs_file
        && lines
            .next_back()
            .is_some_and(|last| last.eq_ignore_ascii_case(":00000001FF"))
        && lines.all(|line| line.starts_with(':'))
}

fn update_cpu(firmware: &Path) -> Result<()> {
    fn run_jlink(jlink: &str, script: &Path) -> Result<()> {
        let mut child = std::process::Command::new(jlink)
            .args([
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
            ])
            .arg(script)
            .current_dir(std::env::temp_dir())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn `{jlink}` (is it installed and on PATH?)"))?;
        let stdout = child.stdout.take().context("J-Link stdout is not piped")?;
        let stderr = child.stderr.take().context("J-Link stderr is not piped")?;
        let out = std::thread::spawn(move || {
            let mut log = String::new();
            tee_lines(stdout, &mut log);
            log
        });
        let err = std::thread::spawn(move || {
            let mut log = String::new();
            tee_lines(stderr, &mut log);
            log
        });
        let status = child.wait().context("waiting for J-Link")?;
        let mut log = out.join().unwrap_or_default();
        log.push_str(&err.join().unwrap_or_default());
        if !status.success() {
            bail!("`{jlink}` exited with {status}");
        }
        if log.contains(JLINK_NO_FLASH_BANK) {
            bail!(
                "J-Link did not erase the serial flash: {JLINK_NO_FLASH_BANK}. \
                 The QSPI bank stayed untouched, so an OTA image in slot B would still win the boot."
            );
        }
        Ok(())
    }

    fn resolve_jlink() -> Result<String> {
        if let Some(path) = std::env::var_os("JLINK").filter(|path| !path.is_empty()) {
            return Ok(path.to_string_lossy().into_owned());
        }
        let candidates = if cfg!(windows) {
            ["JLink", "jlink"].as_slice()
        } else {
            ["JLinkExe"].as_slice()
        };
        for name in candidates {
            if let Some(path) = which(name) {
                return Ok(path.to_string_lossy().into_owned());
            }
        }
        #[cfg(windows)]
        if let Some(path) = find_jlink_windows() {
            return Ok(path);
        }
        bail!(
            "J-Link not found. Install SEGGER J-Link and put its commander \
             (`JLinkExe` / `JLink.exe`) on PATH, or set JLINK to its path."
        );
    }

    let jlink = resolve_jlink()?;

    let slot_b_start = SERIAL_FLASH_XIP_BASE + autd3_cpu_wire::update::SLOT_B_BASE;
    let slot_b_last = slot_b_start + autd3_cpu_wire::update::SLOT_BYTES - 1;
    let script = format!(
        "r\nexec EnableEraseAllFlashBanks\nerase 0x{slot_b_start:X} 0x{slot_b_last:X}\nloadfile {} 0x{SERIAL_FLASH_XIP_BASE:X}\nq\n",
        tcl_path(firmware)
    );
    let script_path = std::env::temp_dir().join("autd3-cpu-flash.jlink");
    std::fs::write(&script_path, script)
        .with_context(|| format!("writing {}", script_path.display()))?;

    eprintln!("Flashing CPU via {jlink}...");
    run_jlink(&jlink, &script_path)
        .context("J-Link failed. Make sure the AUTD3 is connected and powered on.")?;
    eprintln!("CPU update done.");
    Ok(())
}

const JLINK_NO_FLASH_BANK: &str = "No Flash bank within given address range";

fn tee_lines(reader: impl std::io::Read, log: &mut String) {
    for line in std::io::BufReader::new(reader).lines() {
        let Ok(line) = line else { break };
        eprintln!("{line}");
        log.push_str(&line);
        log.push('\n');
    }
}

const FPGA_CONFIGURATION_TCL: &str =
    include_str!("../../../firmware/fpga/scripts/configuration.tcl");

fn update_fpga(firmware: &Path) -> Result<()> {
    fn resolve_vivado() -> Result<String> {
        for name in ["vivado_lab", "vivado"] {
            if let Some(path) = which(name) {
                return Ok(path.to_string_lossy().into_owned());
            }
        }
        #[cfg(windows)]
        if let Some(path) = find_vivado_windows() {
            return Ok(path);
        }
        bail!(
            "Vivado not found. Put `vivado` (or `vivado_lab`) on PATH (source Vivado's \
             `settings64` script) or install it so it can be auto-detected."
        );
    }

    let vivado = resolve_vivado()?;

    let script_path = std::env::temp_dir().join("autd3-fpga-configuration.tcl");
    std::fs::write(&script_path, FPGA_CONFIGURATION_TCL)
        .with_context(|| format!("writing {}", script_path.display()))?;

    eprintln!("Flashing FPGA via {vivado}...");
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
            "-tclargs",
            &tcl_path(firmware),
        ],
        &std::env::temp_dir(),
    )
    .context("Vivado failed. Make sure the AUTD3 is connected and powered on.")?;
    eprintln!("FPGA update done.");
    Ok(())
}

const SERIAL_FLASH_XIP_BASE: u32 = 0x3000_0000;

fn tcl_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
        if display_name.contains("J-Link")
            && let Ok(loc) = subkey.get_value::<String, _>("InstallLocation")
        {
            let exe = WinPathBuf::from(loc).join("JLink.exe");
            if exe.is_file() {
                return Some(exe.to_string_lossy().into_owned());
            }
        }
    }
    None
}

#[cfg(windows)]
fn find_vivado_windows() -> Option<String> {
    let roots = xilinx_install_locations();
    for exe in ["vivado_lab.bat", "vivado.bat"] {
        let mut found = Vec::new();
        for root in &roots {
            collect_vivado_bins(root, 0, exe, &mut found);
        }
        found.sort();
        if let Some(path) = found.pop() {
            return Some(path.to_string_lossy().into_owned());
        }
    }
    None
}

#[cfg(windows)]
fn xilinx_install_locations() -> Vec<WinPathBuf> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    const NEEDLES: [&str; 4] = [
        "Vivado",
        "Vitis",
        "Xilinx Design Tools FPGAs",
        "AMDDesignTools",
    ];

    let Ok(uninstall) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall")
    else {
        return Vec::new();
    };

    let mut out: Vec<WinPathBuf> = Vec::new();
    for subkey_name in uninstall.enum_keys().flatten() {
        let Ok(subkey) = uninstall.open_subkey(&subkey_name) else {
            continue;
        };
        let display_name: String = subkey.get_value("DisplayName").unwrap_or_default();
        if NEEDLES.iter().any(|n| display_name.contains(n))
            && let Ok(loc) = subkey.get_value::<String, _>("InstallLocation")
            && !loc.is_empty()
        {
            let path = WinPathBuf::from(loc);
            if path.is_dir() && !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

#[cfg(windows)]
fn collect_vivado_bins(dir: &Path, depth: usize, exe: &str, out: &mut Vec<WinPathBuf>) {
    let candidate = dir.join("bin").join(exe);
    if candidate.is_file() {
        out.push(candidate);
    }
    if depth >= 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_vivado_bins(&path, depth + 1, exe, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MCS: &[u8] = b":020000040000FA\r\n:0400000011223344AA\r\n:00000001FF\r\n";

    #[test]
    fn an_intel_hex_mcs_is_accepted() {
        assert!(is_mcs(Path::new("autd3-fpga.mcs"), MCS));
        assert!(is_mcs(Path::new("AUTD3-FPGA.MCS"), MCS));
        assert!(is_mcs(Path::new("fw.mcs"), b":00000001FF\n"));
    }

    #[test]
    fn a_file_without_the_mcs_extension_is_rejected() {
        for path in ["fw.bin", "fw-fpga-update.img", "fw"] {
            assert!(!is_mcs(Path::new(path), MCS), "{path}");
        }
    }

    #[test]
    fn a_truncated_mcs_is_rejected() {
        assert!(!is_mcs(Path::new("fw.mcs"), &MCS[..MCS.len() - 13]));
        assert!(!is_mcs(Path::new("fw.mcs"), b""));
    }

    #[test]
    fn content_that_is_not_intel_hex_is_rejected() {
        assert!(!is_mcs(Path::new("fw.mcs"), b"not hex\n:00000001FF\n"));
        assert!(!is_mcs(Path::new("fw.mcs"), &[0xFF, 0x00, 0xAA, 0x55]));
    }
}
