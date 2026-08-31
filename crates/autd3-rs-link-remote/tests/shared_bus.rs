use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link, LinkStats};
use autd3_rs_core::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_link_remote::{
    Actual, BusOption, BusPacing, BusServer, BusServerOption, Desired, RejectKind, RemoteLink,
    RemoteLinkError, SharedBus,
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

fn bus_option() -> BusOption {
    BusOption {
        pacing: BusPacing::Period(Duration::from_micros(100)),
        rt_priority: None,
        ..BusOption::default()
    }
}

fn geometry(num_devices: usize) -> autd3_rs_core::Geometry {
    autd3_rs_core::Geometry::new(
        (0..num_devices)
            .map(|_| autd3_rs_core::Autd3::default())
            .collect::<Vec<_>>(),
    )
}

fn spin<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn shared_bus(num_devices: usize) -> Arc<SharedBus> {
    SharedBus::new(bus_option(), move || {
        Ok::<_, RemoteLinkError>(EchoLink { num_devices })
    })
    .unwrap()
}

#[test]
fn a_failing_bus_is_retried_until_the_link_appears() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let bus = SharedBus::new(bus_option(), move || {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(RemoteLinkError::Link("no device found on eth0".to_owned()))
        } else {
            Ok(EchoLink { num_devices: 1 })
        }
    })
    .unwrap();

    bus.set_desired(Desired::Open);

    let failure = spin("the first attempt to fail", || {
        match bus.snapshot().actual {
            Actual::Failed { reason } => Some(reason),
            _ => None,
        }
    });
    assert!(failure.contains("no device found on eth0"), "{failure}");

    let snapshot = spin("the retry to bring the bus up", || {
        let snapshot = bus.snapshot();
        (snapshot.actual == Actual::Open).then_some(snapshot)
    });
    assert_eq!(snapshot.desired, Desired::Open);
    assert_eq!(snapshot.num_devices, 1);
    assert!(attempts.load(Ordering::SeqCst) >= 2);
}

#[test]
fn a_panicking_bus_thread_leaves_an_observable_failure_instead_of_a_silent_stop() {
    let bus = SharedBus::new(
        bus_option(),
        move || -> Result<EchoLink, RemoteLinkError> { panic!("the link factory blew up") },
    )
    .unwrap();

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    bus.set_desired(Desired::Open);

    let reason = spin("the panic to surface as a failure", || {
        match bus.snapshot().actual {
            Actual::Failed { reason } => Some(reason),
            _ => None,
        }
    });
    std::panic::set_hook(previous);

    assert!(reason.contains("panicked"), "{reason}");
    assert!(reason.contains("the link factory blew up"), "{reason}");
    assert!(
        spin("the loop to stop", || bus
            .snapshot()
            .stopped
            .then_some(true)),
        "a panicking bus thread must stop the loop",
    );

    assert!(
        bus.probe().is_err(),
        "a request on a dead bus must fail rather than block",
    );
}

#[test]
fn closing_the_bus_drops_the_link_and_probing_opens_it_briefly() {
    let opens = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&opens);
    let bus = SharedBus::new(bus_option(), move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok::<_, RemoteLinkError>(EchoLink { num_devices: 2 })
    })
    .unwrap();

    bus.set_desired(Desired::Open);
    spin("the bus to open", || {
        (bus.snapshot().actual == Actual::Open).then_some(())
    });
    assert_eq!(opens.load(Ordering::SeqCst), 1);

    bus.set_desired(Desired::Closed);
    spin("the bus to close", || {
        (bus.snapshot().actual == Actual::Closed).then_some(())
    });
    assert_eq!(bus.snapshot().num_devices, 0);

    assert_eq!(bus.probe().unwrap(), 2);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(
        bus.snapshot().actual,
        Actual::Closed,
        "a probe must not leave the bus open",
    );
}

const SAMPLE_PERIOD: Duration = Duration::from_millis(100);
const READS: usize = 10_000;

#[test]
fn readers_share_one_sampled_snapshot_instead_of_taking_the_bus_lock_each() {
    let bus = shared_bus(1);
    bus.set_desired(Desired::Open);
    spin("the sampler to report the bus open", || {
        (bus.sampled().actual == Actual::Open).then_some(())
    });

    let started = Instant::now();
    let mut last = bus.sampled();
    let mut taken = 1_u32;
    for _ in 0..READS {
        let next = bus.sampled();
        if !Arc::ptr_eq(&next, &last) {
            taken += 1;
            last = next;
        }
    }

    let allowed =
        u32::try_from(started.elapsed().as_nanos() / SAMPLE_PERIOD.as_nanos()).unwrap() + 2;
    assert!(
        taken <= allowed,
        "{READS} readers saw {taken} snapshots; the bus lock is taken per reader, \
         not once per {SAMPLE_PERIOD:?} sample",
    );

    bus.set_desired(Desired::Closed);
    spin(
        "the sampled snapshot to catch up with the closed bus",
        || (bus.sampled().actual == Actual::Closed).then_some(()),
    );
}

#[test]
fn probing_an_open_bus_reports_the_running_device_count() {
    let bus = shared_bus(3);
    bus.set_desired(Desired::Open);
    spin("the bus to open", || {
        (bus.snapshot().actual == Actual::Open).then_some(())
    });
    assert_eq!(bus.probe().unwrap(), 3);
}

#[test]
fn a_client_is_refused_while_the_bus_is_closed() {
    let bus = shared_bus(1);
    let mut server = BusServer::new(
        BusServerOption {
            auto_open: false,
            ..BusServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        },
        Arc::clone(&bus),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let Err(err) = RemoteLink::open(addr, None, &geometry(1)) else {
        panic!("a closed bus must not accept a client");
    };
    let RemoteLinkError::SessionRejected { kind, .. } = err else {
        panic!("expected a session rejection, got {err}");
    };
    assert_eq!(kind, RejectKind::BusClosed);
    assert_eq!(bus.snapshot().actual, Actual::Closed);

    let _ = handle.join().unwrap();
}

#[test]
fn a_held_bus_refuses_clients_until_it_is_released() {
    let bus = shared_bus(1);
    let mut server = BusServer::new(
        BusServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
        Arc::clone(&bus),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let _ = server.serve_once();
        server.serve_once()
    });

    bus.hold("a tune sweep is driving the bus");
    let Err(err) = RemoteLink::open(addr, None, &geometry(1)) else {
        panic!("a held bus must not accept a client");
    };
    let RemoteLinkError::SessionRejected { kind, .. } = err else {
        panic!("expected a session rejection, got {err}");
    };
    assert_eq!(kind, RejectKind::BusUnavailable);
    assert_eq!(bus.snapshot().actual, Actual::Closed);

    bus.release();
    let link = RemoteLink::open(addr, None, &geometry(1)).unwrap();
    drop(link);
    let _ = handle.join().unwrap();
}

#[test]
fn waiting_on_the_actual_state_sees_the_open_and_honours_a_give_up() {
    let bus = shared_bus(1);
    bus.set_desired(Desired::Open);
    assert!(bus.wait_actual(
        Duration::from_secs(20),
        |actual| matches!(actual, Actual::Open),
        || false,
    ));
    assert!(!bus.wait_actual(
        Duration::from_secs(20),
        |actual| matches!(actual, Actual::Failed { .. }),
        || true,
    ));
}

#[test]
fn auto_open_brings_the_bus_up_for_the_first_client() {
    let bus = shared_bus(1);
    let mut server = BusServer::new(
        BusServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
        Arc::clone(&bus),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let mut link = RemoteLink::open(addr, None, &geometry(1)).unwrap();
    assert_eq!(link.num_devices(), 1);

    let mut tx = vec![[0u8; TX_FRAME_BYTES]; 1];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; 1];
    tx[0][0] = 42;
    assert!(link.cycle(&tx, &mut rx).unwrap().rx_valid());
    assert_eq!(rx[0][0], 42);
    assert_eq!(bus.snapshot().desired, Desired::Open);

    drop(link);
    let _ = handle.join().unwrap();
}

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
}

impl Gate {
    fn wait(&self) {
        let mut open = self
            .open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*open {
            open = self
                .cv
                .wait(open)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn release(&self) {
        *self
            .open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.cv.notify_all();
    }
}

#[test]
fn a_probe_that_loses_the_bus_to_a_client_gives_up_and_lets_the_next_one_run() {
    let gate = Arc::new(Gate::default());
    let held = Arc::clone(&gate);
    let bus = SharedBus::new(bus_option(), move || {
        held.wait();
        Ok::<_, RemoteLinkError>(EchoLink { num_devices: 2 })
    })
    .unwrap();

    bus.set_desired(Desired::Open);
    spin("the bus loop to block inside the factory", || {
        (bus.snapshot().actual == Actual::Opening).then_some(())
    });
    bus.set_desired(Desired::Closed);

    let probing = Arc::clone(&bus);
    let (probed_tx, probed_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || probed_tx.send(probing.probe()));
    std::thread::sleep(Duration::from_millis(200));
    bus.set_desired(Desired::Open);

    let probed = probed_rx.recv_timeout(Duration::from_secs(30));
    gate.release();
    let Err(err) = probed.expect("the probe must not wait for a bus it will never be given") else {
        panic!("a probe the bus was taken away from must not report a device count");
    };
    assert!(
        matches!(err, RemoteLinkError::ProbeBusOpened),
        "expected the probe to say the bus was opened, got {err}",
    );

    spin("the bus to open for the client that took it", || {
        (bus.snapshot().actual == Actual::Open).then_some(())
    });

    bus.set_desired(Desired::Closed);
    spin("the bus to close", || {
        (bus.snapshot().actual == Actual::Closed).then_some(())
    });
    assert_eq!(
        bus.probe().unwrap(),
        2,
        "the abandoned probe must not block the next one",
    );
}

struct PanickingLink {
    cycles_left: usize,
}

impl Link for PanickingLink {
    type Error = Infallible;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        1
    }

    fn state_checker(&self) -> ConstStateChecker {
        ConstStateChecker::new(1)
    }

    fn cycle(
        &mut self,
        _tx: &[[u8; TX_FRAME_BYTES]],
        _rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Infallible> {
        assert!(self.cycles_left > 0, "the bus link exploded");
        self.cycles_left -= 1;
        Ok(CycleOutcome::new(true))
    }
}

#[test]
fn a_bus_thread_that_panics_ends_the_session_instead_of_freezing_it() {
    let bus = SharedBus::new(bus_option(), || {
        Ok::<_, RemoteLinkError>(PanickingLink { cycles_left: 50 })
    })
    .unwrap();
    let mut server = BusServer::new(
        BusServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
        Arc::clone(&bus),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let mut link = RemoteLink::open(addr, None, &geometry(1)).unwrap();
    let (failed_tx, failed_rx) = std::sync::mpsc::channel();
    let driver = std::thread::spawn(move || {
        let mut tx = vec![[0u8; TX_FRAME_BYTES]; 1];
        let mut rx = vec![[0u8; RX_FRAME_BYTES]; 1];
        loop {
            tx[0][0] = tx[0][0].wrapping_add(1);
            if let Err(e) = link.cycle(&tx, &mut rx) {
                let _ = failed_tx.send(e.to_string());
                return;
            }
        }
    });

    spin("the bus to report itself gone after the panic", || {
        bus.snapshot().stopped.then_some(())
    });
    failed_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the client must be told the session died, not left waiting for a reply");
    assert!(
        handle.join().unwrap().is_err(),
        "the server must report why the session ended",
    );
    let _ = driver.join();
}

#[test]
fn a_client_that_vanishes_without_closing_does_not_block_the_next_one() {
    let bus = shared_bus(1);
    let mut server = BusServer::new(
        BusServerOption {
            idle_timeout: Duration::from_millis(300),
            ..BusServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        },
        Arc::clone(&bus),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    std::thread::spawn(move || server.serve());

    let ghost = RemoteLink::open(addr, None, &geometry(1)).unwrap();
    std::mem::forget(ghost);

    let mut link = spin("the server to drop the client that vanished", || {
        RemoteLink::open(addr, Some(Duration::from_secs(1)), &geometry(1)).ok()
    });
    let mut tx = vec![[0u8; TX_FRAME_BYTES]; 1];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; 1];
    tx[0][0] = 7;
    assert!(link.cycle(&tx, &mut rx).unwrap().rx_valid());
    assert_eq!(rx[0][0], 7);
}

#[test]
fn a_geometry_that_does_not_match_the_bus_is_refused() {
    let bus = shared_bus(2);
    let mut server = BusServer::new(
        BusServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
        Arc::clone(&bus),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let Err(err) = RemoteLink::open(addr, None, &geometry(1)) else {
        panic!("a one-device geometry must not attach to a two-device bus");
    };
    let RemoteLinkError::SessionRejected { kind, detail } = err else {
        panic!("expected a session rejection, got {err}");
    };
    assert_eq!(kind, RejectKind::DeviceCount);
    assert!(detail.contains('2'), "{detail}");

    let _ = handle.join().unwrap();
}

#[test]
fn closing_the_bus_ends_the_session() {
    let bus = shared_bus(1);
    let mut server = BusServer::new(
        BusServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
        Arc::clone(&bus),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let mut link = RemoteLink::open(addr, None, &geometry(1)).unwrap();
    let mut tx = vec![[0u8; TX_FRAME_BYTES]; 1];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; 1];
    link.cycle(&tx, &mut rx).unwrap();

    bus.set_desired(Desired::Closed);

    spin("the session to end once the bus is closed", || {
        tx[0][0] = tx[0][0].wrapping_add(1);
        link.cycle(&tx, &mut rx).err().map(|_| ())
    });

    let result = handle.join().unwrap();
    assert!(
        result.is_err(),
        "the server must report why the session ended",
    );
    drop(link);
}

#[derive(Debug, thiserror::Error)]
#[error("the counting link died")]
struct DeadCountingLink;

struct CountingLink {
    stats: LinkStats,
    cycles_left: Option<u32>,
    record_stale: bool,
}

impl Link for CountingLink {
    type Error = DeadCountingLink;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        1
    }

    fn stats(&self) -> LinkStats {
        self.stats.clone()
    }

    fn state_checker(&self) -> ConstStateChecker {
        ConstStateChecker::new(1)
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, DeadCountingLink> {
        if let Some(left) = &mut self.cycles_left {
            if *left == 0 {
                return Err(DeadCountingLink);
            }
            *left -= 1;
        }
        if self.record_stale {
            self.stats.record_stale_cycle();
        }
        for (t, r) in tx.iter().zip(rx.iter_mut()) {
            r[0] = t[0];
        }
        Ok(CycleOutcome::new(true))
    }
}

#[test]
fn the_bus_counters_do_not_rewind_when_the_link_is_reopened() {
    let opens = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&opens);
    let bus = SharedBus::new(bus_option(), move || {
        let first = counted.fetch_add(1, Ordering::SeqCst) == 0;
        Ok::<_, RemoteLinkError>(CountingLink {
            stats: LinkStats::default(),
            cycles_left: first.then_some(500),
            record_stale: first,
        })
    })
    .unwrap();
    bus.set_desired(Desired::Open);

    let before = spin("the first link to record stale cycles", || {
        let snapshot = bus.snapshot();
        (snapshot.stale_cycles >= 8).then_some(snapshot)
    });

    let mut last = before.stale_cycles;
    spin("the bus to come back on a fresh link", || {
        let snapshot = bus.snapshot();
        assert!(
            snapshot.stale_cycles >= last,
            "stale_cycles rewound from {last} to {}",
            snapshot.stale_cycles,
        );
        last = snapshot.stale_cycles;
        (opens.load(Ordering::SeqCst) >= 2 && snapshot.actual == Actual::Open).then_some(())
    });

    spin("the reopen to reach the counters", || {
        let snapshot = bus.snapshot();
        assert!(
            snapshot.stale_cycles >= last,
            "stale_cycles rewound from {last} to {} after the reopen",
            snapshot.stale_cycles,
        );
        (snapshot.recoveries >= 1).then_some(())
    });
}

#[test]
fn a_client_that_arrives_in_the_retry_window_waits_instead_of_being_refused() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let bus = SharedBus::new(bus_option(), move || {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(RemoteLinkError::Link("no device found on eth0".to_owned()))
        } else {
            Ok(EchoLink { num_devices: 1 })
        }
    })
    .unwrap();
    let mut server = BusServer::new(
        BusServerOption::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
        Arc::clone(&bus),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    bus.set_desired(Desired::Open);
    spin("the first attempt to fail", || {
        matches!(bus.snapshot().actual, Actual::Failed { .. }).then_some(())
    });

    let mut link = RemoteLink::open(addr, None, &geometry(1))
        .expect("a client inside the retry backoff must wait for the bus, not be refused");
    let mut tx = vec![[0u8; TX_FRAME_BYTES]; 1];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; 1];
    tx[0][0] = 5;
    assert!(link.cycle(&tx, &mut rx).unwrap().rx_valid());
    assert_eq!(rx[0][0], 5);
    assert!(attempts.load(Ordering::SeqCst) >= 2);

    drop(link);
    let _ = handle.join().unwrap();
}
