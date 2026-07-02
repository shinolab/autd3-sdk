use std::num::NonZeroU16;
use std::time::Duration;

use autd3_ffi_abi::{drop_handle, into_handle};
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

    unsafe { &*geometry }.num_devices()
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
pub unsafe extern "C" fn autd3_core_geometry_num_transducers(geometry: *const Geometry) -> usize {
    if geometry.is_null() {
        return 0;
    }

    unsafe { &*geometry }.num_transducers()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_num_transducers(
    geometry: *const Geometry,
    dev: usize,
) -> usize {
    if geometry.is_null() {
        return 0;
    }

    let Some(device) = unsafe { &*geometry }.iter().nth(dev) else {
        return 0;
    };
    device.num_transducers()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_idx(geometry: *const Geometry, dev: usize) -> usize {
    if geometry.is_null() {
        return 0;
    }

    let Some(device) = unsafe { &*geometry }.iter().nth(dev) else {
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
    if geometry.is_null() || out.is_null() {
        return;
    }

    let Some(device) = unsafe { &*geometry }.iter().nth(dev) else {
        return;
    };
    let rotation = device.rotation();

    unsafe {
        *out = rotation.w;
        *out.add(1) = rotation.i;
        *out.add(2) = rotation.j;
        *out.add(3) = rotation.k;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_transducer_position(
    geometry: *const Geometry,
    dev: usize,
    tr: usize,
    out: *mut f32,
) -> i32 {
    if geometry.is_null() || out.is_null() {
        return -1;
    }

    let Some(device) = unsafe { &*geometry }.iter().nth(dev) else {
        return -1;
    };
    if tr >= device.num_transducers() {
        return -1;
    }
    let p = device.position(tr);

    unsafe {
        *out = p.x;
        *out.add(1) = p.y;
        *out.add(2) = p.z;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_transducer_direction(
    geometry: *const Geometry,
    dev: usize,
    tr: usize,
    out: *mut f32,
) -> i32 {
    if geometry.is_null() || out.is_null() {
        return -1;
    }

    let Some(device) = unsafe { &*geometry }.iter().nth(dev) else {
        return -1;
    };
    if tr >= device.num_transducers() {
        return -1;
    }
    let d = device.direction(tr).into_inner();

    unsafe {
        *out = d.x;
        *out.add(1) = d.y;
        *out.add(2) = d.z;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_x(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    if geometry.is_null() || out.is_null() {
        return;
    }

    let Some(device) = unsafe { &*geometry }.iter().nth(dev) else {
        return;
    };
    let direction = device.x_direction();

    unsafe {
        *out = direction.x;
        *out.add(1) = direction.y;
        *out.add(2) = direction.z;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_y(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    if geometry.is_null() || out.is_null() {
        return;
    }

    let Some(device) = unsafe { &*geometry }.iter().nth(dev) else {
        return;
    };
    let direction = device.y_direction();

    unsafe {
        *out = direction.x;
        *out.add(1) = direction.y;
        *out.add(2) = direction.z;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_device_direction_axial(
    geometry: *const Geometry,
    dev: usize,
    out: *mut f32,
) {
    if geometry.is_null() || out.is_null() {
        return;
    }

    let Some(device) = unsafe { &*geometry }.iter().nth(dev) else {
        return;
    };
    let direction = device.axial_direction();

    unsafe {
        *out = direction.x;
        *out.add(1) = direction.y;
        *out.add(2) = direction.z;
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
    if config.is_null() || out.is_null() {
        return -1;
    }

    let Ok(freq) = unsafe { &*config }.freq() else {
        return -1;
    };

    unsafe { *out = freq.hz() };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_sampling_config_period_value(
    config: *const SamplingConfig,
    out: *mut u64,
) -> i32 {
    if config.is_null() || out.is_null() {
        return -1;
    }

    let Ok(period) = unsafe { &*config }.period() else {
        return -1;
    };

    let Ok(nanos) = u64::try_from(period.as_nanos()) else {
        return -1;
    };

    unsafe { *out = nanos };
    0
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
