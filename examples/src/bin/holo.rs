// Two simultaneous foci via GSPAT hologram optimization.
//
// Run with: cargo xtask rust example holo

use anyhow::Result;

use autd3_rs::Client;
use autd3_rs::commands::{Clear, Modulation, Pattern, SetSilencer, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::SamplingConfig;
use autd3_rs_pattern_holo::{AmplitudeTarget, GspatOption, NalgebraBackend, Pa, gspat};
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, &autd3_rs_examples::transport_option())?;
    client.send((Clear, Synchronize)).await?;

    println!("devices: {}", client.num_devices());

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let foci = [
        AmplitudeTarget {
            point: center + offset(-30.0 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
        AmplitudeTarget {
            point: center + offset(30.0 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
    ];

    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();
    gspat(
        &NalgebraBackend,
        &geometry,
        &foci,
        wavelength,
        &GspatOption::default(),
        &mut phases,
        &mut intensities,
    )?;

    let sampling_config = SamplingConfig::FREQ_4K;
    let mut modulation = autd3_rs_modulation::modulation_buffer();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        sampling_config,
        &mut modulation,
    )?;

    client.send(SetSilencer::default()).await?;
    client.send(Pattern::new(&phases, &intensities)).await?;
    client
        .send(Modulation::new(sampling_config, &modulation))
        .await?;

    println!("emitting two GSPAT foci with a 200 Hz AM — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.close().await?;
    Ok(())
}
