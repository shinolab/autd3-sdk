#[cfg(feature = "gpu")]
mod gpu;
mod instant;
#[cfg(feature = "gpu")]
mod instant_gpu;
mod rms;
#[cfg(feature = "gpu")]
mod rms_gpu;

pub use instant::{Instant, InstantRecordOption};
pub use rms::{Rms, RmsRecordOption};

use std::time::Duration;

use autd3_rs_core::common::ULTRASOUND_PERIOD;
use autd3_rs_core::geometry::Point3;

use crate::error::EmulatorError;
use crate::range::Range;
use crate::raw::{RawColumn, RawFrame};
use crate::record::Record;

pub trait SoundFieldOption<'a> {
    type Output;

    fn sound_field(
        self,
        record: &'a Record,
        range: impl Range,
    ) -> Result<Self::Output, EmulatorError>;
}

impl Record {
    pub fn sound_field<'a, T: SoundFieldOption<'a>>(
        &'a self,
        range: impl Range,
        option: T,
    ) -> Result<T::Output, EmulatorError> {
        option.sound_field(self, range)
    }

    pub(crate) fn transducer_positions(&self) -> Vec<Point3<f32>> {
        self.records.iter().map(|tr| tr.position).collect()
    }
}

pub(crate) fn distances(
    x: &[f32],
    y: &[f32],
    z: &[f32],
    positions: &[Point3<f32>],
) -> Vec<Vec<f32>> {
    x.iter()
        .zip(y.iter())
        .zip(z.iter())
        .map(|((&px, &py), &pz)| {
            let p = Point3::new(px, py, pz);
            positions.iter().map(|tp| (p - tp).norm()).collect()
        })
        .collect()
}

pub(crate) fn num_frames(duration: Duration) -> Result<usize, EmulatorError> {
    let period = ULTRASOUND_PERIOD.as_nanos();
    if !duration.as_nanos().is_multiple_of(period) {
        return Err(EmulatorError::InvalidDuration);
    }
    Ok(usize::try_from(duration.as_nanos() / period).unwrap_or(usize::MAX))
}

pub(crate) fn observe_points_raw(x: &[f32], y: &[f32], z: &[f32]) -> RawFrame {
    RawFrame {
        rows: x.len(),
        columns: vec![
            ("x[mm]".to_string(), RawColumn::F32(x.to_vec())),
            ("y[mm]".to_string(), RawColumn::F32(y.to_vec())),
            ("z[mm]".to_string(), RawColumn::F32(z.to_vec())),
        ],
    }
}
