use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use ads::notif::{Attributes, Handle, Notification, TransmissionMode};
use ads::{AmsAddr, AmsNetId, Client, Source, Timeouts};
use autd3_rs_core::{CycleOutcome, IntoLink, Link, RX_FRAME_BYTES, TX_FRAME_BYTES};
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
    ) -> Result<TwinCATLink, autd3_rs_core::Error> {
        TwinCATLink::open(self).map_err(|e| autd3_rs_core::Error::Link(e.to_string()))
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
        })
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

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
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
