mod instant;
mod rms;

pub use instant::{Instant, InstantRecordOption};
pub use rms::{Rms, RmsRecordOption};

use autd3_rs_core::geometry::Point3;

use crate::error::EmulatorError;
use crate::range::Range;
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
