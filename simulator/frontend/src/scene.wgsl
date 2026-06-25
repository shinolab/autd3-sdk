struct Uniforms {
    view_proj: mat4x4<f32>,
    origin: vec4<f32>,
    u: vec4<f32>,
    v: vec4<f32>,
    eye: vec4<f32>,
    sound_speed: f32,
    max_pressure: f32,
    num_trans: u32,
    marker_size: f32,
    colormap: u32,
    gizmo_len: f32,
    active_axis: i32,
};

@group(0) @binding(0) var<uniform> uni: Uniforms;
@group(0) @binding(1) var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> states: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> directions: array<vec4<f32>>;

const PI: f32 = 3.14159265358979;
const ULTRASOUND_FREQ: f32 = 40000.0;
const T4010A1_AMPLITUDE: f32 = 55114.85;
const P0: f32 = T4010A1_AMPLITUDE / (4.0 * PI);

fn inferno(x: f32) -> vec3<f32> {
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

fn viridis(x: f32) -> vec3<f32> {
    var stops = array<vec3<f32>, 6>(
        vec3<f32>(0.267, 0.005, 0.329),
        vec3<f32>(0.283, 0.141, 0.458),
        vec3<f32>(0.254, 0.265, 0.530),
        vec3<f32>(0.164, 0.471, 0.558),
        vec3<f32>(0.478, 0.821, 0.318),
        vec3<f32>(0.993, 0.906, 0.144),
    );
    let t = clamp(x, 0.0, 1.0) * 5.0;
    let i = min(u32(floor(t)), 4u);
    return mix(stops[i], stops[i + 1u], fract(t));
}

fn colormap(x: f32) -> vec3<f32> {
    if uni.colormap == 1u {
        return viridis(x);
    }
    return inferno(x);
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

    let n = normalize(directions[iid].xyz);
    var up = vec3<f32>(0.0, 0.0, 1.0);
    if abs(n.z) > 0.99 {
        up = vec3<f32>(0.0, 1.0, 0.0);
    }
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    let world = center + tangent * (q.x * uni.marker_size) + bitangent * (q.y * uni.marker_size);

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

struct GizmoOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

fn axis_dir(axis: u32) -> vec3<f32> {
    if axis == 0u {
        return vec3<f32>(1.0, 0.0, 0.0);
    } else if axis == 1u {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return vec3<f32>(0.0, 0.0, 1.0);
}

fn axis_color(axis: u32) -> vec3<f32> {
    if i32(axis) == uni.active_axis {
        return vec3<f32>(1.0, 0.95, 0.2);
    }
    if axis == 0u {
        return vec3<f32>(0.95, 0.25, 0.25);
    } else if axis == 1u {
        return vec3<f32>(0.30, 0.90, 0.30);
    }
    return vec3<f32>(0.35, 0.5, 1.0);
}

@vertex
fn gizmo_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> GizmoOut {
    let center = uni.origin.xyz + 0.5 * uni.u.xyz + 0.5 * uni.v.xyz;
    let dir = axis_dir(iid);
    let len = uni.gizmo_len;
    let thick = len * 0.018;

    let view_dir = normalize(center - uni.eye.xyz);
    var perp = cross(dir, view_dir);
    if length(perp) < 1e-4 {
        perp = cross(dir, vec3<f32>(0.0, 0.0, 1.0));
    }
    perp = normalize(perp);

    let tip = center + dir * len;
    let head_base = center + dir * (len * 0.78);

    var world: vec3<f32>;
    if vid < 6u {
        var quad = array<vec3<f32>, 6>(
            center - perp * thick,
            head_base - perp * thick,
            center + perp * thick,
            center + perp * thick,
            head_base - perp * thick,
            head_base + perp * thick,
        );
        world = quad[vid];
    } else {
        let h = vid - 6u;
        var tri = array<vec3<f32>, 3>(
            head_base - perp * thick * 3.2,
            head_base + perp * thick * 3.2,
            tip,
        );
        world = tri[h];
    }

    var out: GizmoOut;
    out.clip = uni.view_proj * vec4<f32>(world, 1.0);
    out.color = axis_color(iid);
    return out;
}

@fragment
fn gizmo_fs(vtx: GizmoOut) -> @location(0) vec4<f32> {
    return vec4<f32>(vtx.color, 1.0);
}


const RING_SEGMENTS: u32 = 64u;
const TAU: f32 = 6.28318530718;

@vertex
fn ring_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> GizmoOut {
    let center = uni.origin.xyz + 0.5 * uni.u.xyz + 0.5 * uni.v.xyz;
    let radius = uni.gizmo_len;
    let thick = radius * 0.012;

    var e0: vec3<f32>;
    var e1: vec3<f32>;
    if iid == 0u {
        e0 = vec3<f32>(0.0, 1.0, 0.0);
        e1 = vec3<f32>(0.0, 0.0, 1.0);
    } else if iid == 1u {
        e0 = vec3<f32>(0.0, 0.0, 1.0);
        e1 = vec3<f32>(1.0, 0.0, 0.0);
    } else {
        e0 = vec3<f32>(1.0, 0.0, 0.0);
        e1 = vec3<f32>(0.0, 1.0, 0.0);
    }

    let seg = vid / 6u;
    let corner = vid % 6u;
    let a0 = f32(seg) / f32(RING_SEGMENTS) * TAU;
    let a1 = f32(seg + 1u) / f32(RING_SEGMENTS) * TAU;
    let p0 = center + radius * (cos(a0) * e0 + sin(a0) * e1);
    let p1 = center + radius * (cos(a1) * e0 + sin(a1) * e1);

    let seg_dir = normalize(p1 - p0);
    let view_dir = normalize((p0 + p1) * 0.5 - uni.eye.xyz);
    var perp = cross(seg_dir, view_dir);
    if length(perp) < 1e-4 {
        perp = cross(seg_dir, vec3<f32>(0.0, 0.0, 1.0));
    }
    perp = normalize(perp);

    var quad = array<vec3<f32>, 6>(
        p0 - perp * thick,
        p1 - perp * thick,
        p0 + perp * thick,
        p0 + perp * thick,
        p1 - perp * thick,
        p1 + perp * thick,
    );

    var out: GizmoOut;
    out.clip = uni.view_proj * vec4<f32>(quad[corner], 1.0);
    out.color = axis_color(iid);
    return out;
}
