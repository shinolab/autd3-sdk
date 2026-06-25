use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::{IntoLink, RX_FRAME_BYTES, TX_FRAME_BYTES};

use crate::error::RemoteLinkError;
use crate::wire;

pub struct RemoteLinkOption {
    pub addr: SocketAddr,
}

impl RemoteLinkOption {
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }
}

impl IntoLink for RemoteLinkOption {
    type Link = RemoteLink;

    async fn into_link(
        self,
        geometry: &autd3_rs_core::Geometry,
    ) -> Result<RemoteLink, autd3_rs_core::Error> {
        RemoteLink::open(self.addr, geometry).map_err(|e| autd3_rs_core::Error::Link(e.to_string()))
    }
}

pub struct RemoteLink {
    stream: TcpStream,
    num_devices: usize,
    rx_buf: Vec<u8>,
}

impl RemoteLink {
    pub fn open(
        addr: SocketAddr,
        geometry: &autd3_rs_core::Geometry,
    ) -> Result<Self, RemoteLinkError> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;

        stream.write_all(&wire::MAGIC)?;
        stream.write_all(&[wire::VERSION])?;
        stream.flush()?;

        let mut buf = [0u8; 2];
        stream.read_exact(&mut buf)?;
        let num_devices = usize::from(u16::from_le_bytes(buf));
        if num_devices == 0 {
            return Err(RemoteLinkError::InvalidDeviceCount { found: num_devices });
        }

        let layout: Vec<crate::TransducerLayout> = geometry
            .iter()
            .flat_map(|dev| {
                dev.positions()
                    .iter()
                    .zip(dev.directions())
                    .map(|(p, d)| crate::TransducerLayout {
                        pos: [p.x, p.y, p.z],
                        dir: [d.x, d.y, d.z],
                    })
            })
            .collect();
        stream.write_all(&wire::encode_geometry(&layout))?;
        stream.flush()?;

        Ok(Self {
            stream,
            num_devices,
            rx_buf: vec![0u8; num_devices * RX_FRAME_BYTES],
        })
    }
}

impl Link for RemoteLink {
    type Error = RemoteLinkError;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn state_checker(&self) -> ConstStateChecker {
        ConstStateChecker::new(self.num_devices)
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, RemoteLinkError> {
        self.stream.write_all(&[wire::TAG_FRAME])?;
        self.stream.write_all(tx.as_flattened())?;
        self.stream.flush()?;

        let mut valid = [0u8; 1];
        self.stream.read_exact(&mut valid)?;
        self.stream.read_exact(&mut self.rx_buf)?;
        rx.as_flattened_mut().copy_from_slice(&self.rx_buf);

        Ok(CycleOutcome {
            rx_valid: valid[0] != 0,
        })
    }
}

impl Drop for RemoteLink {
    fn drop(&mut self) {
        let _ = self.stream.write_all(&[wire::TAG_CLOSE]);
        let _ = self.stream.flush();
    }
}
