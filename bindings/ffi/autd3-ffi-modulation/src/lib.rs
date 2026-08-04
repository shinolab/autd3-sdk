use std::ffi::c_char;

use autd3_ffi_abi::{
    AUTD3_ERR, AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, ModulationBuffer, drop_handle, into_handle,
    write_cstr,
};
use autd3_rs_core::Angle;
use autd3_rs_core::units::Hz;
use autd3_rs_core::value::{Nearest, SamplingConfig};
use autd3_rs_modulation::{FourierOption, SamplingMode, SineComponent, SineOption, SquareOption};

fn to_sampling_mode(mode: u8, freq: f32, freq_int: u32) -> Option<SamplingMode> {
    match mode {
        0 => Some(SamplingMode::from(freq * Hz)),
        1 => Some(SamplingMode::from(freq_int * Hz)),
        2 => Some(SamplingMode::from(Nearest(freq * Hz))),
        _ => None,
    }
}

unsafe fn finish<E: std::fmt::Display>(
    result: Result<(), E>,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    match result {
        Ok(()) => AUTD3_OK,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            AUTD3_ERR
        }
    }
}

unsafe fn invalid_argument(out_err: *mut c_char, out_err_len: usize, message: &str) -> i32 {
    unsafe { write_cstr(out_err, out_err_len, message) };
    AUTD3_ERR_INVALID_ARGUMENT
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_samples_per_period(
    divider: u16,
    freq_hz: u32,
    out: *mut u32,
) -> bool {
    if out.is_null() {
        return false;
    }

    let Some(value) = autd3_rs_modulation::samples_per_period(divider, freq_hz) else {
        return false;
    };

    unsafe { *out = value };
    true
}

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
pub unsafe extern "C" fn autd3_modulation_buffer_get(
    buffer: *const ModulationBuffer,
    index: usize,
    out: *mut u8,
) -> i32 {
    if buffer.is_null() || out.is_null() {
        return -1;
    }

    let Some(&value) = unsafe { &*buffer }.0.get(index) else {
        return -1;
    };
    unsafe { *out = value };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_set(
    buffer: *mut ModulationBuffer,
    index: usize,
    value: u8,
) -> i32 {
    if buffer.is_null() {
        return -1;
    }

    let Some(v) = unsafe { &mut *buffer }.0.get_mut(index) else {
        return -1;
    };
    *v = value;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_free(buffer: *mut ModulationBuffer) {
    unsafe { drop_handle(buffer) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_sine_option_new(
    amplitude: u8,
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
        amplitude,
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
    mode: u8,
    freq: f32,
    freq_int: u32,
    option: *const SineOption,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    if option.is_null() || buffer.is_null() {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    }
    let Some(mode) = to_sampling_mode(mode, freq, freq_int) else {
        return unsafe { invalid_argument(out_err, out_err_len, "unknown sampling mode") };
    };

    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    let result = autd3_rs_modulation::sine(mode, option, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_square_option_new(
    low: u8,
    high: u8,
    duty: f32,
    sampling_config: *const SamplingConfig,
) -> *mut SquareOption {
    let sampling_config = if sampling_config.is_null() {
        SamplingConfig::FREQ_4K
    } else {
        *unsafe { &*sampling_config }
    };
    into_handle(SquareOption {
        low,
        high,
        duty,
        sampling_config,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_square_option_free(option: *mut SquareOption) {
    unsafe { drop_handle(option) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_square(
    mode: u8,
    freq: f32,
    freq_int: u32,
    option: *const SquareOption,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    if option.is_null() || buffer.is_null() {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    }
    let Some(mode) = to_sampling_mode(mode, freq, freq_int) else {
        return unsafe { invalid_argument(out_err, out_err_len, "unknown sampling mode") };
    };

    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    let result = autd3_rs_modulation::square(mode, option, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_constant(
    intensity: u8,
    buffer: *mut ModulationBuffer,
) -> i32 {
    if buffer.is_null() {
        return -1;
    }

    let buffer = unsafe { &mut *buffer };
    autd3_rs_modulation::constant(intensity, &mut buffer.0);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_modulation_fourier_option_new(
    has_scale_factor: bool,
    scale_factor: f32,
    clamp: bool,
    offset: u8,
) -> *mut FourierOption {
    into_handle(FourierOption {
        scale_factor: has_scale_factor.then_some(scale_factor),
        clamp,
        offset,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_fourier_option_free(option: *mut FourierOption) {
    unsafe { drop_handle(option) }
}

#[repr(C)]
pub struct Autd3SineComponent {
    pub mode: u8,
    pub freq: f32,
    pub freq_int: u32,
    pub option: *const SineOption,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_fourier(
    components: *const Autd3SineComponent,
    num_components: usize,
    option: *const FourierOption,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    if components.is_null() || option.is_null() || buffer.is_null() {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    }

    let slice = unsafe { std::slice::from_raw_parts(components, num_components) };
    if slice.iter().any(|c| c.option.is_null()) {
        return unsafe { invalid_argument(out_err, out_err_len, "null sine option") };
    }
    let mut sine_components = Vec::with_capacity(slice.len());
    for c in slice {
        let Some(freq) = to_sampling_mode(c.mode, c.freq, c.freq_int) else {
            return unsafe { invalid_argument(out_err, out_err_len, "unknown sampling mode") };
        };
        sine_components.push(SineComponent::<SamplingMode> {
            freq,
            option: *unsafe { &*c.option },
        });
    }
    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    let result = autd3_rs_modulation::fourier(&sine_components, option, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_radiation_pressure(
    src: *const ModulationBuffer,
    dst: *mut ModulationBuffer,
) -> i32 {
    if src.is_null() || dst.is_null() {
        return -1;
    }

    let src = unsafe { &*src };
    let dst = unsafe { &mut *dst };
    autd3_rs_modulation::radiation_pressure(&src.0, &mut dst.0);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_radiation_pressure_inplace(
    buffer: *mut ModulationBuffer,
) -> i32 {
    if buffer.is_null() {
        return -1;
    }

    let buffer = unsafe { &mut *buffer };
    autd3_rs_modulation::radiation_pressure_inplace(&mut buffer.0);
    0
}

autd3_ffi_abi::export_abi_version!();
