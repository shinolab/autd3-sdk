@group(0)
@binding(0)
var<storage, read> v_ult: array<f32>;

@group(0)
@binding(1)
var<storage, read> v_tr_pos: array<vec3<f32>>;

@group(0)
@binding(2)
var<storage, read> v_tar_pos: array<vec3<f32>>;

@group(0)
@binding(3)
var<storage, read_write> v_dst: array<f32>;

struct Pc {
    t: f32,
    sound_speed: f32,
    num_trans: u32,
    offset: i32,
    output_ultrasound_stride: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

var<immediate> pc: Pc;

const ULTRASOUND_FREQ: f32 = 40000;
const ULTRASOUND_PERIOD_COUNT: f32 = 512;
const TS: f32 = 1. / (ULTRASOUND_FREQ * ULTRASOUND_PERIOD_COUNT);

const PI: f32 = radians(180.0);
const T4010A1_AMPLITUDE: f32 = 55114.85; // [Pa*mm]
const P0: f32 = T4010A1_AMPLITUDE * 1.41421356237309504880168872420969808 / (4. * PI);

@compute
@workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    if global_id.x >= arrayLength(&v_dst) {
        return;
    }
    var res: f32 = 0.;
    for (var i: u32 = 0; i < pc.num_trans; i++) {
        let dist = distance(v_tar_pos[global_id.x], v_tr_pos[i]);
        let t_out = pc.t - dist / pc.sound_speed;
        let a = t_out / TS;
        let idx = i32(floor(a));
        let alpha = a - f32(idx);
        let idx_ = i * pc.output_ultrasound_stride + u32(idx - pc.offset);
        res += mix(v_ult[idx_], v_ult[idx_ + 1], alpha) / dist;
    }
    v_dst[global_id.x] = P0 * res;
}
