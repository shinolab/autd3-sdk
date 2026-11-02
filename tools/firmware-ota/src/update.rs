use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use autd3_cpu_wire::describe_device_error;
use autd3_cpu_wire::fpga_update::FpgaBootImage;
use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::update::RunningImage;
use autd3_firmware_writer::bundle::MIN_CPU_FIRMWARE_VERSION;
use autd3_firmware_writer::image::CpuFirmwareImage;
use autd3_rs::commands::{
    Command, Expansion, FpgaUpdateActivate, FpgaUpdateBegin, FpgaUpdateChunk, FpgaUpdateCommit,
    Operation, Reboot, UpdateBegin, UpdateChunk, UpdateCommit, UpdateConfirm,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::protocol::Cmd;
use autd3_rs::{Client, DeviceState, Error, FirmwareVersion, TransportOption};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::fpga_image::FpgaFirmwareImage;

const HEARTBEAT: Duration = Duration::from_millis(10);
pub(crate) const ACK_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const FPGA_RECONFIG_WAIT: Duration =
    Duration::from_millis(autd3_cpu_wire::fpga_update::FPGA_RECONFIG_WORST_MS as u64 + 2000);
pub(crate) const FPGA_ACTIVATE_ATTEMPTS: usize = 3;

#[derive(Debug, thiserror::Error)]
pub(crate) enum OtaError {
    #[error("network error: {0}")]
    Network(#[source] Box<Error>),
    #[error("not every device acknowledged {cmd:?}; {}", timeout_hint(*cmd))]
    Timeout { cmd: Cmd },
    #[error("device {device} rejected {cmd:?} with firmware error {code:#04x}: {}{}", describe_device_error(*code), device_hint(*cmd, *code))]
    Device { device: usize, cmd: Cmd, code: u8 },
    #[error("device {device} returned a shorter reply to {cmd:?} than the command defines")]
    ShortReply { device: usize, cmd: Cmd },
    #[error(
        "device {device} runs an FPGA image without configuration-flash access; flash `autd3-fpga.mcs` once via JTAG"
    )]
    FpgaUpdateUnsupported { device: usize },
    #[error(
        "device {device} did not reconfigure after activation; it keeps its output off and boots the written image at the next power cycle"
    )]
    FpgaReconfigFailed { device: usize },
    #[error(
        "device {device} runs CPU firmware {}.{}.{}, but the update needs {}.{}.{} or newer; flash it once via J-Link",
        found[0], found[1], found[2], required[0], required[1], required[2]
    )]
    UnsupportedFirmware {
        device: usize,
        found: [u8; 3],
        required: [u8; 3],
    },
}

fn timeout_hint(cmd: Cmd) -> &'static str {
    match cmd {
        Cmd::UpdateBegin | Cmd::UpdateChunk | Cmd::UpdateCommit => {
            "the device keeps the half-written slot and boots its previous image; rerun the update from the beginning"
        }
        Cmd::FpgaUpdateBegin | Cmd::FpgaUpdateChunk | Cmd::FpgaUpdateCommit => {
            "the device stays locked with its output off until the next power cycle; power-cycle it and rerun the FPGA update"
        }
        Cmd::UpdateConfirm => {
            "the running image stays unconfirmed on the devices that did not answer; check the cable and rerun with `--confirm-only` before the next power cycle"
        }
        Cmd::Reboot => {
            "the devices that received the request are resetting and come back unassigned; check which image each device runs with `--verify-only` before resetting them again (a device already running an unconfirmed trial image rolls back to its previous image when it is reset again)"
        }
        Cmd::Synchronize
        | Cmd::Clear
        | Cmd::Nop
        | Cmd::SetCpuConfig
        | Cmd::ReleaseFailsafe
        | Cmd::SetFpgaConfig
        | Cmd::WriteFociBuffer
        | Cmd::ConfigPattern
        | Cmd::ActivatePatternBank
        | Cmd::WritePatternPhase
        | Cmd::WritePatternRaw
        | Cmd::WritePatternFused
        | Cmd::WriteModulationBuffer
        | Cmd::ConfigModulation
        | Cmd::ActivateModulationBank
        | Cmd::SetSilencer
        | Cmd::SetPhaseCorrection
        | Cmd::SetOutputMask
        | Cmd::SetPulseWidthTable
        | Cmd::EmulateGpioIn
        | Cmd::SetGpioOut
        | Cmd::ForceFan
        | Cmd::FpgaUpdateActivate
        | Cmd::ReadFpgaState
        | Cmd::ReadTelemetry
        | Cmd::ReadFirmwareInfo
        | Cmd::ReadRunningImage
        | Cmd::WriteFpgaMemory
        | Cmd::ReadFpgaMemory => "check the cable and the host firewall, then rerun",
        _ => unreachable!(),
    }
}

fn device_hint(cmd: Cmd, code: u8) -> &'static str {
    match (cmd, autd3_cpu_wire::Error::from_u8(code)) {
        (Cmd::Reboot, Some(autd3_cpu_wire::Error::FpgaUpdateInProgress)) => {
            " (an FPGA update holds the device until it reconfigures or is power-cycled; a committed CPU image boots as a trial at the next power cycle, run `--confirm-only` after that)"
        }
        (
            Cmd::UpdateBegin | Cmd::FpgaUpdateBegin,
            Some(autd3_cpu_wire::Error::UpdateActivating),
        ) => " (wait for the reboot, then reconnect and rerun)",
        (_, Some(autd3_cpu_wire::Error::UpdateImageInvalid)) => {
            " (nothing was activated; the previous image keeps running, rerun the update)"
        }
        (
            _,
            None
            | Some(
                autd3_cpu_wire::Error::None
                | autd3_cpu_wire::Error::UnknownCmd
                | autd3_cpu_wire::Error::InvalidPayload
                | autd3_cpu_wire::Error::InvalidSilencerSetting
                | autd3_cpu_wire::Error::FpgaTimeout
                | autd3_cpu_wire::Error::SyncNotReady
                | autd3_cpu_wire::Error::UpdateNotStarted
                | autd3_cpu_wire::Error::UpdateFlash
                | autd3_cpu_wire::Error::UpdateNotCommitted
                | autd3_cpu_wire::Error::UpdateNothingToConfirm
                | autd3_cpu_wire::Error::UpdateUnsupported
                | autd3_cpu_wire::Error::FpgaUpdateInProgress
                | autd3_cpu_wire::Error::UpdateActivating
                | autd3_cpu_wire::Error::SyncMissed
                | autd3_cpu_wire::Error::FailsafeConditionActive,
            ),
        ) => "",
        (_, Some(_)) => unreachable!(),
    }
}

fn first_unsupported(versions: &[[u8; 3]]) -> Option<(usize, [u8; 3])> {
    versions
        .iter()
        .copied()
        .enumerate()
        .find(|&(_, version)| version < MIN_CPU_FIRMWARE_VERSION)
}

fn ensure_cpu_supported(infos: &[FirmwareVersion]) -> Result<(), OtaError> {
    let versions: Vec<[u8; 3]> = infos.iter().map(|info| version_triple(info.cpu)).collect();
    match first_unsupported(&versions) {
        None => Ok(()),
        Some((device, found)) => Err(OtaError::UnsupportedFirmware {
            device,
            found,
            required: MIN_CPU_FIRMWARE_VERSION,
        }),
    }
}

#[must_use]
pub(crate) fn version_triple(version: autd3_rs::Version) -> [u8; 3] {
    [version.major, version.minor, version.patch]
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UpdateProgress {
    pub(crate) sent: usize,
    pub(crate) total: usize,
}

pub(crate) struct KeepaliveClient {
    client: Arc<Client>,
    keepalive: JoinHandle<()>,
}

impl KeepaliveClient {
    pub(crate) async fn open(option: &TransportOption, num_devices: usize) -> Result<Self, Error> {
        let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()));
        let client = Arc::new(Client::open(&geometry, option).await?);
        let keepalive = tokio::spawn({
            let client = Arc::clone(&client);
            async move {
                let mut ticker = tokio::time::interval(HEARTBEAT);
                ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
                loop {
                    ticker.tick().await;
                    if client.send_heartbeat().await.is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Self { client, keepalive })
    }
}

impl Deref for KeepaliveClient {
    type Target = Client;

    fn deref(&self) -> &Client {
        &self.client
    }
}

impl Drop for KeepaliveClient {
    fn drop(&mut self) {
        self.keepalive.abort();
    }
}

struct ExplicitOperation<O>(O);

impl<O: Operation> Command for ExplicitOperation<O> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        expansion.push_op(&self.0)
    }
}

struct ExplicitOperationOn<'a, O> {
    operation: O,
    devices: &'a [usize],
}

impl<O: Operation> Command for ExplicitOperationOn<'_, O> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        expansion.push_per_device(|device| {
            self.devices
                .contains(&device.idx())
                .then_some(&self.operation)
        })
    }
}

#[must_use]
fn ota_error(cmd: Cmd, e: Error) -> OtaError {
    match e {
        Error::Timeout { .. } | Error::DeviceLost { .. } => OtaError::Timeout { cmd },
        Error::DeviceError { device, code } => OtaError::Device { device, cmd, code },
        Error::UnexpectedReply { device } => OtaError::ShortReply { device, cmd },
        Error::Network(_) | Error::DeviceTimeUnknown | Error::InvalidPayload(_) | Error::Closed => {
            OtaError::Network(Box::new(e))
        }
        _ => unreachable!(),
    }
}

fn ensure_no_device_lost(client: &Client, cmd: Cmd) -> Result<(), OtaError> {
    let lost = client
        .device_status()
        .map_err(|e| ota_error(cmd, e))?
        .iter()
        .any(|status| status.state == DeviceState::Lost);
    if lost {
        Err(OtaError::Timeout { cmd })
    } else {
        Ok(())
    }
}

async fn send(client: &Client, cmd: Cmd, command: impl Command) -> Result<(), OtaError> {
    client.send(command).await.map_err(|e| ota_error(cmd, e))?;
    ensure_no_device_lost(client, cmd)
}

pub(crate) async fn read_firmware_version(
    client: &Client,
) -> Result<Vec<FirmwareVersion>, OtaError> {
    client
        .read_firmware_version()
        .await
        .map_err(|e| ota_error(Cmd::ReadFirmwareInfo, e))
}

pub(crate) async fn read_running_image(
    client: &Client,
) -> Result<Vec<Option<RunningImage>>, OtaError> {
    client
        .read_running_image()
        .await
        .map_err(|e| ota_error(Cmd::ReadRunningImage, e))
}

async fn ensure_fpga_update_supported(client: &Client) -> Result<(), OtaError> {
    let infos = read_firmware_version(client).await?;
    ensure_cpu_supported(&infos)?;
    match infos.iter().position(|info| !info.supports_fpga_ota()) {
        None => Ok(()),
        Some(device) => Err(OtaError::FpgaUpdateUnsupported { device }),
    }
}

pub(crate) async fn ensure_fpga_reconfigured(client: &Client) -> Result<(), OtaError> {
    match read_firmware_version(client)
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
    mut on_progress: impl FnMut(UpdateProgress),
    mut send_chunk: impl AsyncFnMut(u32, &[u8]) -> Result<(), OtaError>,
) -> Result<(), OtaError> {
    let total = bytes.len();
    on_progress(UpdateProgress { sent: 0, total });
    for (index, data) in bytes.chunks(UPDATE_CHUNK_MAX_DATA_LEN).enumerate() {
        let offset = index * UPDATE_CHUNK_MAX_DATA_LEN;
        send_chunk(
            u32::try_from(offset).expect("bounded by the slot capacity"),
            data,
        )
        .await?;
        on_progress(UpdateProgress {
            sent: offset + data.len(),
            total,
        });
    }
    Ok(())
}

pub(crate) async fn update_fpga(
    client: &Client,
    image: &FpgaFirmwareImage,
    on_progress: impl FnMut(UpdateProgress),
) -> Result<(), OtaError> {
    ensure_fpga_update_supported(client).await?;
    let bytes = image.as_bytes();
    send(
        client,
        Cmd::FpgaUpdateBegin,
        ExplicitOperation(FpgaUpdateBegin {
            length: u32::try_from(bytes.len()).expect("bounded by the slot capacity"),
            crc32: image.crc32(),
        }),
    )
    .await?;
    stream(bytes, on_progress, async |offset, data| {
        send(
            client,
            Cmd::FpgaUpdateChunk,
            ExplicitOperation(FpgaUpdateChunk { offset, data }),
        )
        .await
    })
    .await?;
    send(
        client,
        Cmd::FpgaUpdateCommit,
        ExplicitOperation(FpgaUpdateCommit),
    )
    .await
}

pub(crate) async fn activate_fpga(client: &Client) -> Result<(), OtaError> {
    send(
        client,
        Cmd::FpgaUpdateActivate,
        ExplicitOperation(FpgaUpdateActivate),
    )
    .await
}

pub(crate) async fn activate_unreconfigured_fpga(client: &Client) -> Result<Vec<usize>, OtaError> {
    let failed: Vec<usize> = read_firmware_version(client)
        .await?
        .iter()
        .enumerate()
        .filter(|(_, info)| info.fpga_boot_image() == FpgaBootImage::ReconfigFailed)
        .map(|(device, _)| device)
        .collect();
    if failed.is_empty() {
        return Ok(failed);
    }
    send(
        client,
        Cmd::FpgaUpdateActivate,
        ExplicitOperationOn {
            operation: FpgaUpdateActivate,
            devices: &failed,
        },
    )
    .await?;
    Ok(failed)
}

pub(crate) async fn update_cpu(
    client: &Client,
    image: &CpuFirmwareImage,
    on_progress: impl FnMut(UpdateProgress),
) -> Result<(), OtaError> {
    ensure_cpu_supported(&read_firmware_version(client).await?)?;
    let bytes = image.as_bytes();
    send(
        client,
        Cmd::UpdateBegin,
        ExplicitOperation(UpdateBegin {
            length: u32::try_from(bytes.len()).expect("bounded by the slot capacity"),
            crc32: image.crc32(),
        }),
    )
    .await?;
    stream(bytes, on_progress, async |offset, data| {
        send(
            client,
            Cmd::UpdateChunk,
            ExplicitOperation(UpdateChunk { offset, data }),
        )
        .await
    })
    .await?;
    send(client, Cmd::UpdateCommit, ExplicitOperation(UpdateCommit)).await
}

pub(crate) async fn confirm(client: &Client) -> Result<(), OtaError> {
    send(client, Cmd::UpdateConfirm, ExplicitOperation(UpdateConfirm)).await
}

async fn send_reboot(client: &Client, command: impl Command) -> Result<(), OtaError> {
    client.set_ack_timeout(Duration::from_secs(5));
    send(client, Cmd::Reboot, command).await
}

pub(crate) async fn reboot(client: &Client) -> Result<(), OtaError> {
    send_reboot(client, ExplicitOperation(Reboot)).await
}

pub(crate) async fn activate_unrebooted(client: &Client) -> Result<Vec<usize>, OtaError> {
    let unrebooted: Vec<usize> = read_running_image(client)
        .await?
        .iter()
        .enumerate()
        .filter(|(_, image)| **image == Some(RunningImage::Confirmed))
        .map(|(device, _)| device)
        .collect();
    if unrebooted.is_empty() {
        return Ok(unrebooted);
    }
    send_reboot(
        client,
        ExplicitOperationOn {
            operation: Reboot,
            devices: &unrebooted,
        },
    )
    .await?;
    Ok(unrebooted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_names_the_first_device_below_the_minimum() {
        assert_eq!(first_unsupported(&[]), None);
        assert_eq!(first_unsupported(&[[0, 10, 0], [1, 0, 0]]), None);
        assert_eq!(
            first_unsupported(&[[0, 10, 0], [0, 9, 99], [0, 6, 1]]),
            Some((1, [0, 9, 99]))
        );
        assert_eq!(first_unsupported(&[[0, 6, 1]]), Some((0, [0, 6, 1])));
    }
}
