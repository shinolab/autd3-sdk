use std::ffi::c_char;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, ModulationBuffer, drop_handle, handle_mut, handle_ref,
    into_handle, slice_mut, slice_ref, write_cstr, write_out,
};
use autd3_rs_core::Angle;
use autd3_rs_core::units::Hz;
use autd3_rs_core::value::SamplingConfig;
use autd3_rs_modulation::{FourierOption, SineComponent, SineOption, SquareOption};

fn to_sampling_config(divide: u16) -> Option<SamplingConfig> {
    std::num::NonZeroU16::new(divide).map(SamplingConfig::new)
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
    freq_hz: f32,
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

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Autd3SineOption {
    pub amplitude: u8,
    pub offset: u8,
    pub phase_rad: f32,
    pub clamp: bool,
}

impl From<Autd3SineOption> for SineOption {
    fn from(option: Autd3SineOption) -> Self {
        Self {
            amplitude: option.amplitude,
            offset: option.offset,
            phase: Angle::from_rad(option.phase_rad),
            clamp: option.clamp,
        }
    }
}

#[repr(C)]
pub struct Autd3SquareOption {
    pub low: u8,
    pub high: u8,
    pub duty: f32,
}

impl From<Autd3SquareOption> for SquareOption {
    fn from(option: Autd3SquareOption) -> Self {
        Self {
            low: option.low,
            high: option.high,
            duty: option.duty,
        }
    }
}

#[repr(C)]
pub struct Autd3FourierOption {
    pub has_scale_factor: bool,
    pub scale_factor: f32,
    pub clamp: bool,
    pub offset: u8,
}

impl From<Autd3FourierOption> for FourierOption {
    fn from(option: Autd3FourierOption) -> Self {
        Self {
            scale_factor: option.has_scale_factor.then_some(option.scale_factor),
            clamp: option.clamp,
            offset: option.offset,
        }
    }
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
pub unsafe extern "C" fn autd3_modulation_sine(
    freq: f32,
    option: Autd3SineOption,
    sampling_divide: u16,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let (Some(config), Some(buffer)) = (to_sampling_config(sampling_divide), unsafe {
        handle_mut(buffer)
    }) else {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    };
    let result = autd3_rs_modulation::sine(freq * Hz, &option.into(), config, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_square(
    freq: f32,
    option: Autd3SquareOption,
    sampling_divide: u16,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let (Some(config), Some(buffer)) = (to_sampling_config(sampling_divide), unsafe {
        handle_mut(buffer)
    }) else {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    };
    let result = autd3_rs_modulation::square(freq * Hz, &option.into(), config, &mut buffer.0);
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

#[repr(C)]
pub struct Autd3SineComponent {
    pub freq: f32,
    pub option: Autd3SineOption,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_fourier(
    components: *const Autd3SineComponent,
    num_components: usize,
    option: Autd3FourierOption,
    sampling_divide: u16,
    buffer: *mut ModulationBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let (Some(slice), Some(config), Some(buffer)) = (
        unsafe { slice_ref(components, num_components) },
        to_sampling_config(sampling_divide),
        unsafe { handle_mut(buffer) },
    ) else {
        return unsafe { invalid_argument(out_err, out_err_len, "null argument") };
    };

    let sine_components = slice
        .iter()
        .map(|c| SineComponent {
            freq: c.freq * Hz,
            option: c.option.into(),
        })
        .collect::<Vec<_>>();
    let result =
        autd3_rs_modulation::fourier(&sine_components, &option.into(), config, &mut buffer.0);
    unsafe { finish(result, out_err, out_err_len) }
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
    fn sine_requires_a_sampling_config_and_uses_it() {
        let option = Autd3SineOption {
            amplitude: 0xFF,
            offset: 0x80,
            phase_rad: 0.0,
            clamp: false,
        };
        let buffer = autd3_modulation_buffer_new();
        let config = SamplingConfig::FREQ_40K.divide().get();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_modulation_sine(200.0, option, 0, buffer, std::ptr::null_mut(), 0)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_modulation_sine(200.0, option, config, buffer, std::ptr::null_mut(), 0)
        });
        assert_eq!(200, unsafe { &*buffer }.0.len());
        unsafe { autd3_modulation_buffer_free(buffer) };
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
