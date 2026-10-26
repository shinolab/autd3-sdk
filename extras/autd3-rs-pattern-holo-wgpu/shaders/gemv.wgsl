const WG: u32 = 256u;
const COLS_PER_CHUNK: u32 = 1024u;
const ROWWISE_COL_LIMIT: u32 = 256u;

struct Dims {
    rows: u32,
    cols: u32,
    chunks: u32,
    normalize: u32,
    a_row_major: u32,
    x_chunks: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> a: array<vec2<f32>>;
@group(0) @binding(1) var<storage, read> x: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read_write> partial: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> out: array<vec2<f32>>;
@group(0) @binding(4) var<uniform> d: Dims;
@group(0) @binding(5) var<storage, read> r: array<vec2<f32>>;

var<workgroup> scratch: array<vec2<f32>, WG>;

fn cmul(p: vec2<f32>, q: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(p.x * q.x - p.y * q.y, p.x * q.y + p.y * q.x);
}

fn a_row_base(i: u32) -> u32 {
    if (d.a_row_major == 1u) {
        return i * d.cols;
    }
    return i;
}

fn a_col_step() -> u32 {
    if (d.a_row_major == 1u) {
        return 1u;
    }
    return d.rows;
}

fn apply_x(b: vec2<f32>, ri: u32) -> vec2<f32> {
    if (d.normalize == 0u) {
        return b;
    }
    let inv = 1.0 / sqrt(b.x * b.x + b.y * b.y);
    return cmul(vec2<f32>(b.x * inv, b.y * inv), r[ri]);
}

fn load_x(j: u32) -> vec2<f32> {
    return apply_x(x[j], j);
}

fn reduce(lid: u32) {
    var s: u32 = WG / 2u;
    loop {
        if (s == 0u) {
            break;
        }
        if (lid < s) {
            scratch[lid] = scratch[lid] + scratch[lid + s];
        }
        workgroupBarrier();
        s = s >> 1u;
    }
}

@compute @workgroup_size(256)
fn rowwise(@builtin(global_invocation_id) gid: vec3<u32>) {
    let row = gid.x;
    if (row >= d.rows) {
        return;
    }
    let a_row = a_row_base(row);
    let a_step = a_col_step();
    var acc = vec2<f32>(0.0, 0.0);
    for (var j: u32 = 0u; j < d.cols; j = j + 1u) {
        acc = acc + cmul(a[a_row + j * a_step], load_x(j));
    }
    out[row] = acc;
}

var<workgroup> xs: array<vec2<f32>, ROWWISE_COL_LIMIT>;

@compute @workgroup_size(256)
fn rowwise_pending(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    var j = lid.x;
    while (j < d.cols) {
        let base = j * d.x_chunks;
        var sum = vec2<f32>(0.0, 0.0);
        for (var c: u32 = 0u; c < d.x_chunks; c = c + 1u) {
            sum = sum + partial[base + c];
        }
        xs[j] = sum;
        j = j + WG;
    }
    workgroupBarrier();

    let row = gid.x;
    if (row >= d.rows) {
        return;
    }
    let a_row = a_row_base(row);
    let a_step = a_col_step();
    var acc = vec2<f32>(0.0, 0.0);
    for (var i: u32 = 0u; i < d.cols; i = i + 1u) {
        acc = acc + cmul(a[a_row + i * a_step], apply_x(xs[i], i));
    }
    out[row] = acc;
}

@compute @workgroup_size(256)
fn partial_pass(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let chunk = wg.x;
    let row = wg.y;
    let base = chunk * COLS_PER_CHUNK;
    let end = min(base + COLS_PER_CHUNK, d.cols);

    let a_row = a_row_base(row);
    let a_step = a_col_step();
    var acc = vec2<f32>(0.0, 0.0);
    var j = base + lid.x;
    while (j < end) {
        acc = acc + cmul(a[a_row + j * a_step], load_x(j));
        j = j + WG;
    }
    scratch[lid.x] = acc;
    workgroupBarrier();
    reduce(lid.x);
    if (lid.x == 0u) {
        partial[row * d.chunks + chunk] = scratch[0];
    }
}

@compute @workgroup_size(256)
fn reduce_pass(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let row = wg.x;
    var acc = vec2<f32>(0.0, 0.0);
    var c = lid.x;
    while (c < d.chunks) {
        acc = acc + partial[row * d.chunks + c];
        c = c + WG;
    }
    scratch[lid.x] = acc;
    workgroupBarrier();
    reduce(lid.x);
    if (lid.x == 0u) {
        out[row] = scratch[0];
    }
}
