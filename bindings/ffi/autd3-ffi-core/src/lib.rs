use std::num::NonZeroU16;

use autd3_ffi_abi::{drop_handle, into_handle};
use autd3_rs_core::value::{Phase, SamplingConfig};
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
    if devices.is_null() {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(devices, len) };
    let devices: Vec<Autd3> = slice
        .iter()
        .map(|d| {
            Autd3::new(
                Point3::new(d.origin[0], d.origin[1], d.origin[2]),
                UnitQuaternion::from_quaternion(Quaternion::new(
                    d.rotation[0],
                    d.rotation[1],
                    d.rotation[2],
                    d.rotation[3],
                )),
            )
        })
        .collect();
    into_handle(Geometry::new(devices))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_num_devices(geometry: *const Geometry) -> usize {
    if geometry.is_null() {
        return 0;
    }

    unsafe { &*geometry }.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_center(geometry: *const Geometry, out: *mut f32) {
    if geometry.is_null() || out.is_null() {
        return;
    }

    let center = unsafe { &*geometry }.center();

    unsafe {
        *out = center.x;
        *out.add(1) = center.y;
        *out.add(2) = center.z;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_free(geometry: *mut Geometry) {
    unsafe { drop_handle(geometry) }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_core_phase_radian(value: u8) -> f32 {
    Phase(value).radian()
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
        Some(divide) => into_handle(SamplingConfig::Divide(divide)),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_sampling_config_divide_value(
    config: *const SamplingConfig,
    out: *mut u16,
) -> i32 {
    if config.is_null() || out.is_null() {
        return -1;
    }

    let Ok(value) = unsafe { &*config }.divide() else {
        return -1;
    };

    unsafe { *out = value };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_sampling_config_free(config: *mut SamplingConfig) {
    unsafe { drop_handle(config) }
}
