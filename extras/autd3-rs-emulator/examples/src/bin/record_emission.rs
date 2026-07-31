// Records the per-transducer emission (phase / pulse width) of a 200 Hz AM focus
//
// Run with: cargo xtask emulator example record_emission

use std::time::Duration;

use anyhow::Result;
use polars::prelude::DataFrame;
use textplots::{Chart, Plot, Shape};

use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Emission, SamplingConfig};

use autd3_rs_emulator::{ClientApi, Emulator};

fn transducer0(df: &DataFrame) -> Vec<f32> {
    df.columns()
        .iter()
        .map(|c| f32::from(c.u16().unwrap().get(0).unwrap()))
        .collect()
}

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
        r.tick(Duration::from_millis(10))?;
        Ok(())
    })?;

    println!(
        "recorded {} transducers x {} samples",
        record.num_transducers(),
        record.num_samples()
    );
    let pulse_width_df = record.pulse_width();
    println!("--- phase ---\n{}", record.phase());
    println!("--- pulse width ---\n{pulse_width_df}");

    let pulse_width: Vec<(f32, f32)> = transducer0(&pulse_width_df)
        .into_iter()
        .enumerate()
        .map(|(i, w)| (i as f32, w))
        .collect();
    println!("pulse width over time (transducer 0, 1 sample = 25 us)");
    Chart::new(220, 50, 0.0, record.num_samples() as f32)
        .lineplot(&Shape::Lines(&pulse_width))
        .display();
    Ok(())
}
