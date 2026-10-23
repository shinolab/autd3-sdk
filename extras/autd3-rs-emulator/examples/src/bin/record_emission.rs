// Records the per-transducer emission (phase / pulse width) of a 200 Hz AM focus
//
// Run with: cargo xtask emulator example record_emission

use std::time::Duration;

use anyhow::Result;
use textplots::{Chart, Plot, Shape};

use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, SamplingConfig};

use autd3_rs_emulator::{Emulator, RawColumn, RawFrame};

fn transducer0(frame: &RawFrame) -> Vec<f32> {
    frame
        .columns
        .iter()
        .filter_map(|(_, column)| match column {
            RawColumn::U8(v) => v.first().copied().map(f32::from),
            RawColumn::U16(v) => v.first().copied().map(f32::from),
            RawColumn::F32(v) => v.first().copied(),
        })
        .collect()
}

fn describe(name: &str, frame: &RawFrame) {
    let first = frame.columns.first().map_or("-", |(label, _)| label);
    let last = frame.columns.last().map_or("-", |(label, _)| label);
    println!(
        "{name}: {} transducers x {} columns ({first} .. {last})",
        frame.rows,
        frame.columns.len()
    );
}

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut phases = geometry.phase_buffer();
    autd3_rs_pattern::focus(&geometry, target, wavelength, &mut phases);

    let sampling_config = SamplingConfig::FREQ_4K;
    let mut modulation = Vec::new();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        sampling_config,
        &mut modulation,
    )?;

    let emulator = Emulator::new(geometry);
    let record = emulator.record(|r| {
        r.send(SetSilencer::default())?;
        r.send(Pattern::new(&phases, Intensity::MAX))?;
        r.send(Modulation::new(sampling_config, &modulation))?;
        r.tick(Duration::from_millis(10))?;
        Ok(())
    })?;

    println!(
        "recorded {} transducers x {} samples",
        record.num_transducers(),
        record.num_samples()
    );
    let pulse_width_frame = record.pulse_width();
    describe("phase", &record.phase());
    describe("pulse width", &pulse_width_frame);

    let pulse_width: Vec<(f32, f32)> = transducer0(&pulse_width_frame)
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
