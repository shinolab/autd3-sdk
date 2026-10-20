use std::ffi::c_char;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, ModulationBuffer, drop_handle, handle_mut, handle_ref,
    into_handle, slice_mut, slice_ref, write_cstr, write_out,
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

unsafe fn finish(
    result: Result<(), autd3_rs_modulation::ModulationError>,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    match result {
        Ok(()) => AUTD3_OK,
        Err(e) => unsafe { invalid_argument(out_err, out_err_len, &e.to_string()) },
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
pub unsafe extern "C" fn autd3_modulation_buffer_copy_to(
    buffer: *const ModulationBuffer,
    dst: *mut u8,
    len: usize,
) -> i32 {
    let Some(buffer) = (unsafe { handle_ref(buffer) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    if len != buffer.0.len() {
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    let Some(dst) = (unsafe { slice_mut(dst, len) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    dst.copy_from_slice(&buffer.0);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_buffer_free(buffer: *mut ModulationBuffer) {
    unsafe { drop_handle(buffer) }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_modulation_sine_option_new(
    amplitude: u8,
    offset: u8,
    phase: f32,
    clamp: bool,
) -> *mut SineOption {
    into_handle(SineOption {
        amplitude,
        offset,
        phase: Angle::from_rad(phase),
        clamp,
    })
}

fn first_error(codes: &[i32]) -> i32 {
    codes
        .iter()
        .copied()
        .find(|&code| code != AUTD3_OK)
        .unwrap_or(AUTD3_OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_sine_option_default(
    out_amplitude: *mut u8,
    out_offset: *mut u8,
    out_phase_rad: *mut f32,
    out_clamp: *mut bool,
) -> i32 {
    let default = SineOption::default();
    first_error(&[
        unsafe { write_out(out_amplitude, default.amplitude) },
        unsafe { write_out(out_offset, default.offset) },
        unsafe { write_out(out_phase_rad, default.phase.rad()) },
        unsafe { write_out(out_clamp, default.clamp) },
    ])
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_square_option_default(
    out_low: *mut u8,
    out_high: *mut u8,
    out_duty: *mut f32,
) -> i32 {
    let default = SquareOption::default();
    first_error(&[
        unsafe { write_out(out_low, default.low) },
        unsafe { write_out(out_high, default.high) },
        unsafe { write_out(out_duty, default.duty) },
    ])
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_fourier_option_default(
    out_has_scale_factor: *mut bool,
    out_scale_factor: *mut f32,
    out_clamp: *mut bool,
    out_offset: *mut u8,
) -> i32 {
    let default = FourierOption::default();
    first_error(&[
        unsafe { write_out(out_has_scale_factor, default.scale_factor.is_some()) },
        unsafe { write_out(out_scale_factor, default.scale_factor.unwrap_or(0.0)) },
        unsafe { write_out(out_clamp, default.clamp) },
        unsafe { write_out(out_offset, default.offset) },
    ])
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
    config: *const SamplingConfig,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let (Some(option), Some(config), Some(buffer)) = (
        unsafe { handle_ref(option) },
        unsafe { handle_ref(config) },
        unsafe { handle_mut(buffer) },
    ) else {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    };
    let Some(mode) = to_sampling_mode(mode, freq, freq_int) else {
        return unsafe { invalid_argument(out_err, out_err_len, "unknown sampling mode") };
    };

    let result = autd3_rs_modulation::sine(mode, option, *config, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_modulation_square_option_new(
    low: u8,
    high: u8,
    duty: f32,
) -> *mut SquareOption {
    into_handle(SquareOption { low, high, duty })
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
    config: *const SamplingConfig,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let (Some(option), Some(config), Some(buffer)) = (
        unsafe { handle_ref(option) },
        unsafe { handle_ref(config) },
        unsafe { handle_mut(buffer) },
    ) else {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    };
    let Some(mode) = to_sampling_mode(mode, freq, freq_int) else {
        return unsafe { invalid_argument(out_err, out_err_len, "unknown sampling mode") };
    };

    let result = autd3_rs_modulation::square(mode, option, *config, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_constant(
    amplitude: u8,
    buffer: *mut ModulationBuffer,
) -> i32 {
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };

    autd3_rs_modulation::constant(amplitude, &mut buffer.0);
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
    config: *const SamplingConfig,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let (Some(slice), Some(option), Some(config), Some(buffer)) = (
        unsafe { slice_ref(components, num_components) },
        unsafe { handle_ref(option) },
        unsafe { handle_ref(config) },
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
        sine_components.push(SineComponent {
            freq,
            option: *component_option,
        });
    }
    let result = autd3_rs_modulation::fourier(&sine_components, option, *config, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_radiation_pressure(
    src: *const ModulationBuffer,
    dst: *mut ModulationBuffer,
) -> i32 {
    if std::ptr::eq(src, dst.cast_const()) {
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radiation_pressure_rejects_the_same_buffer_as_src_and_dst() {
        let buffer = autd3_modulation_buffer_new();
        unsafe { &mut *buffer }.0 = vec![0x80, 0xFF];
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_modulation_radiation_pressure(buffer, buffer)
        });
        assert_eq!(vec![0x80, 0xFF], unsafe { &*buffer }.0);
        let dst = autd3_modulation_buffer_new();
        assert_eq!(0, unsafe {
            autd3_modulation_radiation_pressure(buffer, dst)
        });
        assert_eq!(2, unsafe { &*dst }.0.len());
        unsafe {
            autd3_modulation_buffer_free(buffer);
            autd3_modulation_buffer_free(dst);
        }
    }

    #[test]
    fn sine_requires_a_sampling_config_and_uses_it() {
        let option = autd3_modulation_sine_option_new(0xFF, 0x80, 0.0, false);
        let buffer = autd3_modulation_buffer_new();
        let config = SamplingConfig::FREQ_40K;
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_modulation_sine(
                1,
                0.0,
                200,
                option,
                std::ptr::null(),
                buffer,
                std::ptr::null_mut(),
                0,
            )
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_modulation_sine(
                1,
                0.0,
                200,
                option,
                &raw const config,
                buffer,
                std::ptr::null_mut(),
                0,
            )
        });
        assert_eq!(200, unsafe { &*buffer }.0.len());
        unsafe {
            autd3_modulation_sine_option_free(option);
            autd3_modulation_buffer_free(buffer);
        }
    }

    #[test]
    fn the_option_defaults_match_the_rust_default() {
        let (mut amplitude, mut offset, mut phase, mut clamp) = (0, 0, 1.0, true);
        assert_eq!(AUTD3_OK, unsafe {
            autd3_modulation_sine_option_default(
                &raw mut amplitude,
                &raw mut offset,
                &raw mut phase,
                &raw mut clamp,
            )
        });
        let sine = SineOption::default();
        assert_eq!(sine.amplitude, amplitude);
        assert_eq!(sine.offset, offset);
        assert_eq!(sine.phase.rad().to_bits(), phase.to_bits());
        assert_eq!(sine.clamp, clamp);

        let (mut low, mut high, mut duty) = (1, 0, 0.0);
        assert_eq!(AUTD3_OK, unsafe {
            autd3_modulation_square_option_default(&raw mut low, &raw mut high, &raw mut duty)
        });
        let square = SquareOption::default();
        assert_eq!(square.low, low);
        assert_eq!(square.high, high);
        assert_eq!(square.duty.to_bits(), duty.to_bits());

        let (mut has_scale, mut scale, mut fourier_clamp, mut fourier_offset) =
            (true, 1.0, true, 1);
        assert_eq!(AUTD3_OK, unsafe {
            autd3_modulation_fourier_option_default(
                &raw mut has_scale,
                &raw mut scale,
                &raw mut fourier_clamp,
                &raw mut fourier_offset,
            )
        });
        let fourier = FourierOption::default();
        assert_eq!(fourier.scale_factor.is_some(), has_scale);
        assert_eq!(fourier.clamp, fourier_clamp);
        assert_eq!(fourier.offset, fourier_offset);

        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_modulation_fourier_option_default(
                std::ptr::null_mut(),
                &raw mut scale,
                &raw mut fourier_clamp,
                &raw mut fourier_offset,
            )
        });
    }
}
