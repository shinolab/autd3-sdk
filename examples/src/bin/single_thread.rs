use anyhow::Result;

use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, SamplingConfig};
use autd3_rs::{ClientConfig, Controller, Driver, TransportOption};

#[cfg(unix)]
async fn drive(mut driver: Driver) -> Result<()> {
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

#[cfg(not(unix))]
async fn drive(mut driver: Driver) -> Result<()> {
    const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1);

    while driver.poll()?.is_some() {
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let geometry = Geometry::new(vec![Autd3::default()]);

    let (controller, driver) = Controller::open(
        &geometry,
        &TransportOption::default(),
        ClientConfig::default(),
    )?;
    let driving = tokio::spawn(drive(driver));
    controller.initialize().await?;

    println!("devices: {}", controller.num_devices());

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut phases = geometry.phase_buffer();
    autd3_rs_pattern::focus(&geometry, target, wavelength, &mut phases);

    let mut modulation = autd3_rs_modulation::modulation_buffer();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        &mut modulation,
    )?;

    controller.send(SetSilencer::default()).await?;
    controller
        .send(Pattern::new(&phases, Intensity::MAX))
        .await?;
    controller
        .send(Modulation::new(SamplingConfig::FREQ_4K, &modulation))
        .await?;

    println!("press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    controller.close().await?;
    driving.await??;
    Ok(())
}
