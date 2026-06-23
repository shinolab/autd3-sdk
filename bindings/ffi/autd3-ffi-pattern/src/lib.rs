use autd3_ffi_abi::{PatternBuffer, drop_handle, into_handle};
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_core::{Geometry, Length, Point3, Velocity};

#[repr(C)]
pub struct Autd3Emission {
    pub phase: u8,
    pub intensity: u8,
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_pattern_wavelength(sound_speed_mm_per_s: f32) -> f32 {
    autd3_rs_pattern::wavelength(Velocity::from_mm_s(sound_speed_mm_per_s)).mm()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_pattern_buffer_new(num_devices: usize) -> *mut PatternBuffer {
    into_handle(PatternBuffer(vec![
        [Emission::default(); NUM_TRANSDUCERS];
        num_devices
    ]))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_from_array(
    emissions: *const Autd3Emission,
    num_devices: usize,
) -> *mut PatternBuffer {
    if emissions.is_null() {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(emissions, num_devices * NUM_TRANSDUCERS) };
    let buffer = slice
        .chunks_exact(NUM_TRANSDUCERS)
        .map(|device| {
            let mut slot = [Emission::default(); NUM_TRANSDUCERS];
            for (e, src) in slot.iter_mut().zip(device) {
                *e = Emission {
                    phase: Phase(src.phase),
                    intensity: Intensity(src.intensity),
                };
            }
            slot
        })
        .collect();
    into_handle(PatternBuffer(buffer))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_num_devices(buffer: *const PatternBuffer) -> usize {
    if buffer.is_null() {
        return 0;
    }

    unsafe { &*buffer }.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_free(buffer: *mut PatternBuffer) {
    unsafe { drop_handle(buffer) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_focus(
    geometry: *const Geometry,
    target: *const f32,
    wavelength_mm: f32,
    intensity: u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    if geometry.is_null() || target.is_null() || buffer.is_null() {
        return -1;
    }

    let geometry = unsafe { &*geometry };
    let target = unsafe { std::slice::from_raw_parts(target, 3) };
    let buffer = unsafe { &mut *buffer };
    if buffer.0.len() != geometry.len() {
        return -1;
    }
    autd3_rs_pattern::focus(
        geometry,
        Point3::new(target[0], target[1], target[2]),
        Length::millimeters(wavelength_mm),
        Intensity(intensity),
        &mut buffer.0,
    );
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_null(buffer: *mut PatternBuffer) {
    if buffer.is_null() {
        return;
    }

    autd3_rs_pattern::null(&mut unsafe { &mut *buffer }.0);
}
