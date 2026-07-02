use core::num::NonZeroU16;
use std::time::Duration;

use autd3_rs::commands::{FixedCompletionTime, FixedUpdateRate, SetSilencer};

fn main() {
    let intensity = Duration::from_micros(250);
    let phase = Duration::from_micros(1000);
    let strict_mode = true;
    // ANCHOR: api
    SetSilencer::default();

    SetSilencer::disable();

    SetSilencer::new(FixedCompletionTime {
        intensity,
        phase,
        strict_mode,
    });
    // ANCHOR_END: api

    let intensity = NonZeroU16::new(256).unwrap();
    let phase = NonZeroU16::new(256).unwrap();

    // ANCHOR: api
    SetSilencer::new(FixedUpdateRate { intensity, phase });
    // ANCHOR_END: api
}
