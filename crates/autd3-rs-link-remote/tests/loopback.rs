use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_link_remote::{RemoteLink, RemoteServer};

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
            r[1] = t[1];
        }
        Ok(CycleOutcome { rx_valid: true })
    }
}

#[test]
fn loopback_relays_frames() {
    let num_devices = 2;

    let mut server = RemoteServer::with_link(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        EchoLink { num_devices },
    )
    .unwrap();
    assert_eq!(server.num_devices(), num_devices);
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let geometry = autd3_rs_core::Geometry::new(
        (0..num_devices)
            .map(|_| autd3_rs_core::Autd3::default())
            .collect::<Vec<_>>(),
    );
    let mut link = RemoteLink::open(addr, &geometry).unwrap();
    assert_eq!(link.num_devices(), num_devices);

    let mut tx = vec![[0u8; TX_FRAME_BYTES]; num_devices];
    for (d, frame) in tx.iter_mut().enumerate() {
        frame[0] = u8::try_from(d + 1).unwrap();
        frame[1] = u8::try_from(d + 100).unwrap();
    }

    let mut rx = vec![[0u8; RX_FRAME_BYTES]; num_devices];
    let outcome = link.cycle(&tx, &mut rx).unwrap();
    assert!(outcome.rx_valid);
    for (d, r) in rx.iter().enumerate() {
        assert_eq!(r[0], u8::try_from(d + 1).unwrap());
        assert_eq!(r[1], u8::try_from(d + 100).unwrap());
    }

    drop(link);
    handle.join().unwrap().unwrap();
}
