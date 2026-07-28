struct Dims {
    len: u32,
    r_stride: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
}

@group(0) @binding(0) var<storage, read_write> x: array<vec2<f32>>;
@group(0) @binding(1) var<storage, read> r: array<vec2<f32>>;
@group(0) @binding(2) var<uniform> d: Dims;

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

@compute @workgroup_size(256)
fn hadamard_normalize(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= d.len) {
        return;
    }
    let k = gid.z;
    let xi = k * d.len + i;
    let b = x[xi];
    let inv = 1.0 / sqrt(b.x * b.x + b.y * b.y);
    x[xi] = cmul(vec2<f32>(b.x * inv, b.y * inv), r[k * d.r_stride + i]);
}

@compute @workgroup_size(256)
fn amplitude_correct(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= d.len) {
        return;
    }
    let k = gid.z;
    let xi = k * d.len + i;
    let b = x[xi];
    let inv = 1.0 / (b.x * b.x + b.y * b.y);
    let a = r[k * d.r_stride + i];
    let aa = vec2<f32>(a.x * a.x - a.y * a.y, 2.0 * a.x * a.y);
    x[xi] = cmul(vec2<f32>(b.x * inv, b.y * inv), aa);
}
