mod link;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};

use autd3_cpu_wire::fpga_update::FpgaBootImage;
use autd3_firmware_writer::bundle;
use autd3_firmware_writer::series::Series;
use autd3_rs_core::link::Link;
use autd3_rs_firmware_ota::{CpuFirmwareImage, Driver, FPGA_RECONFIG_WAIT, FpgaFirmwareImage};

use link::{LinkArgs, LinkKind};

type Version = (u8, u8, u8);

const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const CONFIRM_ONLY_HINT: &str = "the new CPU image is running but not confirmed: rerun the same command with \
     `--confirm-only` (or simply run the update again) before the next power cycle, otherwise \
     the previous image boots again";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum Target {
    #[default]
    Both,
    Cpu,
    Fpga,
}

impl Target {
    fn cpu(self) -> bool {
        matches!(self, Self::Both | Self::Cpu)
    }

    fn fpga(self) -> bool {
        matches!(self, Self::Both | Self::Fpga)
    }
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "autd3-rs-firmware-ota",
    about,
    long_about = "Update the AUTD3 CPU / FPGA firmware over EtherCAT (no J-Link / Vivado).\n\n\
        Pass a flash image, or --version to download a release bundle. The CPU image is \
        written to the inactive slot, the devices reboot into it, and the tool reconnects \
        to confirm it; an unconfirmed image rolls back at the next reset. The FPGA image is \
        written to the update slot and the FPGA reconfigures in place (golden image fallback).\n\n\
        Requires firmware v0.9.0 or newer on the devices; older devices must be written via JTAG."
)]
struct Cli {
    #[arg(
        required_unless_present = "version",
        help = "Flash image: `autd3-cpu.bin` (or `*-fpga-update.img` with --fpga)"
    )]
    image: Option<PathBuf>,
    #[arg(
        long,
        conflicts_with = "image",
        help = "Release to download and write (e.g. 0.9.0) instead of a local image"
    )]
    version: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value_t = Target::Both,
        requires = "version",
        help = "Which images of the release to write (CPU is updated before FPGA)"
    )]
    target: Target,
    #[arg(
        long,
        requires = "version",
        help = "Download the release bundle again even if it is cached"
    )]
    force_download: bool,
    #[arg(
        long,
        conflicts_with = "version",
        help = "Treat <IMAGE> as an FPGA update image (`*-fpga-update.img`)"
    )]
    fpga: bool,
    #[arg(
        long,
        conflicts_with_all = ["fpga", "version"],
        help = "Treat <IMAGE> as a bare slot image (body only) instead of a full flash image"
    )]
    slot_image: bool,
    #[arg(
        long,
        help = "Expected number of devices (default: whatever the link reports)"
    )]
    devices: Option<usize>,
    #[arg(
        long,
        help = "Write and commit the image but do not reboot into it (it boots as a trial at the next reset)"
    )]
    no_activate: bool,
    #[arg(
        long,
        default_value_t = 10,
        help = "Seconds to wait after the CPU reboot before reconnecting"
    )]
    reboot_wait_secs: u64,
    #[arg(
        long,
        default_value_t = 5,
        help = "How many times to retry the reconnection after the reboot (2 s apart)"
    )]
    reconnect_attempts: u32,
    #[arg(
        long,
        help = "Only read the firmware versions (and the FPGA boot image); write nothing"
    )]
    verify_only: bool,
    #[arg(
        long,
        conflicts_with_all = ["verify_only", "no_activate", "fpga"],
        help = "Reboot into the new CPU image but do not confirm it (it rolls back at the next reset unless --confirm-only is run)"
    )]
    no_confirm: bool,
    #[arg(
        long,
        conflicts_with_all = ["verify_only", "fpga"],
        help = "Only confirm the CPU image that is currently running (after --no-confirm or a failed reconnection)"
    )]
    confirm_only: bool,
    #[command(flatten)]
    link: LinkArgs,
}

#[derive(Clone, Copy)]
enum Expect<'a> {
    Exactly(Version),
    ChangedFrom(&'a [Version]),
    Anything,
}

#[derive(Clone, Copy)]
enum Stage<'a> {
    Update {
        image: &'a CpuFirmwareImage,
        activate: bool,
    },
    Verify {
        confirm: bool,
        expect: Expect<'a>,
    },
    FpgaUpdate {
        image: &'a FpgaFirmwareImage,
        activate: bool,
    },
    FpgaVerify,
}

struct Outcome {
    devices: usize,
    cpu_versions: Vec<Version>,
}

#[derive(Debug, thiserror::Error)]
#[error(
    "device {device} runs CPU firmware {}.{}.{} after the reboot, but {expected}; the trial image is not the one running, so it is left unconfirmed and the previous image stays",
    found.0, found.1, found.2
)]
struct TrialNotRunning {
    device: usize,
    found: Version,
    expected: String,
}

fn open_and_run(cli: &Cli, devices: Option<usize>, stage: Stage<'_>) -> Result<Outcome> {
    match cli.link.link {
        LinkKind::Echocat => run(cli.link.open_echocat(devices)?, stage),
        LinkKind::Twincat => run(cli.link.open_twincat(devices)?, stage),
        LinkKind::Remote => run(cli.link.open_remote(devices)?, stage),
    }
}

fn fmt_version((major, minor, patch): Version) -> String {
    format!("{major}.{minor}.{patch}")
}

fn print_versions<L: Link>(driver: &mut Driver<L>, label: &str) -> Result<Vec<Version>> {
    let versions = driver.read_cpu_version()?;
    for (device, &version) in versions.iter().enumerate() {
        println!(
            "device {device}: CPU firmware {label} = {}",
            fmt_version(version)
        );
    }
    Ok(versions)
}

fn print_fpga<L: Link>(driver: &mut Driver<L>, label: &str) -> Result<Vec<FpgaBootImage>> {
    let versions = driver.read_fpga_version()?;
    let images = driver.read_fpga_boot_image()?;
    for (device, (&version, image)) in versions.iter().zip(&images).enumerate() {
        println!(
            "device {device}: FPGA firmware {label} = {} ({image:?} image)",
            fmt_version(version)
        );
    }
    Ok(images)
}

fn progress_printer(total: usize) -> impl FnMut(autd3_rs_firmware_ota::UpdateProgress) {
    let interactive = std::io::stderr().is_terminal();
    let mut last_percent = None;
    move |p| {
        let percent = p.sent * 100 / total.max(1);
        if last_percent == Some(percent) {
            return;
        }
        last_percent = Some(percent);
        if interactive {
            eprint!("\r{:>8} / {total} bytes ({percent:>3}%)", p.sent);
        } else if percent.is_multiple_of(10) {
            eprintln!("{:>8} / {total} bytes ({percent:>3}%)", p.sent);
        }
    }
}

fn end_progress() {
    if std::io::stderr().is_terminal() {
        eprintln!();
    }
}

fn check_expectation(versions: &[Version], expect: Expect<'_>) -> Result<()> {
    match expect {
        Expect::Exactly(expected) => {
            if let Some((device, &found)) =
                versions.iter().enumerate().find(|&(_, v)| *v != expected)
            {
                return Err(TrialNotRunning {
                    device,
                    found,
                    expected: format!("the release is {}", fmt_version(expected)),
                }
                .into());
            }
        }
        Expect::ChangedFrom(before) => {
            if before.len() == versions.len() && before == versions {
                eprintln!(
                    "warning: the CPU firmware version did not change across the reboot; \
                     this is expected only if the same version was written again, otherwise \
                     the trial image did not boot and the previous image is running"
                );
            }
        }
        Expect::Anything => {}
    }
    Ok(())
}

fn run_fpga<L: Link>(
    driver: &mut Driver<L>,
    image: &FpgaFirmwareImage,
    activate: bool,
) -> Result<()> {
    print_fpga(driver, "before")?;
    driver.update_fpga(image, progress_printer(image.len()))?;
    end_progress();
    println!(
        "FPGA image committed on every device (transducer output stays off until reconfiguration)"
    );
    if !activate {
        println!(
            "not activated (--no-activate); output stays off until the next power cycle, \
             which boots the new image"
        );
        return Ok(());
    }
    driver.activate_fpga()?;
    println!(
        "activation acknowledged; waiting {FPGA_RECONFIG_WAIT:?} for the FPGAs to reconfigure"
    );
    driver.idle(FPGA_RECONFIG_WAIT)?;
    let images = print_fpga(driver, "after")?;
    driver.ensure_fpga_reconfigured()?;
    if let Some(device) = images.iter().position(|&i| i != FpgaBootImage::Update) {
        bail!(
            "device {device} did not come back with the update image (reads {:?}); \
             Golden means the update image failed to configure and the golden image took over. \
             Write the FPGA via JTAG (autd3-console Firmware tab with Method = JTAG, or \
             `autd3-firmware-writer`)",
            images[device]
        );
    }
    println!("Ok!");
    Ok(())
}

fn run<L: Link>(link: L, stage: Stage<'_>) -> Result<Outcome> {
    let mut driver = Driver::open(link).context("opening the link / protocol handshake")?;
    let devices = driver.num_devices();
    println!("{devices} device(s) on the bus");
    let cpu_versions = match stage {
        Stage::Update { image, activate } => {
            let before = print_versions(&mut driver, "before")?;
            driver.update(image, progress_printer(image.len()))?;
            end_progress();
            println!("image committed on every device");
            if activate {
                driver.activate()?;
                println!("activation acknowledged; the devices reboot in about 100 ms");
                if let Err(e) = driver.close() {
                    eprintln!(
                        "warning: closing the link after activation failed ({e:#}); \
                         this can happen while the devices reboot, continuing"
                    );
                }
            } else {
                driver.close()?;
            }
            before
        }
        Stage::FpgaUpdate { image, activate } => {
            run_fpga(&mut driver, image, activate)?;
            driver.close()?;
            Vec::new()
        }
        Stage::FpgaVerify => {
            print_fpga(&mut driver, "now")?;
            driver.close()?;
            Vec::new()
        }
        Stage::Verify { confirm, expect } => {
            let label = if confirm { "after" } else { "now" };
            let after = print_versions(&mut driver, label)?;
            check_expectation(&after, expect)?;
            if confirm {
                driver.confirm()?;
                println!("running image confirmed on every device");
            }
            driver.close()?;
            after
        }
    };
    Ok(Outcome {
        devices,
        cpu_versions,
    })
}

fn reconnect_and_run(cli: &Cli, devices: usize, stage: Stage<'_>) -> Result<Outcome> {
    let attempts = cli.reconnect_attempts.max(1);
    let mut attempt = 0;
    loop {
        attempt += 1;
        match open_and_run(cli, Some(devices), stage) {
            Ok(outcome) => return Ok(outcome),
            Err(e) if e.downcast_ref::<TrialNotRunning>().is_some() => return Err(e),
            Err(e) if attempt < attempts => {
                eprintln!(
                    "reconnection attempt {attempt}/{attempts} failed: {e:#}; retrying in {RECONNECT_INTERVAL:?}"
                );
                std::thread::sleep(RECONNECT_INTERVAL);
            }
            Err(e) => {
                return Err(e).context(format!(
                    "reconnection failed {attempts} time(s) after the reboot; {CONFIRM_ONLY_HINT}"
                ));
            }
        }
    }
}

fn main_fpga(cli: &Cli, bytes: Vec<u8>) -> Result<()> {
    if cli.verify_only {
        open_and_run(cli, cli.devices, Stage::FpgaVerify)?;
        return Ok(());
    }
    let image = FpgaFirmwareImage::from_update_bin(bytes)?;
    println!(
        "FPGA update image: {} bytes, crc32 {:#010x}",
        image.len(),
        image.crc32()
    );
    open_and_run(
        cli,
        cli.devices,
        Stage::FpgaUpdate {
            image: &image,
            activate: !cli.no_activate,
        },
    )?;
    Ok(())
}

fn read_image(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading {}", path.display()))
}

fn parse_version(version: &str) -> Option<Version> {
    let mut parts = version.trim_start_matches('v').split('.');
    let mut next = || parts.next()?.parse::<u8>().ok();
    let v = (next()?, next()?, next()?);
    parts.next().is_none().then_some(v)
}

fn main_cpu(cli: &Cli, bytes: Vec<u8>, release: Option<Version>) -> Result<()> {
    let image = if cli.slot_image {
        CpuFirmwareImage::from_slot_image(bytes)?
    } else {
        CpuFirmwareImage::from_flash_image(&bytes)?
    };
    println!(
        "image: {} bytes, crc32 {:#010x}",
        image.len(),
        image.crc32()
    );

    if cli.verify_only || cli.confirm_only {
        open_and_run(
            cli,
            cli.devices,
            Stage::Verify {
                confirm: cli.confirm_only,
                expect: Expect::Anything,
            },
        )?;
        return Ok(());
    }
    let activate = !cli.no_activate;
    let before = open_and_run(
        cli,
        cli.devices,
        Stage::Update {
            image: &image,
            activate,
        },
    )?;
    if !activate {
        println!(
            "not activated (--no-activate); the committed image boots as a trial at the next \
             reset or power cycle. Run `--confirm-only` once it is up, or it rolls back at the \
             reset after that"
        );
        return Ok(());
    }

    let wait = Duration::from_secs(cli.reboot_wait_secs);
    println!("waiting {wait:?} for the devices to reboot and the bus to re-enumerate");
    std::thread::sleep(wait);
    let expect = release.map_or(Expect::ChangedFrom(&before.cpu_versions), Expect::Exactly);
    reconnect_and_run(
        cli,
        before.devices,
        Stage::Verify {
            confirm: !cli.no_confirm,
            expect,
        },
    )?;
    if cli.no_confirm {
        println!("not confirmed (--no-confirm); {CONFIRM_ONLY_HINT}");
        return Ok(());
    }
    println!("Ok!");
    Ok(())
}

fn main_release(cli: &Cli, version: &str) -> Result<()> {
    if cli.confirm_only && cli.target != Target::Cpu {
        bail!("--confirm-only only applies to the CPU image; pass --target cpu");
    }
    let bundle = bundle::fetch(version, cli.force_download, Series::Sdk)?;
    if bundle.fpga_update.is_none() {
        bail!(
            "the release bundle has no FPGA update image (*-fpga-update.img): either this \
             release predates EtherCAT updates (before v0.9.0) and must be written via JTAG, \
             or the cached bundle is incomplete (retry with --force-download)"
        );
    }
    let cpu = if cli.target.cpu() {
        let path = bundle
            .cpu
            .context("the release bundle has no CPU firmware image (*.bin)")?;
        Some(read_image(&path)?)
    } else {
        None
    };
    let fpga = if cli.target.fpga() {
        let path = bundle
            .fpga_update
            .context("the release bundle has no FPGA update image (*-fpga-update.img)")?;
        Some(read_image(&path)?)
    } else {
        None
    };
    let release = parse_version(version);
    let shown = version.trim_start_matches('v');
    if let Some(bytes) = cpu {
        println!("== CPU firmware v{shown} ==");
        main_cpu(cli, bytes, release)?;
    }
    if let Some(bytes) = fpga {
        println!("== FPGA firmware v{shown} ==");
        main_fpga(cli, bytes)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut cli = Cli::parse();
    if cli.devices == Some(0) {
        bail!("--devices must be at least 1");
    }
    cli.link.resolve_remote()?;
    match (&cli.image, &cli.version) {
        (Some(path), _) => {
            let bytes = read_image(path)?;
            if cli.fpga {
                main_fpga(&cli, bytes)
            } else {
                main_cpu(&cli, bytes, None)
            }
        }
        (None, Some(version)) => main_release(&cli, version),
        (None, None) => bail!("pass an image file or --version"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_version_parses_with_or_without_the_v_prefix() {
        assert_eq!(parse_version("0.9.0"), Some((0, 9, 0)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("a.b.c"), None);
    }

    #[test]
    fn the_exact_expectation_names_the_first_device_running_something_else() {
        assert!(check_expectation(&[(0, 9, 0), (0, 9, 0)], Expect::Exactly((0, 9, 0))).is_ok());
        let err =
            check_expectation(&[(0, 9, 0), (0, 6, 1)], Expect::Exactly((0, 9, 0))).unwrap_err();
        let err = err.downcast_ref::<TrialNotRunning>().unwrap();
        assert_eq!((err.device, err.found), (1, (0, 6, 1)));
    }

    #[test]
    fn an_unchanged_version_is_only_a_warning() {
        let before = [(0, 9, 0)];
        assert!(check_expectation(&[(0, 9, 0)], Expect::ChangedFrom(&before)).is_ok());
        assert!(check_expectation(&[(0, 9, 1)], Expect::ChangedFrom(&before)).is_ok());
        assert!(check_expectation(&[(0, 6, 1)], Expect::Anything).is_ok());
    }
}
