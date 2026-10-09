use anyhow::Result;

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, TransducerGroups, offset};
use autd3_rs::rt::{TracingOption, init_tracing};
use autd3_rs::units::{m, mm, s};
use autd3_rs::{Client, ClientConfig, TransportOption};
use autd3_rs_pattern::{focus, group_compute};
use autd3_rs_pattern_holo::{AmplitudeTarget, GspatOption, NalgebraBackend, Pa, gspat};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());

    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(
        &geometry,
        &TransportOption::default(),
        ClientConfig::default(),
    )
    .await?;

    println!("devices: {}", client.num_devices());

    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let center = geometry.center();

    let groups = TransducerGroups::new(&geometry, |device, tr| {
        if device.position(tr).x < center.x {
            Side::Left
        } else {
            Side::Right
        }
    });

    let left_foci = [
        AmplitudeTarget {
            point: center + offset(-50.0 * mm, 0.0 * mm, 150.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
        AmplitudeTarget {
            point: center + offset(-20.0 * mm, 0.0 * mm, 150.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
    ];
    let right_target = center + offset(40.0 * mm, 0.0 * mm, 150.0 * mm);

    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();
    group_compute(
        &geometry,
        &groups,
        |side, mask, phases, intensities| match side {
            Side::Left => gspat(
                &NalgebraBackend,
                &geometry,
                &left_foci,
                wavelength,
                &GspatOption {
                    mask,
                    ..Default::default()
                },
                phases,
                intensities,
            ),
            Side::Right => {
                focus(&geometry, right_target, wavelength, phases);
                Ok(())
            }
        },
        &mut phases,
        &mut intensities,
    )?;

    client.send(SetSilencer::default()).await?;
    client.send(Pattern::new(&phases, &intensities)).await?;

    println!("left half -> two GSPAT foci, right half -> single focus — press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;

    client.silent_stop().await?;
    client.close().await?;
    Ok(())
}
