#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

use std::collections::VecDeque;
use std::f32::consts::{PI, SQRT_2};
use std::time::Duration;

use autd3_rs_core::common::ULTRASOUND_PERIOD;
use autd3_rs_core::geometry::Point3;
use rayon::prelude::*;

#[cfg(feature = "polars")]
use polars::df;
#[cfg(feature = "polars")]
use polars::frame::DataFrame;
#[cfg(feature = "polars")]
use polars::prelude::Column;

use crate::aabb::{aabb_max_dist, aabb_min_dist};
use crate::error::EmulatorError;
use crate::output_ultrasound::OutputUltrasound;
use crate::range::Range;
use crate::record::{Record, T4010A1_AMPLITUDE, TS, ULTRASOUND_PERIOD_COUNT};
use crate::sound_field::{SoundFieldOption, distances};

const P0: f32 = T4010A1_AMPLITUDE * SQRT_2 / (4.0 * PI);

#[derive(Debug, Clone, Copy)]
pub struct InstantRecordOption {
    pub sound_speed: f32,
    pub time_step: Duration,
    pub memory_limits_hint_mb: usize,
}

impl Default for InstantRecordOption {
    fn default() -> Self {
        Self {
            sound_speed: 340e3,
            time_step: Duration::from_micros(1),
            memory_limits_hint_mb: 128,
        }
    }
}

struct Cpu<'a> {
    output_ultrasound: Vec<OutputUltrasound<'a>>,
    cache: Vec<VecDeque<f32>>,
    dists: Vec<Vec<f32>>,
    field: Vec<Vec<f32>>,
    frame_window_size: usize,
}

fn next_frame(ut: &mut OutputUltrasound<'_>) -> Vec<f32> {
    ut.next_frames(1)
        .unwrap_or_else(|| vec![0.0; ULTRASOUND_PERIOD_COUNT])
}

impl<'a> Cpu<'a> {
    fn new(
        x: &[f32],
        y: &[f32],
        z: &[f32],
        positions: &[Point3<f32>],
        output_ultrasound: Vec<OutputUltrasound<'a>>,
        frame_window_size: usize,
        num_points_in_frame: usize,
    ) -> Self {
        let dists = distances(x, y, z, positions);
        Self {
            output_ultrasound,
            cache: Vec::new(),
            field: vec![vec![0.0; dists.len()]; num_points_in_frame],
            dists,
            frame_window_size,
        }
    }

    fn init(&mut self, cache_size: isize, cursor: &mut isize, rem_frame: &mut usize) {
        if self.cache.is_empty() {
            let start = *cursor;
            self.cache = self
                .output_ultrasound
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
            *cursor += cache_size;
            *rem_frame = self.frame_window_size;
        }
    }

    fn progress(&mut self, cursor: &mut isize) {
        let window = self.frame_window_size as isize;
        let n = match *cursor {
            c if (c + window) < 0 => 0,
            c if c >= 0 => self.frame_window_size,
            c => (c + window) as usize,
        };
        self.cache
            .par_iter_mut()
            .zip(self.output_ultrasound.par_iter_mut())
            .for_each(|(cache, ut)| {
                drop(cache.drain(0..ULTRASOUND_PERIOD_COUNT * n));
                for _ in 0..n {
                    cache.extend(next_frame(ut));
                }
            });
        *cursor += window;
    }

    fn compute(
        &mut self,
        start_time: Duration,
        time_step: Duration,
        num_points_in_frame: usize,
        sound_speed: f32,
        offset: isize,
    ) -> &Vec<Vec<f32>> {
        let dists = &self.dists;
        let cache = &self.cache;
        self.field = (0..num_points_in_frame)
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
            .collect();
        &self.field
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
    cpu: Cpu<'a>,
}

impl Instant<'_> {
    fn advance(
        &mut self,
        duration: Duration,
        skip: bool,
    ) -> Result<Vec<(u64, Vec<f32>)>, EmulatorError> {
        if !duration
            .as_nanos()
            .is_multiple_of(ULTRASOUND_PERIOD.as_nanos())
        {
            return Err(EmulatorError::InvalidDuration);
        }
        let num_frames = (duration.as_nanos() / ULTRASOUND_PERIOD.as_nanos()) as usize;
        if self.last_frame + num_frames > self.max_frame {
            return Err(EmulatorError::NotRecorded);
        }

        self.cpu
            .init(self.cache_size, &mut self.cursor, &mut self.rem_frame);

        let time_step = self.option.time_step;
        let sound_speed = self.option.sound_speed;
        let target = self.last_frame + num_frames;
        let mut cur_frame = self.last_frame;
        let mut out = Vec::new();

        while cur_frame != target {
            let end_frame = if self.rem_frame == 0 {
                self.cpu.progress(&mut self.cursor);
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
                    let field = self.cpu.compute(
                        start_time,
                        time_step,
                        self.num_points_in_frame,
                        sound_speed,
                        offset,
                    );
                    for (ti, pressure) in field.iter().enumerate() {
                        let t = (start_time + ti as u32 * time_step).as_nanos() as u64;
                        out.push((t, pressure.clone()));
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
        let rows = self.x.len();
        let columns = self
            .advance(duration, false)?
            .into_iter()
            .map(|(t, pressure)| Column::new(format!("p[Pa]@{t}[ns]").into(), pressure.as_slice()))
            .collect::<Vec<_>>();
        Ok(DataFrame::new(rows, columns).unwrap())
    }
}

impl Record {
    #[allow(clippy::needless_pass_by_value)]
    fn sound_field_instant(
        &self,
        range: impl Range,
        option: InstantRecordOption,
    ) -> Result<Instant<'_>, EmulatorError> {
        if !ULTRASOUND_PERIOD
            .as_nanos()
            .is_multiple_of(option.time_step.as_nanos())
        {
            return Err(EmulatorError::InvalidTimeStep);
        }
        let max_frame = self.records.first().map_or(0, |tr| tr.pulse_width.len());
        let num_points_in_frame =
            (ULTRASOUND_PERIOD.as_nanos() / option.time_step.as_nanos()) as usize;

        let (x, y, z): (Vec<f32>, Vec<f32>, Vec<f32>) = range.points().collect();
        let positions = self.transducer_positions();

        let period_secs = ULTRASOUND_PERIOD.as_secs_f32();
        let min_dist = aabb_min_dist(&self.aabb, &range.aabb());
        let max_dist = aabb_max_dist(&self.aabb, &range.aabb());
        let required_frame_size = (max_dist / option.sound_speed / period_secs).ceil() as usize
            - (min_dist / option.sound_speed / period_secs).floor() as usize;

        let frame_window_size = {
            let num_transducers = self.records.len();
            let mem_usage = (x.len() + y.len() + z.len()) * size_of::<f32>()
                + x.len() * num_transducers * size_of::<f32>();
            let memory_limits = option.memory_limits_hint_mb.saturating_mul(1024 * 1024);
            let frame_window_size_mem = (memory_limits.saturating_sub(mem_usage)
                / (ULTRASOUND_PERIOD_COUNT * num_transducers.max(1) * size_of::<f32>()))
            .saturating_sub(required_frame_size)
            .max(1);
            let frame_window_size_time = ((self.end_ns() - self.start_ns())
                / ULTRASOUND_PERIOD.as_nanos() as u64)
                .max(1) as usize;
            frame_window_size_mem.min(frame_window_size_time)
        };

        let cursor = -((max_dist / option.sound_speed / period_secs).ceil() as isize);
        let cache_size = (required_frame_size + frame_window_size) as isize;

        let output_ultrasound = self
            .records
            .iter()
            .map(crate::record::TransducerRecord::output_ultrasound_iter)
            .collect();

        let cpu = Cpu::new(
            &x,
            &y,
            &z,
            &positions,
            output_ultrasound,
            frame_window_size,
            num_points_in_frame,
        );

        Ok(Instant {
            option,
            cursor,
            last_frame: 0,
            rem_frame: 0,
            max_frame,
            x,
            y,
            z,
            frame_window_size,
            cache_size,
            num_points_in_frame,
            cpu,
        })
    }
}

impl<'a> SoundFieldOption<'a> for InstantRecordOption {
    type Output = Instant<'a>;

    fn sound_field(
        self,
        record: &'a Record,
        range: impl Range,
    ) -> Result<Self::Output, EmulatorError> {
        record.sound_field_instant(range, self)
    }
}
