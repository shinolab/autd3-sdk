// Computes the RMS sound field on a plane through a focus and visualizes it
//
// Run with: cargo xtask emulator example rms

use anyhow::Result;

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;

use autd3_rs_emulator::{AxisOrder, Emulator, Grid, RmsRecordOption};

#[path = "../plot.rs"]
mod plot;

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut phases = geometry.phase_buffer();
    autd3_rs_pattern::focus(&geometry, target, wavelength, &mut phases);

    let center = geometry.center();
    let emulator = Emulator::new(geometry);
    let record = emulator.record(|r| {
        r.send(SetSilencer::disable())?;
        r.send(Pattern::new(&phases, Intensity::MAX))?;
        r.tick(ULTRASOUND_PERIOD)?;
        Ok(())
    })?;

    println!("calculating RMS sound field around the focus...");
    let mut rms = record.sound_field(
        Grid {
            x: (center.x - 20.0)..=(center.x + 20.0),
            y: (center.y - 20.0)..=(center.y + 20.0),
            z: 150.0..=150.0,
            resolution: 1.0,
            order: AxisOrder::XYZ,
        },
        RmsRecordOption::default(),
    )?;

    let points = rms.observe_points();
    let field = rms.next(ULTRASOUND_PERIOD)?;

    let csv = std::env::temp_dir().join("autd3_emulator_rms.csv");
    plot::save_csv(&csv, &[&points, &field])?;
    println!("saved: {}", csv.display());

    plot::visualize(&csv);
    Ok(())
}
