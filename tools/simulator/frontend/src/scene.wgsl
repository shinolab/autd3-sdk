struct Uniforms {
    view_proj: mat4x4<f32>,
    origin: vec4<f32>,
    u: vec4<f32>,
    v: vec4<f32>,
    sound_speed: f32,
    max_pressure: f32,
    num_trans: u32,
    marker_size: f32,
};

@group(0) @binding(0) var<uniform> uni: Uniforms;
@group(0) @binding(1) var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> states: array<vec4<f32>>;

const PI: f32 = 3.14159265358979;
const ULTRASOUND_FREQ: f32 = 40000.0;
const T4010A1_AMPLITUDE: f32 = 55114.85;
const P0: f32 = T4010A1_AMPLITUDE / (4.0 * PI);

fn colormap(x: f32) -> vec3<f32> {
    var stops = array<vec3<f32>, 6>(
        vec3<f32>(0.001, 0.000, 0.014),
        vec3<f32>(0.282, 0.068, 0.377),
        vec3<f32>(0.610, 0.162, 0.506),
        vec3<f32>(0.898, 0.318, 0.388),
        vec3<f32>(0.988, 0.645, 0.212),
        vec3<f32>(0.988, 0.998, 0.645),
    );
    let t = clamp(x, 0.0, 1.0) * 5.0;
    let i = min(u32(floor(t)), 4u);
    return mix(stops[i], stops[i + 1u], fract(t));
}

struct SliceOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn slice_vs(@builtin(vertex_index) vid: u32) -> SliceOut {
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = uvs[vid];
    let world = uni.origin.xyz + uni.u.xyz * uv.x + uni.v.xyz * uv.y;
    var out: SliceOut;
    out.clip = uni.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    return out;
}

@fragment
fn slice_fs(vtx: SliceOut) -> @location(0) vec4<f32> {
    let wavenum = 2.0 * PI * ULTRASOUND_FREQ / uni.sound_speed;
    var re = 0.0;
    var im = 0.0;
    for (var i = 0u; i < uni.num_trans; i = i + 1u) {
        let d = distance(positions[i].xyz, vtx.world);
        let amp = states[i].x;
        let phase = states[i].y;
        let en = states[i].z;
        let a = en * P0 * amp / d;
        let p = -phase - wavenum * d;
        re = re + a * cos(p);
        im = im + a * sin(p);
    }
    let t = sqrt(re * re + im * im) / uni.max_pressure;
    return vec4<f32>(colormap(t), 1.0);
}

struct MarkerOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) amp: f32,
};

@vertex
fn marker_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> MarkerOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
    );
    let q = corners[vid];
    let center = positions[iid].xyz;
    let world = center + vec3<f32>(q.x * uni.marker_size, q.y * uni.marker_size, 0.0);
    var out: MarkerOut;
    out.clip = uni.view_proj * vec4<f32>(world, 1.0);
    out.local = q;
    out.amp = states[iid].x;
    return out;
}

@fragment
fn marker_fs(vtx: MarkerOut) -> @location(0) vec4<f32> {
    if dot(vtx.local, vtx.local) > 1.0 {
        discard;
    }
    return vec4<f32>(colormap(vtx.amp * 0.9 + 0.05), 1.0);
}
