use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use autd3_rs_core::link::{
    ConstStateChecker, CycleOutcome, DeviceState, Link, LinkStatus, StateCheck,
};
use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_link_remote::{
    BusOption, BusPacing, DeviceLayout, RemoteLink, RemoteLinkError, RemoteServer,
    RemoteServerOption,
};

const NUM_DEVICES: usize = 1;

fn option() -> RemoteServerOption {
    RemoteServerOption {
        bus: BusOption {
            pacing: BusPacing::Period(Duration::from_micros(100)),
            rt_priority: None,
            ..BusOption::default()
        },
        ..RemoteServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
    }
}

fn geometry() -> autd3_rs_core::Geometry {
    autd3_rs_core::Geometry::new(
        (0..NUM_DEVICES)
            .map(|_| autd3_rs_core::Autd3::default())
            .collect::<Vec<_>>(),
    )
}

#[derive(Debug, thiserror::Error)]
#[error("the bus link died")]
struct DeadBus;

struct FlakyLink {
    cycles_left: Option<u32>,
}

impl Link for FlakyLink {
    type Error = DeadBus;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        NUM_DEVICES
    }

    fn state_checker(&self) -> ConstStateChecker {
        ConstStateChecker::new(NUM_DEVICES)
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, DeadBus> {
        if let Some(left) = &mut self.cycles_left {
            if *left == 0 {
                return Err(DeadBus);
            }
            *left -= 1;
        }
        for (t, r) in tx.iter().zip(rx.iter_mut()) {
            r[0] = t[0];
        }
        Ok(CycleOutcome::valid())
    }
}

fn spin<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn the_bus_loop_reopens_the_link_and_the_server_survives() {
    let opens = Arc::new(AtomicUsize::new(0));
    let factory_opens = Arc::clone(&opens);
    let mut server = RemoteServer::new(option(), move |_: &[DeviceLayout]| {
        match factory_opens.fetch_add(1, Ordering::SeqCst) {
            0 => Ok(FlakyLink {
                cycles_left: Some(4),
            }),
            1 => Err(RemoteLinkError::Link("the bus is not back yet".to_owned())),
            _ => Ok(FlakyLink { cycles_left: None }),
        }
    })
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let mut link = RemoteLink::open(addr, None, &geometry()).unwrap();
    let mut checker = link.state_checker();
    let mut tx = vec![[0u8; TX_FRAME_BYTES]; NUM_DEVICES];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; NUM_DEVICES];

    let mut saw_invalid = false;
    spin("the bus to come back after a link error", || {
        tx[0][0] = tx[0][0].wrapping_add(1);
        let outcome = link
            .cycle(&tx, &mut rx)
            .expect("the client keeps running while the server recovers");
        saw_invalid |= !outcome.rx_valid();
        (outcome.rx_valid() && opens.load(Ordering::SeqCst) >= 3).then_some(())
    });

    assert!(saw_invalid, "the client must see the gap as an invalid rx");

    let status = spin("the recovery to show up in the bus status", || {
        tx[0][0] = tx[0][0].wrapping_add(1);
        let _ = link.cycle(&tx, &mut rx);
        let status: LinkStatus = futures_lite_block_on(checker.check()).unwrap();
        (status.recoveries() > 0).then_some(status)
    });
    assert!(status.all_op());

    drop(link);
    let _ = handle.join().unwrap();
}

struct WatchedLink {
    states: Arc<std::sync::Mutex<Vec<DeviceState>>>,
}

struct WatchedChecker {
    states: Arc<std::sync::Mutex<Vec<DeviceState>>>,
}

impl StateCheck for WatchedChecker {
    type Error = Infallible;

    fn check(&mut self) -> impl Future<Output = Result<LinkStatus, Self::Error>> + Send {
        let devices = self
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        std::future::ready(Ok(LinkStatus::new(devices, 0)))
    }
}

impl Link for WatchedLink {
    type Error = Infallible;
    type Checker = WatchedChecker;

    fn num_devices(&self) -> usize {
        NUM_DEVICES
    }

    fn state_checker(&self) -> WatchedChecker {
        WatchedChecker {
            states: Arc::clone(&self.states),
        }
    }

    fn cycle(
        &mut self,
        _tx: &[[u8; TX_FRAME_BYTES]],
        _rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Infallible> {
        Ok(CycleOutcome::valid())
    }
}

#[test]
fn the_real_bus_state_reaches_the_remote_client() {
    let bus_states = Arc::new(std::sync::Mutex::new(vec![DeviceState::Op; NUM_DEVICES]));
    let for_factory = Arc::clone(&bus_states);
    let mut server = RemoteServer::new(option(), move |_: &[DeviceLayout]| {
        Ok::<_, RemoteLinkError>(WatchedLink {
            states: Arc::clone(&for_factory),
        })
    })
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let mut link = RemoteLink::open(addr, None, &geometry()).unwrap();
    let mut checker = link.state_checker();
    let mut tx = vec![[0u8; TX_FRAME_BYTES]; NUM_DEVICES];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; NUM_DEVICES];

    link.cycle(&tx, &mut rx).unwrap();
    assert!(
        futures_lite_block_on(checker.check()).unwrap().all_op(),
        "a healthy bus reads as OP",
    );

    bus_states
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)[0] = DeviceState::SafeOpError;

    let status = spin("the SAFE-OP + ERROR state to reach the client", || {
        tx[0][0] = tx[0][0].wrapping_add(1);
        link.cycle(&tx, &mut rx).unwrap();
        let status = futures_lite_block_on(checker.check()).unwrap();
        (!status.all_op()).then_some(status)
    });
    assert_eq!(
        status.devices(),
        vec![DeviceState::SafeOpError; NUM_DEVICES]
    );

    drop(link);
    let _ = handle.join().unwrap();
}

struct CountingLink(Arc<AtomicUsize>);

impl Link for CountingLink {
    type Error = Infallible;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        NUM_DEVICES
    }

    fn state_checker(&self) -> ConstStateChecker {
        ConstStateChecker::new(NUM_DEVICES)
    }

    fn cycle(
        &mut self,
        _tx: &[[u8; TX_FRAME_BYTES]],
        _rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Infallible> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(CycleOutcome::valid())
    }
}

#[test]
fn the_bus_pacing_bounds_the_cycle_rate() {
    let cycles = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&cycles);

    let option = RemoteServerOption {
        bus: BusOption {
            pacing: BusPacing::Period(Duration::from_millis(20)),
            rt_priority: None,
            ..BusOption::default()
        },
        ..RemoteServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
    };
    let mut server = RemoteServer::new(option, move |_: &[DeviceLayout]| {
        Ok::<_, RemoteLinkError>(CountingLink(Arc::clone(&counted)))
    })
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let link = RemoteLink::open(addr, None, &geometry()).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    drop(link);
    let _ = handle.join().unwrap();

    let count = cycles.load(Ordering::Relaxed);
    assert!(
        count <= 40,
        "a 20 ms period must not run more than ~10 cycles in 200 ms, ran {count}",
    );
}

fn futures_lite_block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = std::pin::pin!(fut);
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    loop {
        if let std::task::Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}
