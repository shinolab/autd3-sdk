mod common;

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};
use std::thread::JoinHandle;

use autd3_rs::commands::{ForceFan, Nop};
use autd3_rs::udp::TransportOption;
use autd3_rs::{Controller, Driver, Error};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{emulator, fans, geometry, option};

fn open(emulator: &UdpEmulator, option: &TransportOption) -> (Controller, Driver) {
    Controller::open(&geometry(emulator.num_devices()), option).unwrap()
}

fn drive(mut driver: Driver) -> JoinHandle<Result<(), Error>> {
    std::thread::spawn(move || {
        while let Some(deadline) = driver.poll()? {
            driver.wait(deadline)?;
        }
        Ok(())
    })
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

    controller.send(ForceFan { value: true }).await.unwrap();
    assert_eq!(fans(&emulator), [true, true]);
    assert_eq!(controller.read_firmware_version().await.unwrap().len(), 2);
    assert_eq!(controller.num_devices(), 2);
    assert_eq!(controller.geometry().num_devices(), 2);
    assert!(controller.device_time_now().is_ok());
    controller.close().unwrap();

    assert!(thread.join().unwrap().is_ok());
    assert!(matches!(controller.send(Nop).await, Err(Error::Closed)));
}

#[test]
#[ignore = "depends on real time"]
fn one_thread_can_run_both_the_controller_and_the_driver() {
    let emulator = emulator(2);
    let (controller, mut driver) = open(&emulator, &option(&emulator));

    drive_until_ready(&mut driver, controller.send(ForceFan { value: true })).unwrap();
    let telemetry = drive_until_ready(&mut driver, controller.read_telemetry()).unwrap();
    controller.close().unwrap();

    assert_eq!(telemetry.len(), 2);
    assert_eq!(fans(&emulator), [true, true]);
    assert!(matches!(driver.poll(), Ok(None)));
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
