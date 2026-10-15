mod common;

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};
use std::thread::JoinHandle;

use autd3_rs::commands::{ForceFan, Nop};
use autd3_rs::udp::{UdpError, UdpOption};
use autd3_rs::{Controller, Driver, Error, Frames};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{config, emulator, geometry, option, patient_config};

fn open(emulator: &UdpEmulator, option: &UdpOption) -> (Controller, Driver) {
    Controller::open(&geometry(emulator.num_devices()), option, config()).unwrap()
}

fn drive(mut driver: Driver) -> JoinHandle<Result<(), Error>> {
    std::thread::spawn(move || {
        while let Some(deadline) = driver.poll()? {
            driver.wait(deadline)?;
        }
        Ok(())
    })
}

fn fans(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().force_fan()).collect())
}

fn drive_until_ready<T>(driver: &mut Driver, future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        let deadline = driver.poll().unwrap().unwrap();
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        driver.wait(deadline).unwrap();
    }
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_user_thread_polling_and_waiting_drives_the_controller() {
    let emulator = emulator(2);
    let (controller, driver) = open(&emulator, &option(&emulator));
    let thread = drive(driver);

    controller.initialize().await.unwrap();
    controller.send(ForceFan { value: true }).await.unwrap();
    assert_eq!(fans(&emulator), [true, true]);
    assert_eq!(controller.read_firmware_version().await.unwrap().len(), 2);
    assert_eq!(controller.num_devices(), 2);
    assert_eq!(controller.geometry().num_devices(), 2);
    assert!(controller.device_time_now().is_ok());
    controller.close().await.unwrap();

    assert!(thread.join().unwrap().is_ok());
    assert!(matches!(controller.send(Nop).await, Err(Error::Closed)));
}

#[test]
#[ignore = "depends on real time"]
fn one_thread_can_run_both_the_controller_and_the_driver() {
    let emulator = emulator(2);
    let (controller, mut driver) = open(&emulator, &option(&emulator));

    drive_until_ready(&mut driver, controller.initialize()).unwrap();
    drive_until_ready(&mut driver, controller.send(ForceFan { value: true })).unwrap();
    let telemetry = drive_until_ready(&mut driver, controller.read_telemetry()).unwrap();
    drive_until_ready(&mut driver, controller.close()).unwrap();

    assert_eq!(telemetry.len(), 2);
    assert_eq!(fans(&emulator), [true, true]);
    assert!(matches!(driver.poll(), Ok(None)));
}

#[test]
#[ignore = "depends on real time"]
fn a_controller_resynchronizes_by_itself_when_initialize_is_skipped() {
    let emulator = emulator(1);
    let (controller, mut driver) = open(&emulator, &option(&emulator));
    assert_eq!(controller.bus_stats().resets(), 0);

    drive_until_ready(&mut driver, controller.send(ForceFan { value: true })).unwrap();

    assert_eq!(controller.bus_stats().resets(), 1);
    assert_eq!(controller.bus_stats().frames(), 1);
    assert_eq!(fans(&emulator), [true]);
}

#[test]
#[ignore = "depends on real time"]
fn dropping_the_controller_stops_the_driver() {
    let emulator = emulator(1);
    let (controller, driver) = open(&emulator, &option(&emulator));
    let thread = drive(driver);

    drop(controller);

    assert!(thread.join().unwrap().is_ok());
}

#[test]
#[ignore = "depends on real time"]
fn dropping_the_driver_fails_the_frames_in_flight_and_later_sends_with_closed() {
    let emulator = emulator(1);
    let (controller, mut driver) =
        Controller::open(&geometry(1), option(&emulator), patient_config()).unwrap();
    drive_until_ready(&mut driver, controller.initialize()).unwrap();
    let frames = Frames::encode(controller.geometry(), Nop).unwrap();
    let checker = controller.state_checker();

    emulator.drop_next_replies(0, 1);
    let unanswered =
        drive_until_ready(&mut driver, controller.send_frame(frames.frame(0).unwrap())).unwrap();
    drop(driver);

    assert!(matches!(pollster::block_on(unanswered), Err(Error::Closed)));
    assert!(matches!(
        pollster::block_on(controller.send(Nop)),
        Err(Error::Closed)
    ));
    assert!(matches!(
        pollster::block_on(controller.close()),
        Err(Error::Closed)
    ));
    assert!(matches!(checker.check(), Err(UdpError::Closed)));
}
