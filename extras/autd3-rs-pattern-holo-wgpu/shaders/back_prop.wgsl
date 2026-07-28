const WG: u32 = 256u;

struct Dims {
    rows: u32,
    cols: u32,
    g_row_major: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
}

@group(0) @binding(0) var<storage, read> g: array<vec2<f32>>;
@group(0) @binding(1) var<storage, read_write> row_norm: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<vec2<f32>>;
@group(0) @binding(3) var<uniform> d: Dims;

var<workgroup> scratch: array<f32, WG>;

fn at(i: u32, j: u32) -> u32 {
    if (d.g_row_major == 1u) {
        return i * d.cols + j;
    }
    return j * d.rows + i;
}

@compute @workgroup_size(256)
fn row_norm_pass(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let i = wg.x;
    let k = wg.z;
    let g_base = k * d.rows * d.cols;
    var acc = 0.0;
    var j = lid.x;
    while (j < d.cols) {
        let v = g[g_base + at(i, j)];
        acc = acc + v.x * v.x + v.y * v.y;
        j = j + WG;
    }
    scratch[lid.x] = acc;
    workgroupBarrier();

    var s: u32 = WG / 2u;
    loop {
        if (s == 0u) {
            break;
        }
        if (lid.x < s) {
            scratch[lid.x] = scratch[lid.x] + scratch[lid.x + s];
        }
        workgroupBarrier();
        s = s >> 1u;
    }
    if (lid.x == 0u) {
        row_norm[k * d.rows + i] = scratch[0];
    }
}

@compute @workgroup_size(256)
fn write_pass(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= d.rows * d.cols) {
        return;
    }
    let i = idx / d.cols;
    let j = idx % d.cols;
    let k = gid.z;
    let base = k * d.rows * d.cols;
    let v = g[base + at(i, j)];
    let scale = 1.0 / row_norm[k * d.rows + i];
    out[base + idx] = vec2<f32>(v.x * scale, -v.y * scale);
}
