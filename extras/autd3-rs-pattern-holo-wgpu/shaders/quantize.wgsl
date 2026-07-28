const WG: u32 = 256u;
const INV_TWO_PI: f32 = 0.15915494309189535;

struct Dims {
    len: u32,
    mode: u32,
    p0: u32,
    p1: u32,
    words: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<storage, read> q: array<vec2<f32>>;
@group(0) @binding(1) var<storage, read_write> max_coefficient: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<u32>;
@group(0) @binding(3) var<uniform> d: Dims;

var<workgroup> scratch: array<f32, WG>;

@compute @workgroup_size(256)
fn max_pass(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let k = wg.z;
    let base = k * d.len;
    var acc = 0.0;
    var i = lid.x;
    while (i < d.len) {
        let v = q[base + i];
        acc = max(acc, v.x * v.x + v.y * v.y);
        i = i + WG;
    }
    scratch[lid.x] = acc;
    workgroupBarrier();

    var s: u32 = WG / 2u;
    loop {
        if (s == 0u) {
            break;
        }
        if (lid.x < s) {
            scratch[lid.x] = max(scratch[lid.x], scratch[lid.x + s]);
        }
        workgroupBarrier();
        s = s >> 1u;
    }
    if (lid.x == 0u) {
        max_coefficient[k] = sqrt(scratch[0]);
    }
}

fn round_half_away(x: f32) -> f32 {
    return sign(x) * floor(abs(x) + 0.5);
}

fn quantize_phase(v: vec2<f32>) -> u32 {
    if (v.x == 0.0 && v.y == 0.0) {
        return 0u;
    }
    let p = round_half_away(atan2(v.y, v.x) * INV_TWO_PI * 256.0);
    return u32(i32(p) & 255);
}

fn quantize_intensity(value: f32, m: f32) -> u32 {
    let normalized = select(0.0, value / m, m > 0.0);
    switch d.mode {
        case 0u: {
            return u32(clamp(round_half_away(normalized * 255.0), 0.0, 255.0));
        }
        case 1u: {
            let scaled = normalized * 255.0 * bitcast<f32>(d.p0);
            return u32(clamp(round_half_away(scaled), 0.0, 255.0));
        }
        case 2u: {
            return d.p0;
        }
        default: {
            let scaled = round_half_away(value * 255.0);
            return u32(clamp(scaled, bitcast<f32>(d.p0), bitcast<f32>(d.p1)));
        }
    }
}

fn pack(i: u32, base: u32, m: f32) -> u32 {
    let v = q[base + i];
    return quantize_phase(v) | (quantize_intensity(length(v), m) << 8u);
}

@compute @workgroup_size(256)
fn quantize_pass(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = gid.x;
    if (w >= d.words) {
        return;
    }
    let k = gid.z;
    let base = k * d.len;
    var m = 0.0;
    if (d.mode < 2u) {
        m = max_coefficient[k];
    }
    let i = w * 2u;
    let lo = pack(i, base, m);
    var hi = 0u;
    if (i + 1u < d.len) {
        hi = pack(i + 1u, base, m);
    }
    out[k * d.words + w] = lo | (hi << 16u);
}
