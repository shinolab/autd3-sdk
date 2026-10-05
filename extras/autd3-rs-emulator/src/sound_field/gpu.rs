#![allow(clippy::cast_possible_truncation)]

use std::borrow::Cow;
use std::sync::mpsc;

use autd3_rs_core::geometry::Point3;
use bytemuck::NoUninit;
use wgpu::util::DeviceExt;

use crate::error::EmulatorError;

const WORKGROUP_SIZE: usize = 64;

#[derive(NoUninit, Clone, Copy)]
#[repr(C)]
pub(super) struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
    _pad: f32,
}

pub(super) fn points(x: &[f32], y: &[f32], z: &[f32]) -> Vec<Vec3> {
    x.iter()
        .zip(y)
        .zip(z)
        .map(|((&x, &y), &z)| Vec3 { x, y, z, _pad: 0. })
        .collect()
}

pub(super) fn positions(positions: &[Point3<f32>]) -> Vec<Vec3> {
    positions
        .iter()
        .map(|p| Vec3 {
            x: p.x,
            y: p.y,
            z: p.z,
            _pad: 0.,
        })
        .collect()
}

pub(super) fn request_device(
    immediate_size: usize,
    storage_buffers: u32,
    max_binding: usize,
) -> Result<(wgpu::Device, wgpu::Queue), EmulatorError> {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    Ok(pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::IMMEDIATES,
            required_limits: wgpu::Limits {
                max_immediate_size: immediate_size as u32,
                max_storage_buffers_per_shader_stage: storage_buffers,
                max_storage_buffer_binding_size: max_binding as _,
                ..wgpu::Limits::downlevel_defaults()
            },
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        },
    ))?)
}

pub(super) fn storage_init<T: NoUninit>(device: &wgpu::Device, data: &[T]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(data),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

pub(super) struct Kernel {
    device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    dst: wgpu::Buffer,
    staging: wgpu::Buffer,
    num_points: usize,
}

impl Kernel {
    pub(super) fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        wgsl: &str,
        immediate_size: usize,
        inputs: &[&wgpu::Buffer],
        num_points: usize,
    ) -> Self {
        let dst_size = (num_points * size_of::<f32>()) as wgpu::BufferAddress;
        let dst = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: dst_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: dst_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout_entries: Vec<_> = (0..=inputs.len())
            .map(|i| wgpu::BindGroupLayoutEntry {
                binding: i as u32,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage {
                        read_only: i < inputs.len(),
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &layout_entries,
        });
        let entries: Vec<_> = inputs
            .iter()
            .copied()
            .chain([&dst])
            .enumerate()
            .map(|(i, buf)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: buf.as_entire_binding(),
            })
            .collect();
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &entries,
        });

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(wgsl)),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: immediate_size as u32,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: None,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            device,
            queue,
            pipeline,
            bind_group,
            dst,
            staging,
            num_points,
        }
    }

    pub(super) fn run(&self, immediates: &impl NoUninit) -> Result<Vec<f32>, EmulatorError> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_immediates(0, bytemuck::bytes_of(immediates));
            pass.dispatch_workgroups(self.num_points.div_ceil(WORKGROUP_SIZE) as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.dst, 0, &self.staging, 0, self.dst.size());
        self.queue.submit(Some(encoder.finish()));

        let slice = self.staging.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("failed to poll device");
        rx.recv()
            .expect("the map callback runs before the poll returns")?;
        let field = bytemuck::cast_slice(
            &slice
                .get_mapped_range()
                .expect("failed to map the staging buffer"),
        )
        .to_vec();
        self.staging.unmap();
        Ok(field)
    }
}
