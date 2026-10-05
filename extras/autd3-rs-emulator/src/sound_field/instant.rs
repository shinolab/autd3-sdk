#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

use std::collections::VecDeque;
use std::f32::consts::{PI, SQRT_2};
use std::time::Duration;

use autd3_rs_core::common::{ULTRASOUND_PERIOD, Velocity};
use rayon::prelude::*;

#[cfg(feature = "polars")]
use polars::frame::DataFrame;

use crate::aabb::{aabb_max_dist, aabb_min_dist};
use crate::error::EmulatorError;
use crate::output_ultrasound::OutputUltrasound;
use crate::range::Range;
use crate::raw::{RawColumn, RawFrame};
use crate::record::{Record, T4010A1_AMPLITUDE, TS, TransducerRecord, ULTRASOUND_PERIOD_COUNT};
use crate::sound_field::{SoundFieldOption, distances, num_frames, observe_points_raw};

pub(super) const P0: f32 = T4010A1_AMPLITUDE * SQRT_2 / (4.0 * PI);

#[derive(Debug, Clone, Copy)]
pub struct InstantRecordOption {
    pub sound_speed: Velocity,
    pub time_step: Duration,
    pub memory_limits_hint_mb: usize,
    #[cfg(feature = "gpu")]
    pub gpu: bool,
}

impl Default for InstantRecordOption {
    fn default() -> Self {
        Self {
            sound_speed: Velocity::from_m_s(340.0),
            time_step: Duration::from_micros(1),
            memory_limits_hint_mb: 128,
            #[cfg(feature = "gpu")]
            gpu: false,
        }
    }
}

struct UltrasoundCache<'a> {
    sources: Vec<OutputUltrasound<'a>>,
    frames: Vec<VecDeque<f32>>,
    updated: bool,
}

fn next_frame(ut: &mut OutputUltrasound<'_>) -> Vec<f32> {
    ut.next_frames(1)
        .unwrap_or_else(|| vec![0.0; ULTRASOUND_PERIOD_COUNT])
}

impl UltrasoundCache<'_> {
    fn fill(&mut self, cache_size: isize, start: isize) {
        self.frames = self
            .sources
            .par_iter_mut()
            .map(|ut| {
                (0..cache_size)
                    .flat_map(|i| {
                        if start + i >= 0 {
                            next_frame(ut)
                        } else {
                            vec![0.0; ULTRASOUND_PERIOD_COUNT]
                        }
                    })
                    .collect()
            })
            .collect();
        self.updated = true;
    }

    fn slide(&mut self, n: usize) {
        self.frames
            .par_iter_mut()
            .zip(self.sources.par_iter_mut())
            .for_each(|(cache, ut)| {
                drop(cache.drain(0..ULTRASOUND_PERIOD_COUNT * n));
                for _ in 0..n {
                    cache.extend(next_frame(ut));
                }
            });
        self.updated |= n > 0;
    }
}

struct Cpu {
    dists: Vec<Vec<f32>>,
}

impl Cpu {
    fn compute(
        &self,
        cache: &[VecDeque<f32>],
        start_time: Duration,
        time_step: Duration,
        num_points_in_frame: usize,
        sound_speed: f32,
        offset: isize,
    ) -> Vec<Vec<f32>> {
        let dists = &self.dists;
        (0..num_points_in_frame)
            .into_par_iter()
            .map(|i| (start_time + i as u32 * time_step).as_secs_f32())
            .map(|t| {
                dists
                    .iter()
                    .map(|d| {
                        P0 * d
                            .iter()
                            .zip(cache.iter())
                            .map(|(dist, output)| {
                                let t_out = t - dist / sound_speed;
                                let a = t_out / TS;
                                let idx = a.floor() as isize;
                                let alpha = a - idx as f32;
                                let idx = (idx - offset) as usize;
                                (output[idx] * (1.0 - alpha) + output[idx + 1] * alpha) / dist
                            })
                            .sum::<f32>()
                    })
                    .collect()
            })
            .collect()
    }
}

enum ComputeDevice {
    Cpu(Cpu),
    #[cfg(feature = "gpu")]
    Gpu(super::instant_gpu::GpuInstant),
}

impl ComputeDevice {
    #[cfg_attr(not(feature = "gpu"), allow(clippy::unnecessary_wraps))]
    fn compute(
        &mut self,
        cache: &mut UltrasoundCache<'_>,
        start_time: Duration,
        time_step: Duration,
        num_points_in_frame: usize,
        sound_speed: f32,
        offset: isize,
    ) -> Result<Vec<Vec<f32>>, EmulatorError> {
        match self {
            ComputeDevice::Cpu(cpu) => Ok(cpu.compute(
                &cache.frames,
                start_time,
                time_step,
                num_points_in_frame,
                sound_speed,
                offset,
            )),
            #[cfg(feature = "gpu")]
            ComputeDevice::Gpu(gpu) => gpu.compute(
                &cache.frames,
                std::mem::take(&mut cache.updated),
                start_time,
                time_step,
                num_points_in_frame,
                sound_speed,
                offset,
            ),
        }
    }
}

pub struct Instant<'a> {
    option: InstantRecordOption,
    cursor: isize,
    last_frame: usize,
    rem_frame: usize,
    max_frame: usize,
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
    frame_window_size: usize,
    cache_size: isize,
    num_points_in_frame: usize,
    cache: UltrasoundCache<'a>,
    device: ComputeDevice,
}

impl Instant<'_> {
    fn advance(
        &mut self,
        duration: Duration,
        skip: bool,
    ) -> Result<Vec<(u64, Vec<f32>)>, EmulatorError> {
        let num_frames = num_frames(duration)?;
        if self.last_frame.saturating_add(num_frames) > self.max_frame {
            return Err(EmulatorError::NotRecorded);
        }

        if self.cache.frames.is_empty() {
            self.cache.fill(self.cache_size, self.cursor);
            self.cursor += self.cache_size;
            self.rem_frame = self.frame_window_size;
        }

        let time_step = self.option.time_step;
        let sound_speed = self.option.sound_speed.mm_s();
        let target = self.last_frame + num_frames;
        let mut cur_frame = self.last_frame;
        let mut out = Vec::new();

        while cur_frame != target {
            let end_frame = if self.rem_frame == 0 {
                let window = self.frame_window_size as isize;
                let n = match self.cursor {
                    c if (c + window) < 0 => 0,
                    c if c >= 0 => self.frame_window_size,
                    c => (c + window) as usize,
                };
                self.cache.slide(n);
                self.cursor += window;
                cur_frame + self.frame_window_size
            } else {
                cur_frame + self.rem_frame
            };
            let end_frame = if end_frame > target {
                self.rem_frame = end_frame - target;
                target
            } else {
                self.rem_frame = 0;
                end_frame
            };
            let local_frames = end_frame - cur_frame;

            if !skip {
                let offset = (self.cursor - self.cache_size) * ULTRASOUND_PERIOD_COUNT as isize;
                for i in 0..local_frames {
                    let start_time = (cur_frame + i) as u32 * ULTRASOUND_PERIOD;
                    let field = self.device.compute(
                        &mut self.cache,
                        start_time,
                        time_step,
                        self.num_points_in_frame,
                        sound_speed,
                        offset,
                    )?;
                    for (ti, pressure) in field.into_iter().enumerate() {
                        let t = (start_time + ti as u32 * time_step).as_nanos() as u64;
                        out.push((t, pressure));
                    }
                }
            }
            cur_frame = end_frame;
        }
        self.last_frame = cur_frame;
        Ok(out)
    }

    pub fn skip(&mut self, duration: Duration) -> Result<&mut Self, EmulatorError> {
        self.advance(duration, true)?;
        Ok(self)
    }

    #[must_use]
    pub fn observe_points_raw(&self) -> RawFrame {
        observe_points_raw(&self.x, &self.y, &self.z)
    }

    pub fn next_raw(&mut self, duration: Duration) -> Result<RawFrame, EmulatorError> {
        let rows = self.x.len();
        let columns = self
            .advance(duration, false)?
            .into_iter()
            .map(|(t, pressure)| (format!("p[Pa]@{t}[ns]"), RawColumn::F32(pressure)))
            .collect::<Vec<_>>();
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

impl<'a> SoundFieldOption<'a> for InstantRecordOption {
    type Output = Instant<'a>;

    fn sound_field(
        self,
        record: &'a Record,
        range: impl Range,
    ) -> Result<Self::Output, EmulatorError> {
        if !ULTRASOUND_PERIOD
            .as_nanos()
            .is_multiple_of(self.time_step.as_nanos())
        {
            return Err(EmulatorError::InvalidTimeStep);
        }
        let num_points_in_frame =
            (ULTRASOUND_PERIOD.as_nanos() / self.time_step.as_nanos()) as usize;

        let (x, y, z): (Vec<f32>, Vec<f32>, Vec<f32>) = range.points().collect();
        let positions = record.transducer_positions();

        let period_secs = ULTRASOUND_PERIOD.as_secs_f32();
        let sound_speed = self.sound_speed.mm_s();
        let min_dist = aabb_min_dist(&record.aabb, &range.aabb());
        let max_dist = aabb_max_dist(&record.aabb, &range.aabb());
        let required_frame_size = (max_dist / sound_speed / period_secs).ceil() as usize
            - (min_dist / sound_speed / period_secs).floor() as usize;

        let frame_window_size = {
            let num_transducers = record.records.len();
            let mem_usage = (x.len() + y.len() + z.len()) * size_of::<f32>()
                + x.len() * num_transducers * size_of::<f32>();
            let memory_limits = self.memory_limits_hint_mb.saturating_mul(1024 * 1024);
            let frame_window_size_mem = (memory_limits.saturating_sub(mem_usage)
                / (ULTRASOUND_PERIOD_COUNT * num_transducers.max(1) * size_of::<f32>()))
            .saturating_sub(required_frame_size)
            .max(1);
            let frame_window_size_time = record.num_samples().max(1);
            frame_window_size_mem.min(frame_window_size_time)
        };

        let cursor = -((max_dist / sound_speed / period_secs).ceil() as isize);
        let cache_size = (required_frame_size + frame_window_size) as isize;

        let cache = UltrasoundCache {
            sources: record
                .records
                .iter()
                .map(TransducerRecord::output_ultrasound_iter)
                .collect(),
            frames: Vec::new(),
            updated: false,
        };

        let cpu = || {
            ComputeDevice::Cpu(Cpu {
                dists: distances(&x, &y, &z, &positions),
            })
        };
        #[cfg(feature = "gpu")]
        let device = if self.gpu {
            ComputeDevice::Gpu(super::instant_gpu::GpuInstant::new(
                &x, &y, &z, &positions, cache_size,
            )?)
        } else {
            cpu()
        };
        #[cfg(not(feature = "gpu"))]
        let device = cpu();

        Ok(Instant {
            option: self,
            cursor,
            last_frame: 0,
            rem_frame: 0,
            max_frame: record.num_samples(),
            x,
            y,
            z,
            frame_window_size,
            cache_size,
            num_points_in_frame,
            cache,
            device,
        })
    }
}
