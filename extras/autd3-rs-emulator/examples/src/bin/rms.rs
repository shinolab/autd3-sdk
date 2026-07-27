// Computes the RMS sound field on a plane through a focus (silencer disabled so
// the focus forms immediately), saves it as CSV, and visualizes it with
// matplotlib (plot_field.py). Pass `--no-plot` to skip the Python step.
// Run with:
//   cargo xtask emulator example rms
//   cargo xtask emulator example rms --no-plot

use std::fs::File;

use anyhow::Result;
use polars::prelude::{CsvWriter, SerWriter};

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Emission;

use autd3_rs_emulator::{ClientApi, Emulator, RangeXY, RmsRecordOption};

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
        r.tick(ULTRASOUND_PERIOD)?;
        Ok(())
    })?;

    println!("calculating RMS sound field around the focus...");
    let mut rms = record.sound_field(
        RangeXY {
            x: (center.x - 20.0)..=(center.x + 20.0),
            y: (center.y - 20.0)..=(center.y + 20.0),
            z: 150.0,
            resolution: 1.0,
        },
        RmsRecordOption::default(),
    )?;

    let points = rms.observe_points();
    let field = rms.next(ULTRASOUND_PERIOD)?;
    let mut df = points.hstack(field.columns())?;

    let csv = std::env::temp_dir().join("autd3_emulator_rms.csv");
    CsvWriter::new(File::create(&csv)?)
        .include_header(true)
        .finish(&mut df)?;
    println!("saved: {}", csv.display());

    plot::visualize(&csv);
    Ok(())
}
