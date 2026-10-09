#[cfg(unix)]
use std::time::Duration;

use autd3_rs::commands::{Modulation, SetSilencer};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{ClientConfig, Controller, DeviceState, Driver, Error};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use pollster::block_on;

mod common;
use common::{full_modulation, geometry, option};

fn open(emulator: &UdpEmulator) -> (Controller, Driver) {
    Controller::open(
        &geometry(emulator.num_devices()),
        &option(emulator),
        ClientConfig::default(),
    )
    .unwrap()
}

async fn exercise(controller: &Controller) {
    controller.initialize().await.unwrap();
    controller.send(SetSilencer::default()).await.unwrap();
    let buffer = full_modulation();
    controller
        .send_streaming(Modulation::new(SamplingConfig::FREQ_4K, &buffer))
        .await
        .unwrap()
        .await
        .unwrap();
    let versions = controller.read_firmware_version().await.unwrap();
    assert_eq!(versions.len(), controller.num_devices());
    let status = controller.state_checker().check().unwrap();
    assert!(status.devices().iter().all(|s| *s == DeviceState::Ready));
}

async fn drive_by_polling(mut driver: Driver) -> Result<(), Error> {
    while driver.poll()?.is_some() {
        tokio::task::yield_now().await;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn a_controller_and_a_polled_driver_share_one_thread() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let (controller, driver) = open(&emulator);
    let driving = tokio::spawn(drive_by_polling(driver));

    exercise(&controller).await;
    controller.close().await.unwrap();

    driving.await.unwrap().unwrap();
    assert!(matches!(
        controller.send(SetSilencer::default()).await,
        Err(Error::Closed)
    ));
}

#[cfg(unix)]
async fn drive_on_the_reactor(mut driver: Driver) -> Result<(), Error> {
    use std::os::fd::{AsFd, AsRawFd};
    use tokio::io::Interest;
    use tokio::io::unix::AsyncFd;

    let socket = AsyncFd::with_interest(driver.as_fd().as_raw_fd(), Interest::READABLE).unwrap();
    while let Some(deadline) = driver.poll()? {
        tokio::select! {
            readable = socket.readable() => readable.unwrap().clear_ready(),
            () = tokio::time::sleep_until(deadline.into()) => {}
        }
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_driver_registered_with_the_reactor_wakes_on_replies_and_deadlines() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let (controller, driver) = open(&emulator);
    let driving = tokio::spawn(drive_on_the_reactor(driver));

    exercise(&controller).await;
    let heartbeats = controller.bus_stats().heartbeats();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(controller.bus_stats().heartbeats() > heartbeats);
    controller.close().await.unwrap();

    driving.await.unwrap().unwrap();
}

#[test]
fn a_driver_runs_on_a_thread_the_user_owns() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let (controller, driver) = open(&emulator);
    let driving = std::thread::spawn(move || driver.run());

    block_on(async {
        exercise(&controller).await;
        controller.close().await.unwrap();
    });

    driving.join().unwrap().unwrap();
}

#[test]
fn dropping_the_controller_ends_the_driver() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let (controller, driver) = open(&emulator);
    let driving = std::thread::spawn(move || driver.run());

    block_on(controller.initialize()).unwrap();
    drop(controller);

    driving.join().unwrap().unwrap();
}

#[test]
fn dropping_the_driver_fails_the_frames_in_flight_and_later_sends() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let (controller, mut driver) = open(&emulator);

    let frames = autd3_rs::Frames::encode(controller.geometry(), SetSilencer::default()).unwrap();
    let initialize = controller.initialize();
    let mut initialize = std::pin::pin!(initialize);
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    assert!(initialize.as_mut().poll(&mut cx).is_pending());
    assert!(driver.poll().unwrap().is_some());
    drop(driver);

    assert!(matches!(block_on(initialize), Err(Error::Closed)));
    let frame = (&frames).into_iter().next().unwrap();
    assert!(matches!(
        block_on(controller.send_frame(frame)),
        Err(Error::Closed)
    ));
    assert!(matches!(block_on(controller.close()), Err(Error::Closed)));
    assert!(matches!(block_on(controller.close()), Err(Error::Closed)));
}
