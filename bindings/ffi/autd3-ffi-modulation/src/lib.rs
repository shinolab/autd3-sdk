use autd3_ffi_abi::{ModulationBuffer, drop_handle, into_handle};
use autd3_rs_core::Angle;
use autd3_rs_core::units::Hz;
use autd3_rs_core::value::{Intensity, SamplingConfig};
use autd3_rs_modulation::SineOption;

#[unsafe(no_mangle)]
pub extern "C" fn autd3_modulation_buffer_new() -> *mut ModulationBuffer {
    into_handle(ModulationBuffer(Vec::new()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_from_bytes(
    data: *const u8,
    len: usize,
) -> *mut ModulationBuffer {
    if data.is_null() {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    into_handle(ModulationBuffer(slice.to_vec()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_len(buffer: *const ModulationBuffer) -> usize {
    if buffer.is_null() {
        return 0;
    }

    unsafe { &*buffer }.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_free(buffer: *mut ModulationBuffer) {
    unsafe { drop_handle(buffer) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_sine_option_new(
    intensity: u8,
    offset: u8,
    phase: f32,
    clamp: bool,
    sampling_config: *const SamplingConfig,
) -> *mut SineOption {
    let sampling_config = if sampling_config.is_null() {
        SamplingConfig::FREQ_4K
    } else {
        *unsafe { &*sampling_config }
    };
    into_handle(SineOption {
        intensity: Intensity(intensity),
        offset,
        phase: Angle::from_radian(phase),
        clamp,
        sampling_config,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_sine_option_free(option: *mut SineOption) {
    unsafe { drop_handle(option) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_sine(
    freq: f32,
    option: *const SineOption,
    buffer: *mut ModulationBuffer,
) -> i32 {
    if option.is_null() || buffer.is_null() {
        return -1;
    }

    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    match autd3_rs_modulation::sine(freq * Hz, option, &mut buffer.0) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}
