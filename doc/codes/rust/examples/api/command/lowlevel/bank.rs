use core::num::NonZeroU16;

use anyhow::Result;

use autd3_rs::commands::{
    ChangePatternBank, ConfigPattern, PatternCompression, WritePatternBuffer,
    WritePatternCompressed,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{LoopBehavior, PatternBank, SamplingConfig, TransitionMode};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let bank = PatternBank::B0;
    let index = 0;
    let _emissions = geometry.pattern_buffer();
    let emissions = &_emissions;
    // ANCHOR: write
    WritePatternBuffer {
        bank,
        index,
        emissions,
    };
    // ANCHOR_END: write
    let config = SamplingConfig::FREQ_4K;
    let size = 1;
    let loop_behavior = LoopBehavior::Infinite;
    // ANCHOR: config
    ConfigPattern {
        bank,
        config,
        size,
        loop_behavior,
    };
    // ANCHOR_END: config
    let transition_mode = TransitionMode::Immediate;
    // ANCHOR: change
    ChangePatternBank {
        bank,
        transition_mode,
    };
    // ANCHOR_END: change

    let p0 = geometry.pattern_buffer();
    let p1 = geometry.pattern_buffer();
    let p2 = geometry.pattern_buffer();
    let p3 = geometry.pattern_buffer();
    let patterns = [Some(&p0[..]), Some(&p1[..]), Some(&p2[..]), Some(&p3[..])];
    let index = 0;
    let format = PatternCompression::PhaseHalf;
    // ANCHOR: compressed
    WritePatternCompressed {
        bank,
        index,
        format,
        patterns,
    };
    // ANCHOR_END: compressed
    Ok(())
}
