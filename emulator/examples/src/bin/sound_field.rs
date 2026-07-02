// Computes the instantaneous (time-domain) sound field on a plane through a
// focus (silencer disabled), saves it as CSV, and animates it with matplotlib
// (plot_field.py) so the ultrasound oscillation is visible. The recording must
// be long enough for the sound to reach the observation plane; we skip the
// propagation delay and capture one ultrasound period (25 time steps).
// Pass `--no-plot` to skip the Python step. Run with:
//   cargo xtask emulator example sound_field
//   cargo xtask emulator example sound_field --no-plot

use std::fs::File;
use std::time::Duration;

use anyhow::Result;
use polars::prelude::{CsvWriter, SerWriter};

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Emission;

use autd3_rs_emulator::{ClientApi, Emulator, InstantRecordOption, RangeXY};

#[path = "../plot.rs"]
mod plot;

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut patterns =
        vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut patterns,
    );

    let center = geometry.center();
    let emulator = Emulator::new(geometry);
    let record = emulator.record(async move |r| {
        let mut builder = r.datagram_builder();
        builder
            .push(SetSilencer::disable())
            .push(Pattern::new(&patterns));
        let datagrams = builder.build()?;
        for frame in &datagrams {
            r.send_checked(frame).await?;
        }
        r.tick(Duration::from_millis(1))?;
        Ok(())
    })?;

    println!("calculating instantaneous sound field around the focus...");
    let mut instant = record.sound_field(
        RangeXY {
            x: (center.x - 20.0)..=(center.x + 20.0),
            y: (center.y - 20.0)..=(center.y + 20.0),
            z: 150.0,
            resolution: 1.0,
        },
        InstantRecordOption {
            time_step: Duration::from_micros(1),
            ..Default::default()
        },
    )?;

    instant.skip(Duration::from_micros(500))?;
    let points = instant.observe_points();
    let field = instant.next(Duration::from_micros(25))?;
    let mut df = points.hstack(field.columns())?;

    let csv = std::env::temp_dir().join("autd3_emulator_sound_field.csv");
    CsvWriter::new(File::create(&csv)?)
        .include_header(true)
        .finish(&mut df)?;
    println!("saved: {}", csv.display());

    plot::visualize(&csv);
    Ok(())
}
