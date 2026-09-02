use std::convert::Infallible;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use autd3_rs_core::link::{CycleOutcome, DcClock, Link, LinkStats, LinkStatus, StateCheck};
use autd3_rs_core::value::DcSysTime;
use autd3_rs_core::{IntoLink, RX_FRAME_BYTES, TX_FRAME_BYTES};

use crate::error::{RejectKind, RemoteLinkError};
use crate::wire::{self, BusStatus};

fn reject_kind(code: u8) -> RejectKind {
    match code {
        wire::SESSION_BUS_CLOSED => RejectKind::BusClosed,
        wire::SESSION_DEVICE_COUNT => RejectKind::DeviceCount,
        wire::SESSION_BUS_UNAVAILABLE => RejectKind::BusUnavailable,
        wire::SESSION_INTERNAL => RejectKind::Internal,
        other => RejectKind::Unknown(other),
    }
}

pub struct RemoteLinkOption {
    pub addr: SocketAddr,
    pub timeout: Option<Duration>,
}

impl RemoteLinkOption {
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            timeout: None,
        }
    }
}

impl IntoLink for RemoteLinkOption {
    type Link = RemoteLink;

    fn into_link(
        self,
        geometry: &autd3_rs_core::Geometry,
    ) -> Result<RemoteLink, autd3_rs_core::error::LinkError> {
        RemoteLink::open(self.addr, self.timeout, geometry)
            .map_err(|e| autd3_rs_core::error::LinkError::with_source(e.to_string(), e))
    }
}

#[derive(Clone)]
pub struct RemoteStateChecker {
    status: Arc<Mutex<LinkStatus>>,
}

impl RemoteStateChecker {
    pub fn check(&mut self) -> Result<LinkStatus, Infallible> {
        Ok(self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone())
    }
}

impl StateCheck for RemoteStateChecker {
    type Error = Infallible;

    fn check(&mut self) -> Result<LinkStatus, Self::Error> {
        RemoteStateChecker::check(self)
    }
}

pub struct RemoteLink {
    stream: TcpStream,
    num_devices: usize,
    reply_buf: Vec<u8>,
    status: BusStatus,
    shared_status: Arc<Mutex<LinkStatus>>,
    stats: LinkStats,
    counters: [u64; 4],
    dc_clock: DcClock,
}

impl RemoteLink {
    pub fn open(
        addr: SocketAddr,
        timeout: Option<Duration>,
        geometry: &autd3_rs_core::Geometry,
    ) -> Result<Self, RemoteLinkError> {
        let mut stream = match timeout {
            Some(timeout) => TcpStream::connect_timeout(&addr, timeout)?,
            None => TcpStream::connect(addr)?,
        };
        stream.set_nodelay(true)?;
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(timeout)?;

        stream.write_all(&wire::encode_hello())?;
        stream.flush()?;

        let peer = wire::read_hello(&mut stream)?;
        if peer.as_ref().is_none_or(|p| p.wire != wire::VERSION) {
            return Err(RemoteLinkError::ProtocolMismatch {
                local: wire::local_version(),
                peer,
            });
        }

        let layout: Vec<crate::DeviceLayout> = geometry
            .iter()
            .map(|dev| crate::DeviceLayout {
                transducers: dev
                    .positions()
                    .iter()
                    .zip(dev.directions())
                    .map(|(p, d)| crate::TransducerLayout {
                        pos: [p.x, p.y, p.z],
                        dir: [d.x, d.y, d.z],
                    })
                    .collect(),
            })
            .collect();
        stream.write_all(&wire::encode_geometry(&layout))?;
        stream.flush()?;

        let num_devices = match wire::read_session_reply(&mut stream)? {
            Ok(num_devices) => num_devices,
            Err((code, detail)) => {
                return Err(RemoteLinkError::SessionRejected {
                    kind: reject_kind(code),
                    detail,
                });
            }
        };
        if num_devices == 0 {
            return Err(RemoteLinkError::InvalidDeviceCount { found: num_devices });
        }

        Ok(Self {
            stream,
            num_devices,
            reply_buf: vec![
                0u8;
                wire::REPLY_HEADER_BYTES + num_devices + num_devices * RX_FRAME_BYTES
            ],
            status: BusStatus::new(num_devices),
            shared_status: Arc::new(Mutex::new(LinkStatus::op(num_devices))),
            stats: LinkStats::default(),
            counters: [0; 4],
            dc_clock: DcClock::new(),
        })
    }

    fn publish_status(&mut self) {
        let observed = [
            self.status.stale_cycles,
            self.status.lost_cycles,
            self.status.phase_excursions,
            self.status.worst_phase_deviation_ns,
        ];
        self.stats
            .add_stale_cycles(observed[0].saturating_sub(self.counters[0]));
        self.stats
            .add_lost_cycles(observed[1].saturating_sub(self.counters[1]));
        self.stats
            .add_phase_excursions(observed[2].saturating_sub(self.counters[2]), observed[3]);
        self.counters = observed;

        let mut status = self
            .shared_status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.set_devices(self.status.devices.iter().copied());
        status.set_recoveries(self.status.recoveries);
    }
}

impl Link for RemoteLink {
    type Error = RemoteLinkError;
    type Checker = RemoteStateChecker;

    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn stats(&self) -> LinkStats {
        self.stats.clone()
    }

    fn state_checker(&self) -> RemoteStateChecker {
        RemoteStateChecker {
            status: Arc::clone(&self.shared_status),
        }
    }

    fn dc_clock(&self) -> Option<DcClock> {
        Some(self.dc_clock.clone())
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, RemoteLinkError> {
        self.stream.write_all(&[wire::TAG_FRAME])?;
        self.stream.write_all(tx.as_flattened())?;
        self.stream.flush()?;

        self.stream.read_exact(&mut self.reply_buf)?;
        let states_end = wire::REPLY_HEADER_BYTES + self.num_devices;
        let (rx_valid, dc_time_ns) =
            wire::decode_reply_header(&self.reply_buf[..states_end], &mut self.status);
        rx.as_flattened_mut()
            .copy_from_slice(&self.reply_buf[states_end..]);
        self.publish_status();

        if dc_time_ns != wire::DC_TIME_UNAVAILABLE {
            let _ = self.dc_clock.observe(DcSysTime::from_nanos(dc_time_ns));
        }

        Ok(if rx_valid {
            CycleOutcome::valid()
        } else {
            CycleOutcome::stale()
        })
    }
}

impl Drop for RemoteLink {
    fn drop(&mut self) {
        let _ = self.stream.write_all(&[wire::TAG_CLOSE]);
        let _ = self.stream.flush();
    }
}
