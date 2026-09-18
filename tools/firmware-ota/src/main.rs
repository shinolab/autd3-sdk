use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};

use autd3_rs_core::geometry::{Autd3, Geometry};
use autd3_rs_core::link::{IntoLink, Link};
use autd3_rs_firmware_ota::{CpuFirmwareImage, Driver};
use autd3_rs_link_echocat::EchocatLinkOption;
use autd3_rs_link_remote::RemoteLinkOption;
use autd3_rs_link_twincat::{AmsNetId, TwinCATLinkOption};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum LinkKind {
    #[default]
    Echocat,
    Twincat,
    Remote,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "autd3-rs-firmware-ota", about)]
struct Cli {
    image: PathBuf,
    #[arg(long)]
    slot_image: bool,
    #[arg(long, value_enum, default_value_t = LinkKind::Echocat)]
    link: LinkKind,
    #[arg(long)]
    interface: Option<String>,
    #[arg(long, default_value_t = 1)]
    devices: usize,
    #[arg(long, default_value_t = 1000)]
    cycle_us: u64,
    #[arg(long, default_value_t = default_remote_addr())]
    remote_addr: SocketAddr,
    #[arg(long)]
    twincat_remote: Option<IpAddr>,
    #[arg(long)]
    ams_net_id: Option<AmsNetId>,
    #[arg(long)]
    no_activate: bool,
    #[arg(long, default_value_t = 10)]
    reboot_wait_secs: u64,
    #[arg(long)]
    verify_only: bool,
    #[arg(long, conflicts_with_all = ["verify_only", "no_activate"])]
    no_confirm: bool,
    #[arg(long, conflicts_with = "verify_only")]
    confirm_only: bool,
}

fn default_remote_addr() -> SocketAddr {
    "127.0.0.1:8080".parse().expect("valid default addr")
}

#[derive(Clone, Copy)]
enum Stage<'a> {
    Update(&'a CpuFirmwareImage, bool),
    Verify { confirm: bool },
}

fn open_and_run(cli: &Cli, geometry: &Geometry, stage: Stage<'_>) -> Result<()> {
    match cli.link {
        LinkKind::Echocat => {
            let option = EchocatLinkOption {
                iface: cli.interface.clone().into(),
                sync0_period: Duration::from_micros(cli.cycle_us),
                ..Default::default()
            };
            run(option.into_link(geometry)?, stage)
        }
        LinkKind::Twincat => {
            let option = match (cli.twincat_remote, cli.ams_net_id) {
                (Some(addr), Some(ams_net_id)) => TwinCATLinkOption::remote(addr, ams_net_id),
                _ => TwinCATLinkOption::local(),
            };
            run(option.into_link(geometry)?, stage)
        }
        LinkKind::Remote => run(
            RemoteLinkOption::new(cli.remote_addr).into_link(geometry)?,
            stage,
        ),
    }
}

fn print_versions<L: Link>(driver: &mut Driver<L>, label: &str) -> Result<()> {
    for (device, (major, minor, patch)) in driver.read_cpu_version()?.into_iter().enumerate() {
        println!("device {device}: CPU firmware {label} = {major}.{minor}.{patch}");
    }
    Ok(())
}

fn run<L: Link>(link: L, stage: Stage<'_>) -> Result<()> {
    let mut driver = Driver::open(link).context("opening the link / protocol handshake")?;
    match stage {
        Stage::Update(image, activate) => {
            print_versions(&mut driver, "before")?;
            let total = image.len();
            let mut last_percent = None;
            driver.update(image, |p| {
                let percent = p.sent * 100 / total;
                if last_percent != Some(percent) {
                    last_percent = Some(percent);
                    eprint!("\r{:>7} / {total} bytes ({percent:>3}%)", p.sent);
                }
            })?;
            eprintln!();
            println!("image committed on every device");
            if activate {
                driver.activate()?;
                println!("activation acknowledged; the devices reboot in about 100 ms");
            }
            driver.close()?;
        }
        Stage::Verify { confirm } => {
            print_versions(&mut driver, "after")?;
            if confirm {
                driver.confirm()?;
                println!("running image confirmed on every device");
            }
            driver.close()?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.devices == 0 {
        bail!("--devices must be at least 1");
    }
    let bytes =
        std::fs::read(&cli.image).with_context(|| format!("reading {}", cli.image.display()))?;
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

    let geometry = Geometry::new((0..cli.devices).map(|_| Autd3::default()).collect());
    if cli.verify_only || cli.confirm_only {
        return open_and_run(
            &cli,
            &geometry,
            Stage::Verify {
                confirm: cli.confirm_only,
            },
        );
    }
    open_and_run(&cli, &geometry, Stage::Update(&image, !cli.no_activate))?;
    if cli.no_activate {
        println!(
            "not activated (--no-activate); the new image boots after the next UpdateActivate or never"
        );
        return Ok(());
    }

    let wait = Duration::from_secs(cli.reboot_wait_secs);
    println!("waiting {wait:?} for the devices to reboot and the bus to re-enumerate");
    std::thread::sleep(wait);
    open_and_run(
        &cli,
        &geometry,
        Stage::Verify {
            confirm: !cli.no_confirm,
        },
    )?;
    if cli.no_confirm {
        println!(
            "not confirmed (--no-confirm); the previous image boots again after the next reset \
             unless `--confirm-only` is run first"
        );
        return Ok(());
    }
    println!("Ok!");
    Ok(())
}
