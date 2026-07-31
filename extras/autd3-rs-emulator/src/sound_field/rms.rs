#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::f32::consts::{PI, SQRT_2};
use std::time::Duration;

use autd3_rs_core::common::{ULTRASOUND_PERIOD, Velocity};
use autd3_rs_core::params::ULTRASOUND_FREQ_HZ;
use autd3_rs_core::value::Phase;
use rayon::prelude::*;

#[cfg(feature = "polars")]
use polars::frame::DataFrame;

use crate::error::EmulatorError;
use crate::range::Range;
use crate::raw::{RawColumn, RawFrame};
use crate::record::{Record, T4010A1_AMPLITUDE, ULTRASOUND_PERIOD_COUNT};
use crate::sound_field::{SoundFieldOption, distances};

const P0: f32 = T4010A1_AMPLITUDE / (4.0 * PI) / SQRT_2;

#[derive(Debug, Clone, Copy)]
pub struct RmsRecordOption {
    pub sound_speed: Velocity,
    #[cfg(feature = "gpu")]
    pub gpu: bool,
}

impl Default for RmsRecordOption {
    fn default() -> Self {
        Self {
            sound_speed: Velocity::from_m_s(340.0),
            #[cfg(feature = "gpu")]
            gpu: false,
        }
    }
}

pub(crate) struct RmsSource {
    pub(crate) amp: Vec<f32>,
    pub(crate) phase: Vec<f32>,
}

struct CpuRms {
    dists: Vec<Vec<f32>>,
    sources: Vec<RmsSource>,
}

impl CpuRms {
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
}

enum ComputeDevice {
    Cpu(CpuRms),
    #[cfg(feature = "gpu")]
    Gpu(super::rms_gpu::GpuRms),
}

impl ComputeDevice {
    #[cfg_attr(not(feature = "gpu"), allow(clippy::unnecessary_wraps))]
    fn compute(&mut self, frame: usize, wavenumber: f32) -> Result<Vec<f32>, EmulatorError> {
        match self {
            ComputeDevice::Cpu(cpu) => Ok(cpu.frame(frame, wavenumber)),
            #[cfg(feature = "gpu")]
            ComputeDevice::Gpu(gpu) => Ok(gpu.compute(frame, wavenumber)?.clone()),
        }
    }
}

pub struct Rms {
    option: RmsRecordOption,
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
    device: ComputeDevice,
    cursor: usize,
    max_frame: usize,
}

impl Rms {
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

    #[must_use]
    pub fn observe_points_raw(&self) -> RawFrame {
        RawFrame {
            rows: self.x.len(),
            columns: vec![
                ("x[mm]".to_string(), RawColumn::F32(self.x.clone())),
                ("y[mm]".to_string(), RawColumn::F32(self.y.clone())),
                ("z[mm]".to_string(), RawColumn::F32(self.z.clone())),
            ],
        }
    }

    pub fn next_raw(&mut self, duration: Duration) -> Result<RawFrame, EmulatorError> {
        let num_frames = self.advance(duration)?;
        let wavenumber = 2.0 * PI * ULTRASOUND_FREQ_HZ as f32 / self.option.sound_speed.mm_per_s();
        let rows = self.x.len();
        let columns = (0..num_frames)
            .map(|i| {
                let frame = self.cursor + i;
                let t = (frame as u32 * ULTRASOUND_PERIOD).as_nanos() as u64;
                let rms = self.device.compute(frame, wavenumber)?;
                Ok((format!("rms[Pa]@{t}[ns]"), RawColumn::F32(rms)))
            })
            .collect::<Result<Vec<_>, EmulatorError>>()?;
        self.cursor += num_frames;
        Ok(RawFrame { rows, columns })
    }

    #[cfg(feature = "polars")]
    #[must_use]
    pub fn observe_points(&self) -> DataFrame {
        self.observe_points_raw().into_polars()
    }

    #[cfg(feature = "polars")]
    pub fn next(&mut self, duration: Duration) -> Result<DataFrame, EmulatorError> {
        Ok(self.next_raw(duration)?.into_polars())
    }
}

impl Record {
    #[allow(clippy::needless_pass_by_value)]
    #[cfg_attr(not(feature = "gpu"), allow(clippy::unnecessary_wraps))]
    fn sound_field_rms(
        &self,
        range: impl Range,
        option: RmsRecordOption,
    ) -> Result<Rms, EmulatorError> {
        let max_frame = self.records.first().map_or(0, |tr| tr.pulse_width.len());
        let (x, y, z): (Vec<f32>, Vec<f32>, Vec<f32>) = range.points().collect();
        let positions = self.transducer_positions();
        let sources: Vec<RmsSource> = self
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

        #[cfg(feature = "gpu")]
        let device = if option.gpu {
            ComputeDevice::Gpu(super::rms_gpu::GpuRms::new(
                &x, &y, &z, &positions, &sources,
            )?)
        } else {
            ComputeDevice::Cpu(CpuRms {
                dists: distances(&x, &y, &z, &positions),
                sources,
            })
        };
        #[cfg(not(feature = "gpu"))]
        let device = ComputeDevice::Cpu(CpuRms {
            dists: distances(&x, &y, &z, &positions),
            sources,
        });

        Ok(Rms {
            option,
            x,
            y,
            z,
            device,
            cursor: 0,
            max_frame,
        })
    }
}

impl<'a> SoundFieldOption<'a> for RmsRecordOption {
    type Output = Rms;

    fn sound_field(
        self,
        record: &'a Record,
        range: impl Range,
    ) -> Result<Self::Output, EmulatorError> {
        record.sound_field_rms(range, self)
    }
}
