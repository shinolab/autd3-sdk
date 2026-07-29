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
    let mut link = RemoteLink::open(addr, None, &geometry).unwrap();
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

struct DcLink {
    num_devices: usize,
    dc_clock: autd3_rs_core::DcClock,
    bus_ahead_ns: i64,
}

impl Link for DcLink {
    type Error = Infallible;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn state_checker(&self) -> ConstStateChecker {
        ConstStateChecker::new(self.num_devices)
    }

    fn dc_clock(&self) -> Option<autd3_rs_core::DcClock> {
        Some(self.dc_clock.clone())
    }

    fn cycle(
        &mut self,
        _tx: &[[u8; TX_FRAME_BYTES]],
        _rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Infallible> {
        let bus = autd3_rs_core::value::DcSysTime::now().with_dc_offset(self.bus_ahead_ns);
        self.dc_clock.observe(bus);
        Ok(CycleOutcome { rx_valid: true })
    }
}

#[test]
fn the_bus_clock_of_the_served_link_reaches_the_remote_client() {
    let num_devices = 1;
    let bus_ahead_ns = 750_000_000i64;

    let mut server = RemoteServer::with_link(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        DcLink {
            num_devices,
            dc_clock: autd3_rs_core::DcClock::new(),
            bus_ahead_ns,
        },
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let geometry = autd3_rs_core::Geometry::new(vec![autd3_rs_core::Autd3::default()]);
    let mut link = RemoteLink::open(addr, None, &geometry).unwrap();
    let clock = link
        .dc_clock()
        .expect("the remote link forwards a bus clock");
    assert_eq!(clock.observation(), None, "nothing observed before a cycle");

    let mut tx = vec![[0u8; TX_FRAME_BYTES]; num_devices];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; num_devices];
    let mut offset_ns = 0;
    for _ in 0..16 {
        link.cycle(&tx, &mut rx).unwrap();
        tx[0][0] = tx[0][0].wrapping_add(1);
        if let Some(o) = clock.offset_ns() {
            offset_ns = o;
            break;
        }
    }

    let error_ns = (offset_ns - bus_ahead_ns).abs();
    assert!(
        error_ns < 100_000_000,
        "the client must see the server-side bus clock, not its own; \
         offset {offset_ns} ns is {error_ns} ns away from the {bus_ahead_ns} ns it was given",
    );

    drop(link);
    handle.join().unwrap().unwrap();
}
