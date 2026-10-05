#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::geometry::Point3;
use bytemuck::NoUninit;

use crate::error::EmulatorError;

use super::gpu::{Kernel, points, positions, request_device, storage_init};
use super::rms::RmsSource;

#[derive(NoUninit, Clone, Copy)]
#[repr(C)]
struct Pc {
    idx: u32,
    wavenumber: f32,
    num_trans: u32,
    stride: u32,
}

pub(crate) struct GpuRms {
    kernel: Kernel,
    num_transducers: u32,
    stride: u32,
}

impl GpuRms {
    pub(crate) fn new(
        x: &[f32],
        y: &[f32],
        z: &[f32],
        transducers: &[Point3<f32>],
        sources: &[RmsSource],
    ) -> Result<Self, EmulatorError> {
        let stride = sources[0].amp.len();
        let target_pos = points(x, y, z);
        let transducer_pos = positions(transducers);
        let amp: Vec<f32> = sources.iter().flat_map(|s| &s.amp).copied().collect();
        let phase: Vec<f32> = sources.iter().flat_map(|s| &s.phase).copied().collect();

        let (device, queue) = request_device(
            size_of::<Pc>(),
            5,
            size_of_val(amp.as_slice())
                .max(size_of_val(target_pos.as_slice()))
                .max(size_of_val(transducer_pos.as_slice())),
        )?;
        let amp = storage_init(&device, &amp);
        let phase = storage_init(&device, &phase);
        let transducer_buf = storage_init(&device, &transducer_pos);
        let target_buf = storage_init(&device, &target_pos);
        let kernel = Kernel::new(
            device,
            queue,
            include_str!("rms.wgsl"),
            size_of::<Pc>(),
            &[&amp, &phase, &transducer_buf, &target_buf],
            target_pos.len(),
        );

        Ok(Self {
            kernel,
            num_transducers: transducer_pos.len() as u32,
            stride: stride as u32,
        })
    }

    pub(crate) fn compute(&self, idx: usize, wavenumber: f32) -> Result<Vec<f32>, EmulatorError> {
        self.kernel.run(&Pc {
            idx: idx as u32,
            wavenumber,
            num_trans: self.num_transducers,
            stride: self.stride,
        })
    }
}
