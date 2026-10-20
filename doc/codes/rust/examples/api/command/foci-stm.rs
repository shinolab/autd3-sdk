use std::f32::consts::PI;

use autd3_rs::commands::{
    ActivatePatternBank, ConfigFociStm, FociStm, FociStmOption, StmConfig, WriteFociBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{ControlPoints, LoopBehavior, PatternBank, TransitionMode};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let radius = 30.0 * mm;
    let num_points = 200;
    // ANCHOR: circle
    let points: Vec<ControlPoints<1>> = (0..num_points)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / num_points as f32;
            ControlPoints::from(center + offset(radius * theta.cos(), radius * theta.sin(), 0.0 * mm))
        })
        .collect();
    // ANCHOR_END: circle
    let freq = 1.0 * Hz;
    let bank = PatternBank::B0;
    let sound_speed = 340.0 * m / s;
    let loop_behavior = LoopBehavior::Infinite;
    let transition_mode = TransitionMode::Immediate;
    let option =
        // ANCHOR: option
        FociStmOption {
            bank,
            sound_speed,
            loop_behavior,
            transition_mode,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    // ANCHOR: api
    FociStm::new(freq, &points, option);
    // ANCHOR_END: api

    const N: u8 = 1;
    // ANCHOR: equivalent
    WriteFociBuffer {
        bank: option.bank,
        start_index: 0,
        points: &points,
    };
    ConfigFociStm {
        bank: option.bank,
        config: StmConfig::new(freq).into_sampling_config(points.len()),
        size: points.len(),
        num_foci: N, // N is the number of foci in the points
        sound_speed: option.sound_speed,
        loop_behavior: option.loop_behavior,
    };
    ActivatePatternBank {
        bank: option.bank,
        transition_mode: option.transition_mode,
    };
    // ANCHOR_END: equivalent
}
