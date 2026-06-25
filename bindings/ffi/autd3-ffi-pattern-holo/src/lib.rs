use std::num::{NonZeroU8, NonZeroUsize};

use autd3_ffi_abi::PatternBuffer;
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::Intensity;
use autd3_rs_core::{Geometry, Length, Point3};
use autd3_rs_pattern_holo::{
    ControlPoint, Directivity, EmissionConstraint, GreedyOption, GsOption, GspatOption,
    NaiveOption, NalgebraBackend, Pa, TransducerMask, abs_objective_func, dB, greedy, gs, gspat,
    kPa, naive,
};

#[repr(C)]
pub struct Autd3HoloControlPoint {
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

fn to_directivity(d: u8) -> Directivity {
    if d == 1 {
        Directivity::T4010A1
    } else {
        Directivity::Sphere
    }
}

fn to_constraint(c: &Autd3EmissionConstraint) -> EmissionConstraint {
    match c.kind {
        1 => EmissionConstraint::Multiply(c.multiply),
        2 => EmissionConstraint::Uniform(Intensity(c.min)),
        3 => EmissionConstraint::Clamp(Intensity(c.min), Intensity(c.max)),
        _ => EmissionConstraint::Normalize,
    }
}

unsafe fn build_foci(foci: *const Autd3HoloControlPoint, num_foci: usize) -> Vec<ControlPoint> {
    let slice = unsafe { std::slice::from_raw_parts(foci, num_foci) };
    slice
        .iter()
        .map(|f| ControlPoint {
            point: Point3::new(f.point[0], f.point[1], f.point[2]),
            amplitude: f.amplitude_pa * Pa,
        })
        .collect()
}

unsafe fn build_mask(mask: *const u8, num_devices: usize) -> Option<Vec<[bool; NUM_TRANSDUCERS]>> {
    if mask.is_null() {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(mask, num_devices * NUM_TRANSDUCERS) };
    Some(
        slice
            .chunks_exact(NUM_TRANSDUCERS)
            .map(|device| {
                let mut slot = [false; NUM_TRANSDUCERS];
                for (m, src) in slot.iter_mut().zip(device) {
                    *m = *src != 0;
                }
                slot
            })
            .collect(),
    )
}

fn mask_ref(mask: Option<&[[bool; NUM_TRANSDUCERS]]>) -> TransducerMask<'_> {
    match mask {
        Some(m) => TransducerMask::Masked(m),
        None => TransducerMask::AllEnabled,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_holo_naive(
    geometry: *const Geometry,
    foci: *const Autd3HoloControlPoint,
    num_foci: usize,
    wavelength_mm: f32,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    if geometry.is_null() || foci.is_null() || constraint.is_null() || buffer.is_null() {
        return -1;
    }
    let geometry = unsafe { &*geometry };
    let buffer = unsafe { &mut *buffer };
    if buffer.0.len() != geometry.len() {
        return -1;
    }
    let foci = unsafe { build_foci(foci, num_foci) };
    let mask = unsafe { build_mask(mask, buffer.0.len()) };
    let option = NaiveOption {
        constraint: to_constraint(unsafe { &*constraint }),
        directivity: to_directivity(directivity),
    };
    match naive(
        geometry,
        &foci,
        Length::millimeters(wavelength_mm),
        &option,
        &NalgebraBackend,
        mask_ref(mask.as_deref()),
        &mut buffer.0,
    ) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_holo_gs(
    geometry: *const Geometry,
    foci: *const Autd3HoloControlPoint,
    num_foci: usize,
    wavelength_mm: f32,
    repeat: usize,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    if geometry.is_null() || foci.is_null() || constraint.is_null() || buffer.is_null() {
        return -1;
    }
    let geometry = unsafe { &*geometry };
    let buffer = unsafe { &mut *buffer };
    if buffer.0.len() != geometry.len() {
        return -1;
    }
    let foci = unsafe { build_foci(foci, num_foci) };
    let mask = unsafe { build_mask(mask, buffer.0.len()) };
    let option = GsOption {
        repeat: NonZeroUsize::new(repeat).unwrap_or(NonZeroUsize::new(100).unwrap()),
        constraint: to_constraint(unsafe { &*constraint }),
        directivity: to_directivity(directivity),
    };
    match gs(
        geometry,
        &foci,
        Length::millimeters(wavelength_mm),
        &option,
        &NalgebraBackend,
        mask_ref(mask.as_deref()),
        &mut buffer.0,
    ) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_holo_gspat(
    geometry: *const Geometry,
    foci: *const Autd3HoloControlPoint,
    num_foci: usize,
    wavelength_mm: f32,
    repeat: usize,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    if geometry.is_null() || foci.is_null() || constraint.is_null() || buffer.is_null() {
        return -1;
    }
    let geometry = unsafe { &*geometry };
    let buffer = unsafe { &mut *buffer };
    if buffer.0.len() != geometry.len() {
        return -1;
    }
    let foci = unsafe { build_foci(foci, num_foci) };
    let mask = unsafe { build_mask(mask, buffer.0.len()) };
    let option = GspatOption {
        repeat: NonZeroUsize::new(repeat).unwrap_or(NonZeroUsize::new(100).unwrap()),
        constraint: to_constraint(unsafe { &*constraint }),
        directivity: to_directivity(directivity),
    };
    match gspat(
        geometry,
        &foci,
        Length::millimeters(wavelength_mm),
        &option,
        &NalgebraBackend,
        mask_ref(mask.as_deref()),
        &mut buffer.0,
    ) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_holo_greedy(
    geometry: *const Geometry,
    foci: *const Autd3HoloControlPoint,
    num_foci: usize,
    wavelength_mm: f32,
    phase_quantization_levels: u8,
    constraint: *const Autd3EmissionConstraint,
    directivity: u8,
    mask: *const u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    if geometry.is_null() || foci.is_null() || constraint.is_null() || buffer.is_null() {
        return -1;
    }
    let geometry = unsafe { &*geometry };
    let buffer = unsafe { &mut *buffer };
    if buffer.0.len() != geometry.len() {
        return -1;
    }
    let foci = unsafe { build_foci(foci, num_foci) };
    let mask = unsafe { build_mask(mask, buffer.0.len()) };
    let option = GreedyOption {
        phase_quantization_levels: NonZeroU8::new(phase_quantization_levels)
            .unwrap_or(NonZeroU8::new(16).unwrap()),
        constraint: to_constraint(unsafe { &*constraint }),
        directivity: to_directivity(directivity),
        objective_func: abs_objective_func,
    };
    match greedy(
        geometry,
        &foci,
        Length::millimeters(wavelength_mm),
        &option,
        mask_ref(mask.as_deref()),
        &mut buffer.0,
    ) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}
