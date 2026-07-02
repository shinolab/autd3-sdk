use autd3_rs::commands::{
    ChangePatternBank, ConfigPattern, PatternStm, PatternStmMode, PatternStmOption, StmConfig,
    WritePatternBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{LoopBehavior, PatternBank, TransitionMode};

const NUM_POINTS: usize = 200;
const RADIUS_MM: f32 = 30.0;

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    // Compute the Pattern (emission of all transducers) for each sample point on the host.
    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let patterns = (0..NUM_POINTS)
        .map(|i| {
            let theta = 2.0 * std::f32::consts::PI * i as f32 / NUM_POINTS as f32;
            let target = center
                + offset(
                    RADIUS_MM * theta.cos() * mm,
                    RADIUS_MM * theta.sin() * mm,
                    0.0 * mm,
                );
            let mut buffer = geometry.pattern_buffer();
            autd3_rs_pattern::focus(
                &geometry,
                target,
                wavelength,
                &autd3_rs_pattern::FocusOption::default(),
                &mut buffer,
            );
            buffer
        })
        .collect::<Vec<_>>();
    let freq = 1.0 * Hz;
    let option =
        // ANCHOR: option
        PatternStmOption {
            bank: PatternBank::B0,
            mode: PatternStmMode::PhaseIntensityFull,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        }
        // ANCHOR_END: option
        ;
    // ANCHOR: api
    PatternStm::new(freq, &patterns, option);
    // ANCHOR_END: api

    // ANCHOR: equivalent
    for (index, pattern) in patterns.iter().enumerate() {
        WritePatternBuffer {
            bank: option.bank,
            index,
            emissions: pattern.as_slice(),
        };
    }
    ConfigPattern {
        bank: option.bank,
        config: StmConfig::new(freq).into_sampling_config(patterns.len()),
        size: patterns.len(),
        loop_behavior: option.loop_behavior,
    };
    ChangePatternBank {
        bank: option.bank,
        transition_mode: option.transition_mode,
    };
    // ANCHOR_END: equivalent
}
