#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::borrow::Cow;
use std::sync::{Arc, Condvar, Mutex};

use autd3_rs_core::geometry::Point3;
use bytemuck::NoUninit;
use wgpu::util::DeviceExt;
use wgpu::{Buffer, BufferAddress};

use crate::error::EmulatorError;

use super::rms::RmsSource;

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
    idx: u32,
    wavenumber: f32,
    num_trans: u32,
    stride: u32,
}

pub(crate) struct GpuRms {
    num_transducers: u32,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    buf_storage_dst: Buffer,
    buf_staging_dst: Buffer,
    buffer: Vec<f32>,
    stride: u32,
}

impl GpuRms {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn new(
        x: &[f32],
        y: &[f32],
        z: &[f32],
        positions: &[Point3<f32>],
        sources: &[RmsSource],
    ) -> Result<Self, EmulatorError> {
        let stride = sources[0].amp.len();

        let target_pos = x
            .iter()
            .zip(y.iter())
            .zip(z.iter())
            .map(|((&x, &y), &z)| Vec3 { x, y, z, _pad: 0. })
            .collect::<Vec<_>>();
        let transducer_pos = positions.iter().map(Vec3::from).collect::<Vec<_>>();

        let buf_amp_size = (sources.len() * stride * size_of::<f32>()) as BufferAddress;
        let buf_dst_size = (target_pos.len() * size_of::<f32>()) as BufferAddress;
        let buf_target_pos_size = (target_pos.len() * size_of::<Vec3>()) as BufferAddress;
        let buf_tr_pos_size = (transducer_pos.len() * size_of::<Vec3>()) as BufferAddress;

        let instance = wgpu::Instance::default();

        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::IMMEDIATES,
                required_limits: wgpu::Limits {
                    max_immediate_size: size_of::<Pc>() as u32,
                    max_storage_buffers_per_shader_stage: 6,
                    max_storage_buffer_binding_size:
                        buf_amp_size.max(buf_target_pos_size).max(buf_tr_pos_size) as _,
                    ..wgpu::Limits::downlevel_defaults()
                },
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            }))?;

        let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("rms.wgsl"))),
        });

        let buf_storage_amp = {
            let amp = sources
                .iter()
                .flat_map(|s| s.amp.iter().copied())
                .collect::<Vec<_>>();
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&amp),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            })
        };
        let buf_storage_phase = {
            let phase = sources
                .iter()
                .flat_map(|s| s.phase.iter().copied())
                .collect::<Vec<_>>();
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&phase),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            })
        };
        let buf_storage_trans_pos = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&transducer_pos),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        let buf_storage_target_pos = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&target_pos),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
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
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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
                    resource: buf_storage_amp.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buf_storage_phase.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buf_storage_trans_pos.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: buf_storage_target_pos.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
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
            num_transducers: transducer_pos.len() as _,
            device,
            queue,
            pipeline,
            bind_group,
            buf_storage_dst,
            buf_staging_dst,
            buffer: vec![0.; target_pos.len()],
            stride: stride as _,
        })
    }

    pub(crate) fn compute(
        &mut self,
        idx: usize,
        wavenumber: f32,
    ) -> Result<&Vec<f32>, EmulatorError> {
        let pc = Pc {
            idx: idx as _,
            wavenumber,
            num_trans: self.num_transducers,
            stride: self.stride,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &self.bind_group, &[]);
            cpass.set_immediates(0, bytemuck::bytes_of(&pc));
            cpass.dispatch_workgroups(((self.buffer.len() - 1) / 64 + 1) as _, 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            &self.buf_storage_dst,
            0,
            &self.buf_staging_dst,
            0,
            (self.buffer.len() * size_of::<f32>()) as u64,
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
            let data = buffer_slice.get_mapped_range();
            self.buffer.copy_from_slice(bytemuck::cast_slice(&data));
        }
        self.buf_staging_dst.unmap();

        Ok(&self.buffer)
    }
}
