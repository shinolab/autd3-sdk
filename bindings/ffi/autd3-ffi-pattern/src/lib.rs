use autd3_ffi_abi::{
    PatternBuffer, drop_handle, handle_mut, handle_ref, into_handle, slice_mut, slice_ref,
    write_out,
};
use autd3_rs_core::geometry::Autd3;
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

unsafe fn point(p: *const f32) -> Option<Point3<f32>> {
    let p = unsafe { slice_ref(p, 3) }?;
    Some(Point3::new(p[0], p[1], p[2]))
}

unsafe fn unit_vector(p: *const f32) -> Option<UnitVector3<f32>> {
    let p = unsafe { slice_ref(p, 3) }?;
    Some(UnitVector3::new_normalize(Vector3::new(p[0], p[1], p[2])))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_pattern_wavelength(sound_speed_mm_per_s: f32) -> f32 {
    autd3_rs_pattern::wavelength(Velocity::from_mm_s(sound_speed_mm_per_s)).mm()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_core_geometry_pattern_buffer(
    geometry: *const Geometry,
) -> *mut PatternBuffer {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return std::ptr::null_mut();
    };

    into_handle(PatternBuffer(geometry.pattern_buffer()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_from_array(
    emissions: *const Autd3Emission,
    num_devices: usize,
) -> *mut PatternBuffer {
    let Some(slice) = (unsafe { slice_ref(emissions, num_devices * Autd3::NUM_TRANSDUCERS) })
    else {
        return std::ptr::null_mut();
    };

    let buffer = slice
        .chunks_exact(Autd3::NUM_TRANSDUCERS)
        .map(|device| {
            let mut slot = vec![Emission::default(); Autd3::NUM_TRANSDUCERS];
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
    let Some(buffer) = (unsafe { handle_ref(buffer) }) else {
        return 0;
    };

    buffer.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_num_transducers(
    buffer: *const PatternBuffer,
    dev: usize,
) -> usize {
    let Some(buffer) = (unsafe { handle_ref(buffer) }) else {
        return 0;
    };

    buffer.0.get(dev).map_or(0, Vec::len)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_get(
    buffer: *const PatternBuffer,
    dev: usize,
    tr: usize,
    out: *mut Autd3Emission,
) -> i32 {
    let Some(buffer) = (unsafe { handle_ref(buffer) }) else {
        return -1;
    };

    let Some(e) = buffer.0.get(dev).and_then(|slot| slot.get(tr)) else {
        return -1;
    };
    if unsafe {
        write_out(
            out,
            Autd3Emission {
                phase: e.phase.0,
                intensity: e.intensity.0,
            },
        )
    } != 0
    {
        return -1;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_buffer_set(
    buffer: *mut PatternBuffer,
    dev: usize,
    tr: usize,
    emission: Autd3Emission,
) -> i32 {
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };

    let Some(e) = buffer.0.get_mut(dev).and_then(|slot| slot.get_mut(tr)) else {
        return -1;
    };
    *e = Emission {
        phase: Phase(emission.phase),
        intensity: Intensity(emission.intensity),
    };
    0
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
    let (Some(geometry), Some(target), Some(option), Some(buffer)) = (
        unsafe { handle_ref(geometry) },
        unsafe { point(target) },
        unsafe { handle_ref(option) },
        unsafe { handle_mut(buffer) },
    ) else {
        return -1;
    };

    if buffer.0.len() != geometry.num_devices() {
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

unsafe fn write_emissions(src: &[Emission], dst: *mut Autd3Emission) -> i32 {
    let Some(dst) = (unsafe { slice_mut(dst, src.len()) }) else {
        return -1;
    };
    for (d, e) in dst.iter_mut().zip(src) {
        *d = Autd3Emission {
            phase: e.phase.0,
            intensity: e.intensity.0,
        };
    }
    0
}

unsafe fn with_device_dst(
    geometry: *const Geometry,
    dev: usize,
    dst: *mut Autd3Emission,
    f: impl FnOnce(&autd3_rs_core::geometry::Device, &mut [Emission]),
) -> i32 {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return -1;
    };

    let Some(device) = geometry.iter().nth(dev) else {
        return -1;
    };
    let mut buf = vec![Emission::default(); device.num_transducers()];
    f(device, &mut buf);
    unsafe { write_emissions(&buf, dst) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_focus_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    dst: *mut Autd3Emission,
) -> i32 {
    let (Some(target), Some(option)) = (unsafe { point(target) }, unsafe { handle_ref(option) })
    else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::focus_device(
                device,
                target,
                Length::millimeters(wavelength_mm),
                &FocusOption {
                    intensity: option.intensity(),
                    phase_offset: option.phase_offset(),
                },
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_focus_transducer(
    position: *const f32,
    target: *const f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    out: *mut Autd3Emission,
) -> i32 {
    let (Some(position), Some(target), Some(option)) = (
        unsafe { point(position) },
        unsafe { point(target) },
        unsafe { handle_ref(option) },
    ) else {
        return -1;
    };

    let e = autd3_rs_pattern::focus_transducer(
        position,
        target,
        Length::millimeters(wavelength_mm),
        &FocusOption {
            intensity: option.intensity(),
            phase_offset: option.phase_offset(),
        },
    );
    unsafe { write_emissions(std::slice::from_ref(&e), out) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_plane(
    geometry: *const Geometry,
    dir: *const f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    buffer: *mut PatternBuffer,
) -> i32 {
    let (Some(geometry), Some(dir), Some(option), Some(buffer)) = (
        unsafe { handle_ref(geometry) },
        unsafe { unit_vector(dir) },
        unsafe { handle_ref(option) },
        unsafe { handle_mut(buffer) },
    ) else {
        return -1;
    };

    if buffer.0.len() != geometry.num_devices() {
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
pub unsafe extern "C" fn autd3_pattern_plane_device(
    geometry: *const Geometry,
    dev: usize,
    dir: *const f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    dst: *mut Autd3Emission,
) -> i32 {
    let (Some(dir), Some(option)) = (unsafe { unit_vector(dir) }, unsafe { handle_ref(option) })
    else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::plane_device(
                device,
                dir,
                Length::millimeters(wavelength_mm),
                &PlaneOption {
                    intensity: option.intensity(),
                    phase_offset: option.phase_offset(),
                },
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_plane_transducer(
    position: *const f32,
    dir: *const f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    out: *mut Autd3Emission,
) -> i32 {
    let (Some(position), Some(dir), Some(option)) = (
        unsafe { point(position) },
        unsafe { unit_vector(dir) },
        unsafe { handle_ref(option) },
    ) else {
        return -1;
    };

    let e = autd3_rs_pattern::plane_transducer(
        position,
        dir,
        Length::millimeters(wavelength_mm),
        &PlaneOption {
            intensity: option.intensity(),
            phase_offset: option.phase_offset(),
        },
    );
    unsafe { write_emissions(std::slice::from_ref(&e), out) }
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
    let (Some(geometry), Some(apex), Some(dir), Some(option), Some(buffer)) = (
        unsafe { handle_ref(geometry) },
        unsafe { point(apex) },
        unsafe { unit_vector(dir) },
        unsafe { handle_ref(option) },
        unsafe { handle_mut(buffer) },
    ) else {
        return -1;
    };

    if buffer.0.len() != geometry.num_devices() {
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
pub unsafe extern "C" fn autd3_pattern_bessel_device(
    geometry: *const Geometry,
    dev: usize,
    apex: *const f32,
    dir: *const f32,
    theta_rad: f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    dst: *mut Autd3Emission,
) -> i32 {
    let (Some(apex), Some(dir), Some(option)) = (
        unsafe { point(apex) },
        unsafe { unit_vector(dir) },
        unsafe { handle_ref(option) },
    ) else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::bessel_device(
                device,
                apex,
                dir,
                Angle::from_radian(theta_rad),
                Length::millimeters(wavelength_mm),
                &BesselOption {
                    intensity: option.intensity(),
                    phase_offset: option.phase_offset(),
                },
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_bessel_transducer(
    position: *const f32,
    apex: *const f32,
    dir: *const f32,
    theta_rad: f32,
    wavelength_mm: f32,
    option: *const Autd3PatternOption,
    out: *mut Autd3Emission,
) -> i32 {
    let (Some(position), Some(apex), Some(dir), Some(option)) = (
        unsafe { point(position) },
        unsafe { point(apex) },
        unsafe { unit_vector(dir) },
        unsafe { handle_ref(option) },
    ) else {
        return -1;
    };

    let e = autd3_rs_pattern::bessel_transducer(
        position,
        apex,
        dir,
        Angle::from_radian(theta_rad),
        Length::millimeters(wavelength_mm),
        &BesselOption {
            intensity: option.intensity(),
            phase_offset: option.phase_offset(),
        },
    );
    unsafe { write_emissions(std::slice::from_ref(&e), out) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_uniform(
    phase: u8,
    intensity: u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };

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
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return;
    };

    autd3_rs_pattern::null(&mut buffer.0);
}

autd3_ffi_abi::export_abi_version!();
