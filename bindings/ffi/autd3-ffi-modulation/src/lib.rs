use autd3_ffi_abi::{ModulationBuffer, drop_handle, into_handle};
use autd3_rs_core::Angle;
use autd3_rs_core::units::Hz;
use autd3_rs_core::value::{Nearest, SamplingConfig};
use autd3_rs_modulation::{FourierOption, SamplingMode, SineComponent, SineOption, SquareOption};

fn to_sampling_mode(mode: u8, freq: f32, freq_int: u32) -> SamplingMode {
    match mode {
        1 => SamplingMode::from(freq_int * Hz),
        2 => SamplingMode::from(Nearest(freq * Hz)),
        _ => SamplingMode::from(freq * Hz),
    }
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
) -> i32 {
    if option.is_null() || buffer.is_null() {
        return -1;
    }

    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    match autd3_rs_modulation::sine(
        to_sampling_mode(mode, freq, freq_int),
        option,
        &mut buffer.0,
    ) {
        Ok(()) => 0,
        Err(_) => -1,
    }
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
) -> i32 {
    if option.is_null() || buffer.is_null() {
        return -1;
    }

    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    match autd3_rs_modulation::square(
        to_sampling_mode(mode, freq, freq_int),
        option,
        &mut buffer.0,
    ) {
        Ok(()) => 0,
        Err(_) => -1,
    }
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
) -> i32 {
    if components.is_null() || option.is_null() || buffer.is_null() {
        return -1;
    }

    let slice = unsafe { std::slice::from_raw_parts(components, num_components) };
    if slice.iter().any(|c| c.option.is_null()) {
        return -1;
    }
    let components: Vec<SineComponent<SamplingMode>> = slice
        .iter()
        .map(|c| SineComponent {
            freq: to_sampling_mode(c.mode, c.freq, c.freq_int),
            option: *unsafe { &*c.option },
        })
        .collect();
    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    match autd3_rs_modulation::fourier(&components, option, &mut buffer.0) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_modulation_radiation_pressure(
    src: *const ModulationBuffer,
    out: *mut ModulationBuffer,
) -> i32 {
    if src.is_null() || out.is_null() {
        return -1;
    }

    let src = unsafe { &*src };
    let out = unsafe { &mut *out };
    autd3_rs_modulation::radiation_pressure(&src.0, &mut out.0);
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
