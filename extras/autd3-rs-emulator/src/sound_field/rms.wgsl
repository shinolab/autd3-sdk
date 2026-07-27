@group(0)
@binding(0)
var<storage, read> v_amp: array<f32>;

@group(0)
@binding(1)
var<storage, read> v_phase: array<f32>;

@group(0)
@binding(2)
var<storage, read> v_tr_pos: array<vec3<f32>>;

@group(0)
@binding(3)
var<storage, read> v_tar_pos: array<vec3<f32>>;

@group(0)
@binding(4)
var<storage, read_write> v_dst: array<f32>;

struct Pc {
    idx: u32,
    wavenumber: f32,
    num_trans: u32,
    stride: u32,
}

var<immediate> pc: Pc;

@compute
@workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    if global_id.x >= arrayLength(&v_dst) {
        return;
    }
    var re: f32 = 0.;
    var im: f32 = 0.;
    for (var i: u32 = 0; i < pc.num_trans; i++) {
        let dist = distance(v_tar_pos[global_id.x], v_tr_pos[i]);
        let phase = pc.wavenumber * dist + v_phase[i * pc.stride + pc.idx];
        let r = v_amp[i * pc.stride + pc.idx] / dist;
        re += r * cos(phase);
        im += r * sin(phase);
    }
    v_dst[global_id.x] = sqrt(re * re + im * im);
}
