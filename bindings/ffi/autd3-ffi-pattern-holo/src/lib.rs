use std::ffi::c_char;
use std::num::{NonZeroU8, NonZeroUsize};

use autd3_ffi_abi::{
    AUTD3_ERR, AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, PatternBuffer, handle_mut, handle_ref,
    slice_ref, write_cstr,
};
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::value::Intensity;
use autd3_rs_core::{Geometry, Length, Point3};
use autd3_rs_pattern_holo::{
    AmplitudeTarget, Directivity, EmissionConstraint, GreedyOption, GsOption, GspatOption,
    NaiveOption, NalgebraBackend, Pa, TransducerMask, abs_objective_func, dB, greedy, gs, gspat,
    kPa, naive,
};

#[repr(C)]
pub struct Autd3HoloAmplitudeTarget {
    pub point: [f32; 3],
    pub amplitude_pa: f32,
}

#[repr(C)]
pub struct Autd3EmissionConstraint {
    pub kind: u8,
    pub min: u8,
    pub max: u8,
    pub multiply: f32,
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_holo_amplitude_pascal(value: f32) -> f32 {
    (value * Pa).pascal()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_holo_amplitude_kilo_pascal(value: f32) -> f32 {
    (value * kPa).pascal()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_holo_amplitude_spl(value: f32) -> f32 {
    (value * dB).pascal()
}

fn to_directivity(d: u8) -> Option<Directivity> {
    match d {
        0 => Some(Directivity::Sphere),
        1 => Some(Directivity::T4010A1),
        _ => None,
    }
}

fn to_constraint(c: &Autd3EmissionConstraint) -> Option<EmissionConstraint> {
    match c.kind {
        0 => Some(EmissionConstraint::Normalize),
        1 => Some(EmissionConstraint::Multiply(c.multiply)),
        2 => Some(EmissionConstraint::Uniform(Intensity(c.min))),
        3 => Some(EmissionConstraint::Clamp(
            Intensity(c.min),
            Intensity(c.max),
        )),
        _ => None,
    }
}

fn build_foci(foci: &[Autd3HoloAmplitudeTarget]) -> Vec<AmplitudeTarget> {
    foci.iter()
        .map(|f| AmplitudeTarget {
            point: Point3::new(f.point[0], f.point[1], f.point[2]),
            amplitude: f.amplitude_pa * Pa,
        })
        .collect()
}

unsafe fn build_mask(mask: *const u8, num_devices: usize) -> Option<Vec<Vec<bool>>> {
    let slice = unsafe { slice_ref(mask, num_devices * Autd3::NUM_TRANSDUCERS) }?;
    Some(
        slice
            .as_chunks::<{ Autd3::NUM_TRANSDUCERS }>()
            .0
            .iter()
            .map(|device| {
                let mut slot = vec![false; Autd3::NUM_TRANSDUCERS];
                for (m, src) in slot.iter_mut().zip(device) {
                    *m = *src != 0;
                }
                slot
            })
            .collect(),
    )
}

fn mask_ref(mask: Option<&[Vec<bool>]>) -> TransducerMask<'_> {
    match mask {
        Some(m) => TransducerMask::Masked(m),
        None => TransducerMask::AllEnabled,
    }
}

struct Common<'a> {
    geometry: &'a Geometry,
    buffer: &'a mut PatternBuffer,
    foci: Vec<AmplitudeTarget>,
    mask: Option<Vec<Vec<bool>>>,
    constraint: EmissionConstraint,
    directivity: Directivity,
}

#[allow(clippy::too_many_arguments)]
unsafe fn prepare<'a>(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    buffer: *mut PatternBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> Result<Common<'a>, i32> {
    let fail = |message: &str| {
        unsafe { write_cstr(out_err, out_err_len, message) };
        AUTD3_ERR_INVALID_ARGUMENT
    };

    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return Err(fail("null geometry"));
    };
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return Err(fail("null pattern buffer"));
    };
    if buffer.0.len() != geometry.num_devices() {
        return Err(fail(
            "the pattern buffer length does not match the geometry",
        ));
    }
    let Some(constraint) = (unsafe { handle_ref(constraint) }) else {
        return Err(fail("null constraint"));
    };
    let Some(constraint) = to_constraint(constraint) else {
        return Err(fail("unknown emission constraint"));
    };
    let Some(directivity) = to_directivity(directivity) else {
        return Err(fail("unknown directivity"));
    };
    let Some(foci) = (unsafe { slice_ref(foci, num_foci) }) else {
        return Err(fail("null foci"));
    };
    let foci = build_foci(foci);
    let num_devices = buffer.0.len();
    let mask = unsafe { build_mask(mask, num_devices) };
    Ok(Common {
        geometry,
        buffer,
        foci,
        mask,
        constraint,
        directivity,
    })
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

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_naive(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    buffer: *mut PatternBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let common = match unsafe {
        prepare(
            geometry,
            foci,
            num_foci,
            constraint,
            directivity,
            mask,
            buffer,
            out_err,
            out_err_len,
        )
    } {
        Ok(common) => common,
        Err(code) => return code,
    };
    let option = NaiveOption {
        constraint: common.constraint,
        directivity: common.directivity,
        mask: mask_ref(common.mask.as_deref()),
        parallel,
    };
    let result = naive(
        &NalgebraBackend,
        common.geometry,
        &common.foci,
        Length::from_mm(wavelength_mm),
        &option,
        &mut common.buffer.0,
    );
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_gs(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    repeat: usize,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    buffer: *mut PatternBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(repeat) = NonZeroUsize::new(repeat) else {
        unsafe { write_cstr(out_err, out_err_len, "repeat must be >= 1") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let common = match unsafe {
        prepare(
            geometry,
            foci,
            num_foci,
            constraint,
            directivity,
            mask,
            buffer,
            out_err,
            out_err_len,
        )
    } {
        Ok(common) => common,
        Err(code) => return code,
    };
    let option = GsOption {
        repeat,
        constraint: common.constraint,
        directivity: common.directivity,
        mask: mask_ref(common.mask.as_deref()),
        parallel,
    };
    let result = gs(
        &NalgebraBackend,
        common.geometry,
        &common.foci,
        Length::from_mm(wavelength_mm),
        &option,
        &mut common.buffer.0,
    );
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_gspat(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    repeat: usize,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    buffer: *mut PatternBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(repeat) = NonZeroUsize::new(repeat) else {
        unsafe { write_cstr(out_err, out_err_len, "repeat must be >= 1") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let common = match unsafe {
        prepare(
            geometry,
            foci,
            num_foci,
            constraint,
            directivity,
            mask,
            buffer,
            out_err,
            out_err_len,
        )
    } {
        Ok(common) => common,
        Err(code) => return code,
    };
    let option = GspatOption {
        repeat,
        constraint: common.constraint,
        directivity: common.directivity,
        mask: mask_ref(common.mask.as_deref()),
        parallel,
    };
    let result = gspat(
        &NalgebraBackend,
        common.geometry,
        &common.foci,
        Length::from_mm(wavelength_mm),
        &option,
        &mut common.buffer.0,
    );
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_greedy(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    phase_quantization_levels: u8,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    buffer: *mut PatternBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(phase_quantization_levels) = NonZeroU8::new(phase_quantization_levels) else {
        unsafe {
            write_cstr(
                out_err,
                out_err_len,
                "phase_quantization_levels must be >= 1",
            );
        }
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let common = match unsafe {
        prepare(
            geometry,
            foci,
            num_foci,
            constraint,
            directivity,
            mask,
            buffer,
            out_err,
            out_err_len,
        )
    } {
        Ok(common) => common,
        Err(code) => return code,
    };
    let option = GreedyOption {
        phase_quantization_levels,
        constraint: common.constraint,
        directivity: common.directivity,
        objective_func: abs_objective_func,
        mask: mask_ref(common.mask.as_deref()),
    };
    let result = greedy(
        common.geometry,
        &common.foci,
        Length::from_mm(wavelength_mm),
        &option,
        &mut common.buffer.0,
    );
    unsafe { finish(result, out_err, out_err_len) }
}

autd3_ffi_abi::export_abi_version!();
