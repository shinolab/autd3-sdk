// Records a uniform full-intensity emission and plots the output voltage waveform and the emitted ultrasound of 0th transducer
//
// Run with: cargo xtask emulator example output_ultrasound

use anyhow::Result;
use autd3_rs::common::ULTRASOUND_PERIOD;
use textplots::{Chart, Plot, Shape};

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::units::rad;
use autd3_rs::value::{Intensity, Phase};

use autd3_rs_emulator::{Emulator, RawColumn, RawFrame};

const PLOT_SAMPLES: usize = 512 * 30;

fn transducer0(frame: &RawFrame) -> Vec<f32> {
    frame
        .columns
        .iter()
        .filter_map(|(_, column)| match column {
            RawColumn::F32(v) => v.first().copied(),
            RawColumn::U8(_) | RawColumn::U16(_) => None,
        })
        .collect()
}

fn lineplot(title: &str, samples: &[f32]) {
    let points: Vec<(f32, f32)> = samples
        .iter()
        .take(PLOT_SAMPLES)
        .enumerate()
        .map(|(i, &v)| (i as f32, v))
        .collect();
    println!("{title}");
    Chart::new(220, 50, 0.0, PLOT_SAMPLES as f32)
        .lineplot(&Shape::Lines(&points))
        .display();
}

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut phases = geometry.phase_buffer();
    for slot in &mut phases {
        slot.fill(Phase::from(std::f32::consts::PI / 2.0 * rad));
    }

    let emulator = Emulator::new(geometry);
    let record = emulator.record(|r| {
        r.send(SetSilencer::disable())?;
        r.send(Pattern::new(&phases, Intensity::MAX))?;
        r.tick(ULTRASOUND_PERIOD * 30)?;
        Ok(())
    })?;

    lineplot(
        "output voltage [V] (transducer 0)",
        &transducer0(&record.output_voltage()),
    );
    lineplot(
        "emitted ultrasound [a.u.] (transducer 0)",
        &transducer0(&record.output_ultrasound()),
    );
    Ok(())
}
