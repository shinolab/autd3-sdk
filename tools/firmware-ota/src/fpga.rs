use anyhow::{Context, Result, bail};

use autd3_cpu_wire::fpga_update::FpgaBootImage;
use autd3_rs::commands::FpgaUpdateActivate;
use autd3_rs::{Client, FirmwareVersion};

use crate::cli::{Connection, UpdateArgs};
use crate::connect;
use crate::fpga_image::FpgaFirmwareImage;
use crate::update::{
    FPGA_ACTIVATE_ATTEMPTS, FPGA_RECONFIG_WAIT, activate_unreconfigured_fpga,
    ensure_fpga_reconfigured, update_fpga,
};

pub(crate) async fn main_fpga(
    connection: &Connection,
    update: &UpdateArgs,
    bytes: Vec<u8>,
    invalid_image_hint: &'static str,
) -> Result<()> {
    let image = FpgaFirmwareImage::from_update_bin(bytes).context(invalid_image_hint)?;
    println!(
        "FPGA update image: {} bytes, crc32 {:#010x}",
        image.len(),
        image.crc32()
    );
    let client = connect(connection, connection.devices).await?;
    print_fpga(&client, "before").await?;
    update_fpga(&client, &image).await?;
    println!(
        "FPGA image committed on every device (transducer output stays off until reconfiguration)"
    );
    if update.no_activate {
        println!(
            "not activated (--no-activate); output stays off until the next power cycle, \
             which boots the new image"
        );
        return Ok(());
    }
    client.send(FpgaUpdateActivate).await?;
    for attempt in 1..=FPGA_ACTIVATE_ATTEMPTS {
        println!(
            "activation acknowledged; waiting {FPGA_RECONFIG_WAIT:?} for the FPGAs to reconfigure"
        );
        tokio::time::sleep(FPGA_RECONFIG_WAIT).await;
        if attempt == FPGA_ACTIVATE_ATTEMPTS {
            break;
        }
        let retried = activate_unreconfigured_fpga(&client).await?;
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
    let images = print_fpga(&client, "after").await?;
    ensure_fpga_reconfigured(&client).await?;
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

pub(crate) async fn print_fpga(client: &Client, label: &str) -> Result<Vec<FpgaBootImage>> {
    let infos = client.read_firmware_version().await?;
    for (device, info) in infos.iter().enumerate() {
        println!(
            "device {device}: FPGA firmware {label} = {} ({:?} image)",
            info.fpga,
            info.fpga_boot_image()
        );
    }
    Ok(infos.iter().map(FirmwareVersion::fpga_boot_image).collect())
}
