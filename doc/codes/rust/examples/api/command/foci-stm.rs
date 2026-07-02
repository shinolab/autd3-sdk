use autd3_rs::commands::{
    ChangePatternBank, ConfigFociStm, FociStm, FociStmOption, StmConfig, WriteFociBuffer, circle,
    line,
};
use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, LoopBehavior, PatternBank, TransitionMode};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let radius = 30.0 * mm;
    let num_points = 200;
    let normal = Vector3::z_axis();
    let intensity = Intensity::MAX;
    let mut out = Vec::new();
    // ANCHOR: circle
    circle(center, radius, num_points, normal, intensity, &mut out);
    // ANCHOR_END: circle

    let start = center + offset(-15.0 * mm, 0.0 * mm, 0.0 * mm);
    let end = center + offset(15.0 * mm, 0.0 * mm, 0.0 * mm);
    // ANCHOR: line
    line(start, end, num_points, intensity, &mut out);
    // ANCHOR_END: line
    let freq = 1.0 * Hz;
    let option =
        // ANCHOR: option
        FociStmOption {
            bank: PatternBank::B0,
            sound_speed: 340.0 * m / s,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        }
        // ANCHOR_END: option
        ;
    let points = out;
    // ANCHOR: api
    FociStm::new(freq, &points, option);
    // ANCHOR_END: api

    const N: u8 = 1;
    // ANCHOR: equivalent
    WriteFociBuffer {
        bank: option.bank,
        index_offset: 0,
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
    ChangePatternBank {
        bank: option.bank,
        transition_mode: option.transition_mode,
    };
    // ANCHOR_END: equivalent
}
