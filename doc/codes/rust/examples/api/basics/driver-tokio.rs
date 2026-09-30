use std::os::fd::AsFd;

use tokio::io::unix::AsyncFd;

use autd3_rs::driver::Poll;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::{Client, ClientConfig, Driver};

async fn drive(mut driver: Driver) -> anyhow::Result<()> {
    let socket = AsyncFd::new(driver.as_fd().try_clone_to_owned()?)?;
    while let Poll::Next(deadline) = driver.poll() {
        tokio::select! {
            () = driver.notified() => {}
            guard = socket.readable() => guard?.clear_ready(),
            () = tokio::time::sleep_until(deadline.into()) => {}
        }
    }
    Ok(driver.close()?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let emulator = UdpEmulator::spawn(geometry.num_devices())?;

    let (driver, connector) = Driver::open(&emulator.option(), geometry.num_devices())?;
    let driver = tokio::spawn(drive(driver));

    let client = Client::open(&geometry, connector, ClientConfig::default()).await?;
    client.close().await?;

    driver.await??;
    Ok(())
}
