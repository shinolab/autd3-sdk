// Records the per-transducer drive (phase / pulse width) of a 200 Hz AM focus
// with the hardware-free emulator. The send loop is identical to a real-device
// example; only the surrounding `record(...)` wrapper and `tick(...)` are
// emulator-specific. Run with:
//   cargo run -p autd3-rs-emulator-examples --bin record_drive

use std::time::Duration;

use anyhow::Result;
use textplots::{Chart, Plot, Shape};

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Emission, SamplingConfig};
use autd3_rs::{Modulation, Pattern, SetSilencer};

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

    let mut modulation = Vec::new();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        &mut modulation,
    )?;

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

    println!(
        "recorded {} transducers x {} samples",
        record.num_transducers(),
        record.num_samples()
    );
    println!("--- phase ---\n{}", record.phase());
    println!("--- pulse width ---\n{}", record.pulse_width());

    let pulse_width: Vec<(f32, f32)> = record
        .pulse_width_of(0)
        .iter()
        .enumerate()
        .map(|(i, &w)| (i as f32, f32::from(w)))
        .collect();
    println!("pulse width over time (transducer 0, 1 sample = 25 us)");
    Chart::new(220, 50, 0.0, record.num_samples() as f32)
        .lineplot(&Shape::Lines(&pulse_width))
        .display();
    Ok(())
}
