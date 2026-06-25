use autd3_ffi_abi::{PatternBuffer, drop_handle, into_handle};
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_core::{Angle, Geometry, Length, Point3, UnitVector3, Vector3, Velocity};
use autd3_rs_pattern::{BesselOption, FocusOption, PlaneOption};

#[repr(C)]
pub struct Autd3Emission {
    pub phase: u8,
    pub intensity: u8,
}

#[repr(C)]
pub struct Autd3PatternOption {
    pub intensity: u8,
    pub phase_offset: u8,
}

impl Autd3PatternOption {
    fn intensity(&self) -> Intensity {
        Intensity(self.intensity)
    }

    fn phase_offset(&self) -> Phase {
        Phase(self.phase_offset)
    }
}

unsafe fn point(p: *const f32) -> Point3<f32> {
    let p = unsafe { std::slice::from_raw_parts(p, 3) };
    Point3::new(p[0], p[1], p[2])
}

unsafe fn unit_vector(p: *const f32) -> UnitVector3<f32> {
    let p = unsafe { std::slice::from_raw_parts(p, 3) };
    UnitVector3::new_normalize(Vector3::new(p[0], p[1], p[2]))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_pattern_wavelength(sound_speed_mm_per_s: f32) -> f32 {
    autd3_rs_pattern::wavelength(Velocity::from_mm_s(sound_speed_mm_per_s)).mm()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_pattern_buffer_new(num_devices: usize) -> *mut PatternBuffer {
    into_handle(PatternBuffer(vec![
        [Emission::default(); NUM_TRANSDUCERS];
        num_devices
    ]))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_from_array(
    emissions: *const Autd3Emission,
    num_devices: usize,
) -> *mut PatternBuffer {
    if emissions.is_null() {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(emissions, num_devices * NUM_TRANSDUCERS) };
    let buffer = slice
        .chunks_exact(NUM_TRANSDUCERS)
        .map(|device| {
            let mut slot = [Emission::default(); NUM_TRANSDUCERS];
            for (e, src) in slot.iter_mut().zip(device) {
                *e = Emission {
                    phase: Phase(src.phase),
                    intensity: Intensity(src.intensity),
                };
            }
            slot
        })
        .collect();
    into_handle(PatternBuffer(buffer))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_num_devices(buffer: *const PatternBuffer) -> usize {
    if buffer.is_null() {
        return 0;
    }

    unsafe { &*buffer }.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_free(buffer: *mut PatternBuffer) {
    unsafe { drop_handle(buffer) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_focus(
    geometry: *const Geometry,
    target: *const f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    buffer: *mut PatternBuffer,
) -> i32 {
    if geometry.is_null() || target.is_null() || option.is_null() || buffer.is_null() {
        return -1;
    }

    let geometry = unsafe { &*geometry };
    let target = unsafe { point(target) };
    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    if buffer.0.len() != geometry.len() {
        return -1;
    }
    autd3_rs_pattern::focus(
        geometry,
        target,
        Length::millimeters(wavelength_mm),
        &FocusOption {
            intensity: option.intensity(),
            phase_offset: option.phase_offset(),
        },
        &mut buffer.0,
    );
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_plane(
    geometry: *const Geometry,
    dir: *const f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    buffer: *mut PatternBuffer,
) -> i32 {
    if geometry.is_null() || dir.is_null() || option.is_null() || buffer.is_null() {
        return -1;
    }

    let geometry = unsafe { &*geometry };
    let dir = unsafe { unit_vector(dir) };
    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    if buffer.0.len() != geometry.len() {
        return -1;
    }
    autd3_rs_pattern::plane(
        geometry,
        dir,
        Length::millimeters(wavelength_mm),
        &PlaneOption {
            intensity: option.intensity(),
            phase_offset: option.phase_offset(),
        },
        &mut buffer.0,
    );
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_bessel(
    geometry: *const Geometry,
    apex: *const f32,
    dir: *const f32,
    theta_rad: f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    buffer: *mut PatternBuffer,
) -> i32 {
    if geometry.is_null() || apex.is_null() || dir.is_null() || option.is_null() || buffer.is_null()
    {
        return -1;
    }

    let geometry = unsafe { &*geometry };
    let apex = unsafe { point(apex) };
    let dir = unsafe { unit_vector(dir) };
    let option = unsafe { &*option };
    let buffer = unsafe { &mut *buffer };
    if buffer.0.len() != geometry.len() {
        return -1;
    }
    autd3_rs_pattern::bessel(
        geometry,
        apex,
        dir,
        Angle::from_radian(theta_rad),
        Length::millimeters(wavelength_mm),
        &BesselOption {
            intensity: option.intensity(),
            phase_offset: option.phase_offset(),
        },
        &mut buffer.0,
    );
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_uniform(
    phase: u8,
    intensity: u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    if buffer.is_null() {
        return -1;
    }

    let buffer = unsafe { &mut *buffer };
    autd3_rs_pattern::uniform(
        Emission {
            phase: Phase(phase),
            intensity: Intensity(intensity),
        },
        &mut buffer.0,
    );
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_null(buffer: *mut PatternBuffer) {
    if buffer.is_null() {
        return;
    }

    autd3_rs_pattern::null(&mut unsafe { &mut *buffer }.0);
}
