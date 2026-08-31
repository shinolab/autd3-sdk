mod buffer;
mod pipelines;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use nalgebra::Complex;

use autd3_rs_core::geometry::{Point3, UnitVector3};
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_pattern_holo::{AmplitudeTarget, Directivity, EmissionConstraint, LinAlgBackend};

pub use buffer::{GpuMatrix, GpuVector, Pooled};

const WG: u32 = 256;
const COLS_PER_CHUNK: u32 = 1024;
const ROWWISE_COL_LIMIT: usize = 256;
const FUSED_REPEAT_LIMIT: usize = 256;
const UNIFORM_SIZE: u64 = 32;
const UNIFORM_SLOTS: u64 = 1024;
const MAX_BINDINGS: usize = 6;
const BIND_CACHE_LIMIT: usize = 64;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WgpuError {
    #[error("no suitable GPU adapter was found")]
    NoAdapter,
    #[error("could not create a GPU device: {0}")]
    Device(String),
}

pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: pipelines::Pipelines,
    unused_ro: Pooled,
    unused_rw: Pooled,
    pool: Rc<buffer::BufferPool>,
    uniforms: RefCell<UniformRing>,
    pending: RefCell<Pending>,
    bind_cache: RefCell<HashMap<BindKey, wgpu::BindGroup>>,
    bind_groups_created: Cell<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BindKey {
    layout: u8,
    buffers: [u64; MAX_BINDINGS],
}

#[derive(Default)]
struct Bindings<'a> {
    entries: Vec<wgpu::BindGroupEntry<'a>>,
    buffers: [u64; MAX_BINDINGS],
}

impl<'a> Bindings<'a> {
    fn storage(mut self, buf: &'a Pooled) -> Self {
        let at = self.entries.len();
        self.buffers[at] = buf.id();
        self.entries.push(wgpu::BindGroupEntry {
            binding: to_u32(at),
            resource: buf.as_entire_binding(),
        });
        self
    }

    fn uniform(mut self, dims: &'a Uniform) -> Self {
        let at = self.entries.len();
        self.entries.push(wgpu::BindGroupEntry {
            binding: to_u32(at),
            resource: dims.binding(),
        });
        self
    }
}

#[derive(Default)]
struct Pending {
    encoder: Option<wgpu::CommandEncoder>,
    pass: Option<wgpu::ComputePass<'static>>,
    retired_uniforms: Vec<wgpu::Buffer>,
    touched: HashSet<u64>,
}

struct UniformRing {
    buf: wgpu::Buffer,
    stride: u64,
    cursor: u64,
}

struct Uniform {
    buf: wgpu::Buffer,
    offset: u32,
}

impl Uniform {
    fn binding(&self) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &self.buf,
            offset: 0,
            size: core::num::NonZeroU64::new(UNIFORM_SIZE),
        })
    }
}

impl WgpuBackend {
    pub fn new() -> Result<Self, WgpuError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|_| WgpuError::NoAdapter)?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|e| WgpuError::Device(e.to_string()))?;
        let pipelines = pipelines::Pipelines::new(&device);
        let stride = u64::from(device.limits().min_uniform_buffer_offset_alignment)
            .max(UNIFORM_SIZE)
            .next_power_of_two();
        let uniforms = UniformRing {
            buf: uniform_chunk(&device, stride),
            stride,
            cursor: 0,
        };
        let pool: Rc<buffer::BufferPool> = Rc::default();
        let placeholder = || {
            Pooled::new(
                &pool,
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 8,
                    usage: wgpu::BufferUsages::STORAGE,
                    mapped_at_creation: false,
                }),
            )
        };
        let read_only = placeholder();
        let read_write = placeholder();
        Ok(Self {
            device,
            queue,
            pipelines,
            unused_ro: read_only,
            unused_rw: read_write,
            pool,
            uniforms: RefCell::new(uniforms),
            pending: RefCell::new(Pending::default()),
            bind_cache: RefCell::new(HashMap::new()),
            bind_groups_created: Cell::new(0),
        })
    }

    #[must_use]
    pub fn bind_groups_created(&self) -> u64 {
        self.bind_groups_created.get()
    }

    fn fresh_storage(&self, size: u64) -> Pooled {
        Pooled::new(
            &self.pool,
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        )
    }

    fn storage(&self, bytes: u64) -> Pooled {
        let size = bytes.max(4);
        Pooled::take(&self.pool, size).unwrap_or_else(|| self.fresh_storage(size))
    }

    fn storage_init<T: bytemuck::NoUninit>(&self, data: &[T]) -> Pooled {
        let raw: &[u8] = bytemuck::cast_slice(data);
        let size = (raw.len() as u64).max(4);
        let buf = Pooled::take_avoiding(&self.pool, size, &self.pending.borrow().touched)
            .unwrap_or_else(|| self.fresh_storage(size));
        self.queue.write_buffer(&buf, 0, raw);
        buf
    }

    fn uniform(&self, dims: [u32; 8]) -> Uniform {
        let mut ring = self.uniforms.borrow_mut();
        if ring.cursor == UNIFORM_SLOTS {
            let fresh = uniform_chunk(&self.device, ring.stride);
            let stale = core::mem::replace(&mut ring.buf, fresh);
            self.pending.borrow_mut().retired_uniforms.push(stale);
            ring.cursor = 0;
            self.bind_cache.borrow_mut().clear();
        }
        let offset = ring.cursor * ring.stride;
        ring.cursor += 1;
        self.queue
            .write_buffer(&ring.buf, offset, bytemuck::cast_slice(&dims));
        Uniform {
            buf: ring.buf.clone(),
            offset: u32::try_from(offset).unwrap_or(0),
        }
    }

    fn with_encoder<R>(&self, f: impl FnOnce(&mut wgpu::CommandEncoder) -> R) -> R {
        let mut pending = self.pending.borrow_mut();
        pending.pass = None;
        let encoder = pending.encoder.get_or_insert_with(|| {
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None })
        });
        f(encoder)
    }

    fn bind_group(&self, layout: &pipelines::Layout, bindings: &Bindings<'_>) -> wgpu::BindGroup {
        let key = BindKey {
            layout: layout.tag,
            buffers: bindings.buffers,
        };
        let mut cache = self.bind_cache.borrow_mut();
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
        if cache.len() >= BIND_CACHE_LIMIT {
            cache.clear();
        }
        let fresh = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout.inner,
            entries: &bindings.entries,
        });
        self.bind_groups_created
            .set(self.bind_groups_created.get() + 1);
        cache.insert(key, fresh.clone());
        fresh
    }

    fn dispatch(
        &self,
        pipeline: &wgpu::ComputePipeline,
        layout: &pipelines::Layout,
        bindings: &Bindings<'_>,
        uniform_offset: u32,
        groups: (u32, u32, u32),
    ) {
        let bind_group = self.bind_group(layout, bindings);
        let mut pending = self.pending.borrow_mut();
        pending
            .touched
            .extend(bindings.buffers.iter().copied().filter(|id| *id != 0));
        if pending.pass.is_none() {
            let encoder = pending.encoder.get_or_insert_with(|| {
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None })
            });
            pending.pass = Some(
                encoder
                    .begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: None,
                        timestamp_writes: None,
                    })
                    .forget_lifetime(),
            );
        }
        let pass = pending.pass.as_mut().unwrap();
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[uniform_offset]);
        pass.dispatch_workgroups(groups.0, groups.1, groups.2);
    }

    fn read_back<T>(&self, src: &wgpu::Buffer, bytes: u64, decode: impl FnOnce(&[u8]) -> T) -> T {
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes.max(4),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.with_encoder(|encoder| {
            encoder.copy_buffer_to_buffer(src, 0, &staging, 0, bytes);
        });
        self.flush();

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        if let Err(e) = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        }) {
            panic!("GPU readback failed: the device never finished the copy ({e})");
        }
        match rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                panic!("GPU readback failed: the staging buffer could not be mapped ({e})")
            }
            Err(e) => panic!("GPU readback failed: the map callback was dropped ({e})"),
        }
        let out = {
            let view = slice.get_mapped_range().unwrap_or_else(|e| {
                panic!("GPU readback failed: the mapped range was unavailable ({e})")
            });
            decode(&view)
        };
        staging.unmap();
        out
    }

    fn flush(&self) {
        self.pending.borrow_mut().pass = None;
        let encoder = self.pending.borrow_mut().encoder.take();
        if let Some(encoder) = encoder {
            self.queue.submit(Some(encoder.finish()));
        }
        let mut pending = self.pending.borrow_mut();
        pending.retired_uniforms.clear();
        pending.touched.clear();
        drop(pending);
        self.uniforms.borrow_mut().cursor = 0;
    }

    fn materialize(&self, v: &GpuVector) {
        let taken = v.partial.borrow_mut().take();
        let Some(partial) = taken else {
            return;
        };
        let dims = self.uniform([to_u32(v.len), 0, partial.chunks, 0, 0, 0, 0, 0]);
        self.dispatch(
            &self.pipelines.gemv_reduce,
            &self.pipelines.gemv_layout,
            &Bindings::default()
                .storage(&self.unused_ro)
                .storage(&self.unused_ro)
                .storage(&partial.buf)
                .storage(&v.buf)
                .uniform(&dims)
                .storage(&self.unused_ro),
            dims.offset,
            (to_u32(v.len).max(1), 1, to_u32(v.batch).max(1)),
        );
    }

    fn elementwise(&self, pipeline: &wgpu::ComputePipeline, x: &GpuVector, r: &GpuVector) {
        self.materialize(x);
        self.materialize(r);
        let dims = self.uniform([to_u32(x.len), broadcast_stride(r), 0, 0, 0, 0, 0, 0]);
        self.dispatch(
            pipeline,
            &self.pipelines.elementwise_layout,
            &Bindings::default()
                .storage(&x.buf)
                .storage(&r.buf)
                .uniform(&dims),
            dims.offset,
            (div_ceil(x.len, WG), 1, to_u32(x.batch).max(1)),
        );
    }
}

fn uniform_chunk(device: &wgpu::Device, stride: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: stride * UNIFORM_SLOTS,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

fn div_ceil(a: usize, b: u32) -> u32 {
    to_u32(a.div_ceil(b as usize)).max(1)
}

fn bytes_of(elems: usize) -> u64 {
    (elems as u64) * 8
}

fn broadcast_stride(v: &GpuVector) -> u32 {
    if v.batch > 1 { to_u32(v.len) } else { 0 }
}

impl LinAlgBackend for WgpuBackend {
    type Matrix = GpuMatrix;
    type Vector = GpuVector;
    type BatchMatrix = GpuMatrix;
    type BatchVector = GpuVector;

    fn make_matrix(&self, rows: usize, cols: usize, data: Vec<Complex<f32>>) -> Self::Matrix {
        let raw = buffer::to_raw(&data);
        GpuMatrix {
            buf: self.storage_init(&raw),
            rows,
            cols,
            batch: 1,
            row_major: false,
        }
    }

    fn make_vector(&self, data: Vec<Complex<f32>>) -> Self::Vector {
        let raw = buffer::to_raw(&data);
        GpuVector::new(self.storage_init(&raw), raw.len(), 1)
    }

    fn clone_vector(&self, v: &Self::Vector) -> Self::Vector {
        self.materialize(v);
        let bytes = bytes_of(v.len * v.batch);
        let buf = self.storage(bytes);
        self.with_encoder(|encoder| {
            encoder.copy_buffer_to_buffer(&v.buf, 0, &buf, 0, bytes);
        });
        let mut pending = self.pending.borrow_mut();
        pending.touched.insert(v.buf.id());
        pending.touched.insert(buf.id());
        drop(pending);
        GpuVector::new(buf, v.len, v.batch)
    }

    fn vector_to_host(&self, v: &Self::Vector) -> Vec<Complex<f32>> {
        self.materialize(v);
        let total = v.len * v.batch;
        self.read_back(&v.buf, bytes_of(total), |raw| {
            buffer::from_raw(bytemuck::cast_slice(raw))
        })
    }

    fn propagation_matrix(
        &self,
        tr_pos: &[Point3<f32>],
        tr_dir: &[UnitVector3<f32>],
        foci: &[AmplitudeTarget],
        wavenumber: f32,
        directivity: Directivity,
    ) -> Self::Matrix {
        self.propagation(tr_pos, tr_dir, foci, 1, wavenumber, directivity)
    }

    fn back_prop(&self, g: &Self::Matrix) -> Self::Matrix {
        let row_norm = self.storage((g.rows * g.batch) as u64 * 4);
        let out = self.storage(bytes_of(g.rows * g.cols * g.batch));
        let dims = self.uniform([
            to_u32(g.rows),
            to_u32(g.cols),
            u32::from(g.row_major),
            0,
            0,
            0,
            0,
            0,
        ]);
        let bindings = Bindings::default()
            .storage(&g.buf)
            .storage(&row_norm)
            .storage(&out)
            .uniform(&dims);
        let batch = to_u32(g.batch).max(1);
        self.dispatch(
            &self.pipelines.back_prop_row_norm,
            &self.pipelines.back_prop_layout,
            &bindings,
            dims.offset,
            (to_u32(g.rows).max(1), 1, batch),
        );
        self.dispatch(
            &self.pipelines.back_prop_write,
            &self.pipelines.back_prop_layout,
            &bindings,
            dims.offset,
            (div_ceil(g.rows * g.cols, WG), 1, batch),
        );
        GpuMatrix {
            buf: out,
            rows: g.cols,
            cols: g.rows,
            batch: g.batch,
            row_major: false,
        }
    }

    fn gemm(&self, a: &Self::Matrix, b: &Self::Matrix) -> Self::Matrix {
        let (rows, inner, cols) = (a.rows, a.cols, b.cols);
        let batch = a.batch;
        let out = self.storage(bytes_of(rows * cols * batch));
        let dims = self.uniform([
            to_u32(rows),
            to_u32(inner),
            to_u32(cols),
            u32::from(a.row_major),
            u32::from(b.row_major),
            0,
            0,
            0,
        ]);
        self.dispatch(
            &self.pipelines.gemm,
            &self.pipelines.gemm_layout,
            &Bindings::default()
                .storage(&a.buf)
                .storage(&b.buf)
                .storage(&out)
                .uniform(&dims),
            dims.offset,
            (
                to_u32(rows).max(1),
                to_u32(cols).max(1),
                to_u32(batch).max(1),
            ),
        );
        GpuMatrix {
            buf: out,
            rows,
            cols,
            batch,
            row_major: false,
        }
    }

    fn gemv(&self, a: &Self::Matrix, x: &Self::Vector) -> Self::Vector {
        self.gemv_maybe_normalized(a, x, None)
    }

    fn gemv_hadamard_normalized(
        &self,
        a: &Self::Matrix,
        x: Self::Vector,
        r: &Self::Vector,
    ) -> Self::Vector {
        self.gemv_maybe_normalized(a, &x, Some(r))
    }

    fn repeat_gemv_normalized(
        &self,
        a: &Self::Matrix,
        x: Self::Vector,
        r: &Self::Vector,
        repeat: usize,
    ) -> Self::Vector {
        self.repeat_normalized(a, x, r, repeat)
    }

    fn hadamard_normalize(&self, x: &mut Self::Vector, r: &Self::Vector) {
        self.elementwise(&self.pipelines.hadamard_normalize, x, r);
    }

    fn amplitude_correct(&self, x: &mut Self::Vector, r: &Self::Vector) {
        self.elementwise(&self.pipelines.amplitude_correct, x, r);
    }

    fn quantize(
        &self,
        v: &Self::Vector,
        constraint: EmissionConstraint,
        _parallel: bool,
    ) -> Vec<Emission> {
        self.quantize_on_device(v, constraint)
    }

    fn quantize_batch(
        &self,
        v: &Self::BatchVector,
        constraint: EmissionConstraint,
        _parallel: bool,
    ) -> Vec<Emission> {
        self.quantize_on_device(v, constraint)
    }

    fn make_batch_vector(&self, batch: usize, data: Vec<Complex<f32>>) -> Self::BatchVector {
        let raw = buffer::to_raw(&data);
        GpuVector::new(self.storage_init(&raw), raw.len() / batch.max(1), batch)
    }

    fn batch_vector_to_host(&self, v: &Self::BatchVector) -> Vec<Complex<f32>> {
        self.vector_to_host(v)
    }

    fn batch_propagation_matrix(
        &self,
        tr_pos: &[Point3<f32>],
        tr_dir: &[UnitVector3<f32>],
        foci: &[AmplitudeTarget],
        batch: usize,
        wavenumber: f32,
        directivity: Directivity,
    ) -> Self::BatchMatrix {
        self.propagation(tr_pos, tr_dir, foci, batch, wavenumber, directivity)
    }

    fn batch_back_prop(&self, g: &Self::BatchMatrix) -> Self::BatchMatrix {
        self.back_prop(g)
    }

    fn batch_gemm(&self, a: &Self::BatchMatrix, b: &Self::BatchMatrix) -> Self::BatchMatrix {
        self.gemm(a, b)
    }

    fn batch_gemv(&self, a: &Self::BatchMatrix, x: &Self::BatchVector) -> Self::BatchVector {
        self.gemv(a, x)
    }

    fn batch_gemv_hadamard_normalized(
        &self,
        a: &Self::BatchMatrix,
        x: Self::BatchVector,
        r: &Self::BatchVector,
    ) -> Self::BatchVector {
        self.gemv_hadamard_normalized(a, x, r)
    }

    fn batch_repeat_gemv_normalized(
        &self,
        a: &Self::BatchMatrix,
        x: Self::BatchVector,
        r: &Self::BatchVector,
        repeat: usize,
    ) -> Self::BatchVector {
        self.repeat_normalized(a, x, r, repeat)
    }

    fn batch_amplitude_correct(&self, x: &mut Self::BatchVector, r: &Self::BatchVector) {
        self.amplitude_correct(x, r);
    }

    fn max_batch(&self, bytes_per_problem: usize) -> usize {
        let limits = self.device.limits();
        let cap = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size);
        let by_size = usize::try_from(cap / bytes_per_problem.max(1) as u64).unwrap_or(usize::MAX);
        let by_dispatch =
            usize::try_from(limits.max_compute_workgroups_per_dimension).unwrap_or(usize::MAX);
        by_size.min(by_dispatch).max(1)
    }
}

fn encode_constraint(constraint: EmissionConstraint) -> (u32, u32, u32) {
    match constraint {
        EmissionConstraint::Normalize => (0, 0, 0),
        EmissionConstraint::Multiply(v) => (1, v.to_bits(), 0),
        EmissionConstraint::Uniform(v) => (2, u32::from(v.0), 0),
        EmissionConstraint::Clamp(min, max) => {
            (3, f32::from(min.0).to_bits(), f32::from(max.0).to_bits())
        }
        other => {
            tracing::warn!(
                ?other,
                "this backend has no shader for the constraint; falling back to Normalize"
            );
            (0, 0, 0)
        }
    }
}

impl WgpuBackend {
    fn quantize_on_device(&self, v: &GpuVector, constraint: EmissionConstraint) -> Vec<Emission> {
        self.materialize(v);
        let (len, batch) = (v.len, v.batch.max(1));
        let words = len.div_ceil(2);
        let bytes = (words * batch) as u64 * 4;
        let (mode, p0, p1) = encode_constraint(constraint);
        let out = self.storage(bytes);
        let max = (mode < 2).then(|| self.storage(batch as u64 * 4));
        let max_buf: &Pooled = max.as_ref().unwrap_or(&self.unused_rw);
        let dims = self.uniform([to_u32(len), mode, p0, p1, to_u32(words), 0, 0, 0]);
        let bindings = Bindings::default()
            .storage(&v.buf)
            .storage(max_buf)
            .storage(&out)
            .uniform(&dims);
        let z = to_u32(batch).max(1);
        if max.is_some() {
            self.dispatch(
                &self.pipelines.quantize_max,
                &self.pipelines.quantize_layout,
                &bindings,
                dims.offset,
                (1, 1, z),
            );
        }
        self.dispatch(
            &self.pipelines.quantize_write,
            &self.pipelines.quantize_layout,
            &bindings,
            dims.offset,
            (div_ceil(words, WG), 1, z),
        );

        self.read_back(&out, bytes, |raw| {
            let mut dst = Vec::with_capacity(len * batch);
            for k in 0..batch {
                let at = k * words * 4;
                dst.extend(
                    raw[at..at + len * 2]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| Emission {
                            phase: Phase(b[0]),
                            intensity: Intensity(b[1]),
                        }),
                );
            }
            dst
        })
    }

    fn propagation(
        &self,
        tr_pos: &[Point3<f32>],
        tr_dir: &[UnitVector3<f32>],
        foci: &[AmplitudeTarget],
        batch: usize,
        wavenumber: f32,
        directivity: Directivity,
    ) -> GpuMatrix {
        let rows = foci.len() / batch.max(1);
        let cols = tr_pos.len();
        let pos: Vec<[f32; 4]> = tr_pos.iter().map(|p| [p.x, p.y, p.z, 0.0]).collect();
        let dir: Vec<[f32; 4]> = tr_dir.iter().map(|d| [d.x, d.y, d.z, 0.0]).collect();
        let tgt: Vec<[f32; 4]> = foci
            .iter()
            .map(|f| [f.point.x, f.point.y, f.point.z, 0.0])
            .collect();
        let pos = self.storage_init(&pos);
        let dir = self.storage_init(&dir);
        let tgt = self.storage_init(&tgt);
        let out = self.storage(bytes_of(rows * cols * batch));
        let dims = self.uniform([
            to_u32(rows),
            to_u32(cols),
            wavenumber.to_bits(),
            u32::from(directivity == Directivity::T4010A1),
            0,
            0,
            0,
            0,
        ]);
        self.dispatch(
            &self.pipelines.propagation,
            &self.pipelines.propagation_layout,
            &Bindings::default()
                .storage(&pos)
                .storage(&dir)
                .storage(&tgt)
                .storage(&out)
                .uniform(&dims),
            dims.offset,
            (div_ceil(rows * cols, WG), 1, to_u32(batch).max(1)),
        );
        GpuMatrix {
            buf: out,
            rows,
            cols,
            batch,
            row_major: true,
        }
    }

    fn repeat_normalized(
        &self,
        a: &GpuMatrix,
        x: GpuVector,
        r: &GpuVector,
        repeat: usize,
    ) -> GpuVector {
        if repeat == 0 {
            return x;
        }
        if a.rows != a.cols || a.cols > FUSED_REPEAT_LIMIT {
            let mut x = x;
            for _ in 0..repeat {
                x = self.gemv_maybe_normalized(a, &x, Some(r));
            }
            return x;
        }
        self.materialize(&x);
        self.materialize(r);
        let (n, batch) = (a.cols, a.batch);
        let out = self.storage(bytes_of(n * batch));
        let dims = self.uniform([
            to_u32(n),
            to_u32(repeat),
            broadcast_stride(r),
            broadcast_stride(&x),
            u32::from(a.row_major),
            0,
            0,
            0,
        ]);
        self.dispatch(
            &self.pipelines.repeat_gemv,
            &self.pipelines.gemv_layout,
            &Bindings::default()
                .storage(&a.buf)
                .storage(&x.buf)
                .storage(&self.unused_rw)
                .storage(&out)
                .uniform(&dims)
                .storage(&r.buf),
            dims.offset,
            (1, 1, to_u32(batch).max(1)),
        );
        GpuVector::new(out, n, batch)
    }

    fn gemv_maybe_normalized(
        &self,
        a: &GpuMatrix,
        x: &GpuVector,
        r: Option<&GpuVector>,
    ) -> GpuVector {
        let batch = a.batch;
        let rowwise = a.cols <= ROWWISE_COL_LIMIT;
        if let Some(r) = r {
            self.materialize(r);
        }
        if !rowwise {
            self.materialize(x);
        }
        let x_partial = x.partial.borrow();
        let x_chunks = x_partial.as_ref().map_or(0, |p| p.chunks);
        let out = self.storage(bytes_of(a.rows * batch));
        let chunks = if rowwise {
            1
        } else {
            div_ceil(a.cols, COLS_PER_CHUNK)
        };
        let partial = if rowwise {
            None
        } else {
            Some(self.storage(bytes_of(a.rows * chunks as usize * batch)))
        };
        let partial_buf: &Pooled = partial
            .as_ref()
            .or_else(|| x_partial.as_ref().map(|p| &p.buf))
            .unwrap_or(&self.unused_rw);
        let dims = self.uniform([
            to_u32(a.rows),
            to_u32(a.cols),
            chunks,
            u32::from(r.is_some()),
            r.map_or(0, broadcast_stride),
            u32::from(a.row_major),
            x_chunks,
            broadcast_stride(x),
        ]);
        let r = r.map_or(&self.unused_ro, |r| &r.buf);
        let bindings = Bindings::default()
            .storage(&a.buf)
            .storage(&x.buf)
            .storage(partial_buf)
            .storage(&out)
            .uniform(&dims)
            .storage(r);
        let z = to_u32(batch).max(1);
        if rowwise {
            self.dispatch(
                if x_chunks == 0 {
                    &self.pipelines.gemv_rowwise
                } else {
                    &self.pipelines.gemv_rowwise_pending
                },
                &self.pipelines.gemv_layout,
                &bindings,
                dims.offset,
                (div_ceil(a.rows, WG), 1, z),
            );
            return GpuVector::new(out, a.rows, batch);
        }
        self.dispatch(
            &self.pipelines.gemv_partial,
            &self.pipelines.gemv_layout,
            &bindings,
            dims.offset,
            (chunks, to_u32(a.rows).max(1), z),
        );
        if batch > 1 {
            self.dispatch(
                &self.pipelines.gemv_reduce,
                &self.pipelines.gemv_layout,
                &bindings,
                dims.offset,
                (to_u32(a.rows).max(1), 1, z),
            );
            return GpuVector::new(out, a.rows, batch);
        }
        drop(bindings);
        GpuVector::deferred(
            out,
            a.rows,
            batch,
            buffer::PartialSum {
                buf: partial.expect("the two-pass path always allocates partials"),
                chunks,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_batch_fits_the_dispatch_limit() {
        let Ok(gpu) = WgpuBackend::new() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let limits = gpu.device.limits();
        let z = limits.max_compute_workgroups_per_dimension as usize;
        assert!(gpu.max_batch(1) <= z);
        assert!(gpu.max_batch(usize::MAX) >= 1);
    }
}
