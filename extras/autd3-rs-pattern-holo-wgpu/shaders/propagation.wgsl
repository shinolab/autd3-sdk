const TWO_PI: f32 = 6.283185307179586;
const P0: f32 = 4385.9004;

struct Dims {
    rows: u32,
    cols: u32,
    wavenumber: f32,
    directivity: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

@group(0) @binding(0) var<storage, read> tr_pos: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> tr_dir: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> foci: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> out: array<vec2<f32>>;
@group(0) @binding(4) var<uniform> d: Dims;

const COEF_A = array<f32, 9>(
    1.0, 1.0, 1.0, 0.891250938, 0.707945784, 0.501187234, 0.354813389, 0.251188643, 0.199526231,
);
const COEF_B = array<f32, 9>(
    0.0, 0.0, -0.00459648054721, -0.0155520765675, -0.0208114779827, -0.0182211227016,
    -0.0122437497109, -0.00780345575475, -0.00312857467007,
);
const COEF_C = array<f32, 9>(
    0.0, 0.0, -0.000787968093807, -0.000307591508224, -0.000218348633296, 0.00047738416141,
    0.000120353137658, 0.000323676257958, 0.000143850511,
);
const COEF_D = array<f32, 9>(
    0.0, 0.0, 1.60125528528e-05, 2.9747624976e-06, 2.31910931569e-05, -1.1901034125e-05,
    6.77743734332e-06, -5.99548024824e-06, -4.79372835035e-06,
);

fn directivity_t4010a1(theta_rad: f32) -> f32 {
    let folded = abs(degrees(theta_rad)) % 180.0;
    let theta_deg = 90.0 - abs(folded - 90.0);
    let i = u32(ceil(theta_deg / 10.0));
    if (i == 0u) {
        return 1.0;
    }
    let idx = i - 1u;
    let a = COEF_A;
    let b = COEF_B;
    let c = COEF_C;
    let e = COEF_D;
    let x = theta_deg - f32(idx) * 10.0;
    return ((e[idx] * x + c[idx]) * x + b[idx]) * x + a[idx];
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= d.rows * d.cols) {
        return;
    }
    let i = idx / d.cols;
    let j = idx % d.cols;
    let k = gid.z;

    let pos = tr_pos[j].xyz;
    let dir = tr_dir[j].xyz;
    let focus = foci[k * d.rows + i].xyz;

    let diff = focus - pos;
    let dist = length(diff);

    var gain = 1.0;
    if (d.directivity == 1u) {
        gain = directivity_t4010a1(atan2(length(cross(dir, diff)), dot(dir, diff)));
    }
    let r = P0 / dist * gain;

    let phase = d.wavenumber * dist;
    let reduced = phase - TWO_PI * floor(phase / TWO_PI);

    out[k * d.rows * d.cols + idx] = vec2<f32>(r * cos(reduced), r * sin(reduced));
}
