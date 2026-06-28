// Records a focus for one ultrasound period and computes the per-transducer
// output voltage waveform and the emitted ultrasound (T4010A1BVD model) with
// the hardware-free emulator. Run with:
//   cargo run -p autd3-rs-emulator-examples --bin output_ultrasound

use anyhow::Result;

use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Emission;
use autd3_rs::{Pattern, SetSilencer};

use autd3_rs_emulator::{ClientApi, Emulator};

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

    let emulator = Emulator::new(geometry);
    let record = emulator.record(async move |r| {
        let mut builder = r.datagram_builder();
        builder
            .push(SetSilencer::default())
            .push(Pattern::new(&patterns));
        let datagrams = builder.build()?;
        for frame in &datagrams {
            r.send_checked(frame).await?;
        }
        r.tick(ULTRASOUND_PERIOD)?;
        Ok(())
    })?;

    println!("--- output voltage [V] ---\n{}", record.output_voltage());
    println!(
        "--- emitted ultrasound [a.u.] ---\n{}",
        record.output_ultrasound()
    );
    Ok(())
}
