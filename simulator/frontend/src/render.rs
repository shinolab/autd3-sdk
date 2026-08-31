mod camera;
mod gizmo;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Quat, Vec2, Vec3};
use wgpu::util::DeviceExt;

use camera::Camera;
pub use gizmo::{DragUpdate, GizmoMode};
use gizmo::{Gizmo, RING_SEGMENTS};

pub const SOUND_SPEED_MM_S: f32 = 340_000.0;
const MAX_PRESSURE: f32 = 8000.0;
const SLICE_MARGIN_MM: f32 = 40.0;
const SLICE_HEIGHT_MM: f32 = 260.0;
const SLICE_BOTTOM_MM: f32 = -10.0;
const MARKER_SIZE_MM: f32 = 4.5;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MSAA_SAMPLES: u32 = 4;
const MAX_DIM: u32 = 4096;
const MAX_DPR: f64 = 2.0;
const MAX_PIXELS: f64 = 4.0e6;
const FIELD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const FIELD_TEXELS_PER_MM: f32 = 2.0;
pub const FIELD_MAX_DIM: u32 = 1024;
pub const SLICE_MIN_MM: f32 = 1.0;
pub const SLICE_MAX_MM: f32 = 1024.0;
pub const RESOLUTION_MIN: f32 = 0.25;
pub const RESOLUTION_MAX: f32 = 8.0;
const FIELD_WORKGROUP: u32 = 8;
const SCENE_STAGES: wgpu::ShaderStages =
    wgpu::ShaderStages::VERTEX_FRAGMENT.union(wgpu::ShaderStages::COMPUTE);
pub const DEFAULT_BG_RGB: [f32; 3] = [0.467, 0.463, 0.482];
const DEFAULT_BG: wgpu::Color = wgpu::Color {
    r: DEFAULT_BG_RGB[0] as f64,
    g: DEFAULT_BG_RGB[1] as f64,
    b: DEFAULT_BG_RGB[2] as f64,
    a: 1.0,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    view_proj: [[f32; 4]; 4],
    origin: [f32; 4],
    u: [f32; 4],
    v: [f32; 4],
    eye: [f32; 4],
    sound_speed: f32,
    max_pressure: f32,
    num_trans: u32,
    marker_size: f32,
    colormap: u32,
    gizmo_len: f32,
    active_axis: i32,
    slice_w: u32,
    slice_h: u32,
    _pad: [u32; 3],
}

pub struct Renderer {
    canvas: web_sys::HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    sample_count: u32,
    device: wgpu::Device,
    queue: wgpu::Queue,
    slice_pipeline: wgpu::RenderPipeline,
    marker_pipeline: wgpu::RenderPipeline,
    gizmo_pipeline: wgpu::RenderPipeline,
    ring_pipeline: wgpu::RenderPipeline,
    field_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    field_compute_layout: wgpu::BindGroupLayout,
    field_render_layout: wgpu::BindGroupLayout,
    field_sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
    depth_tex: wgpu::Texture,
    depth_view: wgpu::TextureView,
    msaa_tex: Option<wgpu::Texture>,
    msaa_view: Option<wgpu::TextureView>,
    field_tex: Option<wgpu::Texture>,
    positions_buf: Option<wgpu::Buffer>,
    directions_buf: Option<wgpu::Buffer>,
    states_buf: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
    field_compute_bg: Option<wgpu::BindGroup>,
    field_render_bg: Option<wgpu::BindGroup>,
    uniforms: SceneUniforms,
    camera: Camera,
    initial_camera: Camera,
    aspect: f32,
    num_trans: u32,
    show_markers: bool,
    bg: wgpu::Color,

    geometry_key: Option<u64>,
    axis_range: [(f32, f32); 3],
    slice_center: Vec3,
    slice_rot: Vec3,
    slice_size: Vec2,
    texels_per_mm: f32,
    gizmo: Gizmo,
}

impl Renderer {
    pub async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Self, String> {
        let (width, height) = backing_size(&canvas);
        canvas.set_width(width);
        canvas.set_height(height);
        let Gpu {
            surface,
            config,
            device,
            queue,
            sample_count,
        } = init_gpu(canvas.clone(), width, height).await?;
        let format = config.format;

        let Pipelines {
            bind_group_layout,
            field_compute_layout,
            field_render_layout,
            slice_pipeline,
            marker_pipeline,
            gizmo_pipeline,
            ring_pipeline,
            field_pipeline,
        } = build_pipelines(&device, format, sample_count);

        let field_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("field-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene-uniforms"),
            size: size_of::<SceneUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (depth_tex, depth_view) = create_depth(&device, width, height, sample_count);
        let (msaa_tex, msaa_view) = create_msaa(&device, format, width, height, sample_count)
            .map_or((None, None), |(t, v)| (Some(t), Some(v)));

        let mut camera = Camera {
            pos: Vec3::new(0.0, -600.0, 180.0),
            rot: Vec3::ZERO,
            pivot: Vec3::ZERO,
            fov: 60.0,
            near: 1.0,
            far: 5000.0,
            move_speed: 1.5,
            free: true,
        };
        camera.aim_at_pivot();
        Ok(Self {
            canvas,
            surface,
            config,
            sample_count,
            device,
            queue,
            slice_pipeline,
            marker_pipeline,
            gizmo_pipeline,
            ring_pipeline,
            field_pipeline,
            bind_group_layout,
            field_compute_layout,
            field_render_layout,
            field_sampler,
            uniform_buf,
            depth_tex,
            depth_view,
            msaa_tex,
            msaa_view,
            field_tex: None,
            positions_buf: None,
            directions_buf: None,
            states_buf: None,
            bind_group: None,
            field_compute_bg: None,
            field_render_bg: None,
            uniforms: SceneUniforms::zeroed(),
            camera,
            initial_camera: camera,
            aspect: width as f32 / height as f32,
            num_trans: 0,
            show_markers: true,
            bg: DEFAULT_BG,
            geometry_key: None,
            axis_range: [(0.0, 0.0); 3],
            slice_center: Vec3::ZERO,
            slice_rot: Vec3::ZERO,
            slice_size: Vec2::ZERO,
            texels_per_mm: FIELD_TEXELS_PER_MM,
            gizmo: Gizmo::new(),
        })
    }

    pub fn sync_size(&mut self) {
        let (width, height) = backing_size(&self.canvas);
        if width == self.config.width && height == self.config.height {
            return;
        }
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);

        self.depth_tex.destroy();
        if let Some(msaa) = &self.msaa_tex {
            msaa.destroy();
        }
        let (depth_tex, depth_view) = create_depth(&self.device, width, height, self.sample_count);
        self.depth_tex = depth_tex;
        self.depth_view = depth_view;
        let (msaa_tex, msaa_view) = create_msaa(
            &self.device,
            self.config.format,
            width,
            height,
            self.sample_count,
        )
        .map_or((None, None), |(t, v)| (Some(t), Some(v)));
        self.msaa_tex = msaa_tex;
        self.msaa_view = msaa_view;

        self.aspect = width as f32 / height as f32;
    }

    #[must_use]
    pub fn to_ndc(&self, x: f64, y: f64) -> Vec2 {
        let width = f64::from(self.canvas.client_width().max(1));
        let height = f64::from(self.canvas.client_height().max(1));
        Vec2::new(
            (x / width * 2.0 - 1.0) as f32,
            (1.0 - y / height * 2.0) as f32,
        )
    }

    pub fn set_geometry(&mut self, positions: &[[f32; 4]], directions: &[[f32; 4]]) {
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
        let directions_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("directions"),
                contents: bytemuck::cast_slice(directions),
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: directions_buf.as_entire_binding(),
                },
            ],
        }));
        self.positions_buf = Some(positions_buf);
        self.directions_buf = Some(directions_buf);
        self.states_buf = Some(states_buf);

        let key = geometry_key(positions, directions);
        if self.geometry_key != Some(key) {
            self.geometry_key = Some(key);
            self.configure_scene(positions);
        }
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
        self.camera.orbit(dx, dy);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.camera.dolly(delta);
    }

    pub fn reset_camera(&mut self) {
        self.camera.pos = self.initial_camera.pos;
        self.camera.rot = self.initial_camera.rot;
        self.camera.pivot = self.initial_camera.pivot;
        if !self.camera.free {
            self.camera.aim_at_pivot();
        }
    }

    pub fn set_camera_free(&mut self, free: bool) {
        self.camera.free = free;
        if !free {
            self.camera.aim_at_pivot();
        }
    }

    pub fn set_fov(&mut self, deg: f32) {
        self.camera.fov = deg.clamp(5.0, 150.0);
    }

    pub fn set_near(&mut self, value: f32) {
        self.camera.near = value.max(0.01);
        self.camera.far = self.camera.far.max(self.camera.near + 1.0);
    }

    pub fn set_far(&mut self, value: f32) {
        self.camera.far = value.max(self.camera.near + 1.0);
    }

    pub fn set_move_speed(&mut self, value: f32) {
        self.camera.move_speed = value.max(0.0);
    }

    #[must_use]
    pub fn camera_pos(&self) -> [f32; 3] {
        self.camera.pos.to_array()
    }

    #[must_use]
    pub fn camera_rot(&self) -> [f32; 3] {
        self.camera.rot.to_array()
    }

    pub fn set_camera_pos(&mut self, axis: usize, value: f32) {
        if axis < 3 {
            self.camera.pos[axis] = value;
            if !self.camera.free {
                self.camera.aim_at_pivot();
            }
        }
    }

    pub fn set_camera_rot(&mut self, axis: usize, degrees: f32) {
        if axis < 3 {
            self.camera.rot[axis] = degrees;
        }
    }

    pub fn set_max_pressure(&mut self, value: f32) {
        self.uniforms.max_pressure = value.max(1.0);
    }

    pub fn set_sound_speed(&mut self, value: f32) {
        self.uniforms.sound_speed = value.max(1.0);
    }

    pub fn set_colormap(&mut self, index: u32) {
        self.uniforms.colormap = index;
    }

    pub fn set_show_markers(&mut self, show: bool) {
        self.show_markers = show;
    }

    pub fn set_background(&mut self, rgb: [f32; 3]) {
        self.bg = wgpu::Color {
            r: f64::from(rgb[0]),
            g: f64::from(rgb[1]),
            b: f64::from(rgb[2]),
            a: 1.0,
        };
    }

    pub fn set_gizmo_visible(&mut self, visible: bool) {
        self.gizmo.set_visible(visible);
    }

    pub fn set_gizmo_mode(&mut self, mode: GizmoMode) {
        self.gizmo.set_mode(mode);
    }

    pub fn set_slice_coord(&mut self, axis: usize, value: f32) {
        if axis < 3 {
            self.slice_center[axis] = value;
            self.apply_slice();
        }
    }

    pub fn set_slice_rot(&mut self, axis: usize, degrees: f32) {
        if axis < 3 {
            self.slice_rot[axis] = degrees;
            self.apply_slice();
        }
    }

    pub fn set_slice_size(&mut self, axis: usize, value: f32) {
        let value = value.clamp(SLICE_MIN_MM, SLICE_MAX_MM);
        match axis {
            0 => self.slice_size.x = value,
            1 => self.slice_size.y = value,
            _ => return,
        }
        self.apply_slice();
        self.create_field();
    }

    pub fn set_slice_resolution(&mut self, texels_per_mm: f32) {
        self.texels_per_mm = texels_per_mm.clamp(RESOLUTION_MIN, RESOLUTION_MAX);
        self.create_field();
    }

    #[must_use]
    pub fn slice_size(&self) -> [f32; 2] {
        self.slice_size.to_array()
    }

    #[must_use]
    pub fn field_dims(&self) -> [u32; 2] {
        [self.uniforms.slice_w, self.uniforms.slice_h]
    }

    #[must_use]
    pub fn slice_center(&self) -> [f32; 3] {
        self.slice_center.to_array()
    }

    #[must_use]
    pub fn slice_rot(&self) -> [f32; 3] {
        self.slice_rot.to_array()
    }

    #[must_use]
    pub fn axis_bounds(&self) -> [(f32, f32); 3] {
        self.axis_range
    }

    fn rotation(&self) -> Quat {
        Quat::from_euler(
            EulerRot::XYZ,
            self.slice_rot.x.to_radians(),
            self.slice_rot.y.to_radians(),
            self.slice_rot.z.to_radians(),
        )
    }

    fn create_field(&mut self) {
        let wanted = self.slice_size * self.texels_per_mm;
        let cap = f32::from(u16::try_from(FIELD_MAX_DIM).unwrap_or(u16::MAX));
        let scale = (cap / wanted.x.max(1.0))
            .min(cap / wanted.y.max(1.0))
            .min(1.0);
        let to_dim = |v: f32| -> u32 { ((v * scale) as u32).clamp(1, FIELD_MAX_DIM) };
        let (width, height) = (to_dim(wanted.x), to_dim(wanted.y));
        self.uniforms.slice_w = width;
        self.uniforms.slice_h = height;

        if let Some(old) = &self.field_tex {
            old.destroy();
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("field"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FIELD_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.field_compute_bg = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("field-compute-bg"),
            layout: &self.field_compute_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        }));
        self.field_render_bg = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("field-render-bg"),
            layout: &self.field_render_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.field_sampler),
                },
            ],
        }));
        self.field_tex = Some(texture);
    }

    fn apply_slice(&mut self) {
        let rot = self.rotation();
        let u = rot * (Vec3::X * self.slice_size.x);
        let v = rot * (Vec3::Z * self.slice_size.y);
        let origin = self.slice_center - u * 0.5 - v * 0.5;
        self.uniforms.origin = origin.extend(0.0).to_array();
        self.uniforms.u = u.extend(0.0).to_array();
        self.uniforms.v = v.extend(0.0).to_array();
    }

    pub fn pick_gizmo_axis(&self, ndc: Vec2) -> Option<usize> {
        self.bind_group.as_ref()?;
        let (o, rd) = self.camera.ray(ndc, self.aspect);
        self.gizmo.pick(self.slice_center, self.slice_rot, o, rd)
    }

    pub fn set_hover(&mut self, ndc: Vec2) {
        let (o, rd) = self.camera.ray(ndc, self.aspect);
        self.gizmo
            .set_hover(self.slice_center, self.slice_rot, o, rd);
    }

    pub fn begin_gizmo_drag(&mut self, axis: usize, ndc: Vec2) {
        let (o, rd) = self.camera.ray(ndc, self.aspect);
        self.gizmo
            .begin_drag(axis, self.slice_center, self.slice_rot, o, rd);
    }

    pub fn update_gizmo_drag(&mut self, ndc: Vec2, snap: bool) -> Option<DragUpdate> {
        let (o, rd) = self.camera.ray(ndc, self.aspect);
        let update = self.gizmo.update_drag(o, rd, snap)?;
        match &update {
            DragUpdate::Translate(c) => self.slice_center = Vec3::from_array(*c),
            DragUpdate::Rotate(r) => self.slice_rot = Vec3::from_array(*r),
        }
        self.apply_slice();
        Some(update)
    }

    pub fn end_gizmo_drag(&mut self) {
        self.gizmo.end_drag();
    }

    pub fn render(&mut self) {
        let (Some(bind_group), Some(field_compute_bg), Some(field_render_bg)) = (
            &self.bind_group,
            &self.field_compute_bg,
            &self.field_render_bg,
        ) else {
            return;
        };

        self.gizmo.len = self.camera.distance() * 0.2;
        self.uniforms.gizmo_len = self.gizmo.len;
        self.uniforms.view_proj = self.camera.view_proj(self.aspect).to_cols_array_2d();
        self.uniforms.eye = self.camera.eye().extend(0.0).to_array();
        self.uniforms.active_axis = self.gizmo.active_axis_i32();
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
        let (attach_view, resolve_target) = match &self.msaa_view {
            Some(msaa) => (msaa, Some(&view)),
            None => (&view, None),
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("field-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.field_pipeline);
            cpass.set_bind_group(0, bind_group, &[]);
            cpass.set_bind_group(1, field_compute_bg, &[]);
            cpass.dispatch_workgroups(
                self.uniforms.slice_w.div_ceil(FIELD_WORKGROUP),
                self.uniforms.slice_h.div_ceil(FIELD_WORKGROUP),
                1,
            );
        }
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: attach_view,
                    resolve_target,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.bg),
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
            rpass.set_bind_group(1, field_render_bg, &[]);
            rpass.set_pipeline(&self.slice_pipeline);
            rpass.draw(0..6, 0..1);
            if self.show_markers {
                rpass.set_pipeline(&self.marker_pipeline);
                rpass.draw(0..6, 0..self.num_trans);
            }
            if self.gizmo.visible && self.gizmo.len > 0.0 {
                match self.gizmo.mode {
                    GizmoMode::Move => {
                        rpass.set_pipeline(&self.gizmo_pipeline);
                        rpass.draw(0..9, 0..3);
                    }
                    GizmoMode::Rotate => {
                        rpass.set_pipeline(&self.ring_pipeline);
                        rpass.draw(0..RING_SEGMENTS * 6, 0..3);
                    }
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
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

        self.axis_range = [
            (min[0] - SLICE_MARGIN_MM, max[0] + SLICE_MARGIN_MM),
            (min[1] - SLICE_MARGIN_MM, max[1] + SLICE_MARGIN_MM),
            (SLICE_BOTTOM_MM, SLICE_BOTTOM_MM + SLICE_HEIGHT_MM),
        ];
        self.slice_rot = Vec3::ZERO;
        self.slice_size = Vec2::new(
            (self.axis_range[0].1 - self.axis_range[0].0).clamp(SLICE_MIN_MM, SLICE_MAX_MM),
            (self.axis_range[2].1 - self.axis_range[2].0).clamp(SLICE_MIN_MM, SLICE_MAX_MM),
        );
        self.slice_center = Vec3::new(
            f32::midpoint(self.axis_range[0].0, self.axis_range[0].1),
            f32::midpoint(self.axis_range[1].0, self.axis_range[1].1),
            f32::midpoint(self.axis_range[2].0, self.axis_range[2].1),
        );
        self.apply_slice();
        self.create_field();

        if self.uniforms.sound_speed == 0.0 {
            self.uniforms.sound_speed = SOUND_SPEED_MM_S;
        }
        if self.uniforms.max_pressure == 0.0 {
            self.uniforms.max_pressure = MAX_PRESSURE;
        }
        if self.uniforms.marker_size == 0.0 {
            self.uniforms.marker_size = MARKER_SIZE_MM;
        }
        self.uniforms.num_trans = self.num_trans;

        let extent = (max[0] - min[0]).max(max[1] - min[1]).max(SLICE_HEIGHT_MM);
        let pivot = Vec3::new(center[0], center[1], SLICE_HEIGHT_MM * 0.4);
        let dist = extent * 2.0 + 250.0;
        self.camera.pivot = pivot;
        self.camera.pos = pivot + Vec3::new(0.0, -0.955, 0.296).normalize() * dist;
        self.camera.aim_at_pivot();
        self.gizmo.len = dist * 0.2;
        self.initial_camera = self.camera;
    }
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    sample_count: u32,
}

fn geometry_key(positions: &[[f32; 4]], directions: &[[f32; 4]]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytemuck::cast_slice::<_, u8>(positions).hash(&mut hasher);
    bytemuck::cast_slice::<_, u8>(directions).hash(&mut hasher);
    hasher.finish()
}

fn backing_size(canvas: &web_sys::HtmlCanvasElement) -> (u32, u32) {
    let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
    let dpr = if dpr.is_finite() {
        dpr.clamp(1.0, MAX_DPR)
    } else {
        1.0
    };
    let width = f64::from(canvas.client_width().max(1)) * dpr;
    let height = f64::from(canvas.client_height().max(1)) * dpr;
    let scale = (MAX_PIXELS / (width * height)).sqrt().min(1.0);
    let to_dim = |px: f64| -> u32 { ((px * scale).round() as u32).clamp(1, MAX_DIM) };
    (to_dim(width), to_dim(height))
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
            apply_limit_buckets: false,
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
    let sample_count = if adapter
        .get_texture_format_features(format)
        .flags
        .sample_count_supported(MSAA_SAMPLES)
    {
        MSAA_SAMPLES
    } else {
        1
    };
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: caps.alpha_modes[0],
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    Ok(Gpu {
        surface,
        config,
        device,
        queue,
        sample_count,
    })
}

struct Pipelines {
    bind_group_layout: wgpu::BindGroupLayout,
    field_compute_layout: wgpu::BindGroupLayout,
    field_render_layout: wgpu::BindGroupLayout,
    slice_pipeline: wgpu::RenderPipeline,
    marker_pipeline: wgpu::RenderPipeline,
    gizmo_pipeline: wgpu::RenderPipeline,
    ring_pipeline: wgpu::RenderPipeline,
    field_pipeline: wgpu::ComputePipeline,
}

#[derive(Clone, Copy)]
struct Target {
    format: wgpu::TextureFormat,
    sample_count: u32,
}

fn build_pipelines(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> Pipelines {
    let target = Target {
        format,
        sample_count,
    };
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: SCENE_STAGES,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            storage_entry(1),
            storage_entry(2),
            storage_entry(3),
        ],
    });
    let field_compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("field-compute-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: FIELD_FORMAT,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        }],
    });
    let field_render_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("field-render-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("scene-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
    });
    let layouts = [&bind_group_layout, &field_render_layout];
    let field_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("field_cs"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("field-pl"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&field_compute_layout)],
                immediate_size: 0,
            }),
        ),
        module: &shader,
        entry_point: Some("field_cs"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    Pipelines {
        slice_pipeline: build_pipeline(
            device, &layouts, &shader, target, "slice_vs", "slice_fs", true,
        ),
        marker_pipeline: build_pipeline(
            device,
            &layouts,
            &shader,
            target,
            "marker_vs",
            "marker_fs",
            true,
        ),
        gizmo_pipeline: build_pipeline(
            device, &layouts, &shader, target, "gizmo_vs", "gizmo_fs", false,
        ),
        ring_pipeline: build_pipeline(
            device, &layouts, &shader, target, "ring_vs", "gizmo_fs", false,
        ),
        field_pipeline,
        bind_group_layout,
        field_compute_layout,
        field_render_layout,
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    bind_group_layouts: &[&wgpu::BindGroupLayout; 2],
    shader: &wgpu::ShaderModule,
    target: Target,
    vs: &str,
    fs: &str,
    depth_test: bool,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scene-pl"),
        bind_group_layouts: &[Some(bind_group_layouts[0]), Some(bind_group_layouts[1])],
        immediate_size: 0,
    });
    let (depth_write, depth_compare) = if depth_test {
        (true, wgpu::CompareFunction::Less)
    } else {
        (false, wgpu::CompareFunction::Always)
    };
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
                format: target.format,
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
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: target.sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

fn create_depth(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    sample_count: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_msaa(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    sample_count: u32,
) -> Option<(wgpu::Texture, wgpu::TextureView)> {
    if sample_count <= 1 {
        return None;
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("msaa-color"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Some((texture, view))
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: SCENE_STAGES,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
