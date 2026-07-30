const WG: u32 = 256u;
const MAX_N: u32 = 256u;

struct Dims {
    n: u32,
    repeat: u32,
    r_stride: u32,
    x_stride: u32,
    a_row_major: u32,
    reserved0: u32,
    reserved1: u32,
    reserved2: u32,
}

@group(0) @binding(0) var<storage, read> a: array<vec2<f32>>;
@group(0) @binding(1) var<storage, read> x: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> out: array<vec2<f32>>;
@group(0) @binding(4) var<uniform> d: Dims;
@group(0) @binding(5) var<storage, read> r: array<vec2<f32>>;

var<workgroup> cur: array<vec2<f32>, MAX_N>;
var<workgroup> nxt: array<vec2<f32>, MAX_N>;

fn cmul(p: vec2<f32>, q: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(p.x * q.x - p.y * q.y, p.x * q.y + p.y * q.x);
}

fn normalized(b: vec2<f32>, s: vec2<f32>) -> vec2<f32> {
    let inv = 1.0 / sqrt(b.x * b.x + b.y * b.y);
    return cmul(vec2<f32>(b.x * inv, b.y * inv), s);
}

@compute @workgroup_size(256)
fn main(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let k = wg.z;
    let n = d.n;
    let a_base = k * n * n;
    let x_base = k * d.x_stride;
    let r_base = k * d.r_stride;
    var a_row_step = 1u;
    var a_col_step = n;
    if (d.a_row_major == 1u) {
        a_row_step = n;
        a_col_step = 1u;
    }

    var i = lid.x;
    while (i < n) {
        cur[i] = x[x_base + i];
        i = i + WG;
    }
    workgroupBarrier();

    for (var it: u32 = 0u; it < d.repeat; it = it + 1u) {
        var row = lid.x;
        while (row < n) {
            let a_row = a_base + row * a_row_step;
            var acc = vec2<f32>(0.0, 0.0);
            for (var j: u32 = 0u; j < n; j = j + 1u) {
                acc = acc + cmul(a[a_row + j * a_col_step], normalized(cur[j], r[r_base + j]));
            }
            nxt[row] = acc;
            row = row + WG;
        }
        workgroupBarrier();
        var t = lid.x;
        while (t < n) {
            cur[t] = nxt[t];
            t = t + WG;
        }
        workgroupBarrier();
    }

    var o = lid.x;
    while (o < n) {
        out[k * n + o] = cur[o];
        o = o + WG;
    }
}
