use autd3_rs::commands::{ChangePatternBank, ConfigFociStm, StmConfig, WriteFociBuffer, circle};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, TransitionMode};

fn main() {
    let bank = PatternBank::B0;
    let mut _points = Vec::new();
    circle(
        autd3_rs::Point3::origin(),
        30.0 * mm,
        200,
        Vector3::z_axis(),
        Intensity::MAX,
        &mut _points,
    );
    let config = StmConfig::new(1.0 * Hz).into_sampling_config(_points.len());

    let index_offset = 0;
    let points = &_points;
    // ANCHOR: write
    WriteFociBuffer {
        bank,
        index_offset,
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
}
