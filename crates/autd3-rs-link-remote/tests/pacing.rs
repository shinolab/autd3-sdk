use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_link_remote::{
    BusPacing, DeviceLayout, RemoteLink, RemoteLinkError, RemoteServer, RemoteServerOption,
};

const PERIOD: Duration = Duration::from_micros(200);
const WARMUP: usize = 20;
const FRAMES: usize = 100;

struct PacedLink {
    num_devices: usize,
    next_at: Option<Instant>,
    cycles: Arc<AtomicU64>,
}

impl PacedLink {
    fn wait(&self) {
        if let Some(deadline) = self.next_at {
            let now = Instant::now();
            if deadline > now {
                std::thread::sleep(deadline - now);
            }
        }
    }
}

impl Link for PacedLink {
    type Error = Infallible;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn state_checker(&self) -> ConstStateChecker {
        ConstStateChecker::new(self.num_devices)
    }

    fn wait_next_cycle(&mut self) {
        self.wait();
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Infallible> {
        self.wait();
        let started = Instant::now();
        self.cycles.fetch_add(1, Ordering::Relaxed);
        for (t, r) in tx.iter().zip(rx.iter_mut()) {
            r[0] = t[0];
            r[1] = t[1];
        }
        self.next_at = Some(started + PERIOD);
        Ok(CycleOutcome::new(true))
    }
}

fn geometry(num_devices: usize) -> autd3_rs_core::Geometry {
    autd3_rs_core::Geometry::new(
        (0..num_devices)
            .map(|_| autd3_rs_core::Autd3::default())
            .collect::<Vec<_>>(),
    )
}

#[test]
fn a_paced_bus_spends_about_one_cycle_per_frame() {
    let num_devices = 1;
    let cycles = Arc::new(AtomicU64::new(0));

    let mut option = RemoteServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)));
    option.bus.pacing = BusPacing::LinkPaced;
    let bus_cycles = Arc::clone(&cycles);
    let mut server = RemoteServer::new(option, move |_: &[DeviceLayout]| {
        Ok::<_, RemoteLinkError>(PacedLink {
            num_devices,
            next_at: None,
            cycles: Arc::clone(&bus_cycles),
        })
    })
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let mut link = RemoteLink::open(addr, None, &geometry(num_devices)).unwrap();
    let tx = vec![[0u8; TX_FRAME_BYTES]; num_devices];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; num_devices];

    for _ in 0..WARMUP {
        link.cycle(&tx, &mut rx).unwrap();
    }

    let before = cycles.load(Ordering::Relaxed);
    let started = Instant::now();
    for _ in 0..FRAMES {
        link.cycle(&tx, &mut rx).unwrap();
    }
    let elapsed = started.elapsed();
    let spent = cycles.load(Ordering::Relaxed) - before;

    drop(link);
    handle.join().unwrap().unwrap();

    let per_frame = spent as f64 / FRAMES as f64;
    assert!(
        per_frame < 1.5,
        "{spent} bus cycles for {FRAMES} frames ({per_frame:.2} per frame, {elapsed:?})",
    );
}
