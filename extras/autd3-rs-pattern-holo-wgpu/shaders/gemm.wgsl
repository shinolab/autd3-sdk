const WG: u32 = 256u;

struct Dims {
    rows: u32,
    inner: u32,
    cols: u32,
    a_row_major: u32,
    b_row_major: u32,
    _pad2: u32,
    _pad3: u32,
    _pad4: u32,
}

@group(0) @binding(0) var<storage, read> a: array<vec2<f32>>;
@group(0) @binding(1) var<storage, read> b: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read_write> out: array<vec2<f32>>;
@group(0) @binding(3) var<uniform> d: Dims;

var<workgroup> scratch: array<vec2<f32>, WG>;

fn cmul(p: vec2<f32>, q: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(p.x * q.x - p.y * q.y, p.x * q.y + p.y * q.x);
}

fn a_row_base(i: u32) -> u32 {
    if (d.a_row_major == 1u) {
        return i * d.inner;
    }
    return i;
}

fn a_inner_step() -> u32 {
    if (d.a_row_major == 1u) {
        return 1u;
    }
    return d.rows;
}

fn b_col_base(j: u32) -> u32 {
    if (d.b_row_major == 1u) {
        return j;
    }
    return j * d.inner;
}

fn b_inner_step() -> u32 {
    if (d.b_row_major == 1u) {
        return d.cols;
    }
    return 1u;
}

@compute @workgroup_size(256)
fn main(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let i = wg.x;
    let j = wg.y;
    let batch = wg.z;
    let a_base = batch * d.rows * d.inner;
    let b_base = batch * d.inner * d.cols;

    let a_row = a_base + a_row_base(i);
    let a_step = a_inner_step();
    let b_col = b_base + b_col_base(j);
    let b_step = b_inner_step();
    var acc = vec2<f32>(0.0, 0.0);
    var k = lid.x;
    while (k < d.inner) {
        acc = acc + cmul(a[a_row + k * a_step], b[b_col + k * b_step]);
        k = k + WG;
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
        out[batch * d.rows * d.cols + j * d.rows + i] = scratch[0];
    }
}
