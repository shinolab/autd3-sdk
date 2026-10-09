use std::ffi::c_char;
use std::num::{NonZeroU8, NonZeroUsize};

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, IntensityBuffer, PhaseBuffer, finish, handle_mut, handle_ref,
    slice_ref, write_cstr,
};
use autd3_rs_core::geometry::{Autd3, TransducerMask};
use autd3_rs_core::value::{Intensity, PatternIntensity, Phase};
use autd3_rs_core::{Geometry, Length, Point3};
use autd3_rs_pattern_holo::{
    AmplitudeTarget, Directivity, GreedyOption, GsOption, GspatOption, IntensityConstraint,
    NaiveOption, NalgebraBackend, Pa, abs_objective_func, dB, greedy, gs, gs_batch, gspat,
    gspat_batch, kPa, naive, naive_batch,
};

#[repr(C)]
pub struct Autd3HoloAmplitudeTarget {
    pub point: [f32; 3],
    pub amplitude_pa: f32,
}

#[repr(C)]
pub struct Autd3IntensityConstraint {
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

fn to_constraint(c: &Autd3IntensityConstraint) -> Option<IntensityConstraint> {
    match c.kind {
        0 => Some(IntensityConstraint::Normalize),
        1 => Some(IntensityConstraint::Multiply(c.multiply)),
        2 => Some(IntensityConstraint::Uniform(Intensity(c.min))),
        3 => Some(IntensityConstraint::Clamp(
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
    phases: &'a mut PhaseBuffer,
    intensities: &'a mut IntensityBuffer,
    foci: Vec<AmplitudeTarget>,
    mask: Option<Vec<Vec<bool>>>,
    constraint: IntensityConstraint,
    directivity: Directivity,
}

#[allow(clippy::too_many_arguments)]
unsafe fn prepare<'a>(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    phases: *mut PhaseBuffer,
    intensities: *mut IntensityBuffer,
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
    if std::ptr::eq(phases.cast::<u8>(), intensities.cast::<u8>()) {
        return Err(fail("the phase and intensity buffers must be distinct"));
    }
    let Some(phases) = (unsafe { handle_mut(phases) }) else {
        return Err(fail("null phase buffer"));
    };
    let Some(intensities) = (unsafe { handle_mut(intensities) }) else {
        return Err(fail("null intensity buffer"));
    };
    if phases.0.len() != geometry.num_devices() || intensities.0.len() != geometry.num_devices() {
        return Err(fail("the buffer length does not match the geometry"));
    }
    let Some(constraint) = (unsafe { handle_ref(constraint) }) else {
        return Err(fail("null constraint"));
    };
    let Some(constraint) = to_constraint(constraint) else {
        return Err(fail("unknown intensity constraint"));
    };
    let Some(directivity) = to_directivity(directivity) else {
        return Err(fail("unknown directivity"));
    };
    let Some(foci) = (unsafe { slice_ref(foci, num_foci) }) else {
        return Err(fail("null foci"));
    };
    let foci = build_foci(foci);
    let mask = unsafe { build_mask(mask, geometry.num_devices()) };
    Ok(Common {
        geometry,
        phases,
        intensities,
        foci,
        mask,
        constraint,
        directivity,
    })
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_naive(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    phases: *mut PhaseBuffer,
    intensities: *mut IntensityBuffer,
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
            phases,
            intensities,
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
        &mut common.phases.0,
        &mut common.intensities.0,
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
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    phases: *mut PhaseBuffer,
    intensities: *mut IntensityBuffer,
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
            phases,
            intensities,
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
        &mut common.phases.0,
        &mut common.intensities.0,
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
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    phases: *mut PhaseBuffer,
    intensities: *mut IntensityBuffer,
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
            phases,
            intensities,
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
        &mut common.phases.0,
        &mut common.intensities.0,
    );
    unsafe { finish(result, out_err, out_err_len) }
}

enum BatchSolver {
    Naive,
    Gs(NonZeroUsize),
    Gspat(NonZeroUsize),
}

unsafe fn take_buffers<T>(
    handles: *const *mut autd3_ffi_abi::Buffer<T>,
    len: usize,
    num_devices: usize,
) -> Option<Vec<Vec<Vec<T>>>> {
    let handles = unsafe { slice_ref(handles, len) }?;
    let distinct: std::collections::HashSet<_> = handles.iter().collect();
    if distinct.len() != handles.len() {
        return None;
    }
    let mut buffers = Vec::with_capacity(len);
    for &handle in handles {
        let buffer = unsafe { handle_mut(handle) }?;
        if buffer.0.len() != num_devices {
            return None;
        }
        buffers.push(handle);
    }
    Some(
        buffers
            .into_iter()
            .filter_map(|handle| unsafe { handle_mut(handle) })
            .map(|buffer| std::mem::take(&mut buffer.0))
            .collect(),
    )
}

unsafe fn restore_buffers<T>(
    handles: *const *mut autd3_ffi_abi::Buffer<T>,
    len: usize,
    buffers: Vec<Vec<Vec<T>>>,
) {
    let Some(handles) = (unsafe { slice_ref(handles, len) }) else {
        return;
    };
    for (&handle, buffer) in handles.iter().zip(buffers) {
        if let Some(slot) = unsafe { handle_mut(handle) } {
            slot.0 = buffer;
        }
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn solve_batch(
    solver: &BatchSolver,
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    phases: *const *mut PhaseBuffer,
    intensities: *const *mut IntensityBuffer,
    num_problems: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let fail = |message: &str| {
        unsafe { write_cstr(out_err, out_err_len, message) };
        AUTD3_ERR_INVALID_ARGUMENT
    };

    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return fail("null geometry");
    };
    let Some(constraint) = (unsafe { handle_ref(constraint) }).and_then(to_constraint) else {
        return fail("null or unknown intensity constraint");
    };
    let Some(directivity) = to_directivity(directivity) else {
        return fail("unknown directivity");
    };
    let Some(foci) = (unsafe { slice_ref(foci, num_foci) }) else {
        return fail("null foci");
    };
    let foci = build_foci(foci);
    let mask = unsafe { build_mask(mask, geometry.num_devices()) };
    let mask = mask_ref(mask.as_deref());

    let Some(mut phase_buffers): Option<Vec<Vec<Vec<Phase>>>> =
        (unsafe { take_buffers(phases, num_problems, geometry.num_devices()) })
    else {
        return fail("the phase buffers must be distinct, non-null and match the geometry");
    };
    let Some(mut intensity_buffers): Option<Vec<Vec<Vec<Intensity>>>> =
        (unsafe { take_buffers(intensities, num_problems, geometry.num_devices()) })
    else {
        unsafe { restore_buffers(phases, num_problems, phase_buffers) };
        return fail("the intensity buffers must be distinct, non-null and match the geometry");
    };

    let wavelength = Length::from_mm(wavelength_mm);
    let result = match *solver {
        BatchSolver::Naive => naive_batch(
            &NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &NaiveOption {
                constraint,
                directivity,
                mask,
                parallel,
            },
            &mut phase_buffers,
            &mut intensity_buffers,
        ),
        BatchSolver::Gs(repeat) => gs_batch(
            &NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &GsOption {
                repeat,
                constraint,
                directivity,
                mask,
                parallel,
            },
            &mut phase_buffers,
            &mut intensity_buffers,
        ),
        BatchSolver::Gspat(repeat) => gspat_batch(
            &NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &GspatOption {
                repeat,
                constraint,
                directivity,
                mask,
                parallel,
            },
            &mut phase_buffers,
            &mut intensity_buffers,
        ),
    };
    unsafe { restore_buffers(phases, num_problems, phase_buffers) };
    unsafe { restore_buffers(intensities, num_problems, intensity_buffers) };
    unsafe { finish(result, out_err, out_err_len) }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_naive_batch(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    phases: *const *mut PhaseBuffer,
    intensities: *const *mut IntensityBuffer,
    num_problems: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    unsafe {
        solve_batch(
            &BatchSolver::Naive,
            geometry,
            foci,
            num_foci,
            wavelength_mm,
            constraint,
            directivity,
            mask,
            parallel,
            phases,
            intensities,
            num_problems,
            out_err,
            out_err_len,
        )
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_gs_batch(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    repeat: usize,
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    phases: *const *mut PhaseBuffer,
    intensities: *const *mut IntensityBuffer,
    num_problems: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(repeat) = NonZeroUsize::new(repeat) else {
        unsafe { write_cstr(out_err, out_err_len, "repeat must be >= 1") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe {
        solve_batch(
            &BatchSolver::Gs(repeat),
            geometry,
            foci,
            num_foci,
            wavelength_mm,
            constraint,
            directivity,
            mask,
            parallel,
            phases,
            intensities,
            num_problems,
            out_err,
            out_err_len,
        )
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_gspat_batch(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    repeat: usize,
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    parallel: bool,
    phases: *const *mut PhaseBuffer,
    intensities: *const *mut IntensityBuffer,
    num_problems: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(repeat) = NonZeroUsize::new(repeat) else {
        unsafe { write_cstr(out_err, out_err_len, "repeat must be >= 1") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe {
        solve_batch(
            &BatchSolver::Gspat(repeat),
            geometry,
            foci,
            num_foci,
            wavelength_mm,
            constraint,
            directivity,
            mask,
            parallel,
            phases,
            intensities,
            num_problems,
            out_err,
            out_err_len,
        )
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_greedy(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    phase_quantization_levels: u8,
    constraint: *const Autd3IntensityConstraint,
    directivity: u8,
    mask: *const u8,
    phases: *mut PhaseBuffer,
    intensities: *mut IntensityBuffer,
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
            phases,
            intensities,
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
        &mut common.phases.0,
        &mut common.intensities.0,
    );
    unsafe { finish(result, out_err, out_err_len) }
}

autd3_ffi_abi::export_abi_version!();

#[cfg(test)]
mod tests {
    use super::*;
    use autd3_ffi_abi::Buffer;

    fn geometry() -> Geometry {
        Geometry::new(vec![Autd3::default()])
    }

    fn foci() -> Vec<Autd3HoloAmplitudeTarget> {
        [-30.0f32, 30.0, -10.0, 10.0]
            .into_iter()
            .map(|x| Autd3HoloAmplitudeTarget {
                point: [x, 0.0, 150.0],
                amplitude_pa: 2500.0,
            })
            .collect()
    }

    static NORMALIZE: Autd3IntensityConstraint = Autd3IntensityConstraint {
        kind: 0,
        min: 0,
        max: 0,
        multiply: 0.0,
    };

    #[test]
    fn a_batch_matches_solving_each_problem_alone() {
        let geometry = geometry();
        let foci = foci();
        let mut err = [0 as c_char; 256];

        let mut phases = [
            Buffer(geometry.phase_buffer()),
            Buffer(geometry.phase_buffer()),
        ];
        let mut intensities = [
            Buffer(geometry.intensity_buffer()),
            Buffer(geometry.intensity_buffer()),
        ];
        let [p0, p1] = &mut phases;
        let [i0, i1] = &mut intensities;
        let phase_handles = [&raw mut *p0, &raw mut *p1];
        let intensity_handles = [&raw mut *i0, &raw mut *i1];
        assert_eq!(0, unsafe {
            autd3_holo_naive_batch(
                &raw const geometry,
                foci.as_ptr(),
                foci.len(),
                8.5,
                &raw const NORMALIZE,
                0,
                std::ptr::null(),
                false,
                phase_handles.as_ptr(),
                intensity_handles.as_ptr(),
                2,
                err.as_mut_ptr(),
                err.len(),
            )
        });

        for (problem, chunk) in foci.chunks(2).enumerate() {
            let mut single_phases = Buffer(geometry.phase_buffer());
            let mut single_intensities = Buffer(geometry.intensity_buffer());
            assert_eq!(0, unsafe {
                autd3_holo_naive(
                    &raw const geometry,
                    chunk.as_ptr(),
                    chunk.len(),
                    8.5,
                    &raw const NORMALIZE,
                    0,
                    std::ptr::null(),
                    false,
                    &raw mut single_phases,
                    &raw mut single_intensities,
                    err.as_mut_ptr(),
                    err.len(),
                )
            });
            assert_eq!(single_phases.0, phases[problem].0);
            assert_eq!(single_intensities.0, intensities[problem].0);
        }
    }

    #[test]
    fn a_rejected_batch_leaves_the_buffers_intact() {
        let geometry = geometry();
        let foci = foci();
        let mut err = [0 as c_char; 256];
        let mut phases = Buffer(geometry.phase_buffer());
        let mut intensities = Buffer(geometry.intensity_buffer());
        let phase_handles = [&raw mut phases, &raw mut phases];
        let intensity_handles = [&raw mut intensities, &raw mut intensities];
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_holo_gs_batch(
                &raw const geometry,
                foci.as_ptr(),
                foci.len(),
                8.5,
                10,
                &raw const NORMALIZE,
                0,
                std::ptr::null(),
                false,
                phase_handles.as_ptr(),
                intensity_handles.as_ptr(),
                2,
                err.as_mut_ptr(),
                err.len(),
            )
        });
        assert_eq!(1, phases.0.len());
        assert_eq!(1, intensities.0.len());
    }
}
