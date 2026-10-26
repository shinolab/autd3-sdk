use std::ffi::c_char;
use std::num::{NonZeroU8, NonZeroUsize};

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, IntensityBuffer, PhaseBuffer, handle_mut, handle_ref,
    slice_ref, write_cstr, write_out,
};
use autd3_rs_core::geometry::{Autd3, TransducerMask};
use autd3_rs_core::value::{Intensity, PatternIntensity};
use autd3_rs_core::{Geometry, Length, Point3};
use autd3_rs_pattern_holo::{
    AmplitudeTarget, Directivity, GreedyOption, GsOption, GspatOption, IntensityConstraint,
    NaiveOption, NalgebraBackend, Pa, abs_objective_func, dB, greedy, gs, gspat, kPa, naive,
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
pub extern "C" fn autd3_holo_amplitude_from_db(value: f32) -> f32 {
    (value * dB).pascal()
}

unsafe fn finish(
    result: Result<(), autd3_rs_pattern_holo::HoloError>,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    match result {
        Ok(()) => AUTD3_OK,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            AUTD3_ERR_INVALID_ARGUMENT
        }
    }
}

fn from_directivity(d: Directivity) -> u8 {
    match d {
        Directivity::Sphere => 0,
        Directivity::T4010A1 => 1,
        _ => unreachable!(),
    }
}

fn from_constraint(c: IntensityConstraint) -> Autd3IntensityConstraint {
    let (kind, min, max, multiply) = match c {
        IntensityConstraint::Multiply(value) => (0, 0, 0, value),
        IntensityConstraint::Clamp(min, max) => (1, min.0, max.0, 0.0),
        _ => unreachable!(),
    };
    Autd3IntensityConstraint {
        kind,
        min,
        max,
        multiply,
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
pub unsafe extern "C" fn autd3_holo_naive_option_default(
    out_constraint: *mut Autd3IntensityConstraint,
    out_directivity: *mut u8,
) -> i32 {
    let default = NaiveOption::default();
    first_error(&[
        unsafe { write_out(out_constraint, from_constraint(default.constraint)) },
        unsafe { write_out(out_directivity, from_directivity(default.directivity)) },
    ])
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_holo_gs_option_default(
    out_repeat: *mut usize,
    out_constraint: *mut Autd3IntensityConstraint,
    out_directivity: *mut u8,
) -> i32 {
    let default = GsOption::default();
    first_error(&[
        unsafe { write_out(out_repeat, default.repeat.get()) },
        unsafe { write_out(out_constraint, from_constraint(default.constraint)) },
        unsafe { write_out(out_directivity, from_directivity(default.directivity)) },
    ])
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_holo_gspat_option_default(
    out_repeat: *mut usize,
    out_constraint: *mut Autd3IntensityConstraint,
    out_directivity: *mut u8,
) -> i32 {
    let default = GspatOption::default();
    first_error(&[
        unsafe { write_out(out_repeat, default.repeat.get()) },
        unsafe { write_out(out_constraint, from_constraint(default.constraint)) },
        unsafe { write_out(out_directivity, from_directivity(default.directivity)) },
    ])
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_holo_greedy_option_default(
    out_phase_quantization_levels: *mut u8,
    out_directivity: *mut u8,
) -> i32 {
    let default = GreedyOption::default();
    first_error(&[
        unsafe {
            write_out(
                out_phase_quantization_levels,
                default.phase_quantization_levels.get(),
            )
        },
        unsafe { write_out(out_directivity, from_directivity(default.directivity)) },
    ])
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
        0 => Some(IntensityConstraint::Multiply(c.multiply)),
        1 => Some(IntensityConstraint::Clamp(
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

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_holo_greedy(
    geometry: *const Geometry,
    foci: *const Autd3HoloAmplitudeTarget,
    num_foci: usize,
    wavelength_mm: f32,
    intensities: *const IntensityBuffer,
    uniform_intensity: u8,
    order: *const u32,
    order_len: usize,
    phase_quantization_levels: u8,
    directivity: u8,
    dst: *mut PhaseBuffer,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let fail = |message: &str| {
        unsafe { write_cstr(out_err, out_err_len, message) };
        AUTD3_ERR_INVALID_ARGUMENT
    };

    let Some(phase_quantization_levels) = NonZeroU8::new(phase_quantization_levels) else {
        return fail("phase_quantization_levels must be >= 1");
    };
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return fail("null geometry");
    };
    let intensities = match unsafe { handle_ref(intensities) } {
        Some(intensities) => PatternIntensity::PerDevice(&intensities.0),
        None => PatternIntensity::Uniform(Intensity(uniform_intensity)),
    };
    let Some(dst) = (unsafe { handle_mut(dst) }) else {
        return fail("null phase buffer");
    };
    let Some(directivity) = to_directivity(directivity) else {
        return fail("unknown directivity");
    };
    let Some(foci) = (unsafe { slice_ref(foci, num_foci) }) else {
        return fail("null foci");
    };
    let Some(order) = (unsafe { slice_ref(order, order_len) }) else {
        return fail("null order");
    };
    let order: Vec<usize> = order.iter().map(|&index| index as usize).collect();
    let foci = build_foci(foci);
    let option = GreedyOption {
        phase_quantization_levels,
        directivity,
        objective_func: abs_objective_func,
    };
    let result = greedy(
        geometry,
        &foci,
        Length::from_mm(wavelength_mm),
        intensities,
        &order,
        &option,
        &mut dst.0,
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

    static MULTIPLY_ONE: Autd3IntensityConstraint = Autd3IntensityConstraint {
        kind: 0,
        min: 0,
        max: 0,
        multiply: 1.0,
    };

    fn message(err: &[c_char]) -> String {
        unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn a_mismatched_buffer_reaches_the_rust_validation() {
        let geometry = geometry();
        let foci = foci();
        let mut err = [0 as c_char; 256];
        let mut phases =
            Buffer(Geometry::new(vec![Autd3::default(), Autd3::default()]).phase_buffer());
        let mut intensities = Buffer(geometry.intensity_buffer());
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_holo_naive(
                &raw const geometry,
                foci.as_ptr(),
                foci.len(),
                8.5,
                &raw const MULTIPLY_ONE,
                0,
                std::ptr::null(),
                &raw mut phases,
                &raw mut intensities,
                err.as_mut_ptr(),
                err.len(),
            )
        });
        assert_eq!(
            message(&err),
            autd3_rs_pattern_holo::HoloError::DstDeviceCountMismatch {
                got: 2,
                expected: 1
            }
            .to_string()
        );
    }

    #[test]
    fn a_holo_error_is_an_invalid_argument() {
        let geometry = geometry();
        let mut err = [0 as c_char; 256];
        let mut phases = Buffer(geometry.phase_buffer());
        let mut intensities = Buffer(geometry.intensity_buffer());
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_holo_gspat(
                &raw const geometry,
                std::ptr::null(),
                0,
                8.5,
                10,
                &raw const MULTIPLY_ONE,
                0,
                std::ptr::null(),
                &raw mut phases,
                &raw mut intensities,
                err.as_mut_ptr(),
                err.len(),
            )
        });
        assert_eq!(
            message(&err),
            autd3_rs_pattern_holo::HoloError::NoFoci.to_string()
        );
    }

    #[test]
    fn greedy_follows_the_given_order() {
        let geometry = geometry();
        let foci = foci();
        let mut err = [0 as c_char; 256];
        let order: Vec<u32> = (0..Autd3::NUM_TRANSDUCERS)
            .rev()
            .map(|i| u32::try_from(i).unwrap())
            .collect();
        let run = |order: &[u32], dst: &mut PhaseBuffer, err: &mut [c_char]| unsafe {
            autd3_holo_greedy(
                &raw const geometry,
                foci.as_ptr(),
                foci.len(),
                8.5,
                std::ptr::null(),
                u8::MAX,
                order.as_ptr(),
                order.len(),
                16,
                0,
                dst,
                err.as_mut_ptr(),
                err.len(),
            )
        };

        let mut dst = Buffer(geometry.phase_buffer());
        assert_eq!(AUTD3_OK, run(&order, &mut dst, &mut err));
        let mut expected = geometry.phase_buffer();
        greedy(
            &geometry,
            &build_foci(&foci),
            Length::from_mm(8.5),
            Intensity::MAX,
            &order.iter().map(|&i| i as usize).collect::<Vec<_>>(),
            &GreedyOption::default(),
            &mut expected,
        )
        .unwrap();
        assert_eq!(expected, dst.0);

        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_holo_greedy(
                &raw const geometry,
                foci.as_ptr(),
                foci.len(),
                8.5,
                std::ptr::null(),
                u8::MAX,
                std::ptr::null(),
                Autd3::NUM_TRANSDUCERS,
                16,
                0,
                &raw mut dst,
                err.as_mut_ptr(),
                err.len(),
            )
        });
        assert_eq!(message(&err), "null order");
    }

    #[test]
    fn the_option_defaults_match_the_rust_default() {
        let mut constraint = Autd3IntensityConstraint {
            kind: u8::MAX,
            min: 0,
            max: 0,
            multiply: 0.0,
        };
        let mut directivity = u8::MAX;
        let mut repeat = 0usize;
        assert_eq!(AUTD3_OK, unsafe {
            autd3_holo_gs_option_default(&raw mut repeat, &raw mut constraint, &raw mut directivity)
        });
        let default = GsOption::default();
        assert_eq!(default.repeat.get(), repeat);
        assert_eq!(Some(default.constraint), to_constraint(&constraint));
        assert_eq!(Some(default.directivity), to_directivity(directivity));

        assert_eq!(AUTD3_OK, unsafe {
            autd3_holo_naive_option_default(&raw mut constraint, &raw mut directivity)
        });
        let default = NaiveOption::default();
        assert_eq!(Some(default.constraint), to_constraint(&constraint));

        assert_eq!(AUTD3_OK, unsafe {
            autd3_holo_gspat_option_default(
                &raw mut repeat,
                &raw mut constraint,
                &raw mut directivity,
            )
        });
        assert_eq!(GspatOption::default().repeat.get(), repeat);

        let mut levels = 0u8;
        assert_eq!(AUTD3_OK, unsafe {
            autd3_holo_greedy_option_default(&raw mut levels, &raw mut directivity)
        });
        let default = GreedyOption::default();
        assert_eq!(default.phase_quantization_levels.get(), levels);
        assert_eq!(Some(default.directivity), to_directivity(directivity));

        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_holo_greedy_option_default(std::ptr::null_mut(), &raw mut directivity)
        });
        for c in [
            IntensityConstraint::Multiply(0.5),
            IntensityConstraint::Clamp(Intensity(1), Intensity(2)),
        ] {
            assert_eq!(Some(c), to_constraint(&from_constraint(c)));
        }
    }
}
