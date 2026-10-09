mod udp;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};

use autd3_cpu_wire::fpga_update::FpgaBootImage;
use autd3_cpu_wire::update::RunningImage;
use autd3_firmware_writer::bundle;
use autd3_rs_firmware_ota::{
    CpuFirmwareImage, Driver, DriverError, Exchange, FPGA_RECONFIG_WAIT, FpgaFirmwareImage,
    boot_image,
};

use udp::UdpArgs;

type Version = [u8; 3];

const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const CONFIRM_ONLY_HINT: &str = "the new CPU image is running but not confirmed: rerun the same command with \
     `--confirm-only` (or simply run the update again) before the next power cycle, otherwise \
     the previous image boots again";
const UNACKNOWLEDGED_ACTIVATION_HINT: &str = "the activation was not acknowledged by every device: the devices that received it \
     run the new CPU image unconfirmed, the others still run the previous image and boot the \
     new one at their next power cycle. Once every device is reachable, check which image \
     runs with `--verify-only` and confirm it with `--confirm-only` (or simply run the \
     update again)";

type Connect<'a, L> = &'a dyn Fn(Option<usize>) -> Result<L>;

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
    name = "autd3-firmware-ota",
    about,
    long_about = "Update the AUTD3 CPU / FPGA firmware over UDP (no J-Link / Vivado).\n\n\
        Pass a flash image, or --version to download a release bundle. The CPU image is \
        written to the inactive slot, the devices reboot into it, and the tool reconnects \
        to confirm it (the reboot leaves the devices unassigned, so the chain is enumerated \
        again); an unconfirmed image rolls back at the next reset. The FPGA image is \
        written to the update slot and the FPGA reconfigures in place (golden image fallback).\n\n\
        Only UDP firmware images (v0.10.0 or newer) are accepted, and only devices already \
        running the UDP firmware are reached. Devices on the EtherCAT firmware (v0.9.x or \
        older) must be written once via J-Link."
)]
struct Cli {
    #[arg(
        required_unless_present_any = ["version", "reboot_only"],
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
        help = "Number of devices on the chain (default: whatever the enumeration finds)"
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
    #[arg(
        long,
        conflicts_with_all = ["image", "version", "fpga", "slot_image", "no_activate", "verify_only", "no_confirm", "confirm_only"],
        help = "Only reboot every device (software reset, no image is written); the devices come back unassigned and the tool does not reconnect"
    )]
    reboot_only: bool,
    #[command(flatten)]
    udp: UdpArgs,
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
        activate_unrebooted: bool,
        require_trial: bool,
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
    activation_unacknowledged: bool,
    activated_late: Vec<usize>,
}

#[derive(Debug, thiserror::Error)]
#[error(
    "device {device} runs CPU firmware {}.{}.{} after the reboot, but {expected}; the trial image is not the one running, so it is left unconfirmed and the previous image stays",
    found[0], found[1], found[2]
)]
struct TrialNotRunning {
    device: usize,
    found: Version,
    expected: String,
}

#[derive(Debug, PartialEq, Eq)]
struct OffTrial {
    device: usize,
    version: Version,
    image: Option<RunningImage>,
}

impl std::fmt::Display for OffTrial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "device {} ({}, CPU firmware {})",
            self.device,
            describe_running_image(self.image),
            fmt_version(self.version)
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "not every device runs the trial image after the reboot: {} fell back to the previous image. \
     No device was confirmed, so the devices that do run the trial image fall back as well at \
     their next reset; run the update again, or reset every device (`--reboot-only` or a power \
     cycle) to bring them all back to the previous image",
    devices.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
)]
struct NotOnTrial {
    devices: Vec<OffTrial>,
}

fn is_final(e: &anyhow::Error) -> bool {
    e.downcast_ref::<TrialNotRunning>().is_some() || e.downcast_ref::<NotOnTrial>().is_some()
}

fn describe_running_image(image: Option<RunningImage>) -> &'static str {
    match image {
        Some(RunningImage::Unconfirmed) => "unconfirmed trial image",
        Some(RunningImage::Confirmed) => "confirmed image",
        Some(_) => "image of unknown state",
        None => "image state not reported by this firmware",
    }
}

fn open_and_run<L: Exchange>(
    connect: Connect<'_, L>,
    devices: Option<usize>,
    stage: Stage<'_>,
) -> Result<Outcome> {
    run(connect(devices)?, stage)
}

fn request_activation<L: Exchange>(driver: &mut Driver<L>) -> Result<bool> {
    match driver.activate() {
        Ok(()) => {
            println!("activation acknowledged; the devices reboot in about 100 ms");
            Ok(true)
        }
        Err(DriverError::Timeout { device, .. }) => {
            eprintln!(
                "warning: device {device} did not acknowledge the activation; the devices that \
                 have not rebooted are activated after the reconnection"
            );
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

fn fmt_version([major, minor, patch]: Version) -> String {
    format!("{major}.{minor}.{patch}")
}

fn print_versions<L: Exchange>(driver: &mut Driver<L>, label: &str) -> Result<Vec<Version>> {
    let versions: Vec<Version> = driver
        .read_firmware_info()?
        .iter()
        .map(|info| info.cpu_version)
        .collect();
    for (device, &version) in versions.iter().enumerate() {
        println!(
            "device {device}: CPU firmware {label} = {}",
            fmt_version(version)
        );
    }
    Ok(versions)
}

fn print_running_images<L: Exchange>(driver: &mut Driver<L>) -> Result<Vec<Option<RunningImage>>> {
    let images = driver.read_running_image()?;
    for (device, &image) in images.iter().enumerate() {
        println!("device {device}: CPU {}", describe_running_image(image));
    }
    Ok(images)
}

fn print_fpga<L: Exchange>(driver: &mut Driver<L>, label: &str) -> Result<Vec<FpgaBootImage>> {
    let infos = driver.read_firmware_info()?;
    for (device, info) in infos.iter().enumerate() {
        println!(
            "device {device}: FPGA firmware {label} = {} ({:?} image)",
            fmt_version(info.fpga_version),
            boot_image(info)
        );
    }
    Ok(infos.iter().map(boot_image).collect())
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
        Expect::ChangedFrom(_) | Expect::Anything => {}
    }
    Ok(())
}

fn off_trial_by_version(versions: &[Version], expect: Expect<'_>) -> Vec<usize> {
    let Expect::ChangedFrom(before) = expect else {
        return Vec::new();
    };
    if versions.windows(2).all(|pair| pair[0] == pair[1]) {
        if before == versions {
            eprintln!(
                "warning: the CPU firmware version did not change across the reboot; \
                 this is expected only if the same version was written again, otherwise \
                 the trial image did not boot and the previous image is running"
            );
        }
        return Vec::new();
    }
    let unchanged: Vec<usize> = (0..versions.len())
        .filter(|&device| before.get(device) == Some(&versions[device]))
        .collect();
    if unchanged.is_empty() {
        (0..versions.len()).collect()
    } else {
        unchanged
    }
}

fn check_trial(
    images: &[Option<RunningImage>],
    versions: &[Version],
    expect: Expect<'_>,
) -> Result<()> {
    let off_trial = if images.iter().any(Option::is_some) {
        (0..images.len())
            .filter(|&device| images[device] != Some(RunningImage::Unconfirmed))
            .collect()
    } else {
        eprintln!(
            "warning: this CPU firmware cannot report whether the trial image is running; \
             only the versions are compared"
        );
        off_trial_by_version(versions, expect)
    };
    if off_trial.is_empty() {
        return Ok(());
    }
    Err(NotOnTrial {
        devices: off_trial
            .into_iter()
            .map(|device| OffTrial {
                device,
                version: versions[device],
                image: images[device],
            })
            .collect(),
    }
    .into())
}

fn run_fpga<L: Exchange>(
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

fn run<L: Exchange>(link: L, stage: Stage<'_>) -> Result<Outcome> {
    let mut driver = Driver::open(link).context("opening the devices / protocol handshake")?;
    let devices = driver.num_devices();
    println!("{devices} device(s) on the bus");
    let mut activation_unacknowledged = false;
    let mut activated_late = Vec::new();
    let cpu_versions = match stage {
        Stage::Update { image, activate } => {
            let before = print_versions(&mut driver, "before")?;
            driver.update(image, progress_printer(image.len()))?;
            end_progress();
            println!("image committed on every device");
            if activate {
                activation_unacknowledged = !request_activation(&mut driver)?;
            }
            driver.close()?;
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
        Stage::Verify {
            confirm,
            expect,
            activate_unrebooted,
            require_trial,
        } => {
            if activate_unrebooted {
                activated_late = driver.activate_unrebooted()?;
            }
            let after = if activated_late.is_empty() {
                let label = if confirm { "after" } else { "now" };
                let after = print_versions(&mut driver, label)?;
                check_expectation(&after, expect)?;
                let images = print_running_images(&mut driver)?;
                if require_trial {
                    check_trial(&images, &after, expect)?;
                }
                if confirm {
                    driver.confirm()?;
                    println!("running image confirmed on every device");
                }
                after
            } else {
                for device in &activated_late {
                    println!(
                        "device {device} had not rebooted; activated now, it reboots in about 100 ms"
                    );
                }
                Vec::new()
            };
            driver.close()?;
            after
        }
    };
    Ok(Outcome {
        devices,
        cpu_versions,
        activation_unacknowledged,
        activated_late,
    })
}

fn main_reboot(cli: &Cli) -> Result<()> {
    let mut driver = Driver::open(cli.udp.open(cli.devices)?)
        .context("opening the devices / protocol handshake")?;
    println!("{} device(s) on the bus", driver.num_devices());
    driver.reboot()?;
    println!("reboot acknowledged; the devices reset in about 100 ms and come back unassigned");
    driver.close()?;
    Ok(())
}

fn reconnect_and_run<L: Exchange>(
    cli: &Cli,
    connect: Connect<'_, L>,
    devices: usize,
    stage: Stage<'_>,
) -> Result<Outcome> {
    let attempts = cli.reconnect_attempts.max(1);
    let hint = match stage {
        Stage::Verify {
            activate_unrebooted: true,
            ..
        } => UNACKNOWLEDGED_ACTIVATION_HINT,
        _ => CONFIRM_ONLY_HINT,
    };
    let mut attempt = 0;
    loop {
        attempt += 1;
        match open_and_run(connect, Some(devices), stage) {
            Ok(outcome) => return Ok(outcome),
            Err(e) if is_final(&e) => return Err(e),
            Err(e) if attempt < attempts => {
                eprintln!(
                    "reconnection attempt {attempt}/{attempts} failed: {e:#}; retrying in {RECONNECT_INTERVAL:?}"
                );
                std::thread::sleep(RECONNECT_INTERVAL);
            }
            Err(e) => {
                return Err(e).context(format!(
                    "reconnection failed {attempts} time(s) after the reboot; {hint}"
                ));
            }
        }
    }
}

fn main_fpga(cli: &Cli, bytes: Vec<u8>) -> Result<()> {
    let connect: Connect<'_, _> = &|devices| cli.udp.open(devices);
    if cli.verify_only {
        open_and_run(connect, cli.devices, Stage::FpgaVerify)?;
        return Ok(());
    }
    let image = FpgaFirmwareImage::from_update_bin(bytes)?;
    println!(
        "FPGA update image: {} bytes, crc32 {:#010x}",
        image.len(),
        image.crc32()
    );
    open_and_run(
        connect,
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
    let v = [next()?, next()?, next()?];
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
    update_cpu(cli, &|devices| cli.udp.open(devices), &image, release)
}

fn update_cpu<L: Exchange>(
    cli: &Cli,
    connect: Connect<'_, L>,
    image: &CpuFirmwareImage,
    release: Option<Version>,
) -> Result<()> {
    if cli.verify_only || cli.confirm_only {
        open_and_run(
            connect,
            cli.devices,
            Stage::Verify {
                confirm: cli.confirm_only,
                expect: Expect::Anything,
                activate_unrebooted: false,
                require_trial: false,
            },
        )?;
        return Ok(());
    }
    let activate = !cli.no_activate;
    let before = open_and_run(connect, cli.devices, Stage::Update { image, activate })?;
    if !activate {
        println!(
            "not activated (--no-activate); the committed image boots as a trial at the next \
             reset or power cycle. Run `--confirm-only` once it is up, or it rolls back at the \
             reset after that"
        );
        return Ok(());
    }

    let wait = Duration::from_secs(cli.reboot_wait_secs);
    let expect = release.map_or(Expect::ChangedFrom(&before.cpu_versions), Expect::Exactly);
    let mut activate_unrebooted = before.activation_unacknowledged;
    loop {
        println!("waiting {wait:?} for the devices to reboot before enumerating them again");
        std::thread::sleep(wait);
        let outcome = reconnect_and_run(
            cli,
            connect,
            before.devices,
            Stage::Verify {
                confirm: !cli.no_confirm,
                expect,
                activate_unrebooted,
                require_trial: true,
            },
        )?;
        if outcome.activated_late.is_empty() {
            break;
        }
        activate_unrebooted = false;
    }
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
    let bundle = bundle::fetch(version, cli.force_download)?;
    if bundle.fpga_update.is_none() {
        bail!(
            "the release bundle has no FPGA update image (*-fpga-update.img): either this \
             release predates firmware updates (before v0.9.0) and must be written via JTAG, \
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
    let cli = Cli::parse();
    if cli.devices == Some(0) {
        bail!("--devices must be at least 1");
    }
    if cli.reboot_only {
        return main_reboot(&cli);
    }
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
        assert_eq!(parse_version("0.9.0"), Some([0, 9, 0]));
        assert_eq!(parse_version("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("a.b.c"), None);
    }

    #[test]
    fn the_exact_expectation_names_the_first_device_running_something_else() {
        assert!(check_expectation(&[[0, 9, 0], [0, 9, 0]], Expect::Exactly([0, 9, 0])).is_ok());
        let err =
            check_expectation(&[[0, 9, 0], [0, 6, 1]], Expect::Exactly([0, 9, 0])).unwrap_err();
        let err = err.downcast_ref::<TrialNotRunning>().unwrap();
        assert_eq!((err.device, err.found), (1, [0, 6, 1]));
    }

    #[test]
    fn an_unchanged_version_is_only_a_warning() {
        let before = [[0, 9, 0]];
        assert!(check_expectation(&[[0, 9, 0]], Expect::ChangedFrom(&before)).is_ok());
        assert!(check_expectation(&[[0, 9, 1]], Expect::ChangedFrom(&before)).is_ok());
        assert!(check_expectation(&[[0, 6, 1]], Expect::Anything).is_ok());
        assert!(check_trial(&[None], &[[0, 9, 0]], Expect::ChangedFrom(&before)).is_ok());
        assert!(check_trial(&[None], &[[0, 9, 1]], Expect::ChangedFrom(&before)).is_ok());
    }

    fn off_trial(
        images: &[Option<RunningImage>],
        versions: &[Version],
        expect: Expect<'_>,
    ) -> Vec<usize> {
        match check_trial(images, versions, expect) {
            Ok(()) => Vec::new(),
            Err(e) => e
                .downcast_ref::<NotOnTrial>()
                .unwrap()
                .devices
                .iter()
                .map(|d| d.device)
                .collect(),
        }
    }

    const TRIAL: Option<RunningImage> = Some(RunningImage::Unconfirmed);
    const CONFIRMED: Option<RunningImage> = Some(RunningImage::Confirmed);
    const SAME: [Version; 3] = [[0, 10, 0]; 3];
    const NONE: [usize; 0] = [];

    #[test]
    fn every_device_must_report_an_unconfirmed_trial() {
        let rewritten = Expect::ChangedFrom(&SAME);
        assert_eq!(off_trial(&[TRIAL; 3], &SAME, rewritten), NONE);
        assert_eq!(off_trial(&[TRIAL, CONFIRMED, TRIAL], &SAME, rewritten), [1]);
        assert_eq!(
            off_trial(
                &[Some(RunningImage::Unknown), TRIAL, CONFIRMED],
                &SAME,
                rewritten
            ),
            [0, 2]
        );
        assert_eq!(off_trial(&[CONFIRMED; 3], &SAME, rewritten), [0, 1, 2]);
        assert_eq!(
            off_trial(&[TRIAL, CONFIRMED], &SAME[..2], Expect::Exactly(SAME[0])),
            [1]
        );
    }

    #[test]
    fn a_device_whose_firmware_cannot_report_among_ones_that_can_is_off_trial() {
        let versions = [[0, 11, 0], [0, 10, 0], [0, 11, 0]];
        assert_eq!(
            off_trial(&[TRIAL, None, TRIAL], &versions, Expect::ChangedFrom(&SAME)),
            [1]
        );
    }

    #[test]
    fn firmware_that_cannot_report_falls_back_to_the_versions() {
        let before = [[0, 10, 1]; 3];
        let unreported = [None; 3];
        assert_eq!(
            off_trial(&unreported, &SAME, Expect::ChangedFrom(&before)),
            NONE
        );
        assert_eq!(
            off_trial(&unreported, &SAME, Expect::ChangedFrom(&SAME)),
            NONE
        );
        assert_eq!(
            off_trial(
                &unreported,
                &[[0, 10, 0], [0, 10, 1], [0, 10, 0]],
                Expect::ChangedFrom(&before)
            ),
            [1]
        );
        assert_eq!(
            off_trial(
                &unreported,
                &[[0, 10, 0], [0, 10, 2], [0, 10, 3]],
                Expect::ChangedFrom(&before)
            ),
            [0, 1, 2]
        );
        assert_eq!(
            off_trial(&unreported, &SAME, Expect::Exactly(SAME[0])),
            NONE
        );
    }

    #[test]
    fn the_error_names_each_device_that_is_off_trial() {
        let err = check_trial(&[TRIAL, CONFIRMED], &SAME[..2], Expect::ChangedFrom(&SAME))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("device 1 (confirmed image, CPU firmware 0.10.0)"),
            "{err}"
        );
        assert!(!err.contains("device 0"), "{err}");
    }
}
