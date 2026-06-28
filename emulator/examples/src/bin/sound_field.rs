// Computes the instantaneous (time-domain) sound field on a plane through a
// focus with the hardware-free emulator. The recording must be long enough for
// the sound to reach the observation plane; we then skip the propagation delay
// and snapshot one ultrasound period. Run with:
//   cargo run -p autd3-rs-emulator-examples --bin sound_field

use std::time::Duration;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Emission, SamplingConfig};
use autd3_rs::{Modulation, Pattern, SetSilencer};

use autd3_rs_emulator::{ClientApi, Emulator, InstantRecordOption, RangeXY};

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
        r.tick(Duration::from_millis(1))?;
        Ok(())
    })?;

    let range = RangeXY {
        x: (center.x - 10.0)..=(center.x + 10.0),
        y: (center.y - 10.0)..=(center.y + 10.0),
        z: 150.0,
        resolution: 5.0,
    };
    let option = InstantRecordOption {
        time_step: Duration::from_micros(5),
        ..Default::default()
    };
    let mut instant = record.sound_field(range, option)?;
    instant.skip(Duration::from_micros(500))?;
    println!("--- observe points ---\n{}", instant.observe_points());
    println!(
        "--- instant pressure [Pa] ---\n{}",
        instant.next(Duration::from_micros(25))?
    );
    Ok(())
}
