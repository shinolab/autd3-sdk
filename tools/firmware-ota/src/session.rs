use std::io::IsTerminal;

use anyhow::{Result, bail};

use autd3_cpu_wire::fpga_update::FpgaBootImage;
use autd3_rs::{Client, FirmwareVersion};
use autd3_rs_firmware_ota::{
    CpuFirmwareImage, FPGA_ACTIVATE_ATTEMPTS, FPGA_RECONFIG_WAIT, FpgaFirmwareImage, OtaError,
    UpdateProgress, activate_fpga, activate_unreconfigured_fpga, connect, ensure_fpga_reconfigured,
    read_firmware_version, read_running_image, reboot, update_cpu, update_fpga, version_triple,
};

use crate::cli::Cli;
use crate::verify::{
    Expect, Version, check_expectation, check_trial, describe_running_image, fmt_version,
};

#[derive(Clone, Copy)]
pub(crate) enum Stage<'a> {
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

pub(crate) struct Outcome {
    pub(crate) devices: usize,
    pub(crate) cpu_versions: Vec<Version>,
    pub(crate) activation_unacknowledged: bool,
    pub(crate) activated_late: Vec<usize>,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn open_and_run(cli: &Cli, devices: usize, stage: Stage<'_>) -> Result<Outcome> {
    let client = connect(cli.interface.as_deref(), cli.open_timeout, devices)?;
    let devices = client.num_devices();
    println!("{devices} device(s) on the bus");
    let mut activation_unacknowledged = false;
    let mut activated_late = Vec::new();
    let cpu_versions = match stage {
        Stage::Update { image, activate } => {
            let before = print_versions(&client, "before")?;
            update_cpu(&client, image, progress_printer(image.len()))?;
            end_progress();
            println!("image committed on every device");
            if activate {
                activation_unacknowledged = match reboot(&client) {
                    Ok(()) => {
                        println!("activation acknowledged; the devices reboot in about 100 ms");
                        false
                    }
                    Err(OtaError::Timeout { .. }) => {
                        eprintln!(
                            "warning: not every device acknowledged the activation; the devices that \
                         have not rebooted are activated after the reconnection"
                        );
                        true
                    }
                    Err(e) => return Err(e.into()),
                };
            }
            before
        }
        Stage::FpgaUpdate { image, activate } => {
            print_fpga(&client, "before")?;
            update_fpga(&client, image, progress_printer(image.len()))?;
            end_progress();
            println!(
                "FPGA image committed on every device (transducer output stays off until reconfiguration)"
            );
            if activate {
                activate_fpga(&client)?;
                for attempt in 1..=FPGA_ACTIVATE_ATTEMPTS {
                    println!(
                        "activation acknowledged; waiting {FPGA_RECONFIG_WAIT:?} for the FPGAs to reconfigure"
                    );
                    std::thread::sleep(FPGA_RECONFIG_WAIT);
                    if attempt == FPGA_ACTIVATE_ATTEMPTS {
                        break;
                    }
                    let retried = activate_unreconfigured_fpga(&client)?;
                    if retried.is_empty() {
                        break;
                    }
                    for device in &retried {
                        println!(
                            "device {device} did not reconfigure; activating it again ({}/{FPGA_ACTIVATE_ATTEMPTS})",
                            attempt + 1
                        );
                    }
                }
                let images = print_fpga(&client, "after")?;
                ensure_fpga_reconfigured(&client)?;
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
            } else {
                println!(
                    "not activated (--no-activate); output stays off until the next power cycle, \
                     which boots the new image"
                );
            }
            Vec::new()
        }
        Stage::FpgaVerify => {
            print_fpga(&client, "now")?;
            Vec::new()
        }
        Stage::Verify {
            confirm,
            expect,
            activate_unrebooted,
            require_trial,
        } => {
            if activate_unrebooted {
                activated_late = autd3_rs_firmware_ota::activate_unrebooted(&client)?;
            }
            if activated_late.is_empty() {
                let label = if confirm { "after" } else { "now" };
                let after = print_versions(&client, label)?;
                check_expectation(&after, expect)?;
                let images = read_running_image(&client)?;
                for (device, &image) in images.iter().enumerate() {
                    println!("device {device}: CPU {}", describe_running_image(image));
                }
                if require_trial {
                    check_trial(&images, &after, expect)?;
                }
                if confirm {
                    autd3_rs_firmware_ota::confirm(&client)?;
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
            }
        }
    };
    Ok(Outcome {
        devices,
        cpu_versions,
        activation_unacknowledged,
        activated_late,
    })
}

fn print_versions(client: &Client, label: &str) -> Result<Vec<Version>> {
    let versions: Vec<Version> = read_firmware_version(client)?
        .iter()
        .map(|info| version_triple(info.cpu))
        .collect();
    for (device, &version) in versions.iter().enumerate() {
        println!(
            "device {device}: CPU firmware {label} = {}",
            fmt_version(version)
        );
    }
    Ok(versions)
}

fn print_fpga(client: &Client, label: &str) -> Result<Vec<FpgaBootImage>> {
    let infos = read_firmware_version(client)?;
    for (device, info) in infos.iter().enumerate() {
        println!(
            "device {device}: FPGA firmware {label} = {} ({:?} image)",
            info.fpga,
            info.fpga_boot_image()
        );
    }
    Ok(infos.iter().map(FirmwareVersion::fpga_boot_image).collect())
}

fn progress_printer(total: usize) -> impl FnMut(UpdateProgress) {
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
