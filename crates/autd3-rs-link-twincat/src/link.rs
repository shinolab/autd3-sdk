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
pub(crate) const AUTD_INDEX_OFFSET_RX: u32 = 0x8000_0000;
const AUTD_AMS_PORT: u16 = 301;

pub enum TwinCATServer {
    Local,
    Remote { addr: IpAddr, ams_net_id: AmsNetId },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TwinCATRoute {
    #[default]
    Auto,
    Notify,
    Ads,
}

pub struct TwinCATLinkOption {
    pub server: TwinCATServer,
    pub num_devices: usize,
    pub timeouts: Timeouts,
    pub route: TwinCATRoute,
}

impl TwinCATLinkOption {
    #[must_use]
    pub fn local(num_devices: usize) -> Self {
        Self {
            server: TwinCATServer::Local,
            num_devices,
            timeouts: Timeouts::none(),
            route: TwinCATRoute::default(),
        }
    }

    #[must_use]
    pub fn remote(addr: IpAddr, ams_net_id: AmsNetId, num_devices: usize) -> Self {
        Self {
            server: TwinCATServer::Remote { addr, ams_net_id },
            num_devices,
            timeouts: Timeouts::none(),
            route: TwinCATRoute::default(),
        }
    }

    #[must_use]
    pub fn with_route(mut self, route: TwinCATRoute) -> Self {
        self.route = route;
        self
    }
}

impl IntoLink for TwinCATLinkOption {
    type Link = TwinCATLink;

    async fn into_link(self) -> Result<TwinCATLink, autd3_rs_core::Error> {
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
        let TwinCATLinkOption {
            server,
            num_devices,
            timeouts,
            route,
        } = option;

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

        let rx = match route {
            TwinCATRoute::Ads => RxSource::Ads,
            TwinCATRoute::Notify => Self::register_notification(&client, ams_addr, num_devices)?,
            TwinCATRoute::Auto => {
                Self::register_notification(&client, ams_addr, num_devices).unwrap_or(RxSource::Ads)
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
