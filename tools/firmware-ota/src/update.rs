use std::time::Duration;

use autd3_cpu_wire::fpga_update::FpgaBootImage;
use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::update::RunningImage;
use autd3_firmware_writer::bundle::MIN_CPU_FIRMWARE_VERSION;
use autd3_firmware_writer::image::CpuFirmwareImage;
use autd3_rs::commands::{
    FpgaUpdateActivate, FpgaUpdateBegin, FpgaUpdateChunk, FpgaUpdateCommit, Reboot, UpdateBegin,
    UpdateChunk, UpdateCommit, each,
};
use autd3_rs::{FirmwareVersion, Version};
use indicatif::{ProgressBar, ProgressStyle};

use crate::Connected;
use crate::error::OtaError;
use crate::fpga_image::FpgaFirmwareImage;

pub(crate) const ACK_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const REBOOT_ACK_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const FPGA_RECONFIG_WAIT: Duration =
    Duration::from_millis(autd3_cpu_wire::fpga_update::FPGA_RECONFIG_WORST_MS as u64 + 2000);
pub(crate) const FPGA_ACTIVATE_ATTEMPTS: usize = 3;

fn ensure_cpu_supported(infos: &[FirmwareVersion]) -> Result<(), OtaError> {
    let required = Version {
        major: MIN_CPU_FIRMWARE_VERSION[0],
        minor: MIN_CPU_FIRMWARE_VERSION[1],
        patch: MIN_CPU_FIRMWARE_VERSION[2],
    };
    match infos
        .iter()
        .enumerate()
        .find(|(_, info)| info.cpu < required)
    {
        None => Ok(()),
        Some((device, info)) => Err(OtaError::UnsupportedFirmware {
            device,
            found: info.cpu,
            required,
        }),
    }
}

async fn ensure_fpga_update_supported(client: &Connected) -> Result<(), OtaError> {
    let infos = client.read_firmware_version().await?;
    ensure_cpu_supported(&infos)?;
    match infos.iter().position(|info| !info.supports_fpga_ota()) {
        None => Ok(()),
        Some(device) => Err(OtaError::FpgaUpdateUnsupported { device }),
    }
}

pub(crate) async fn ensure_fpga_reconfigured(client: &Connected) -> Result<(), OtaError> {
    match client
        .read_firmware_version()
        .await?
        .iter()
        .position(|info| info.fpga_boot_image() == FpgaBootImage::ReconfigFailed)
    {
        None => Ok(()),
        Some(device) => Err(OtaError::FpgaReconfigFailed { device }),
    }
}

async fn stream(
    bytes: &[u8],
    mut send_chunk: impl AsyncFnMut(u32, &[u8]) -> Result<(), OtaError>,
) -> Result<(), OtaError> {
    let total = bytes.len();
    let bar = ProgressBar::new(total as u64).with_style(
        ProgressStyle::with_template("{wide_bar} {bytes}/{total_bytes} ({percent:>3}%) ETA {eta}")
            .expect("the template is a valid literal"),
    );
    let mut last_percent = 0;
    for (index, data) in bytes.chunks(UPDATE_CHUNK_MAX_DATA_LEN).enumerate() {
        let offset = index * UPDATE_CHUNK_MAX_DATA_LEN;
        send_chunk(
            u32::try_from(offset).expect("bounded by the slot capacity"),
            data,
        )
        .await?;
        let sent = offset + data.len();
        bar.set_position(sent as u64);
        let percent = sent * 100 / total;
        if bar.is_hidden() && percent != last_percent && percent.is_multiple_of(10) {
            eprintln!("{sent:>8} / {total} bytes ({percent:>3}%)");
        }
        last_percent = percent;
    }
    bar.finish();
    Ok(())
}

pub(crate) async fn update_fpga(
    client: &Connected,
    image: &FpgaFirmwareImage,
) -> Result<(), OtaError> {
    ensure_fpga_update_supported(client).await?;
    let bytes = image.as_bytes();
    client
        .send(FpgaUpdateBegin {
            length: u32::try_from(bytes.len()).expect("bounded by the slot capacity"),
            crc32: image.crc32(),
        })
        .await?;
    stream(bytes, async |offset, data| {
        client.send(FpgaUpdateChunk { offset, data }).await
    })
    .await?;
    client.send(FpgaUpdateCommit).await
}

pub(crate) async fn activate_unreconfigured_fpga(
    client: &Connected,
) -> Result<Vec<usize>, OtaError> {
    let failed: Vec<usize> = client
        .read_firmware_version()
        .await?
        .iter()
        .enumerate()
        .filter(|(_, info)| info.fpga_boot_image() == FpgaBootImage::ReconfigFailed)
        .map(|(device, _)| device)
        .collect();
    if failed.is_empty() {
        return Ok(failed);
    }
    client
        .send(each(|device| {
            failed.contains(&device.idx()).then_some(FpgaUpdateActivate)
        }))
        .await?;
    Ok(failed)
}

pub(crate) async fn update_cpu(
    client: &Connected,
    image: &CpuFirmwareImage,
) -> Result<(), OtaError> {
    ensure_cpu_supported(&client.read_firmware_version().await?)?;
    let bytes = image.as_bytes();
    client
        .send(UpdateBegin {
            length: u32::try_from(bytes.len()).expect("bounded by the slot capacity"),
            crc32: image.crc32(),
        })
        .await?;
    stream(bytes, async |offset, data| {
        client.send(UpdateChunk { offset, data }).await
    })
    .await?;
    client.send(UpdateCommit).await
}

pub(crate) async fn activate_unrebooted(client: &Connected) -> Result<Vec<usize>, OtaError> {
    let unrebooted: Vec<usize> = client
        .read_running_image()
        .await?
        .iter()
        .enumerate()
        .filter(|(_, image)| **image == Some(RunningImage::Confirmed))
        .map(|(device, _)| device)
        .collect();
    if unrebooted.is_empty() {
        return Ok(unrebooted);
    }
    client
        .send_within(
            REBOOT_ACK_TIMEOUT,
            each(|device| unrebooted.contains(&device.idx()).then_some(Reboot)),
        )
        .await?;
    Ok(unrebooted)
}
