pub(crate) struct Layout {
    pub(crate) inner: wgpu::BindGroupLayout,
    pub(crate) tag: u8,
}

pub(crate) struct Pipelines {
    pub(crate) elementwise_layout: Layout,
    pub(crate) gemv_layout: Layout,
    pub(crate) gemm_layout: Layout,
    pub(crate) back_prop_layout: Layout,
    pub(crate) propagation_layout: Layout,
    pub(crate) quantize_layout: Layout,

    pub(crate) hadamard_normalize: wgpu::ComputePipeline,
    pub(crate) amplitude_correct: wgpu::ComputePipeline,
    pub(crate) gemv_rowwise: wgpu::ComputePipeline,
    pub(crate) gemv_rowwise_pending: wgpu::ComputePipeline,
    pub(crate) gemv_partial: wgpu::ComputePipeline,
    pub(crate) gemv_reduce: wgpu::ComputePipeline,
    pub(crate) gemm: wgpu::ComputePipeline,
    pub(crate) back_prop_row_norm: wgpu::ComputePipeline,
    pub(crate) back_prop_write: wgpu::ComputePipeline,
    pub(crate) propagation: wgpu::ComputePipeline,
    pub(crate) quantize_max: wgpu::ComputePipeline,
    pub(crate) quantize_write: wgpu::ComputePipeline,
}

#[derive(Clone, Copy)]
enum Kind {
    Read,
    ReadWrite,
    Uniform,
}

fn layout(device: &wgpu::Device, tag: u8, kinds: &[Kind]) -> Layout {
    let entries: Vec<wgpu::BindGroupLayoutEntry> = kinds
        .iter()
        .enumerate()
        .map(|(i, kind)| wgpu::BindGroupLayoutEntry {
            binding: u32::try_from(i).unwrap_or(0),
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: match kind {
                    Kind::Read => wgpu::BufferBindingType::Storage { read_only: true },
                    Kind::ReadWrite => wgpu::BufferBindingType::Storage { read_only: false },
                    Kind::Uniform => wgpu::BufferBindingType::Uniform,
                },
                has_dynamic_offset: matches!(kind, Kind::Uniform),
                min_binding_size: None,
            },
            count: None,
        })
        .collect();
    Layout {
        inner: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        }),
        tag,
    }
}

fn module(device: &wgpu::Device, src: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(src.into()),
    })
}

fn pipeline(
    device: &wgpu::Device,
    bind_group_layout: &Layout,
    module: &wgpu::ShaderModule,
    entry: &str,
) -> wgpu::ComputePipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&bind_group_layout.inner)],
        immediate_size: 0,
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry),
        layout: Some(&layout),
        module,
        entry_point: Some(entry),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

impl Pipelines {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        use Kind::{Read, ReadWrite, Uniform};

        let elementwise_layout = layout(device, 0, &[ReadWrite, Read, Uniform]);
        let vec_layout = layout(
            device,
            1,
            &[Read, Read, ReadWrite, ReadWrite, Uniform, Read],
        );
        let mat_layout = layout(device, 2, &[Read, Read, ReadWrite, Uniform]);
        let back_prop_layout = layout(device, 3, &[Read, ReadWrite, ReadWrite, Uniform]);
        let prop_layout = layout(device, 4, &[Read, Read, Read, ReadWrite, Uniform]);
        let quantize_layout = layout(device, 5, &[Read, ReadWrite, ReadWrite, Uniform]);

        let elementwise = module(device, include_str!("../shaders/elementwise.wgsl"));
        let vec_mod = module(device, include_str!("../shaders/gemv.wgsl"));
        let mat_mod = module(device, include_str!("../shaders/gemm.wgsl"));
        let back_prop = module(device, include_str!("../shaders/back_prop.wgsl"));
        let prop = module(device, include_str!("../shaders/propagation.wgsl"));
        let quant = module(device, include_str!("../shaders/quantize.wgsl"));

        Self {
            hadamard_normalize: pipeline(
                device,
                &elementwise_layout,
                &elementwise,
                "hadamard_normalize",
            ),
            amplitude_correct: pipeline(
                device,
                &elementwise_layout,
                &elementwise,
                "amplitude_correct",
            ),
            gemv_rowwise: pipeline(device, &vec_layout, &vec_mod, "rowwise"),
            gemv_rowwise_pending: pipeline(device, &vec_layout, &vec_mod, "rowwise_pending"),
            gemv_partial: pipeline(device, &vec_layout, &vec_mod, "partial_pass"),
            gemv_reduce: pipeline(device, &vec_layout, &vec_mod, "reduce_pass"),
            gemm: pipeline(device, &mat_layout, &mat_mod, "main"),
            back_prop_row_norm: pipeline(device, &back_prop_layout, &back_prop, "row_norm_pass"),
            back_prop_write: pipeline(device, &back_prop_layout, &back_prop, "write_pass"),
            propagation: pipeline(device, &prop_layout, &prop, "main"),
            quantize_max: pipeline(device, &quantize_layout, &quant, "max_pass"),
            quantize_write: pipeline(device, &quantize_layout, &quant, "quantize_pass"),

            elementwise_layout,
            gemv_layout: vec_layout,
            gemm_layout: mat_layout,
            back_prop_layout,
            propagation_layout: prop_layout,
            quantize_layout,
        }
    }
}
