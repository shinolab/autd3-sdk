#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_arguments
)]

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use autd3_rs_core::geometry::Point3;
use bytemuck::NoUninit;
use rayon::prelude::*;
use wgpu::util::DeviceExt;
use wgpu::{Buffer, BufferAddress};

use crate::error::EmulatorError;
use crate::output_ultrasound::OutputUltrasound;
use crate::record::ULTRASOUND_PERIOD_COUNT;

#[derive(NoUninit, Clone, Copy)]
#[repr(C)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
    _pad: f32,
}

impl From<&Point3<f32>> for Vec3 {
    fn from(v: &Point3<f32>) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
            _pad: 0.,
        }
    }
}

#[derive(NoUninit, Clone, Copy)]
#[repr(C)]
struct Pc {
    t: f32,
    sound_speed: f32,
    num_trans: u32,
    offset: i32,
    output_ultrasound_stride: u32,
    _pad: [u32; 3],
}

pub(crate) struct GpuInstant<'a> {
    output_ultrasound: Vec<OutputUltrasound<'a>>,
    output_ultrasound_cache: Vec<VecDeque<f32>>,
    frame_window_size: usize,
    num_transducers: u32,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    buf_staging_output_ultrasound: Buffer,
    buf_storage_output_ultrasound: Buffer,
    buf_output_ultrasound_size: BufferAddress,
    buf_storage_dst: Buffer,
    buf_staging_dst: Buffer,
    update_buf_output_ultrasound: bool,
    field: Vec<Vec<f32>>,
}

fn next_frame(ut: &mut OutputUltrasound<'_>) -> Vec<f32> {
    ut.next_frames(1)
        .unwrap_or_else(|| vec![0.0; ULTRASOUND_PERIOD_COUNT])
}

impl<'a> GpuInstant<'a> {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn new(
        x: &[f32],
        y: &[f32],
        z: &[f32],
        positions: &[Point3<f32>],
        output_ultrasound: Vec<OutputUltrasound<'a>>,
        frame_window_size: usize,
        num_points_in_frame: usize,
        cache_size: isize,
    ) -> Result<Self, EmulatorError> {
        let target_pos = x
            .iter()
            .zip(y.iter())
            .zip(z.iter())
            .map(|((&x, &y), &z)| Vec3 { x, y, z, _pad: 0. })
            .collect::<Vec<_>>();
        let transducer_pos = positions.iter().map(Vec3::from).collect::<Vec<_>>();

        let buf_output_ultrasound_size = (output_ultrasound.len()
            * cache_size as usize
            * ULTRASOUND_PERIOD_COUNT
            * size_of::<f32>()) as BufferAddress;
        let buf_dst_size = (target_pos.len() * size_of::<f32>()) as BufferAddress;
        let buf_target_pos_size = (target_pos.len() * size_of::<Vec3>()) as BufferAddress;
        let buf_tr_pos_size = (transducer_pos.len() * size_of::<Vec3>()) as BufferAddress;

        let instance = wgpu::Instance::default();

        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;

        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::IMMEDIATES,
                required_limits: wgpu::Limits {
                    max_immediate_size: size_of::<Pc>() as u32,
                    max_storage_buffers_per_shader_stage: 5,
                    max_storage_buffer_binding_size: buf_output_ultrasound_size
                        .max(buf_target_pos_size)
                        .max(buf_tr_pos_size)
                        as _,
                    ..wgpu::Limits::downlevel_defaults()
                },
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            }),
        )?;

        let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("instant.wgsl"))),
        });

        let buf_storage_target_pos = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&target_pos),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        let buf_storage_trans_pos = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&transducer_pos),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });

        let buf_staging_output_ultrasound = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: buf_output_ultrasound_size,
            usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let buf_storage_output_ultrasound = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            size: buf_output_ultrasound_size,
            mapped_at_creation: false,
        });

        let buf_staging_dst = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: buf_dst_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buf_storage_dst = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: buf_dst_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf_storage_output_ultrasound.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buf_storage_trans_pos.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buf_storage_target_pos.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: buf_storage_dst.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: size_of::<Pc>() as u32,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            module: &cs_module,
            entry_point: None,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Ok(Self {
            output_ultrasound,
            output_ultrasound_cache: Vec::new(),
            frame_window_size,
            num_transducers: transducer_pos.len() as _,
            device,
            queue,
            pipeline,
            bind_group,
            buf_staging_output_ultrasound,
            buf_storage_output_ultrasound,
            buf_output_ultrasound_size,
            update_buf_output_ultrasound: false,
            buf_storage_dst,
            buf_staging_dst,
            field: vec![vec![0.0f32; target_pos.len()]; num_points_in_frame],
        })
    }

    pub(crate) fn init(&mut self, cache_size: isize, cursor: &mut isize, rem_frame: &mut usize) {
        if self.output_ultrasound_cache.is_empty() {
            let start = *cursor;
            self.output_ultrasound_cache = self
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
            self.update_buf_output_ultrasound = true;
            self.buf_output_ultrasound_size = (self
                .output_ultrasound_cache
                .iter()
                .map(VecDeque::len)
                .sum::<usize>()
                * size_of::<f32>()) as _;
        }
    }

    pub(crate) fn progress(&mut self, cursor: &mut isize) {
        let window = self.frame_window_size as isize;
        let n = match *cursor {
            c if (c + window) < 0 => 0,
            c if c >= 0 => {
                self.update_buf_output_ultrasound = true;
                self.frame_window_size
            }
            c => {
                self.update_buf_output_ultrasound = true;
                (c + window) as usize
            }
        };
        self.output_ultrasound_cache
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

    fn copy_output_ultrasound(&self) {
        let buffer_slice = self.buf_staging_output_ultrasound.slice(..);
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        buffer_slice.map_async(wgpu::MapMode::Write, move |_| {
            let (lock, cvar) = &*pair2;
            let mut started = lock.lock().unwrap();
            *started = true;
            cvar.notify_one();
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("failed to poll device");
        let (lock, cvar) = &*pair;
        let mut started = lock.lock().unwrap();
        while !*started {
            started = cvar.wait(started).unwrap();
        }
        let src = self
            .output_ultrasound_cache
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        buffer_slice
            .get_mapped_range_mut()
            .expect("failed to map the staging buffer")
            .copy_from_slice(bytemuck::cast_slice(&src));
        self.buf_staging_output_ultrasound.unmap();
    }

    pub(crate) fn compute(
        &mut self,
        start_time: Duration,
        time_step: Duration,
        num_points_in_frame: usize,
        sound_speed: f32,
        offset: isize,
    ) -> Result<&Vec<Vec<f32>>, EmulatorError> {
        for i in 0..num_points_in_frame {
            let t = (start_time + i as u32 * time_step).as_secs_f32();
            let pc = Pc {
                t,
                sound_speed,
                num_trans: self.num_transducers,
                offset: offset as _,
                output_ultrasound_stride: self.output_ultrasound_cache[0].len() as _,
                _pad: [0; 3],
            };

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            if self.update_buf_output_ultrasound {
                self.copy_output_ultrasound();
                encoder.copy_buffer_to_buffer(
                    &self.buf_staging_output_ultrasound,
                    0,
                    &self.buf_storage_output_ultrasound,
                    0,
                    self.buf_output_ultrasound_size,
                );
                self.update_buf_output_ultrasound = false;
            }

            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.pipeline);
                cpass.set_bind_group(0, &self.bind_group, &[]);
                cpass.set_immediates(0, bytemuck::bytes_of(&pc));
                cpass.dispatch_workgroups(((self.field[0].len() - 1) / 64 + 1) as _, 1, 1);
            }
            encoder.copy_buffer_to_buffer(
                &self.buf_storage_dst,
                0,
                &self.buf_staging_dst,
                0,
                (self.field[0].len() * size_of::<f32>()) as u64,
            );

            self.queue.submit(Some(encoder.finish()));

            let buffer_slice = self.buf_staging_dst.slice(..);
            let result = Arc::new((Mutex::new(None), Condvar::new()));
            let result_clone = result.clone();
            buffer_slice.map_async(wgpu::MapMode::Read, move |r| {
                let (lock, cvar) = &*result_clone;
                let mut pending = lock.lock().unwrap();
                *pending = Some(r);
                cvar.notify_one();
            });
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("failed to poll device");
            let (lock, cvar) = &*result;
            let mut pending = lock.lock().unwrap();
            while pending.is_none() {
                pending = cvar.wait(pending).unwrap();
            }
            pending.take().unwrap()?;
            {
                let data = buffer_slice
                    .get_mapped_range()
                    .expect("failed to map the staging buffer");
                self.field[i].copy_from_slice(bytemuck::cast_slice(&data));
            }
            self.buf_staging_dst.unmap();
        }

        Ok(&self.field)
    }
}
