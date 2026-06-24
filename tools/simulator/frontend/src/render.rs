use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

pub const SOUND_SPEED_MM_S: f32 = 340_000.0;
const MAX_PRESSURE: f32 = 8000.0;
const SLICE_MARGIN_MM: f32 = 40.0;
const SLICE_HEIGHT_MM: f32 = 260.0;
const SLICE_BOTTOM_MM: f32 = -10.0;
const MARKER_SIZE_MM: f32 = 4.5;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    view_proj: [[f32; 4]; 4],
    origin: [f32; 4],
    u: [f32; 4],
    v: [f32; 4],
    sound_speed: f32,
    max_pressure: f32,
    num_trans: u32,
    marker_size: f32,
}

struct Camera {
    target: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

impl Camera {
    fn view_proj(&self, aspect: f32) -> Mat4 {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        let dir = Vec3::new(cp * cy, cp * sy, sp);
        let eye = self.target + dir * self.distance;
        let view = Mat4::look_at_rh(eye, self.target, Vec3::Z);
        let proj = Mat4::perspective_rh(60_f32.to_radians(), aspect.max(0.01), 1.0, 5000.0);
        proj * view
    }
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    slice_pipeline: wgpu::RenderPipeline,
    marker_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buf: wgpu::Buffer,
    depth_view: wgpu::TextureView,
    positions_buf: Option<wgpu::Buffer>,
    states_buf: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
    uniforms: SceneUniforms,
    camera: Camera,
    aspect: f32,
    num_trans: u32,
    show_markers: bool,
    slice_y_range: (f32, f32),
}

impl Renderer {
    pub async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Self, String> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let Gpu {
            surface,
            device,
            queue,
            format,
        } = init_gpu(canvas, width, height).await?;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage_entry(1),
                storage_entry(2),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let slice_pipeline = build_pipeline(
            &device,
            &bind_group_layout,
            &shader,
            format,
            "slice_vs",
            "slice_fs",
        );
        let marker_pipeline = build_pipeline(
            &device,
            &bind_group_layout,
            &shader,
            format,
            "marker_vs",
            "marker_fs",
        );

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene-uniforms"),
            size: size_of::<SceneUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth_view = create_depth(&device, width, height);

        Ok(Self {
            surface,
            device,
            queue,
            slice_pipeline,
            marker_pipeline,
            bind_group_layout,
            uniform_buf,
            depth_view,
            positions_buf: None,
            states_buf: None,
            bind_group: None,
            uniforms: SceneUniforms::zeroed(),
            camera: Camera {
                target: Vec3::ZERO,
                distance: 600.0,
                yaw: -core::f32::consts::FRAC_PI_2,
                pitch: 0.3,
            },
            aspect: width as f32 / height as f32,
            num_trans: 0,
            show_markers: true,
            slice_y_range: (0.0, 0.0),
        })
    }

    pub fn set_geometry(&mut self, positions: &[[f32; 4]]) {
        if positions.is_empty() {
            return;
        }
        self.num_trans = u32::try_from(positions.len()).unwrap_or(u32::MAX);

        let positions_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("positions"),
                contents: bytemuck::cast_slice(positions),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        let states_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("states"),
            size: size_of_val(positions) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: positions_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: states_buf.as_entire_binding(),
                },
            ],
        }));
        self.positions_buf = Some(positions_buf);
        self.states_buf = Some(states_buf);

        self.configure_scene(positions);
    }

    pub fn set_states(&self, states: &[[f32; 4]]) {
        if let Some(buf) = &self.states_buf
            && states.len() == self.num_trans as usize
        {
            self.queue
                .write_buffer(buf, 0, bytemuck::cast_slice(states));
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.camera.yaw -= dx * 0.005;
        self.camera.pitch = (self.camera.pitch + dy * 0.005).clamp(-1.4, 1.4);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.camera.distance = (self.camera.distance * (1.0 + delta * 0.001)).clamp(50.0, 4000.0);
    }

    pub fn set_max_pressure(&mut self, value: f32) {
        self.uniforms.max_pressure = value.max(1.0);
    }

    pub fn set_marker_size(&mut self, value: f32) {
        self.uniforms.marker_size = value.max(0.0);
    }

    pub fn set_show_markers(&mut self, show: bool) {
        self.show_markers = show;
    }

    pub fn set_slice_pos(&mut self, t: f32) {
        let (lo, hi) = self.slice_y_range;
        self.uniforms.origin[1] = lo + (hi - lo) * t.clamp(0.0, 1.0);
    }

    pub fn render(&mut self) {
        let Some(bind_group) = &self.bind_group else {
            return;
        };
        self.uniforms.view_proj = self.camera.view_proj(self.aspect).to_cols_array_2d();
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&self.uniforms));

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                tracing::warn!("surface texture unavailable: {other:?}");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.02,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rpass.set_bind_group(0, bind_group, &[]);
            rpass.set_pipeline(&self.slice_pipeline);
            rpass.draw(0..6, 0..1);
            if self.show_markers {
                rpass.set_pipeline(&self.marker_pipeline);
                rpass.draw(0..6, 0..self.num_trans);
            }
        }
        self.queue.submit([encoder.finish()]);
        frame.present();
    }

    fn configure_scene(&mut self, positions: &[[f32; 4]]) {
        let n = positions.len() as f32;
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        let mut sum = [0.0f32; 3];
        for p in positions {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
                sum[k] += p[k];
            }
        }
        let center = [sum[0] / n, sum[1] / n, sum[2] / n];
        let width = (max[0] - min[0]) + SLICE_MARGIN_MM;

        self.slice_y_range = (min[1] - SLICE_MARGIN_MM, max[1] + SLICE_MARGIN_MM);
        self.uniforms.origin = [center[0] - width / 2.0, center[1], SLICE_BOTTOM_MM, 0.0];
        self.uniforms.u = [width, 0.0, 0.0, 0.0];
        self.uniforms.v = [0.0, 0.0, SLICE_HEIGHT_MM, 0.0];
        self.uniforms.sound_speed = SOUND_SPEED_MM_S;
        self.uniforms.max_pressure = MAX_PRESSURE;
        self.uniforms.num_trans = self.num_trans;
        self.uniforms.marker_size = MARKER_SIZE_MM;

        let extent = (max[0] - min[0]).max(max[1] - min[1]).max(SLICE_HEIGHT_MM);
        self.camera.target = Vec3::new(center[0], center[1], SLICE_HEIGHT_MM * 0.4);
        self.camera.distance = extent * 2.0 + 250.0;
    }
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
}

async fn init_gpu(
    canvas: web_sys::HtmlCanvasElement,
    width: u32,
    height: u32,
) -> Result<Gpu, String> {
    let instance = wgpu::Instance::default();
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|e| format!("create_surface: {e}"))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| format!("request_adapter: {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("simulator-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|e| format!("request_device: {e}"))?;

    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats[0];
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    Ok(Gpu {
        surface,
        device,
        queue,
        format,
    })
}

fn build_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    vs: &str,
    fs: &str,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scene-pl"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(vs),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vs),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
