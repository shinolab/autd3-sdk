use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use autd3_rs::commands::{Modulation, SetSilencer};
use autd3_rs::driver::Poll;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::value::SamplingConfig;
use autd3_rs::{BusStats, Client, ClientConfig, DeviceState, Driver, Error, StateChecker};

const DEVICES: usize = 2;

fn geometry(n: usize) -> Geometry {
    Geometry::new((0..n).map(|_| Autd3::default()).collect())
}

async fn stream(client: &Client) -> Result<(), Error> {
    let modulation: Vec<u8> = (0..autd3_rs::params::MOD_BUFFER_SAMPLES)
        .map(|i| u8::try_from((i * 7) % 251).unwrap())
        .collect();
    let frames = client
        .datagram_builder()
        .push(SetSilencer::default())
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation))
        .build()?;
    let mut pending = Vec::new();
    for frame in &frames {
        pending.push(client.send(frame).await?);
    }
    for future in pending {
        future.await?.check()?;
    }
    for _ in 0..8 {
        client.read_firmware_version().await?;
    }
    Ok(())
}

fn assert_clean(stats: &BusStats, checker: &mut StateChecker) {
    assert_eq!(stats.retransmissions(), 0, "no frame may be retransmitted");
    assert_eq!(stats.resets(), 0, "no sequence reset may happen");
    assert_eq!(
        stats.missed_replies(),
        0,
        "no heartbeat reply may be missed"
    );
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready; DEVICES],
        "no device may be reported lost"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_driver_runs_on_a_dedicated_thread() {
    let emulator = UdpEmulator::spawn(DEVICES).unwrap();
    let (mut driver, connector) = Driver::open(&emulator.option(), DEVICES).unwrap();
    let mut checker = driver.state_checker();
    let stats = driver.stats();
    let driver = std::thread::spawn(move || driver.run());

    let client = Client::open(&geometry(DEVICES), connector, ClientConfig::default())
        .await
        .unwrap();
    stream(&client).await.unwrap();
    assert_clean(&stats, &mut checker);

    client.close().await.unwrap();
    driver.join().unwrap().unwrap();
    assert!(checker.check().is_err());
}

fn poll_with_sleep(
    mut driver: Driver,
    nap: Duration,
    stop: Arc<AtomicBool>,
) -> JoinHandle<(u64, Result<(), Error>)> {
    std::thread::spawn(move || {
        let mut polls = 0;
        loop {
            polls += 1;
            if let Poll::Closed = driver.poll() {
                break;
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(nap);
        }
        (polls, driver.close())
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn polls_separated_by_long_sleeps_only_add_latency() {
    let emulator = UdpEmulator::spawn(DEVICES).unwrap();
    let (driver, connector) = Driver::open(&emulator.option(), DEVICES).unwrap();
    let mut checker = driver.state_checker();
    let stats = driver.stats();
    let driver = poll_with_sleep(
        driver,
        Duration::from_millis(30),
        Arc::new(AtomicBool::new(false)),
    );

    let client = Client::open(&geometry(DEVICES), connector, ClientConfig::default())
        .await
        .unwrap();
    stream(&client).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_clean(&stats, &mut checker);
    assert!(stats.heartbeats() > 0);

    client.close().await.unwrap();
    let (polls, closed) = driver.join().unwrap();
    closed.unwrap();
    assert!(polls > 1);
}

#[cfg(unix)]
async fn drive(mut driver: Driver) -> Result<(), Error> {
    use std::os::fd::AsFd;
    use tokio::io::unix::AsyncFd;

    let fd = AsyncFd::new(driver.as_fd().try_clone_to_owned().unwrap()).unwrap();
    while let Poll::Next(deadline) = driver.poll() {
        tokio::select! {
            () = driver.notified() => {}
            guard = fd.readable() => guard.unwrap().clear_ready(),
            () = tokio::time::sleep_until(deadline.into()) => {}
        }
    }
    driver.close()
}

#[cfg(not(unix))]
async fn drive(mut driver: Driver) -> Result<(), Error> {
    while let Poll::Next(deadline) = driver.poll() {
        tokio::select! {
            () = driver.notified() => {}
            () = tokio::time::sleep_until(deadline.into()) => {}
        }
    }
    driver.close()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_driver_runs_as_a_tokio_task() {
    let emulator = UdpEmulator::spawn(DEVICES).unwrap();
    let (driver, connector) = Driver::open(&emulator.option(), DEVICES).unwrap();
    let mut checker = driver.state_checker();
    let stats = driver.stats();
    let driver = tokio::spawn(drive(driver));

    let client = Client::open(&geometry(DEVICES), connector, ClientConfig::default())
        .await
        .unwrap();
    stream(&client).await.unwrap();
    assert_clean(&stats, &mut checker);

    client.close().await.unwrap();
    driver.await.unwrap().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn a_driver_shares_a_single_threaded_runtime_with_the_client() {
    let emulator = UdpEmulator::spawn(DEVICES).unwrap();
    let (driver, connector) = Driver::open(&emulator.option(), DEVICES).unwrap();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let driver = tokio::task::spawn_local(drive(driver));
            let client = Client::open(&geometry(DEVICES), connector, ClientConfig::default())
                .await
                .unwrap();
            stream(&client).await.unwrap();
            client.close().await.unwrap();
            driver.await.unwrap().unwrap();
        })
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_client_closes_the_driver() {
    let emulator = UdpEmulator::spawn(DEVICES).unwrap();
    let (mut driver, connector) = Driver::open(&emulator.option(), DEVICES).unwrap();
    let mut checker = driver.state_checker();
    let driver = std::thread::spawn(move || driver.run());

    let client = Client::open(&geometry(DEVICES), connector, ClientConfig::default())
        .await
        .unwrap();
    drop(client);
    driver.join().unwrap().unwrap();
    assert!(checker.check().is_err());
}

#[test]
fn dropping_the_connector_closes_the_driver() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let (mut driver, connector) = Driver::open(&emulator.option(), 1).unwrap();
    drop(connector);
    driver.run().unwrap();
    assert_eq!(driver.poll(), Poll::Closed);
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_the_driver_resolves_every_waiting_future() {
    let emulator = UdpEmulator::spawn(DEVICES).unwrap();
    let (driver, connector) = Driver::open(&emulator.option(), DEVICES).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let driver = poll_with_sleep(driver, Duration::from_millis(1), Arc::clone(&stop));

    let client = Client::open(
        &geometry(DEVICES),
        connector,
        ClientConfig {
            ack_timeout: Duration::from_secs(60),
            ..ClientConfig::default()
        },
    )
    .await
    .unwrap();
    emulator.reboot(1);
    let frames = client
        .datagram_builder()
        .push(SetSilencer::default())
        .build()
        .unwrap();
    let pending = client.send(frames.iter().next().unwrap()).await.unwrap();

    stop.store(true, Ordering::Release);
    let (_, closed) = driver.join().unwrap();
    closed.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("a pending future must resolve once the driver is gone");
    assert!(matches!(result, Err(Error::DriverClosed)), "{result:?}");
    let sent = tokio::time::timeout(Duration::from_secs(5), client.stop())
        .await
        .expect("a send must not block once the driver is gone");
    assert!(matches!(sent, Err(Error::DriverClosed)), "{sent:?}");
    let closed = tokio::time::timeout(Duration::from_secs(5), client.close())
        .await
        .expect("close must return once the driver is gone");
    assert!(matches!(closed, Err(Error::DriverClosed)), "{closed:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_driver_resolves_every_waiting_future() {
    let emulator = UdpEmulator::spawn(DEVICES).unwrap();
    let (mut driver, connector) = Driver::open(&emulator.option(), DEVICES).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let driver = std::thread::spawn({
        let stop = Arc::clone(&stop);
        move || {
            while !stop.load(Ordering::Acquire) {
                if let Poll::Next(deadline) = driver.poll() {
                    driver.wait(deadline.min(Instant::now() + Duration::from_millis(1)));
                }
            }
            drop(driver);
        }
    });

    let client = Client::open(
        &geometry(DEVICES),
        connector,
        ClientConfig {
            ack_timeout: Duration::from_secs(60),
            ..ClientConfig::default()
        },
    )
    .await
    .unwrap();
    emulator.reboot(1);
    let frames = client
        .datagram_builder()
        .push(SetSilencer::default())
        .build()
        .unwrap();
    let pending = client.send(frames.iter().next().unwrap()).await.unwrap();

    stop.store(true, Ordering::Release);
    driver.join().unwrap();

    let result = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("a pending future must resolve once the driver is dropped");
    assert!(matches!(result, Err(Error::DriverClosed)), "{result:?}");
    let closed = tokio::time::timeout(Duration::from_secs(5), client.close())
        .await
        .expect("close must return once the driver is dropped");
    assert!(matches!(closed, Err(Error::DriverClosed)), "{closed:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_a_driver_that_never_ran_fails_the_open() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let (driver, connector) = Driver::open(&emulator.option(), 1).unwrap();
    let geometry = geometry(1);
    let open = tokio::spawn(async move {
        Client::open(&geometry, connector, ClientConfig::default())
            .await
            .map(|_| ())
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(driver);
    let opened = tokio::time::timeout(Duration::from_secs(5), open)
        .await
        .expect("open must resolve once the driver is gone")
        .unwrap();
    assert!(matches!(opened, Err(Error::DriverClosed)), "{opened:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_notified_future_resolves_on_a_command_and_on_close() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let (mut driver, connector) = Driver::open(&emulator.option(), 1).unwrap();
    assert!(matches!(driver.poll(), Poll::Next(_)));
    let notified = driver.notified();
    let geometry = geometry(1);
    let open = tokio::spawn(async move {
        let _ = Client::open(&geometry, connector, ClientConfig::default()).await;
    });
    tokio::time::timeout(Duration::from_secs(5), notified)
        .await
        .expect("a connect request must wake the driver");

    assert!(matches!(driver.poll(), Poll::Next(_)));
    let notified = driver.notified();
    open.abort();
    let _ = open.await;
    tokio::time::timeout(Duration::from_secs(5), notified)
        .await
        .expect("dropping the pending open must wake the driver");
    let deadline = Instant::now() + Duration::from_secs(5);
    while driver.poll() != Poll::Closed {
        assert!(Instant::now() < deadline);
        driver.wait(Instant::now() + Duration::from_millis(1));
    }
    tokio::time::timeout(Duration::from_secs(5), driver.notified())
        .await
        .expect("a closed driver must not keep a waiter pending");
}
