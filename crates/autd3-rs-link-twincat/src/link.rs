use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use ads::notif::{Attributes, Handle, Notification, TransmissionMode};
use ads::{AmsAddr, AmsNetId, Client, Source, Timeouts};
use autd3_rs_core::{CycleOutcome, DcClock, IntoLink, Link, RX_FRAME_BYTES, TX_FRAME_BYTES};
use crossbeam_channel::Receiver;

use crate::error::TwinCATLinkError;
use crate::state_check::TwinCATStateChecker;

pub(crate) const AUTD_INDEX_GROUP: u32 = 0x0304_0030;
const AUTD_INDEX_OFFSET_TX: u32 = 0x8100_0000;
const AUTD_INDEX_OFFSET_INPUT_BASE: u32 = 0x8000_0000;
const CFG_SLAVE_COUNT_BYTES: u32 = 2;
const AUTD_INDEX_OFFSET_COUNT: u32 = AUTD_INDEX_OFFSET_INPUT_BASE;
pub(crate) const AUTD_INDEX_OFFSET_RX: u32 = AUTD_INDEX_OFFSET_INPUT_BASE + CFG_SLAVE_COUNT_BYTES;
const AUTD_AMS_PORT: u16 = 301;
const MAX_DEVICES: usize = 128;
pub(crate) const STATE_BYTES_PER_DEVICE: usize = 2;
const DC_OFFSET_BYTES: usize = 8;
const DC_OFFSET_READ_INTERVAL: Duration = Duration::from_millis(100);
const DC_OFFSET_PLAUSIBLE_NS: i64 = 24 * 60 * 60 * 1_000_000_000;

// The task input image built by tools/twincat-cli is
//   [cfg_slave_count u16][input[] rx][state[] u16][dc_to_tc_offset i64]
pub(crate) fn state_index(num_devices: usize) -> u32 {
    AUTD_INDEX_OFFSET_RX
        + u32::try_from(num_devices * RX_FRAME_BYTES).expect("input image size exceeds u32")
}

pub(crate) fn dc_offset_index(num_devices: usize) -> u32 {
    state_index(num_devices)
        + u32::try_from(num_devices * STATE_BYTES_PER_DEVICE).expect("input image size exceeds u32")
}

pub enum TwinCATServer {
    Local,
    Remote { addr: IpAddr, ams_net_id: AmsNetId },
}

pub struct TwinCATLinkOption {
    pub server: TwinCATServer,
    pub timeouts: Timeouts,
}

impl TwinCATLinkOption {
    #[must_use]
    pub fn local() -> Self {
        Self::local_with_timeouts(Timeouts::none())
    }

    #[must_use]
    pub fn local_with_timeouts(timeouts: Timeouts) -> Self {
        Self {
            server: TwinCATServer::Local,
            timeouts,
        }
    }

    #[must_use]
    pub fn remote(addr: IpAddr, ams_net_id: AmsNetId) -> Self {
        Self::remote_with_timeouts(addr, ams_net_id, Timeouts::none())
    }

    #[must_use]
    pub fn remote_with_timeouts(addr: IpAddr, ams_net_id: AmsNetId, timeouts: Timeouts) -> Self {
        Self {
            server: TwinCATServer::Remote { addr, ams_net_id },
            timeouts,
        }
    }
}

impl IntoLink for TwinCATLinkOption {
    type Link = TwinCATLink;

    async fn into_link(
        self,
        _geometry: &autd3_rs_core::Geometry,
    ) -> Result<TwinCATLink, autd3_rs_core::error::LinkError> {
        TwinCATLink::open(self).map_err(|e| autd3_rs_core::error::LinkError(e.to_string()))
    }
}

enum RxSource {
    Ads,
    Notify {
        recv: Receiver<Notification>,
        handle: Handle,
        buf: Vec<u8>,
    },
}

pub struct TwinCATLink {
    client: Client,
    ams_addr: AmsAddr,
    num_devices: usize,
    rx: RxSource,
    conn_addr: SocketAddr,
    source: Source,
    timeouts: Timeouts,
    dc_clock: DcClock,
    dc_offset_index: u32,
    dc_next_read: Option<std::time::Instant>,
    dc_warned: bool,
}

impl TwinCATLink {
    pub fn open(option: TwinCATLinkOption) -> Result<Self, TwinCATLinkError> {
        let TwinCATLinkOption { server, timeouts } = option;

        let (client, ams_addr, conn_addr, source) = match server {
            TwinCATServer::Local => {
                let conn_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), ads::PORT);
                let source = Source::Request;
                let client = Client::new(conn_addr, timeouts, source)?;
                let net_id = client.source().netid();
                (
                    client,
                    AmsAddr::new(net_id, AUTD_AMS_PORT),
                    conn_addr,
                    source,
                )
            }
            TwinCATServer::Remote { addr, ams_net_id } => {
                let conn_addr = SocketAddr::new(addr, ads::PORT);
                let source = Source::Auto;
                let client = Client::new(conn_addr, timeouts, source)?;
                (
                    client,
                    AmsAddr::new(ams_net_id, AUTD_AMS_PORT),
                    conn_addr,
                    source,
                )
            }
        };

        let num_devices = Self::read_device_count(&client, ams_addr)?;

        let rx = match Self::register_notification(&client, ams_addr, num_devices) {
            Ok(rx) => rx,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to register ADS notification; falling back to synchronous ADS read"
                );
                RxSource::Ads
            }
        };

        Ok(Self {
            client,
            ams_addr,
            num_devices,
            rx,
            conn_addr,
            source,
            timeouts,
            dc_clock: DcClock::new(),
            dc_offset_index: dc_offset_index(num_devices),
            dc_next_read: None,
            dc_warned: false,
        })
    }

    fn refresh_dc_offset(&mut self) {
        let now = std::time::Instant::now();
        if self.dc_next_read.is_some_and(|next| now < next) {
            return;
        }
        self.dc_next_read = Some(now + DC_OFFSET_READ_INTERVAL);

        let mut buf = [0u8; DC_OFFSET_BYTES];
        match self.client.device(self.ams_addr).read_exact(
            AUTD_INDEX_GROUP,
            self.dc_offset_index,
            &mut buf,
        ) {
            Ok(()) => {
                let offset_ns = i64::from_le_bytes(buf);
                if offset_ns.abs() > DC_OFFSET_PLAUSIBLE_NS {
                    self.warn_no_dc_offset(format_args!(
                        "InfoData^DcToTcTimeOffset read back an implausible {offset_ns} ns"
                    ));
                    return;
                }
                self.dc_clock.observe_offset(offset_ns);
            }
            Err(e) => self.warn_no_dc_offset(format_args!("{e}")),
        }
    }

    fn warn_no_dc_offset(&mut self, reason: std::fmt::Arguments<'_>) {
        if self.dc_warned {
            return;
        }
        self.dc_warned = true;
        tracing::warn!(
            "could not read the TwinCAT DC clock offset ({reason}); TransitionMode::SysTime and \
             GpioOut::SysTimeEq will drift with the bus clock. Re-run the TwinCAT setup so the \
             task input image carries InfoData^DcToTcTimeOffset",
        );
    }

    fn read_device_count(client: &Client, ams_addr: AmsAddr) -> Result<usize, TwinCATLinkError> {
        let mut buf = [0u8; CFG_SLAVE_COUNT_BYTES as usize];
        client
            .device(ams_addr)
            .read_exact(AUTD_INDEX_GROUP, AUTD_INDEX_OFFSET_COUNT, &mut buf)?;
        let num_devices = usize::from(u16::from_le_bytes(buf));
        if num_devices == 0 || num_devices > MAX_DEVICES {
            return Err(TwinCATLinkError::InvalidDeviceCount { found: num_devices });
        }
        Ok(num_devices)
    }

    fn register_notification(
        client: &Client,
        ams_addr: AmsAddr,
        num_devices: usize,
    ) -> Result<RxSource, TwinCATLinkError> {
        let recv = client.get_notification_channel();
        let attributes = Attributes::new(
            num_devices * RX_FRAME_BYTES,
            TransmissionMode::ServerOnChange,
            Duration::ZERO,
            Duration::ZERO,
        );
        let handle = client.device(ams_addr).add_notification(
            AUTD_INDEX_GROUP,
            AUTD_INDEX_OFFSET_RX,
            &attributes,
        )?;
        Ok(RxSource::Notify {
            recv,
            handle,
            buf: vec![0; num_devices * RX_FRAME_BYTES],
        })
    }
}

impl Link for TwinCATLink {
    type Error = TwinCATLinkError;
    type Checker = TwinCATStateChecker;

    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn state_checker(&self) -> TwinCATStateChecker {
        TwinCATStateChecker::new(
            self.conn_addr,
            self.source,
            self.timeouts,
            self.ams_addr,
            self.num_devices,
        )
    }

    fn dc_clock(&self) -> Option<DcClock> {
        Some(self.dc_clock.clone())
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        self.refresh_dc_offset();
        let device = self.client.device(self.ams_addr);

        device.write(AUTD_INDEX_GROUP, AUTD_INDEX_OFFSET_TX, tx.as_flattened())?;

        let rx_bytes = rx.as_flattened_mut();
        match &mut self.rx {
            RxSource::Ads => {
                device.read_exact(AUTD_INDEX_GROUP, AUTD_INDEX_OFFSET_RX, rx_bytes)?;
            }
            RxSource::Notify { recv, handle, buf } => {
                while let Ok(notification) = recv.try_recv() {
                    for sample in notification.samples() {
                        if sample.handle == *handle {
                            let n = buf.len().min(sample.data.len());
                            buf[..n].copy_from_slice(&sample.data[..n]);
                        }
                    }
                }
                rx_bytes.copy_from_slice(buf);
            }
        }

        Ok(CycleOutcome { rx_valid: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_input_image_puts_the_dc_offset_after_the_state_words() {
        // [cfg_slave_count u16][input[] rx][state[] u16][dc_to_tc_offset i64]
        assert_eq!(AUTD_INDEX_OFFSET_RX, AUTD_INDEX_OFFSET_INPUT_BASE + 2);
        for num_devices in [1usize, 2, 8, MAX_DEVICES] {
            let state = state_index(num_devices);
            assert_eq!(
                state,
                AUTD_INDEX_OFFSET_RX + u32::try_from(num_devices * RX_FRAME_BYTES).unwrap()
            );
            assert_eq!(
                dc_offset_index(num_devices),
                state + u32::try_from(num_devices * STATE_BYTES_PER_DEVICE).unwrap(),
                "the client and tools/twincat-cli must agree on the image layout",
            );
        }
    }
}
