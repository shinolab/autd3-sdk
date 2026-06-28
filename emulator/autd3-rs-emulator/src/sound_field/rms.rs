#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::f32::consts::{PI, SQRT_2};
use std::time::Duration;

use autd3_rs_core::common::ULTRASOUND_PERIOD;
use autd3_rs_core::params::ULTRASOUND_FREQ_HZ;
use autd3_rs_core::value::Phase;
use rayon::prelude::*;

#[cfg(feature = "polars")]
use polars::df;
#[cfg(feature = "polars")]
use polars::frame::DataFrame;
#[cfg(feature = "polars")]
use polars::prelude::Column;

use crate::error::EmulatorError;
use crate::range::Range;
use crate::record::{Record, T4010A1_AMPLITUDE, ULTRASOUND_PERIOD_COUNT};
use crate::sound_field::{SoundFieldOption, distances};

const P0: f32 = T4010A1_AMPLITUDE / (4.0 * PI) / SQRT_2;

#[derive(Debug, Clone, Copy)]
pub struct RmsRecordOption {
    pub sound_speed: f32,
}

impl Default for RmsRecordOption {
    fn default() -> Self {
        Self { sound_speed: 340e3 }
    }
}

struct RmsSource {
    amp: Vec<f32>,
    phase: Vec<f32>,
}

pub struct Rms {
    option: RmsRecordOption,
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
    dists: Vec<Vec<f32>>,
    sources: Vec<RmsSource>,
    cursor: usize,
    max_frame: usize,
}

impl Rms {
    fn frame(&self, frame: usize, wavenumber: f32) -> Vec<f32> {
        self.dists
            .par_iter()
            .map(|d| {
                let (re, im) = d.iter().zip(self.sources.iter()).fold(
                    (0.0f32, 0.0f32),
                    |(re, im), (dist, src)| {
                        let r = src.amp[frame] / dist;
                        let theta = wavenumber * dist + src.phase[frame];
                        (re + r * theta.cos(), im + r * theta.sin())
                    },
                );
                (re * re + im * im).sqrt()
            })
            .collect()
    }

    fn advance(&mut self, duration: Duration) -> Result<usize, EmulatorError> {
        if !duration
            .as_nanos()
            .is_multiple_of(ULTRASOUND_PERIOD.as_nanos())
        {
            return Err(EmulatorError::InvalidDuration);
        }
        let num_frames = (duration.as_nanos() / ULTRASOUND_PERIOD.as_nanos()) as usize;
        if self.cursor + num_frames > self.max_frame {
            return Err(EmulatorError::NotRecorded);
        }
        Ok(num_frames)
    }

    pub fn skip(&mut self, duration: Duration) -> Result<&mut Self, EmulatorError> {
        let num_frames = self.advance(duration)?;
        self.cursor += num_frames;
        Ok(self)
    }

    #[cfg(feature = "polars")]
    #[must_use]
    pub fn observe_points(&self) -> DataFrame {
        df!(
            "x[mm]" => &self.x,
            "y[mm]" => &self.y,
            "z[mm]" => &self.z,
        )
        .unwrap()
    }

    #[cfg(feature = "polars")]
    pub fn next(&mut self, duration: Duration) -> Result<DataFrame, EmulatorError> {
        let num_frames = self.advance(duration)?;
        let wavenumber = 2.0 * PI * ULTRASOUND_FREQ_HZ as f32 / self.option.sound_speed;
        let rows = self.x.len();
        let columns = (0..num_frames)
            .map(|i| {
                let frame = self.cursor + i;
                let t = (frame as u32 * ULTRASOUND_PERIOD).as_nanos() as u64;
                let rms = self.frame(frame, wavenumber);
                Column::new(format!("rms[Pa]@{t}[ns]").into(), rms.as_slice())
            })
            .collect::<Vec<_>>();
        self.cursor += num_frames;
        Ok(DataFrame::new(rows, columns).unwrap())
    }
}

impl Record {
    #[allow(clippy::needless_pass_by_value)]
    fn sound_field_rms(&self, range: impl Range, option: RmsRecordOption) -> Rms {
        let max_frame = self.records.first().map_or(0, |tr| tr.pulse_width.len());
        let (x, y, z): (Vec<f32>, Vec<f32>, Vec<f32>) = range.points().collect();
        let dists = distances(&x, &y, &z, &self.transducer_positions());
        let sources = self
            .records
            .iter()
            .map(|tr| RmsSource {
                amp: tr
                    .pulse_width
                    .iter()
                    .map(|&w| P0 * (PI * f32::from(w) / ULTRASOUND_PERIOD_COUNT as f32).sin())
                    .collect(),
                phase: tr.phase.iter().map(|&p| Phase(p).radian()).collect(),
            })
            .collect();
        Rms {
            option,
            x,
            y,
            z,
            dists,
            sources,
            cursor: 0,
            max_frame,
        }
    }
}

impl<'a> SoundFieldOption<'a> for RmsRecordOption {
    type Output = Rms;

    fn sound_field(
        self,
        record: &'a Record,
        range: impl Range,
    ) -> Result<Self::Output, EmulatorError> {
        Ok(record.sound_field_rms(range, self))
    }
}
