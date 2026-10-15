use std::time::Duration;

use anyhow::Result;

use autd3_rs::commands::SetSilencer;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{ClientConfig, Controller, Driver, Error, UdpOption};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

const POLL_INTERVAL: Duration = Duration::from_millis(1);

async fn with_run(geometry: &Geometry, option: &UdpOption) -> Result<()> {
    // ANCHOR: run
    let (controller, driver) = Controller::open(geometry, option, ClientConfig::default())?;
    let driving = std::thread::spawn(move || driver.run());

    controller.initialize().await?;
    controller.send(SetSilencer::default()).await?;
    controller.close().await?;

    driving.join().unwrap()?;
    // ANCHOR_END: run
    Ok(())
}

// ANCHOR: poll
fn drive(mut driver: Driver) -> Result<(), Error> {
    while let Some(deadline) = driver.poll()? {
        driver.wait(deadline)?;
    }
    Ok(())
}
// ANCHOR_END: poll

async fn with_poll(geometry: &Geometry, option: &UdpOption) -> Result<()> {
    let (controller, driver) = Controller::open(geometry, option, ClientConfig::default())?;
    let driving = std::thread::spawn(move || drive(driver));

    controller.initialize().await?;
    controller.send(SetSilencer::default()).await?;
    controller.close().await?;

    driving.join().unwrap()?;
    Ok(())
}

#[cfg(unix)]
async fn on_single_thread(geometry: &Geometry, option: &UdpOption) -> Result<()> {
    // ANCHOR: single_thread
    #[cfg(unix)]
    async fn drive_on_reactor(mut driver: Driver) -> Result<()> {
        use std::os::fd::{AsFd, AsRawFd};
        use tokio::io::Interest;
        use tokio::io::unix::AsyncFd;

        let socket = AsyncFd::with_interest(driver.as_fd().as_raw_fd(), Interest::READABLE)?;
        while let Some(deadline) = driver.poll()? {
            tokio::select! {
                readable = socket.readable() => readable?.clear_ready(),
                () = tokio::time::sleep_until(deadline.into()) => {}
            }
        }
        Ok(())
    }

    let (controller, driver) = Controller::open(geometry, option, ClientConfig::default())?;
    let driving = tokio::spawn(drive_on_reactor(driver));

    controller.initialize().await?;
    controller.send(SetSilencer::default()).await?;
    controller.close().await?;

    driving.await??;
    // ANCHOR_END: single_thread
    Ok(())
}

async fn drive_at_interval(mut driver: Driver) -> Result<()> {
    // ANCHOR: interval
    while driver.poll()?.is_some() {
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    // ANCHOR_END: interval
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;
    let option = UdpOption::for_emulator(emulator.interface());

    with_run(&geometry, &option).await?;
    println!("run: ok");
    with_poll(&geometry, &option).await?;
    println!("poll / wait: ok");
    #[cfg(unix)]
    on_single_thread(&geometry, &option).await?;
    println!("reactor: ok");
    Ok(())
}
