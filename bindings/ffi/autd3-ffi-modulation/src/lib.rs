use std::ffi::c_char;

use autd3_ffi_abi::{
    AUTD3_ERR, AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, ModulationBuffer, drop_handle, handle_mut,
    handle_ref, into_handle, slice_ref, write_cstr, write_out,
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
    let Some(divider) = core::num::NonZeroU16::new(divider) else {
        return false;
    };
    let Some(value) =
        autd3_rs_modulation::samples_per_period(divider, autd3_rs_core::Freq::from_hz(freq_hz))
    else {
        return false;
    };

    unsafe { write_out(out, value) == AUTD3_OK }
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
    let Some(slice) = (unsafe { slice_ref(data, len) }) else {
        return std::ptr::null_mut();
    };
    into_handle(ModulationBuffer(slice.to_vec()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_len(buffer: *const ModulationBuffer) -> usize {
    let Some(buffer) = (unsafe { handle_ref(buffer) }) else {
        return 0;
    };

    buffer.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_get(
    buffer: *const ModulationBuffer,
    index: usize,
    out: *mut u8,
) -> i32 {
    let Some(buffer) = (unsafe { handle_ref(buffer) }) else {
        return -1;
    };

    let Some(&value) = buffer.0.get(index) else {
        return -1;
    };
    if unsafe { write_out(out, value) } != AUTD3_OK {
        return -1;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_set(
    buffer: *mut ModulationBuffer,
    index: usize,
    value: u8,
) -> i32 {
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };

    let Some(v) = buffer.0.get_mut(index) else {
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
    let sampling_config =
        unsafe { handle_ref(sampling_config) }.map_or(SamplingConfig::FREQ_4K, |c| *c);
    into_handle(SineOption {
        amplitude,
        offset,
        phase: Angle::from_rad(phase),
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
    let (Some(option), Some(buffer)) =
        (unsafe { handle_ref(option) }, unsafe { handle_mut(buffer) })
    else {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    };
    let Some(mode) = to_sampling_mode(mode, freq, freq_int) else {
        return unsafe { invalid_argument(out_err, out_err_len, "unknown sampling mode") };
    };

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
    let sampling_config =
        unsafe { handle_ref(sampling_config) }.map_or(SamplingConfig::FREQ_4K, |c| *c);
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
    let (Some(option), Some(buffer)) =
        (unsafe { handle_ref(option) }, unsafe { handle_mut(buffer) })
    else {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    };
    let Some(mode) = to_sampling_mode(mode, freq, freq_int) else {
        return unsafe { invalid_argument(out_err, out_err_len, "unknown sampling mode") };
    };

    let result = autd3_rs_modulation::square(mode, option, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_constant(
    intensity: u8,
    buffer: *mut ModulationBuffer,
) -> i32 {
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };

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
    let (Some(slice), Some(option), Some(buffer)) = (
        unsafe { slice_ref(components, num_components) },
        unsafe { handle_ref(option) },
        unsafe { handle_mut(buffer) },
    ) else {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    };

    let mut sine_components = Vec::with_capacity(slice.len());
    for c in slice {
        let Some(component_option) = (unsafe { handle_ref(c.option) }) else {
            return unsafe { invalid_argument(out_err, out_err_len, "null sine option") };
        };
        let Some(freq) = to_sampling_mode(c.mode, c.freq, c.freq_int) else {
            return unsafe { invalid_argument(out_err, out_err_len, "unknown sampling mode") };
        };
        sine_components.push(SineComponent::<SamplingMode> {
            freq,
            option: *component_option,
        });
    }
    let result = autd3_rs_modulation::fourier(&sine_components, option, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_radiation_pressure(
    src: *const ModulationBuffer,
    dst: *mut ModulationBuffer,
) -> i32 {
    let (Some(src), Some(dst)) = (unsafe { handle_ref(src) }, unsafe { handle_mut(dst) }) else {
        return -1;
    };

    autd3_rs_modulation::radiation_pressure(&src.0, &mut dst.0);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_radiation_pressure_inplace(
    buffer: *mut ModulationBuffer,
) -> i32 {
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };

    autd3_rs_modulation::radiation_pressure_inplace(&mut buffer.0);
    0
}

autd3_ffi_abi::export_abi_version!();
