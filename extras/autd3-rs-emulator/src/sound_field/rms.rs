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
use crate::sound_field::{SoundFieldOption, distances, num_frames, observe_points_raw};

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
            ComputeDevice::Gpu(gpu) => gpu.compute(frame, wavenumber),
        }
    }
}

pub struct Rms {
    wavenumber: f32,
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
    device: ComputeDevice,
    cursor: usize,
    max_frame: usize,
}

impl Rms {
    fn frames_within(&self, duration: Duration) -> Result<usize, EmulatorError> {
        let num_frames = num_frames(duration)?;
        if self.cursor.saturating_add(num_frames) > self.max_frame {
            return Err(EmulatorError::NotRecorded);
        }
        Ok(num_frames)
    }

    pub fn skip(&mut self, duration: Duration) -> Result<&mut Self, EmulatorError> {
        let num_frames = self.frames_within(duration)?;
        self.cursor += num_frames;
        Ok(self)
    }

    #[must_use]
    pub fn observe_points_raw(&self) -> RawFrame {
        observe_points_raw(&self.x, &self.y, &self.z)
    }

    pub fn next_raw(&mut self, duration: Duration) -> Result<RawFrame, EmulatorError> {
        let num_frames = self.frames_within(duration)?;
        let wavenumber = self.wavenumber;
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

impl<'a> SoundFieldOption<'a> for RmsRecordOption {
    type Output = Rms;

    fn sound_field(
        self,
        record: &'a Record,
        range: impl Range,
    ) -> Result<Self::Output, EmulatorError> {
        let (x, y, z): (Vec<f32>, Vec<f32>, Vec<f32>) = range.points().collect();
        let positions = record.transducer_positions();
        let sources: Vec<RmsSource> = record
            .records
            .iter()
            .map(|tr| RmsSource {
                amp: tr
                    .pulse_width
                    .iter()
                    .map(|&w| P0 * (PI * f32::from(w) / ULTRASOUND_PERIOD_COUNT as f32).sin())
                    .collect(),
                phase: tr.phase.iter().map(|&p| Phase(p).rad()).collect(),
            })
            .collect();

        let cpu = |sources| {
            ComputeDevice::Cpu(CpuRms {
                dists: distances(&x, &y, &z, &positions),
                sources,
            })
        };
        #[cfg(feature = "gpu")]
        let device = if self.gpu {
            ComputeDevice::Gpu(super::rms_gpu::GpuRms::new(
                &x, &y, &z, &positions, &sources,
            )?)
        } else {
            cpu(sources)
        };
        #[cfg(not(feature = "gpu"))]
        let device = cpu(sources);

        Ok(Rms {
            wavenumber: 2.0 * PI * ULTRASOUND_FREQ_HZ as f32 / self.sound_speed.mm_s(),
            x,
            y,
            z,
            device,
            cursor: 0,
            max_frame: record.num_samples(),
        })
    }
}
