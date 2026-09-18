use autd3_ffi_abi::{
    PatternBuffer, drop_handle, handle_mut, handle_ref, into_handle, slice_mut, slice_ref,
    write_out,
};
use autd3_rs_core::geometry::{Autd3, TransducerGroups};
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_core::{Angle, Geometry, Length, Point3, UnitVector3, Vector3, Velocity};

#[repr(C)]
pub struct Autd3Emission {
    pub phase: u8,
    pub intensity: u8,
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
        .as_chunks::<{ Autd3::NUM_TRANSDUCERS }>()
        .0
        .iter()
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
    buffer: *mut PatternBuffer,
) -> i32 {
    let (Some(geometry), Some(target), Some(buffer)) = (
        unsafe { handle_ref(geometry) },
        unsafe { point(target) },
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
        Length::from_mm(wavelength_mm),
        &mut buffer.0,
    );
    0
}

unsafe fn with_emissions(
    dst: *mut Autd3Emission,
    len: usize,
    f: impl FnOnce(&mut [Emission]),
) -> i32 {
    let Some(dst) = (unsafe { slice_mut(dst, len) }) else {
        return -1;
    };
    let mut buf: Vec<Emission> = dst
        .iter()
        .map(|e| Emission {
            phase: Phase(e.phase),
            intensity: Intensity(e.intensity),
        })
        .collect();
    f(&mut buf);
    for (d, e) in dst.iter_mut().zip(&buf) {
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
    unsafe { with_emissions(dst, device.num_transducers(), |buf| f(device, buf)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_focus_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    wavelength_mm: f32,
    dst: *mut Autd3Emission,
) -> i32 {
    let Some(target) = (unsafe { point(target) }) else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::focus_device(device, target, Length::from_mm(wavelength_mm), buf);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_focus_transducer(
    position: *const f32,
    target: *const f32,
    wavelength_mm: f32,
    out: *mut u8,
) -> i32 {
    let (Some(position), Some(target)) = (unsafe { point(position) }, unsafe { point(target) })
    else {
        return -1;
    };

    let Some(out) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    *out = autd3_rs_pattern::focus_transducer(position, target, Length::from_mm(wavelength_mm)).0;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_plane(
    geometry: *const Geometry,
    dir: *const f32,
    wavelength_mm: f32,
    buffer: *mut PatternBuffer,
) -> i32 {
    let (Some(geometry), Some(dir), Some(buffer)) = (
        unsafe { handle_ref(geometry) },
        unsafe { unit_vector(dir) },
        unsafe { handle_mut(buffer) },
    ) else {
        return -1;
    };

    if buffer.0.len() != geometry.num_devices() {
        return -1;
    }
    autd3_rs_pattern::plane(geometry, dir, Length::from_mm(wavelength_mm), &mut buffer.0);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_plane_device(
    geometry: *const Geometry,
    dev: usize,
    dir: *const f32,
    wavelength_mm: f32,
    dst: *mut Autd3Emission,
) -> i32 {
    let Some(dir) = (unsafe { unit_vector(dir) }) else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::plane_device(device, dir, Length::from_mm(wavelength_mm), buf);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_plane_transducer(
    position: *const f32,
    dir: *const f32,
    wavelength_mm: f32,
    out: *mut u8,
) -> i32 {
    let (Some(position), Some(dir)) = (unsafe { point(position) }, unsafe { unit_vector(dir) })
    else {
        return -1;
    };

    let Some(out) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    *out = autd3_rs_pattern::plane_transducer(position, dir, Length::from_mm(wavelength_mm)).0;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_bessel(
    geometry: *const Geometry,
    apex: *const f32,
    dir: *const f32,
    theta_rad: f32,
    wavelength_mm: f32,
    buffer: *mut PatternBuffer,
) -> i32 {
    let (Some(geometry), Some(apex), Some(dir), Some(buffer)) = (
        unsafe { handle_ref(geometry) },
        unsafe { point(apex) },
        unsafe { unit_vector(dir) },
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
        Angle::from_rad(theta_rad),
        Length::from_mm(wavelength_mm),
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
    dst: *mut Autd3Emission,
) -> i32 {
    let (Some(apex), Some(dir)) = (unsafe { point(apex) }, unsafe { unit_vector(dir) }) else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::bessel_device(
                device,
                apex,
                dir,
                Angle::from_rad(theta_rad),
                Length::from_mm(wavelength_mm),
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
    out: *mut u8,
) -> i32 {
    let (Some(position), Some(apex), Some(dir)) =
        (unsafe { point(position) }, unsafe { point(apex) }, unsafe {
            unit_vector(dir)
        })
    else {
        return -1;
    };

    let Some(out) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    *out = autd3_rs_pattern::bessel_transducer(
        position,
        apex,
        dir,
        Angle::from_rad(theta_rad),
        Length::from_mm(wavelength_mm),
    )
    .0;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_twin_trap(
    geometry: *const Geometry,
    target: *const f32,
    normal: *const f32,
    wavelength_mm: f32,
    buffer: *mut PatternBuffer,
) -> i32 {
    let (Some(geometry), Some(target), Some(normal), Some(buffer)) = (
        unsafe { handle_ref(geometry) },
        unsafe { point(target) },
        unsafe { unit_vector(normal) },
        unsafe { handle_mut(buffer) },
    ) else {
        return -1;
    };

    if buffer.0.len() != geometry.num_devices() {
        return -1;
    }
    autd3_rs_pattern::twin_trap(
        geometry,
        target,
        normal,
        Length::from_mm(wavelength_mm),
        &mut buffer.0,
    );
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_twin_trap_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    normal: *const f32,
    wavelength_mm: f32,
    dst: *mut Autd3Emission,
) -> i32 {
    let (Some(target), Some(normal)) = (unsafe { point(target) }, unsafe { unit_vector(normal) })
    else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::twin_trap_device(
                device,
                target,
                normal,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_twin_trap_transducer(
    position: *const f32,
    target: *const f32,
    normal: *const f32,
    wavelength_mm: f32,
    out: *mut u8,
) -> i32 {
    let (Some(position), Some(target), Some(normal)) = (
        unsafe { point(position) },
        unsafe { point(target) },
        unsafe { unit_vector(normal) },
    ) else {
        return -1;
    };

    let Some(out) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    *out = autd3_rs_pattern::twin_trap_transducer(
        position,
        target,
        normal,
        Length::from_mm(wavelength_mm),
    )
    .0;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_vortex(
    geometry: *const Geometry,
    target: *const f32,
    axis: *const f32,
    order: i32,
    wavelength_mm: f32,
    buffer: *mut PatternBuffer,
) -> i32 {
    let (Some(geometry), Some(target), Some(axis), Some(buffer)) = (
        unsafe { handle_ref(geometry) },
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        unsafe { handle_mut(buffer) },
    ) else {
        return -1;
    };

    if buffer.0.len() != geometry.num_devices() {
        return -1;
    }
    autd3_rs_pattern::vortex(
        geometry,
        target,
        axis,
        order,
        Length::from_mm(wavelength_mm),
        &mut buffer.0,
    );
    0
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_vortex_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    axis: *const f32,
    order: i32,
    wavelength_mm: f32,
    dst: *mut Autd3Emission,
) -> i32 {
    let (Some(target), Some(axis)) = (unsafe { point(target) }, unsafe { unit_vector(axis) })
    else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::vortex_device(
                device,
                target,
                axis,
                order,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_vortex_transducer(
    position: *const f32,
    target: *const f32,
    axis: *const f32,
    order: i32,
    wavelength_mm: f32,
    out: *mut u8,
) -> i32 {
    let (Some(position), Some(target), Some(axis)) = (
        unsafe { point(position) },
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
    ) else {
        return -1;
    };

    let Some(out) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    *out = autd3_rs_pattern::vortex_transducer(
        position,
        target,
        axis,
        order,
        Length::from_mm(wavelength_mm),
    )
    .0;
    0
}

unsafe fn with_buffer(buffer: *mut PatternBuffer, f: impl FnOnce(&mut [Vec<Emission>])) -> i32 {
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };
    f(&mut buffer.0);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_intensity(
    intensity: u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    unsafe {
        with_buffer(buffer, |buf| {
            autd3_rs_pattern::set_intensity(Intensity(intensity), buf);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_intensity_device(
    intensity: u8,
    dst: *mut Autd3Emission,
    len: usize,
) -> i32 {
    unsafe {
        with_emissions(dst, len, |buf| {
            autd3_rs_pattern::set_intensity_device(Intensity(intensity), buf);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_phase(phase: u8, buffer: *mut PatternBuffer) -> i32 {
    unsafe { with_buffer(buffer, |buf| autd3_rs_pattern::set_phase(Phase(phase), buf)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_phase_device(
    phase: u8,
    dst: *mut Autd3Emission,
    len: usize,
) -> i32 {
    unsafe {
        with_emissions(dst, len, |buf| {
            autd3_rs_pattern::set_phase_device(Phase(phase), buf);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_phase_and_intensity(
    phase: u8,
    intensity: u8,
    buffer: *mut PatternBuffer,
) -> i32 {
    unsafe {
        with_buffer(buffer, |buf| {
            autd3_rs_pattern::set_phase_and_intensity(Phase(phase), Intensity(intensity), buf);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_phase_and_intensity_device(
    phase: u8,
    intensity: u8,
    dst: *mut Autd3Emission,
    len: usize,
) -> i32 {
    unsafe {
        with_emissions(dst, len, |buf| {
            autd3_rs_pattern::set_phase_and_intensity_device(
                Phase(phase),
                Intensity(intensity),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_add_phase(phase: u8, buffer: *mut PatternBuffer) -> i32 {
    unsafe { with_buffer(buffer, |buf| autd3_rs_pattern::add_phase(Phase(phase), buf)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_add_phase_device(
    phase: u8,
    dst: *mut Autd3Emission,
    len: usize,
) -> i32 {
    unsafe {
        with_emissions(dst, len, |buf| {
            autd3_rs_pattern::add_phase_device(Phase(phase), buf);
        })
    }
}

fn matches_geometry(geometry: &Geometry, buffer: &PatternBuffer) -> bool {
    buffer.0.len() == geometry.num_devices()
        && geometry
            .iter()
            .zip(&buffer.0)
            .all(|(device, slot)| slot.len() == device.num_transducers())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_group(
    geometry: *const Geometry,
    keys: *const i32,
    sources: *const *const PatternBuffer,
    num_sources: usize,
    buffer: *mut PatternBuffer,
) -> i32 {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return -1;
    };
    let (Some(keys), Some(source_ptrs)) = (
        unsafe { slice_ref(keys, geometry.num_transducers()) },
        unsafe { slice_ref(sources, num_sources) },
    ) else {
        return -1;
    };
    if source_ptrs
        .iter()
        .any(|&ptr| std::ptr::eq(ptr, buffer.cast_const()))
    {
        return -1;
    }
    let Some(sources) = source_ptrs
        .iter()
        .map(|&ptr| unsafe { handle_ref(ptr) })
        .collect::<Option<Vec<&PatternBuffer>>>()
    else {
        return -1;
    };
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };

    if !matches_geometry(geometry, buffer)
        || !sources
            .iter()
            .all(|source| matches_geometry(geometry, source))
        || keys
            .iter()
            .any(|&key| usize::try_from(key).is_ok_and(|key| key >= num_sources))
    {
        return -1;
    }

    let mut next = 0;
    let offsets: Vec<usize> = geometry
        .iter()
        .map(|device| {
            let start = next;
            next += device.num_transducers();
            start
        })
        .collect();
    let groups = TransducerGroups::new(geometry, |device, tr| {
        usize::try_from(keys[offsets[device.idx()] + tr]).ok()
    });
    autd3_rs_pattern::group(
        geometry,
        &groups,
        |key| sources[key].0.as_slice(),
        &mut buffer.0,
    );
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_group_null(
    geometry: *const Geometry,
    indices: *const i32,
    buffer: *mut PatternBuffer,
) -> i32 {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return -1;
    };
    let (Some(indices), Some(buffer)) = (
        unsafe { slice_ref(indices, geometry.num_transducers()) },
        unsafe { handle_mut(buffer) },
    ) else {
        return -1;
    };
    if !matches_geometry(geometry, buffer) {
        return -1;
    }

    for (out, &index) in buffer.0.iter_mut().flatten().zip(indices) {
        if index < 0 {
            *out = Emission::NULL;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_group_copy(
    geometry: *const Geometry,
    indices: *const i32,
    index: i32,
    source: *const PatternBuffer,
    buffer: *mut PatternBuffer,
) -> i32 {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return -1;
    };
    if index < 0 || std::ptr::eq(source, buffer.cast_const()) {
        return -1;
    }
    let (Some(indices), Some(source), Some(buffer)) = (
        unsafe { slice_ref(indices, geometry.num_transducers()) },
        unsafe { handle_ref(source) },
        unsafe { handle_mut(buffer) },
    ) else {
        return -1;
    };
    if !matches_geometry(geometry, source) || !matches_geometry(geometry, buffer) {
        return -1;
    }

    for ((out, &e), &i) in buffer
        .0
        .iter_mut()
        .flatten()
        .zip(source.0.iter().flatten())
        .zip(indices)
    {
        if i == index {
            *out = e;
        }
    }
    0
}

autd3_ffi_abi::export_abi_version!();

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Geometry {
        Geometry::new(vec![Autd3::default(), Autd3::default()])
    }

    fn filled(geometry: &Geometry, phase: u8) -> PatternBuffer {
        let mut buffer = PatternBuffer(geometry.pattern_buffer());
        autd3_rs_pattern::set_phase(Phase(phase), &mut buffer.0);
        buffer
    }

    fn keys(geometry: &Geometry, key: impl Fn(usize, usize) -> i32) -> Vec<i32> {
        let key = &key;
        geometry
            .iter()
            .flat_map(|device| (0..device.num_transducers()).map(move |tr| key(device.idx(), tr)))
            .collect()
    }

    #[test]
    fn set_and_add_phase_update_the_buffer_in_place() {
        let geometry = geometry();
        let mut buffer = PatternBuffer(geometry.pattern_buffer());

        assert_eq!(
            unsafe { autd3_pattern_set_intensity(0x80, &raw mut buffer) },
            0
        );
        assert_eq!(unsafe { autd3_pattern_set_phase(0xF0, &raw mut buffer) }, 0);
        assert_eq!(unsafe { autd3_pattern_add_phase(0x20, &raw mut buffer) }, 0);
        for &e in buffer.0.iter().flatten() {
            assert_eq!(e.phase, Phase(0x10));
            assert_eq!(e.intensity, Intensity(0x80));
        }

        assert_eq!(
            unsafe { autd3_pattern_set_phase_and_intensity(0x40, 0x50, &raw mut buffer) },
            0
        );
        for &e in buffer.0.iter().flatten() {
            assert_eq!(e.phase, Phase(0x40));
            assert_eq!(e.intensity, Intensity(0x50));
        }

        assert_eq!(
            unsafe { autd3_pattern_set_intensity(0, std::ptr::null_mut()) },
            -1
        );
    }

    #[test]
    fn device_level_functions_keep_the_untouched_field() {
        let geometry = geometry();
        let n = Autd3::NUM_TRANSDUCERS;
        let mut dst: Vec<Autd3Emission> = (0..n)
            .map(|_| Autd3Emission {
                phase: 0x00,
                intensity: 0x42,
            })
            .collect();
        let target = [0.0_f32, 0.0, 150.0];

        let result = unsafe {
            autd3_pattern_focus_device(
                &raw const geometry,
                0,
                target.as_ptr(),
                8.5,
                dst.as_mut_ptr(),
            )
        };
        assert_eq!(result, 0);
        for (tr, e) in dst.iter().enumerate() {
            let mut phase = 0u8;
            let position = geometry[0].position(tr);
            let pos = [position.x, position.y, position.z];
            assert_eq!(
                unsafe {
                    autd3_pattern_focus_transducer(
                        pos.as_ptr(),
                        target.as_ptr(),
                        8.5,
                        &raw mut phase,
                    )
                },
                0
            );
            assert_eq!(e.phase, phase);
            assert_eq!(e.intensity, 0x42);
        }

        let result = unsafe { autd3_pattern_set_phase_device(0x33, dst.as_mut_ptr(), dst.len()) };
        assert_eq!(result, 0);
        assert!(dst.iter().all(|e| e.phase == 0x33 && e.intensity == 0x42));

        let result = unsafe { autd3_pattern_add_phase_device(0xF0, dst.as_mut_ptr(), dst.len()) };
        assert_eq!(result, 0);
        assert!(dst.iter().all(|e| e.phase == 0x23 && e.intensity == 0x42));

        let result =
            unsafe { autd3_pattern_set_intensity_device(0x11, dst.as_mut_ptr(), dst.len()) };
        assert_eq!(result, 0);
        assert!(dst.iter().all(|e| e.phase == 0x23 && e.intensity == 0x11));

        let result = unsafe {
            autd3_pattern_set_phase_and_intensity_device(0x01, 0x02, dst.as_mut_ptr(), dst.len())
        };
        assert_eq!(result, 0);
        assert!(dst.iter().all(|e| e.phase == 0x01 && e.intensity == 0x02));

        let result = unsafe { autd3_pattern_set_phase_device(0x00, std::ptr::null_mut(), 1) };
        assert_eq!(result, -1);
    }

    #[test]
    fn group_writes_the_source_of_each_key_across_devices() {
        let geometry = geometry();
        let left = filled(&geometry, 0x10);
        let right = filled(&geometry, 0x20);
        let mut dst = filled(&geometry, 0xFF);
        let keys = keys(&geometry, |dev, tr| match (dev, tr % 3) {
            (_, 0) => 0,
            (1, 1) => 1,
            _ => -1,
        });
        let sources = [&raw const left, &raw const right];

        let result = unsafe {
            autd3_pattern_group(
                &raw const geometry,
                keys.as_ptr(),
                sources.as_ptr(),
                sources.len(),
                &raw mut dst,
            )
        };

        assert_eq!(result, 0);
        for (dev, slot) in dst.0.iter().enumerate() {
            for (tr, &e) in slot.iter().enumerate() {
                let expected = match (dev, tr % 3) {
                    (_, 0) => Emission {
                        phase: Phase(0x10),
                        intensity: Intensity::MAX,
                    },
                    (1, 1) => Emission {
                        phase: Phase(0x20),
                        intensity: Intensity::MAX,
                    },
                    _ => Emission::NULL,
                };
                assert_eq!(e, expected, "dev {dev} tr {tr}");
            }
        }
    }

    #[test]
    fn group_rejects_invalid_arguments_without_writing() {
        let geometry = geometry();
        let left = filled(&geometry, 0x10);
        let mut dst = filled(&geometry, 0xFF);
        let zeros = keys(&geometry, |_, _| 0);
        let out_of_range = keys(&geometry, |_, tr| i32::from(tr == 5));
        let single_geometry = Geometry::new(vec![Autd3::default()]);
        let single = filled(&single_geometry, 0x10);
        let dst_ptr = &raw mut dst;

        let call = |keys: &[i32], sources: &[*const PatternBuffer]| unsafe {
            autd3_pattern_group(
                &raw const geometry,
                keys.as_ptr(),
                sources.as_ptr(),
                sources.len(),
                dst_ptr,
            )
        };
        assert_eq!(call(&zeros, &[dst_ptr.cast_const()]), -1);
        assert_eq!(call(&out_of_range, &[&raw const left]), -1);
        assert_eq!(call(&zeros, &[&raw const single]), -1);
        assert_eq!(call(&zeros, &[std::ptr::null()]), -1);
        assert_eq!(
            unsafe {
                autd3_pattern_group(
                    std::ptr::null(),
                    zeros.as_ptr(),
                    [&raw const left].as_ptr(),
                    1,
                    dst_ptr,
                )
            },
            -1
        );

        assert!(dst.0.iter().flatten().all(|e| e.phase == Phase(0xFF)));
    }

    #[test]
    fn group_null_and_copy_write_only_the_selected_transducers() {
        let geometry = geometry();
        let source = filled(&geometry, 0x10);
        let mut dst = filled(&geometry, 0xFF);
        let indices = keys(&geometry, |dev, tr| match (dev, tr % 3) {
            (_, 0) => 0,
            (1, 1) => 1,
            _ => -1,
        });

        assert_eq!(
            unsafe {
                autd3_pattern_group_null(&raw const geometry, indices.as_ptr(), &raw mut dst)
            },
            0
        );
        assert_eq!(
            unsafe {
                autd3_pattern_group_copy(
                    &raw const geometry,
                    indices.as_ptr(),
                    1,
                    &raw const source,
                    &raw mut dst,
                )
            },
            0
        );

        for (dev, slot) in dst.0.iter().enumerate() {
            for (tr, &e) in slot.iter().enumerate() {
                let expected = match (dev, tr % 3) {
                    (_, 0) => Emission {
                        phase: Phase(0xFF),
                        intensity: Intensity::MAX,
                    },
                    (1, 1) => Emission {
                        phase: Phase(0x10),
                        intensity: Intensity::MAX,
                    },
                    _ => Emission::NULL,
                };
                assert_eq!(e, expected, "dev {dev} tr {tr}");
            }
        }
    }

    #[test]
    fn group_null_and_copy_reject_invalid_arguments() {
        let geometry = geometry();
        let source = filled(&geometry, 0x10);
        let mut dst = filled(&geometry, 0xFF);
        let indices = keys(&geometry, |_, _| 0);
        let single_geometry = Geometry::new(vec![Autd3::default()]);
        let single = filled(&single_geometry, 0x10);
        let dst_ptr = &raw mut dst;

        let copy = |index: i32, source: *const PatternBuffer| unsafe {
            autd3_pattern_group_copy(
                &raw const geometry,
                indices.as_ptr(),
                index,
                source,
                dst_ptr,
            )
        };
        assert_eq!(copy(0, dst_ptr.cast_const()), -1);
        assert_eq!(copy(-1, &raw const source), -1);
        assert_eq!(copy(0, &raw const single), -1);
        assert_eq!(copy(0, std::ptr::null()), -1);
        assert_eq!(
            unsafe {
                autd3_pattern_group_copy(
                    &raw const geometry,
                    std::ptr::null(),
                    0,
                    &raw const source,
                    dst_ptr,
                )
            },
            -1
        );
        assert_eq!(
            unsafe { autd3_pattern_group_null(std::ptr::null(), indices.as_ptr(), dst_ptr) },
            -1
        );
        assert_eq!(
            unsafe { autd3_pattern_group_null(&raw const geometry, std::ptr::null(), dst_ptr) },
            -1
        );
        assert_eq!(
            unsafe {
                autd3_pattern_group_null(
                    &raw const geometry,
                    indices.as_ptr(),
                    std::ptr::null_mut(),
                )
            },
            -1
        );

        assert!(dst.0.iter().flatten().all(|e| e.phase == Phase(0xFF)));
    }
}
