#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_arguments
)]

use std::collections::VecDeque;
use std::time::Duration;

use autd3_rs_core::geometry::Point3;
use bytemuck::NoUninit;

use crate::error::EmulatorError;
use crate::record::{TS, ULTRASOUND_PERIOD_COUNT};

use super::gpu::{Kernel, points, positions, request_device, storage_init};
use super::instant::P0;

#[derive(NoUninit, Clone, Copy)]
#[repr(C)]
struct Pc {
    t: f32,
    sound_speed: f32,
    num_trans: u32,
    offset: i32,
    output_ultrasound_stride: u32,
    ts: f32,
    p0: f32,
    _pad: u32,
}

pub(crate) struct GpuInstant {
    kernel: Kernel,
    num_transducers: u32,
    output_ultrasound: wgpu::Buffer,
}

impl GpuInstant {
    pub(crate) fn new(
        x: &[f32],
        y: &[f32],
        z: &[f32],
        transducers: &[Point3<f32>],
        cache_size: isize,
    ) -> Result<Self, EmulatorError> {
        let target_pos = points(x, y, z);
        let transducer_pos = positions(transducers);
        let output_ultrasound_size =
            transducer_pos.len() * cache_size as usize * ULTRASOUND_PERIOD_COUNT * size_of::<f32>();

        let (device, queue) = request_device(
            size_of::<Pc>(),
            4,
            output_ultrasound_size
                .max(size_of_val(target_pos.as_slice()))
                .max(size_of_val(transducer_pos.as_slice())),
        )?;
        let output_ultrasound = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            size: output_ultrasound_size as wgpu::BufferAddress,
            mapped_at_creation: false,
        });
        let transducer_buf = storage_init(&device, &transducer_pos);
        let target_buf = storage_init(&device, &target_pos);
        let kernel = Kernel::new(
            device,
            queue,
            include_str!("instant.wgsl"),
            size_of::<Pc>(),
            &[&output_ultrasound, &transducer_buf, &target_buf],
            target_pos.len(),
        );

        Ok(Self {
            kernel,
            num_transducers: transducer_pos.len() as u32,
            output_ultrasound,
        })
    }

    pub(crate) fn compute(
        &self,
        cache: &[VecDeque<f32>],
        cache_updated: bool,
        start_time: Duration,
        time_step: Duration,
        num_points_in_frame: usize,
        sound_speed: f32,
        offset: isize,
    ) -> Result<Vec<Vec<f32>>, EmulatorError> {
        if cache_updated {
            let samples: Vec<f32> = cache.iter().flatten().copied().collect();
            self.kernel.queue.write_buffer(
                &self.output_ultrasound,
                0,
                bytemuck::cast_slice(&samples),
            );
        }
        (0..num_points_in_frame)
            .map(|i| {
                self.kernel.run(&Pc {
                    t: (start_time + i as u32 * time_step).as_secs_f32(),
                    sound_speed,
                    num_trans: self.num_transducers,
                    offset: offset as i32,
                    output_ultrasound_stride: cache[0].len() as u32,
                    ts: TS,
                    p0: P0,
                    _pad: 0,
                })
            })
            .collect()
    }
}
