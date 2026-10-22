use std::f32::consts::PI;

use autd3_rs::commands::{ActivatePatternBank, ConfigFociStm, WriteFociBuffer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{ControlPoints, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};

fn main() -> anyhow::Result<()> {
    let bank = PatternBank::B0;
    let _points: Vec<ControlPoints> = (0..200)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / 200.0;
            ControlPoints::from(autd3_rs::Point3::origin() + offset(30.0 * mm * theta.cos(), 30.0 * mm * theta.sin(), 0.0 * mm))
        })
        .collect();
    let config = SamplingConfig::from_freq(1.0 * Hz * _points.len() as f32)?;

    let start_index = 0;
    let points = &_points;
    // ANCHOR: write
    WriteFociBuffer {
        bank,
        start_index,
        points,
    };
    // ANCHOR_END: write
    let size = _points.len();
    let num_foci = 1;
    let sound_speed = 340.0 * m / s;
    let loop_behavior = LoopBehavior::Infinite;
    // ANCHOR: config
    ConfigFociStm {
        bank,
        config,
        size,
        num_foci,
        sound_speed,
        loop_behavior,
    };
    // ANCHOR_END: config
    Ok(())
}
