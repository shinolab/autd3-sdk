// Computes the RMS sound field on a plane through a focus with the
// hardware-free emulator. Run with:
//   cargo run -p autd3-rs-emulator-examples --bin rms

use anyhow::Result;

use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Emission, SamplingConfig};
use autd3_rs::{Modulation, Pattern, SetSilencer};

use autd3_rs_emulator::{ClientApi, Emulator, RangeXY, RmsRecordOption};

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut patterns = vec![[Emission::default(); NUM_TRANSDUCERS]; geometry.len()];
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut patterns,
    );

    let mut modulation = Vec::new();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        &mut modulation,
    )?;

    let center = geometry.center();
    let emulator = Emulator::new(geometry);
    let record = emulator.record(async move |r| {
        let mut builder = r.datagram_builder();
        builder
            .push(SetSilencer::default())
            .push(Pattern::new(&patterns))
            .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
        let datagrams = builder.build()?;
        for frame in &datagrams {
            r.send_checked(frame).await?;
        }
        r.tick(ULTRASOUND_PERIOD)?;
        Ok(())
    })?;

    let range = RangeXY {
        x: (center.x - 20.0)..=(center.x + 20.0),
        y: (center.y - 20.0)..=(center.y + 20.0),
        z: 150.0,
        resolution: 2.0,
    };
    let mut rms = record.sound_field(range, RmsRecordOption::default())?;
    println!("--- observe points ---\n{}", rms.observe_points());
    println!("--- rms [Pa] ---\n{}", rms.next(ULTRASOUND_PERIOD)?);
    Ok(())
}
