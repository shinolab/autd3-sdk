// Records a uniform full-intensity drive (silencer disabled) and plots the
// output voltage waveform and the emitted ultrasound (T4010A1BVD model) of
// transducer 0 as terminal line charts. With Intensity::MAX and the silencer
// disabled the duty ratio is 50% (pulse width 256 / 512).
// Run with: cargo xtask emulator example output_ultrasound

use anyhow::Result;
use autd3_rs::common::ULTRASOUND_PERIOD;
use textplots::{Chart, Plot, Shape};

use autd3_rs::commands::{Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::units::rad;
use autd3_rs::value::{Emission, Intensity, Phase};

use autd3_rs_emulator::{ClientApi, Emulator};

const PLOT_SAMPLES: usize = 512 * 30;

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

    let mut patterns =
        vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];
    autd3_rs_pattern::uniform(
        Emission {
            phase: Phase::from(std::f32::consts::PI / 2.0 * rad),
            intensity: Intensity::MAX,
        },
        &mut patterns,
    );

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
        r.tick(ULTRASOUND_PERIOD * 30)?;
        Ok(())
    })?;

    lineplot(
        "output voltage [V] (transducer 0)",
        &record.output_voltage_of(0),
    );
    lineplot(
        "emitted ultrasound [a.u.] (transducer 0)",
        &record.output_ultrasound_of(0),
    );
    Ok(())
}
