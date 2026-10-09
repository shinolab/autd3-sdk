use std::ffi::c_char;
use std::num::NonZeroU16;
use std::time::Duration;

use autd3_ffi_abi::{
    AUTD3_ERR, AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, alloc_cstring, cstr_to_string, drop_handle,
    free_cstring, handle_ref, into_handle, into_handle_or_err, slice_mut, slice_ref, to_ns,
    write_cstr, write_out,
};
use autd3_rs_core::common::ULTRASOUND_PERIOD;
use autd3_rs_core::geometry::Device;
use autd3_rs_core::units::{Hz, rad};
use autd3_rs_core::value::{Nearest, PULSE_WIDTH_PERIOD, Phase, SamplingConfig};
use autd3_rs_core::{Autd3, Geometry, MAX_INFLIGHT, Point3, Quaternion, UnitQuaternion, params};

#[repr(C)]
pub struct Autd3Device {
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_new(
    devices: *const Autd3Device,
    len: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut Geometry {
    let Some(slice) = (unsafe { slice_ref(devices, len) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null devices") };
        return std::ptr::null_mut();
    };

    let devices: Result<Vec<Autd3>, String> = slice.iter().map(to_autd3).collect();
    unsafe { into_handle_or_err(devices.map(Geometry::new), out_err, out_err_len) }
}

fn to_autd3(device: &Autd3Device) -> Result<Autd3, String> {
    let rotation = Quaternion::new(
        device.rotation[0],
        device.rotation[1],
        device.rotation[2],
        device.rotation[3],
    );
    let norm = rotation.norm();
    if norm.is_nan() || (norm - 1.0).abs() > Autd3::ROTATION_NORM_TOLERANCE {
        return Err(format!(
            "`rotation` must be a unit quaternion [w, x, y, z], but its norm is {norm}"
        ));
    }
    Ok(Autd3::new(
        Point3::new(device.origin[0], device.origin[1], device.origin[2]),
        UnitQuaternion::from_quaternion(rotation),
    ))
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

    unsafe { into_handle_or_err(Geometry::from_json(&json), out_err, out_err_len) }
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

unsafe fn device<'a>(geometry: *const Geometry, dev: usize) -> Option<&'a Device> {
    unsafe { handle_ref(geometry) }?.iter().nth(dev)
}

unsafe fn write_floats<const N: usize>(out: *mut f32, values: [f32; N]) -> Option<()> {
    unsafe { slice_mut(out, N) }?.copy_from_slice(&values);
    Some(())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_center(geometry: *const Geometry, out: *mut f32) {
    if let Some(geometry) = unsafe { handle_ref(geometry) } {
        let center = geometry.center();
        unsafe { write_floats(out, [center.x, center.y, center.z]) };
    }
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
    unsafe { device(geometry, dev) }.map_or(0, Device::num_transducers)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_idx(geometry: *const Geometry, dev: usize) -> usize {
    unsafe { device(geometry, dev) }.map_or(0, Device::idx)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_rotation(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    if let Some(device) = unsafe { device(geometry, dev) } {
        let rotation = device.rotation();
        unsafe { write_floats(out, [rotation.w, rotation.i, rotation.j, rotation.k]) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_center(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    if let Some(device) = unsafe { device(geometry, dev) } {
        let v = device.center();
        unsafe { write_floats(out, [v.x, v.y, v.z]) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_transducer_position(
    geometry: *const Geometry,
    dev: usize,
    tr: usize,
    out: *mut f32,
) -> i32 {
    let Some(device) = (unsafe { device(geometry, dev) }) else {
        return -1;
    };
    if tr >= device.num_transducers() {
        return -1;
    }
    let v = device.position(tr);
    match unsafe { write_floats(out, [v.x, v.y, v.z]) } {
        Some(()) => 0,
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_positions(
    geometry: *const Geometry,
    dev: usize,
    dst: *mut f32,
    len: usize,
) -> i32 {
    let Some(device) = (unsafe { device(geometry, dev) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    if len != device.num_transducers() * 3 {
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    let Some(dst) = (unsafe { slice_mut(dst, len) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    for (chunk, p) in dst
        .as_chunks_mut::<3>()
        .0
        .iter_mut()
        .zip(device.positions())
    {
        *chunk = [p.x, p.y, p.z];
    }
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_to_local(
    geometry: *const Geometry,
    dev: usize,
    point: *const f32,
    out: *mut f32,
) -> i32 {
    let (Some(device), Some(point)) = (unsafe { device(geometry, dev) }, unsafe {
        slice_ref(point, 3)
    }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let v = device.to_local(Point3::new(point[0], point[1], point[2]));
    match unsafe { write_floats(out, [v.x, v.y, v.z]) } {
        Some(()) => AUTD3_OK,
        None => AUTD3_ERR_INVALID_ARGUMENT,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_transducer_direction(
    geometry: *const Geometry,
    dev: usize,
    tr: usize,
    out: *mut f32,
) -> i32 {
    let Some(device) = (unsafe { device(geometry, dev) }) else {
        return -1;
    };
    if tr >= device.num_transducers() {
        return -1;
    }
    let v = device.direction(tr).into_inner();
    match unsafe { write_floats(out, [v.x, v.y, v.z]) } {
        Some(()) => 0,
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_x(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    if let Some(device) = unsafe { device(geometry, dev) } {
        let v = device.x_direction();
        unsafe { write_floats(out, [v.x, v.y, v.z]) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_y(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    if let Some(device) = unsafe { device(geometry, dev) } {
        let v = device.y_direction();
        unsafe { write_floats(out, [v.x, v.y, v.z]) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_axial(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    if let Some(device) = unsafe { device(geometry, dev) } {
        let v = device.axial_direction();
        unsafe { write_floats(out, [v.x, v.y, v.z]) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_clone(geometry: *const Geometry) -> *mut Geometry {
    match unsafe { handle_ref(geometry) } {
        Some(geometry) => into_handle(geometry.clone()),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_free(geometry: *mut Geometry) {
    unsafe { drop_handle(geometry) }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_phase_from_rad(radian: f32) -> u8 {
    Phase::from(radian * rad).0
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_ultrasound_period_ns() -> u64 {
    to_ns(ULTRASOUND_PERIOD)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_ultrasound_freq_hz() -> u32 {
    params::ULTRASOUND_FREQ_HZ
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_mod_buffer_samples() -> usize {
    params::MOD_BUFFER_SAMPLES
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_buffer_size_min() -> usize {
    params::BUFFER_SIZE_MIN
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_emission_max_indices() -> usize {
    params::EMISSION_MAX_INDICES
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_num_foci_max() -> u8 {
    params::NUM_FOCI_MAX
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_pulse_width_period() -> u16 {
    PULSE_WIDTH_PERIOD
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_max_inflight() -> usize {
    MAX_INFLIGHT
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_num_transducers() -> usize {
    Autd3::NUM_TRANSDUCERS
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_grid_x() -> u32 {
    Autd3::GRID_X
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_grid_y() -> u32 {
    Autd3::GRID_Y
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_pitch_mm() -> f32 {
    Autd3::PITCH_MM
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_device_width_mm() -> f32 {
    Autd3::DEVICE_WIDTH
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_params_device_height_mm() -> f32 {
    Autd3::DEVICE_HEIGHT
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
pub unsafe extern "C" fn autd3_core_sampling_config_resolve(
    config: *const SamplingConfig,
    out: *mut u16,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null sampling config") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    match config.divide() {
        Ok(divide) => unsafe { write_out(out, divide.get()) },
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            AUTD3_ERR
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_sampling_config_free(config: *mut SamplingConfig) {
    unsafe { drop_handle(config) }
}

autd3_ffi_abi::export_abi_version!();

#[cfg(test)]
mod tests {
    use super::*;

    fn message(err: &[c_char]) -> String {
        unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    fn device(rotation: [f32; 4]) -> Autd3Device {
        Autd3Device {
            origin: [1.0, 2.0, 3.0],
            rotation,
        }
    }

    #[test]
    fn a_rotation_outside_the_layout_tolerance_is_rejected() {
        for rotation in [
            [0.0, 0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0, 0.0],
            [1.0 + 2.0 * Autd3::ROTATION_NORM_TOLERANCE, 0.0, 0.0, 0.0],
            [f32::NAN, 0.0, 0.0, 0.0],
        ] {
            let mut err = [0 as c_char; 256];
            let devices = [device(rotation)];
            let handle = unsafe {
                autd3_core_geometry_new(devices.as_ptr(), 1, err.as_mut_ptr(), err.len())
            };
            assert!(handle.is_null(), "{rotation:?}");
            assert!(message(&err).contains("unit quaternion"), "{rotation:?}");
        }
    }

    #[test]
    fn a_rotation_within_the_layout_tolerance_is_accepted() {
        let mut err = [0 as c_char; 256];
        let devices = [device([
            1.0 + Autd3::ROTATION_NORM_TOLERANCE / 2.0,
            0.0,
            0.0,
            0.0,
        ])];
        let handle =
            unsafe { autd3_core_geometry_new(devices.as_ptr(), 1, err.as_mut_ptr(), err.len()) };
        assert!(!handle.is_null());
        unsafe { autd3_core_geometry_free(handle) };
    }

    #[test]
    fn positions_are_copied_in_transducer_order() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let mut dst = vec![0f32; Autd3::NUM_TRANSDUCERS * 3];
        assert_eq!(AUTD3_OK, unsafe {
            autd3_core_device_positions(&raw const geometry, 0, dst.as_mut_ptr(), dst.len())
        });
        for (chunk, p) in dst.as_chunks::<3>().0.iter().zip(geometry[0].positions()) {
            assert_eq!(*chunk, [p.x, p.y, p.z]);
        }
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_core_device_positions(&raw const geometry, 0, dst.as_mut_ptr(), dst.len() - 1)
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_core_device_positions(&raw const geometry, 1, dst.as_mut_ptr(), dst.len())
        });
    }

    #[test]
    fn to_local_follows_the_device() {
        let geometry = Geometry::new(vec![Autd3::new(
            Point3::new(10.0, 20.0, 30.0),
            UnitQuaternion::identity(),
        )]);
        let point = [11.0f32, 22.0, 33.0];
        let mut out = [0f32; 3];
        assert_eq!(AUTD3_OK, unsafe {
            autd3_core_device_to_local(&raw const geometry, 0, point.as_ptr(), out.as_mut_ptr())
        });
        let expected = geometry[0].to_local(Point3::new(11.0, 22.0, 33.0));
        assert_eq!(out, [expected.x, expected.y, expected.z]);
    }

    #[test]
    fn phase_from_rad_uses_the_rust_rounding() {
        for value in [
            -10.0f32,
            -0.01,
            0.0,
            0.012_271_85,
            1.0,
            std::f32::consts::PI,
            6.2,
            100.0,
        ] {
            assert_eq!(Phase::from(value * rad).0, autd3_core_phase_from_rad(value));
        }
    }

    #[test]
    fn resolve_reports_the_divider_or_the_rust_message() {
        let mut err = [0 as c_char; 256];
        let mut out = 0u16;

        let config = autd3_core_sampling_config_freq(4000.0);
        assert_eq!(AUTD3_OK, unsafe {
            autd3_core_sampling_config_resolve(config, &raw mut out, err.as_mut_ptr(), err.len())
        });
        assert_eq!(10, out);
        unsafe { autd3_core_sampling_config_free(config) };

        let config = autd3_core_sampling_config_freq(4001.0);
        assert_eq!(AUTD3_ERR, unsafe {
            autd3_core_sampling_config_resolve(config, &raw mut out, err.as_mut_ptr(), err.len())
        });
        assert!(message(&err).contains("must divide the ultrasound frequency"));
        unsafe { autd3_core_sampling_config_free(config) };
    }

    #[test]
    fn params_match_the_rust_constants() {
        assert_eq!(
            to_ns(ULTRASOUND_PERIOD),
            autd3_core_params_ultrasound_period_ns()
        );
        assert_eq!(
            params::ULTRASOUND_FREQ_HZ,
            autd3_core_params_ultrasound_freq_hz()
        );
        assert_eq!(Autd3::NUM_TRANSDUCERS, autd3_core_params_num_transducers());
        assert_eq!(MAX_INFLIGHT, autd3_core_params_max_inflight());
    }
}
