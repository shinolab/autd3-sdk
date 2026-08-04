use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_link_remote::{
    DeviceLayout, RemoteLink, RemoteLinkError, RemoteServer, RemoteServerOption,
};

struct EchoLink {
    num_devices: usize,
}

impl Link for EchoLink {
    type Error = Infallible;
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
    ) -> Result<CycleOutcome, Infallible> {
        for (t, r) in tx.iter().zip(rx.iter_mut()) {
            r[0] = t[0];
        }
        Ok(CycleOutcome::new(true))
    }
}

#[test]
fn factory_derives_device_count_from_client_geometry() {
    let num_devices = 3;

    let option = RemoteServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)));
    let mut server = RemoteServer::new(option, |layout: &[DeviceLayout]| {
        Ok::<_, RemoteLinkError>(EchoLink {
            num_devices: layout.len(),
        })
    })
    .unwrap();
    let addr = server.local_addr().unwrap();
    std::thread::spawn(move || server.serve());

    let geometry = autd3_rs_core::Geometry::new(
        (0..num_devices)
            .map(|_| autd3_rs_core::Autd3::default())
            .collect::<Vec<_>>(),
    );
    let mut link = RemoteLink::open(addr, None, &geometry).unwrap();

    assert_eq!(link.num_devices(), num_devices);

    let mut tx = vec![[0u8; TX_FRAME_BYTES]; num_devices];
    for (d, frame) in tx.iter_mut().enumerate() {
        frame[0] = u8::try_from(d + 1).unwrap();
    }
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; num_devices];
    let outcome = link.cycle(&tx, &mut rx).unwrap();
    assert!(outcome.rx_valid());
    for (d, r) in rx.iter().enumerate() {
        assert_eq!(r[0], u8::try_from(d + 1).unwrap());
    }
}
