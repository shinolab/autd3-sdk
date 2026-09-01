use std::ffi::c_char;
use std::num::NonZeroU16;
use std::time::Duration;

use autd3_ffi_abi::{
    alloc_cstring, cstr_to_string, drop_handle, free_cstring, handle_ref, into_handle, slice_mut,
    slice_ref, write_cstr, write_out,
};
use autd3_rs_core::units::Hz;
use autd3_rs_core::value::{Nearest, Phase, SamplingConfig};
use autd3_rs_core::{Autd3, Geometry, Point3, Quaternion, UnitQuaternion};

#[repr(C)]
pub struct Autd3Device {
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_new(
    devices: *const Autd3Device,
    len: usize,
) -> *mut Geometry {
    let Some(slice) = (unsafe { slice_ref(devices, len) }) else {
        return std::ptr::null_mut();
    };

    let devices: Vec<Autd3> = slice.iter().map(to_autd3).collect();
    into_handle(Geometry::new(devices))
}

fn to_autd3(device: &Autd3Device) -> Autd3 {
    Autd3::new(
        Point3::new(device.origin[0], device.origin[1], device.origin[2]),
        UnitQuaternion::from_quaternion(Quaternion::new(
            device.rotation[0],
            device.rotation[1],
            device.rotation[2],
            device.rotation[3],
        )),
    )
}

unsafe fn finish_geometry<E: std::fmt::Display>(
    geometry: Result<Geometry, E>,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut Geometry {
    match geometry {
        Ok(geometry) => into_handle(geometry),
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_from_json(
    json: *const c_char,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut Geometry {
    let Some(json) = (unsafe { cstr_to_string(json) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null layout json") };
        return std::ptr::null_mut();
    };

    unsafe { finish_geometry(Geometry::from_json(&json), out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_to_json(
    geometry: *const Geometry,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut c_char {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null geometry") };
        return std::ptr::null_mut();
    };

    match geometry.to_json() {
        Ok(json) => alloc_cstring(&json),
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_free_string(ptr: *mut c_char) {
    unsafe { free_cstring(ptr) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_num_devices(geometry: *const Geometry) -> usize {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return 0;
    };

    geometry.num_devices()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_center(geometry: *const Geometry, out: *mut f32) {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return;
    };
    let Some(out) = (unsafe { slice_mut(out, 3) }) else {
        return;
    };

    let center = geometry.center();
    out.copy_from_slice(&[center.x, center.y, center.z]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_num_transducers(geometry: *const Geometry) -> usize {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return 0;
    };

    geometry.num_transducers()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_num_transducers(
    geometry: *const Geometry,
    dev: usize,
) -> usize {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return 0;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return 0;
    };
    device.num_transducers()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_idx(geometry: *const Geometry, dev: usize) -> usize {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return 0;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return 0;
    };
    device.idx()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_rotation(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return;
    };
    let Some(out) = (unsafe { slice_mut(out, 4) }) else {
        return;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return;
    };
    let rotation = device.rotation();
    out.copy_from_slice(&[rotation.w, rotation.i, rotation.j, rotation.k]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_center(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return;
    };
    let Some(out) = (unsafe { slice_mut(out, 3) }) else {
        return;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return;
    };
    let center = device.center();
    out.copy_from_slice(&[center.x, center.y, center.z]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_transducer_position(
    geometry: *const Geometry,
    dev: usize,
    tr: usize,
    out: *mut f32,
) -> i32 {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return -1;
    };
    let Some(out) = (unsafe { slice_mut(out, 3) }) else {
        return -1;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return -1;
    };
    if tr >= device.num_transducers() {
        return -1;
    }
    let p = device.position(tr);
    out.copy_from_slice(&[p.x, p.y, p.z]);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_transducer_direction(
    geometry: *const Geometry,
    dev: usize,
    tr: usize,
    out: *mut f32,
) -> i32 {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return -1;
    };
    let Some(out) = (unsafe { slice_mut(out, 3) }) else {
        return -1;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return -1;
    };
    if tr >= device.num_transducers() {
        return -1;
    }
    let d = device.direction(tr).into_inner();
    out.copy_from_slice(&[d.x, d.y, d.z]);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_x(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return;
    };
    let Some(out) = (unsafe { slice_mut(out, 3) }) else {
        return;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return;
    };
    let direction = device.x_direction();
    out.copy_from_slice(&[direction.x, direction.y, direction.z]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_y(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return;
    };
    let Some(out) = (unsafe { slice_mut(out, 3) }) else {
        return;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return;
    };
    let direction = device.y_direction();
    out.copy_from_slice(&[direction.x, direction.y, direction.z]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_axial(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return;
    };
    let Some(out) = (unsafe { slice_mut(out, 3) }) else {
        return;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return;
    };
    let direction = device.axial_direction();
    out.copy_from_slice(&[direction.x, direction.y, direction.z]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_free(geometry: *mut Geometry) {
    unsafe { drop_handle(geometry) }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_phase_radian(value: u8) -> f32 {
    Phase(value).rad()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_sampling_config_freq_4k() -> *mut SamplingConfig {
    into_handle(SamplingConfig::FREQ_4K)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_sampling_config_freq_40k() -> *mut SamplingConfig {
    into_handle(SamplingConfig::FREQ_40K)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_sampling_config_divide(divide: u16) -> *mut SamplingConfig {
    match NonZeroU16::new(divide) {
        Some(divide) => into_handle(SamplingConfig::new(divide)),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_sampling_config_freq(hz: f32) -> *mut SamplingConfig {
    into_handle(SamplingConfig::new(hz * Hz))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_sampling_config_freq_nearest(hz: f32) -> *mut SamplingConfig {
    into_handle(SamplingConfig::new(Nearest(hz * Hz)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_sampling_config_period(nanos: u64) -> *mut SamplingConfig {
    into_handle(SamplingConfig::new(Duration::from_nanos(nanos)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_sampling_config_period_nearest(nanos: u64) -> *mut SamplingConfig {
    into_handle(SamplingConfig::new(Nearest(Duration::from_nanos(nanos))))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_sampling_config_freq_value(
    config: *const SamplingConfig,
    out: *mut f32,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return -1;
    };

    let Ok(freq) = config.freq() else {
        return -1;
    };

    unsafe { write_out(out, freq.hz()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_sampling_config_period_value(
    config: *const SamplingConfig,
    out: *mut u64,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return -1;
    };

    let Ok(period) = config.period() else {
        return -1;
    };

    let Ok(nanos) = u64::try_from(period.as_nanos()) else {
        return -1;
    };

    unsafe { write_out(out, nanos) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_sampling_config_divide_value(
    config: *const SamplingConfig,
    out: *mut u16,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return -1;
    };

    let Ok(value) = config.divide() else {
        return -1;
    };

    unsafe { write_out(out, value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_sampling_config_free(config: *mut SamplingConfig) {
    unsafe { drop_handle(config) }
}

autd3_ffi_abi::export_abi_version!();
