use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_link_remote::{DeviceLayout, RemoteLink, RemoteLinkError, RemoteServer};

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
        Ok(CycleOutcome { rx_valid: true })
    }
}

#[test]
fn factory_derives_device_count_from_client_geometry() {
    let num_devices = 3;

    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    std::thread::spawn(move || {
        let factory = |layout: &[DeviceLayout]| -> Result<EchoLink, RemoteLinkError> {
            Ok(EchoLink {
                num_devices: layout.len(),
            })
        };
        let _ = RemoteServer::serve_with_factory(addr, factory);
    });

    let geometry = autd3_rs_core::Geometry::new(
        (0..num_devices)
            .map(|_| autd3_rs_core::Autd3::default())
            .collect::<Vec<_>>(),
    );
    let mut link = loop {
        match RemoteLink::open(addr, None, &geometry) {
            Ok(link) => break link,
            Err(RemoteLinkError::Io(_)) => std::thread::yield_now(),
            Err(e) => panic!("unexpected error: {e}"),
        }
    };

    assert_eq!(link.num_devices(), num_devices);

    let mut tx = vec![[0u8; TX_FRAME_BYTES]; num_devices];
    for (d, frame) in tx.iter_mut().enumerate() {
        frame[0] = u8::try_from(d + 1).unwrap();
    }
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; num_devices];
    let outcome = link.cycle(&tx, &mut rx).unwrap();
    assert!(outcome.rx_valid);
    for (d, r) in rx.iter().enumerate() {
        assert_eq!(r[0], u8::try_from(d + 1).unwrap());
    }
}
