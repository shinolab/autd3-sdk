use autd3_ffi_abi::{
    Buffer, IntensityBuffer, PhaseBuffer, drop_handle, handle_mut, handle_ref, into_handle,
    slice_mut, slice_ref, write_out,
};
use autd3_rs_core::geometry::{Autd3, TransducerGroups};
use autd3_rs_core::value::{Intensity, Phase};
use autd3_rs_core::{Angle, Geometry, Length, Point3, UnitVector3, Vector3, Velocity};

trait Byte: Copy {
    fn from_u8(v: u8) -> Self;
    fn to_u8(self) -> u8;
}

impl Byte for Phase {
    fn from_u8(v: u8) -> Self {
        Phase(v)
    }

    fn to_u8(self) -> u8 {
        self.0
    }
}

impl Byte for Intensity {
    fn from_u8(v: u8) -> Self {
        Intensity(v)
    }

    fn to_u8(self) -> u8 {
        self.0
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

unsafe fn buffer_from_array<T: Byte>(src: *const u8, num_devices: usize) -> *mut Buffer<T> {
    let Some(slice) = (unsafe { slice_ref(src, num_devices * Autd3::NUM_TRANSDUCERS) }) else {
        return std::ptr::null_mut();
    };
    let buffer = slice
        .as_chunks::<{ Autd3::NUM_TRANSDUCERS }>()
        .0
        .iter()
        .map(|device| device.iter().map(|&v| T::from_u8(v)).collect())
        .collect();
    into_handle(Buffer(buffer))
}

unsafe fn buffer_num_devices<T>(buffer: *const Buffer<T>) -> usize {
    unsafe { handle_ref(buffer) }.map_or(0, |buffer| buffer.0.len())
}

unsafe fn buffer_num_transducers<T>(buffer: *const Buffer<T>, dev: usize) -> usize {
    unsafe { handle_ref(buffer) }
        .and_then(|buffer| buffer.0.get(dev))
        .map_or(0, Vec::len)
}

unsafe fn buffer_get<T: Byte>(
    buffer: *const Buffer<T>,
    dev: usize,
    tr: usize,
    out: *mut u8,
) -> i32 {
    let Some(v) = (unsafe { handle_ref(buffer) })
        .and_then(|buffer| buffer.0.get(dev))
        .and_then(|slot| slot.get(tr))
    else {
        return -1;
    };
    if unsafe { write_out(out, v.to_u8()) } != 0 {
        return -1;
    }
    0
}

unsafe fn buffer_set<T: Byte>(buffer: *mut Buffer<T>, dev: usize, tr: usize, value: u8) -> i32 {
    let Some(v) = (unsafe { handle_mut(buffer) })
        .and_then(|buffer| buffer.0.get_mut(dev))
        .and_then(|slot| slot.get_mut(tr))
    else {
        return -1;
    };
    *v = T::from_u8(value);
    0
}

macro_rules! buffer_api {
    ($ty:ty, $geometry:ident, $make:ident, $from_array:ident, $num_devices:ident, $num_transducers:ident, $get:ident, $set:ident, $free:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $geometry(geometry: *const Geometry) -> *mut $ty {
            let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
                return std::ptr::null_mut();
            };
            into_handle(Buffer(geometry.$make()))
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $from_array(src: *const u8, num_devices: usize) -> *mut $ty {
            unsafe { buffer_from_array(src, num_devices) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $num_devices(buffer: *const $ty) -> usize {
            unsafe { buffer_num_devices(buffer) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $num_transducers(buffer: *const $ty, dev: usize) -> usize {
            unsafe { buffer_num_transducers(buffer, dev) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(
            buffer: *const $ty,
            dev: usize,
            tr: usize,
            out: *mut u8,
        ) -> i32 {
            unsafe { buffer_get(buffer, dev, tr, out) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(buffer: *mut $ty, dev: usize, tr: usize, value: u8) -> i32 {
            unsafe { buffer_set(buffer, dev, tr, value) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $free(buffer: *mut $ty) {
            unsafe { drop_handle(buffer) }
        }
    };
}

buffer_api!(
    PhaseBuffer,
    autd3_core_geometry_phase_buffer,
    phase_buffer,
    autd3_phase_buffer_from_array,
    autd3_phase_buffer_num_devices,
    autd3_phase_buffer_num_transducers,
    autd3_phase_buffer_get,
    autd3_phase_buffer_set,
    autd3_phase_buffer_free
);

buffer_api!(
    IntensityBuffer,
    autd3_core_geometry_intensity_buffer,
    intensity_buffer,
    autd3_intensity_buffer_from_array,
    autd3_intensity_buffer_num_devices,
    autd3_intensity_buffer_num_transducers,
    autd3_intensity_buffer_get,
    autd3_intensity_buffer_set,
    autd3_intensity_buffer_free
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_focus(
    geometry: *const Geometry,
    target: *const f32,
    wavelength_mm: f32,
    buffer: *mut PhaseBuffer,
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

unsafe fn with_bytes<T: Byte>(dst: *mut u8, len: usize, f: impl FnOnce(&mut [T])) -> i32 {
    let Some(dst) = (unsafe { slice_mut(dst, len) }) else {
        return -1;
    };
    let mut buf: Vec<T> = dst.iter().map(|&v| T::from_u8(v)).collect();
    f(&mut buf);
    for (d, v) in dst.iter_mut().zip(&buf) {
        *d = v.to_u8();
    }
    0
}

unsafe fn with_device_dst<T: Byte>(
    geometry: *const Geometry,
    dev: usize,
    dst: *mut u8,
    f: impl FnOnce(&autd3_rs_core::geometry::Device, &mut [T]),
) -> i32 {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return -1;
    };
    let Some(device) = geometry.iter().nth(dev) else {
        return -1;
    };
    unsafe { with_bytes(dst, device.num_transducers(), |buf| f(device, buf)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_focus_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    wavelength_mm: f32,
    dst: *mut u8,
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
    buffer: *mut PhaseBuffer,
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
    dst: *mut u8,
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
    buffer: *mut PhaseBuffer,
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
    dst: *mut u8,
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

fn waist(waist_mm: f32) -> Option<Length> {
    (waist_mm.is_finite() && waist_mm > 0.0).then(|| Length::from_mm(waist_mm))
}

fn laguerre_gaussian_option(
    p: u32,
    l: i32,
    waist_mm: f32,
) -> Option<autd3_rs_pattern::LaguerreGaussianOption> {
    Some(autd3_rs_pattern::LaguerreGaussianOption {
        p,
        l,
        waist: waist(waist_mm)?,
    })
}

fn hermite_gaussian_option(
    m: u32,
    n: u32,
    waist_mm: f32,
) -> Option<autd3_rs_pattern::HermiteGaussianOption> {
    Some(autd3_rs_pattern::HermiteGaussianOption {
        m,
        n,
        waist: waist(waist_mm)?,
    })
}

unsafe fn with_geometry_buffer<T>(
    geometry: *const Geometry,
    buffer: *mut Buffer<T>,
    f: impl FnOnce(&Geometry, &mut [Vec<T>]),
) -> i32 {
    let (Some(geometry), Some(buffer)) = (unsafe { handle_ref(geometry) }, unsafe {
        handle_mut(buffer)
    }) else {
        return -1;
    };
    if buffer.0.len() != geometry.num_devices() {
        return -1;
    }
    f(geometry, &mut buffer.0);
    0
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_laguerre_gaussian_phase(
    geometry: *const Geometry,
    target: *const f32,
    axis: *const f32,
    p: u32,
    l: i32,
    waist_mm: f32,
    wavelength_mm: f32,
    buffer: *mut PhaseBuffer,
) -> i32 {
    let (Some(target), Some(axis), Some(option)) = (
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        laguerre_gaussian_option(p, l, waist_mm),
    ) else {
        return -1;
    };

    unsafe {
        with_geometry_buffer(geometry, buffer, |geometry, buf| {
            autd3_rs_pattern::laguerre_gaussian_phase(
                geometry,
                target,
                axis,
                option,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_laguerre_gaussian_phase_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    axis: *const f32,
    p: u32,
    l: i32,
    waist_mm: f32,
    wavelength_mm: f32,
    dst: *mut u8,
) -> i32 {
    let (Some(target), Some(axis), Some(option)) = (
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        laguerre_gaussian_option(p, l, waist_mm),
    ) else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::laguerre_gaussian_phase_device(
                device,
                target,
                axis,
                option,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_laguerre_gaussian_phase_transducer(
    position: *const f32,
    target: *const f32,
    axis: *const f32,
    p: u32,
    l: i32,
    waist_mm: f32,
    wavelength_mm: f32,
    out: *mut u8,
) -> i32 {
    let (Some(position), Some(target), Some(axis), Some(option)) = (
        unsafe { point(position) },
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        laguerre_gaussian_option(p, l, waist_mm),
    ) else {
        return -1;
    };

    let Some(out) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    *out = autd3_rs_pattern::laguerre_gaussian_phase_transducer(
        position,
        target,
        axis,
        option,
        Length::from_mm(wavelength_mm),
    )
    .0;
    0
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_laguerre_gaussian_intensity(
    geometry: *const Geometry,
    target: *const f32,
    axis: *const f32,
    p: u32,
    l: i32,
    waist_mm: f32,
    wavelength_mm: f32,
    buffer: *mut IntensityBuffer,
) -> i32 {
    let (Some(target), Some(axis), Some(option)) = (
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        laguerre_gaussian_option(p, l, waist_mm),
    ) else {
        return -1;
    };

    unsafe {
        with_geometry_buffer(geometry, buffer, |geometry, buf| {
            autd3_rs_pattern::laguerre_gaussian_intensity(
                geometry,
                target,
                axis,
                option,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_laguerre_gaussian_intensity_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    axis: *const f32,
    p: u32,
    l: i32,
    waist_mm: f32,
    wavelength_mm: f32,
    dst: *mut u8,
) -> i32 {
    let (Some(target), Some(axis), Some(option)) = (
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        laguerre_gaussian_option(p, l, waist_mm),
    ) else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::laguerre_gaussian_intensity_device(
                device,
                target,
                axis,
                option,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_hermite_gaussian_phase(
    geometry: *const Geometry,
    target: *const f32,
    axis: *const f32,
    x_dir: *const f32,
    m: u32,
    n: u32,
    waist_mm: f32,
    wavelength_mm: f32,
    buffer: *mut PhaseBuffer,
) -> i32 {
    let (Some(target), Some(axis), Some(x_dir), Some(option)) = (
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        unsafe { unit_vector(x_dir) },
        hermite_gaussian_option(m, n, waist_mm),
    ) else {
        return -1;
    };

    unsafe {
        with_geometry_buffer(geometry, buffer, |geometry, buf| {
            autd3_rs_pattern::hermite_gaussian_phase(
                geometry,
                target,
                axis,
                x_dir,
                option,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_hermite_gaussian_phase_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    axis: *const f32,
    x_dir: *const f32,
    m: u32,
    n: u32,
    waist_mm: f32,
    wavelength_mm: f32,
    dst: *mut u8,
) -> i32 {
    let (Some(target), Some(axis), Some(x_dir), Some(option)) = (
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        unsafe { unit_vector(x_dir) },
        hermite_gaussian_option(m, n, waist_mm),
    ) else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::hermite_gaussian_phase_device(
                device,
                target,
                axis,
                x_dir,
                option,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_hermite_gaussian_phase_transducer(
    position: *const f32,
    target: *const f32,
    axis: *const f32,
    x_dir: *const f32,
    m: u32,
    n: u32,
    waist_mm: f32,
    wavelength_mm: f32,
    out: *mut u8,
) -> i32 {
    let (Some(position), Some(target), Some(axis), Some(x_dir), Some(option)) = (
        unsafe { point(position) },
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        unsafe { unit_vector(x_dir) },
        hermite_gaussian_option(m, n, waist_mm),
    ) else {
        return -1;
    };

    let Some(out) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    *out = autd3_rs_pattern::hermite_gaussian_phase_transducer(
        position,
        target,
        axis,
        x_dir,
        option,
        Length::from_mm(wavelength_mm),
    )
    .0;
    0
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_hermite_gaussian_intensity(
    geometry: *const Geometry,
    target: *const f32,
    axis: *const f32,
    x_dir: *const f32,
    m: u32,
    n: u32,
    waist_mm: f32,
    wavelength_mm: f32,
    buffer: *mut IntensityBuffer,
) -> i32 {
    let (Some(target), Some(axis), Some(x_dir), Some(option)) = (
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        unsafe { unit_vector(x_dir) },
        hermite_gaussian_option(m, n, waist_mm),
    ) else {
        return -1;
    };

    unsafe {
        with_geometry_buffer(geometry, buffer, |geometry, buf| {
            autd3_rs_pattern::hermite_gaussian_intensity(
                geometry,
                target,
                axis,
                x_dir,
                option,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_pattern_hermite_gaussian_intensity_device(
    geometry: *const Geometry,
    dev: usize,
    target: *const f32,
    axis: *const f32,
    x_dir: *const f32,
    m: u32,
    n: u32,
    waist_mm: f32,
    wavelength_mm: f32,
    dst: *mut u8,
) -> i32 {
    let (Some(target), Some(axis), Some(x_dir), Some(option)) = (
        unsafe { point(target) },
        unsafe { unit_vector(axis) },
        unsafe { unit_vector(x_dir) },
        hermite_gaussian_option(m, n, waist_mm),
    ) else {
        return -1;
    };

    unsafe {
        with_device_dst(geometry, dev, dst, |device, buf| {
            autd3_rs_pattern::hermite_gaussian_intensity_device(
                device,
                target,
                axis,
                x_dir,
                option,
                Length::from_mm(wavelength_mm),
                buf,
            );
        })
    }
}

unsafe fn with_buffer<T>(buffer: *mut Buffer<T>, f: impl FnOnce(&mut [Vec<T>])) -> i32 {
    let Some(buffer) = (unsafe { handle_mut(buffer) }) else {
        return -1;
    };
    f(&mut buffer.0);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_intensity(
    intensity: u8,
    buffer: *mut IntensityBuffer,
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
    dst: *mut u8,
    len: usize,
) -> i32 {
    unsafe {
        with_bytes(dst, len, |buf| {
            autd3_rs_pattern::set_intensity_device(Intensity(intensity), buf);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_phase(phase: u8, buffer: *mut PhaseBuffer) -> i32 {
    unsafe { with_buffer(buffer, |buf| autd3_rs_pattern::set_phase(Phase(phase), buf)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_set_phase_device(
    phase: u8,
    dst: *mut u8,
    len: usize,
) -> i32 {
    unsafe {
        with_bytes(dst, len, |buf| {
            autd3_rs_pattern::set_phase_device(Phase(phase), buf);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_add_phase(phase: u8, buffer: *mut PhaseBuffer) -> i32 {
    unsafe { with_buffer(buffer, |buf| autd3_rs_pattern::add_phase(Phase(phase), buf)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_add_phase_device(
    phase: u8,
    dst: *mut u8,
    len: usize,
) -> i32 {
    unsafe {
        with_bytes(dst, len, |buf| {
            autd3_rs_pattern::add_phase_device(Phase(phase), buf);
        })
    }
}

fn matches_geometry<T>(geometry: &Geometry, buffer: &Buffer<T>) -> bool {
    buffer.0.len() == geometry.num_devices()
        && geometry
            .iter()
            .zip(&buffer.0)
            .all(|(device, slot)| slot.len() == device.num_transducers())
}

unsafe fn group_impl<T: Copy>(
    geometry: *const Geometry,
    keys: *const i32,
    sources: *const *const Buffer<T>,
    num_sources: usize,
    buffer: *mut Buffer<T>,
    null: T,
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
        .collect::<Option<Vec<&Buffer<T>>>>()
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
        null,
        &mut buffer.0,
    );
    0
}

unsafe fn group_null_impl<T: Copy>(
    geometry: *const Geometry,
    indices: *const i32,
    buffer: *mut Buffer<T>,
    null: T,
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
            *out = null;
        }
    }
    0
}

unsafe fn group_copy_impl<T: Copy>(
    geometry: *const Geometry,
    indices: *const i32,
    index: i32,
    source: *const Buffer<T>,
    buffer: *mut Buffer<T>,
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

    for ((out, &v), &i) in buffer
        .0
        .iter_mut()
        .flatten()
        .zip(source.0.iter().flatten())
        .zip(indices)
    {
        if i == index {
            *out = v;
        }
    }
    0
}

macro_rules! group_api {
    ($ty:ty, $null:expr, $group:ident, $group_null:ident, $group_copy:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $group(
            geometry: *const Geometry,
            keys: *const i32,
            sources: *const *const $ty,
            num_sources: usize,
            buffer: *mut $ty,
        ) -> i32 {
            unsafe { group_impl(geometry, keys, sources, num_sources, buffer, $null) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $group_null(
            geometry: *const Geometry,
            indices: *const i32,
            buffer: *mut $ty,
        ) -> i32 {
            unsafe { group_null_impl(geometry, indices, buffer, $null) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $group_copy(
            geometry: *const Geometry,
            indices: *const i32,
            index: i32,
            source: *const $ty,
            buffer: *mut $ty,
        ) -> i32 {
            unsafe { group_copy_impl(geometry, indices, index, source, buffer) }
        }
    };
}

group_api!(
    PhaseBuffer,
    Phase::ZERO,
    autd3_pattern_group_phase,
    autd3_pattern_group_null_phase,
    autd3_pattern_group_copy_phase
);

group_api!(
    IntensityBuffer,
    Intensity::MIN,
    autd3_pattern_group_intensity,
    autd3_pattern_group_null_intensity,
    autd3_pattern_group_copy_intensity
);

autd3_ffi_abi::export_abi_version!();

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Geometry {
        Geometry::new(vec![Autd3::default(), Autd3::default()])
    }

    fn phases(geometry: &Geometry, phase: u8) -> PhaseBuffer {
        let mut buffer = Buffer(geometry.phase_buffer());
        autd3_rs_pattern::set_phase(Phase(phase), &mut buffer.0);
        buffer
    }

    fn intensities(geometry: &Geometry, intensity: u8) -> IntensityBuffer {
        let mut buffer = Buffer(geometry.intensity_buffer());
        autd3_rs_pattern::set_intensity(Intensity(intensity), &mut buffer.0);
        buffer
    }

    fn keys(geometry: &Geometry, key: impl Fn(usize, usize) -> i32) -> Vec<i32> {
        let key = &key;
        geometry
            .iter()
            .flat_map(|device| (0..device.num_transducers()).map(move |tr| key(device.idx(), tr)))
            .collect()
    }

    fn sides(dev: usize, tr: usize) -> i32 {
        match (dev, tr % 3) {
            (_, 0) => 0,
            (1, 1) => 1,
            _ => -1,
        }
    }

    #[test]
    fn buffers_are_created_read_and_written_per_transducer() {
        let geometry = geometry();
        let phases = unsafe { autd3_core_geometry_phase_buffer(&raw const geometry) };
        let intensities = unsafe { autd3_core_geometry_intensity_buffer(&raw const geometry) };
        assert_eq!(unsafe { autd3_phase_buffer_num_devices(phases) }, 2);
        assert_eq!(
            unsafe { autd3_intensity_buffer_num_transducers(intensities, 1) },
            Autd3::NUM_TRANSDUCERS
        );
        assert_eq!(
            unsafe { autd3_intensity_buffer_num_transducers(intensities, 2) },
            0
        );

        let mut v = 0xAA;
        assert_eq!(
            unsafe { autd3_phase_buffer_get(phases, 1, 3, &raw mut v) },
            0
        );
        assert_eq!(v, Phase::ZERO.0);
        assert_eq!(
            unsafe { autd3_intensity_buffer_get(intensities, 1, 3, &raw mut v) },
            0
        );
        assert_eq!(v, Intensity::MAX.0);

        assert_eq!(unsafe { autd3_phase_buffer_set(phases, 1, 3, 0x42) }, 0);
        assert_eq!(
            unsafe { autd3_phase_buffer_get(phases, 1, 3, &raw mut v) },
            0
        );
        assert_eq!(v, 0x42);
        assert_eq!(unsafe { autd3_phase_buffer_set(phases, 2, 0, 0x42) }, -1);
        assert_eq!(
            unsafe {
                autd3_intensity_buffer_get(intensities, 0, Autd3::NUM_TRANSDUCERS, &raw mut v)
            },
            -1
        );

        unsafe { autd3_phase_buffer_free(phases) };
        unsafe { autd3_intensity_buffer_free(intensities) };
    }

    #[test]
    fn buffers_are_built_from_flat_arrays() {
        let n = Autd3::NUM_TRANSDUCERS;
        let src: Vec<u8> = (0..2 * n).map(|i| u8::try_from(i % 256).unwrap()).collect();
        let phases = unsafe { autd3_phase_buffer_from_array(src.as_ptr(), 2) };
        let intensities = unsafe { autd3_intensity_buffer_from_array(src.as_ptr(), 2) };
        let (p, i) = unsafe { (&*phases, &*intensities) };
        for (k, (&p, &i)) in p.0.iter().flatten().zip(i.0.iter().flatten()).enumerate() {
            assert_eq!(p, Phase(src[k]));
            assert_eq!(i, Intensity(src[k]));
        }
        assert!(unsafe { autd3_phase_buffer_from_array(std::ptr::null(), 1) }.is_null());
        unsafe { autd3_phase_buffer_free(phases) };
        unsafe { autd3_intensity_buffer_free(intensities) };
    }

    #[test]
    fn set_and_add_update_the_buffers_in_place() {
        let geometry = geometry();
        let mut phase_buffer = phases(&geometry, 0);
        let mut intensity_buffer = intensities(&geometry, 0);

        assert_eq!(
            unsafe { autd3_pattern_set_intensity(0x80, &raw mut intensity_buffer) },
            0
        );
        assert_eq!(
            unsafe { autd3_pattern_set_phase(0xF0, &raw mut phase_buffer) },
            0
        );
        assert_eq!(
            unsafe { autd3_pattern_add_phase(0x20, &raw mut phase_buffer) },
            0
        );
        assert!(phase_buffer.0.iter().flatten().all(|&p| p == Phase(0x10)));
        assert!(
            intensity_buffer
                .0
                .iter()
                .flatten()
                .all(|&i| i == Intensity(0x80))
        );

        assert_eq!(
            unsafe { autd3_pattern_set_intensity(0, std::ptr::null_mut()) },
            -1
        );
    }

    #[test]
    fn device_level_functions_write_the_given_array() {
        let geometry = geometry();
        let n = Autd3::NUM_TRANSDUCERS;
        let mut dst = vec![0u8; n];
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
        for (tr, &p) in dst.iter().enumerate() {
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
            assert_eq!(p, phase);
        }

        let result = unsafe { autd3_pattern_set_phase_device(0x33, dst.as_mut_ptr(), dst.len()) };
        assert_eq!(result, 0);
        assert!(dst.iter().all(|&p| p == 0x33));

        let result = unsafe { autd3_pattern_add_phase_device(0xF0, dst.as_mut_ptr(), dst.len()) };
        assert_eq!(result, 0);
        assert!(dst.iter().all(|&p| p == 0x23));

        let result =
            unsafe { autd3_pattern_set_intensity_device(0x11, dst.as_mut_ptr(), dst.len()) };
        assert_eq!(result, 0);
        assert!(dst.iter().all(|&i| i == 0x11));

        let result = unsafe { autd3_pattern_set_phase_device(0x00, std::ptr::null_mut(), 1) };
        assert_eq!(result, -1);
        let result = unsafe {
            autd3_pattern_focus_device(
                &raw const geometry,
                2,
                target.as_ptr(),
                8.5,
                dst.as_mut_ptr(),
            )
        };
        assert_eq!(result, -1);
    }

    #[test]
    fn group_writes_the_source_of_each_key_and_the_type_specific_null() {
        let geometry = geometry();
        let keys = keys(&geometry, sides);

        let left = phases(&geometry, 0x10);
        let right = phases(&geometry, 0x20);
        let mut dst = phases(&geometry, 0xFF);
        let sources = [&raw const left, &raw const right];
        let result = unsafe {
            autd3_pattern_group_phase(
                &raw const geometry,
                keys.as_ptr(),
                sources.as_ptr(),
                sources.len(),
                &raw mut dst,
            )
        };
        assert_eq!(result, 0);

        let left_i = intensities(&geometry, 0x30);
        let right_i = intensities(&geometry, 0x40);
        let mut dst_i = intensities(&geometry, 0xFF);
        let sources_i = [&raw const left_i, &raw const right_i];
        let result = unsafe {
            autd3_pattern_group_intensity(
                &raw const geometry,
                keys.as_ptr(),
                sources_i.as_ptr(),
                sources_i.len(),
                &raw mut dst_i,
            )
        };
        assert_eq!(result, 0);

        for dev in 0..2 {
            for tr in 0..Autd3::NUM_TRANSDUCERS {
                let (p, i) = match sides(dev, tr) {
                    0 => (Phase(0x10), Intensity(0x30)),
                    1 => (Phase(0x20), Intensity(0x40)),
                    _ => (Phase::ZERO, Intensity::MIN),
                };
                assert_eq!(dst.0[dev][tr], p, "dev {dev} tr {tr}");
                assert_eq!(dst_i.0[dev][tr], i, "dev {dev} tr {tr}");
            }
        }
    }

    #[test]
    fn group_rejects_invalid_arguments_without_writing() {
        let geometry = geometry();
        let left = phases(&geometry, 0x10);
        let mut dst = phases(&geometry, 0xFF);
        let zeros = keys(&geometry, |_, _| 0);
        let out_of_range = keys(&geometry, |_, tr| i32::from(tr == 5));
        let single_geometry = Geometry::new(vec![Autd3::default()]);
        let single = phases(&single_geometry, 0x10);
        let dst_ptr = &raw mut dst;

        let call = |keys: &[i32], sources: &[*const PhaseBuffer]| unsafe {
            autd3_pattern_group_phase(
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
                autd3_pattern_group_phase(
                    std::ptr::null(),
                    zeros.as_ptr(),
                    [&raw const left].as_ptr(),
                    1,
                    dst_ptr,
                )
            },
            -1
        );

        assert!(dst.0.iter().flatten().all(|&p| p == Phase(0xFF)));
    }

    #[test]
    fn group_null_and_copy_write_only_the_selected_transducers() {
        let geometry = geometry();
        let indices = keys(&geometry, sides);

        let source = phases(&geometry, 0x10);
        let mut dst = phases(&geometry, 0xFF);
        assert_eq!(
            unsafe {
                autd3_pattern_group_null_phase(&raw const geometry, indices.as_ptr(), &raw mut dst)
            },
            0
        );
        assert_eq!(
            unsafe {
                autd3_pattern_group_copy_phase(
                    &raw const geometry,
                    indices.as_ptr(),
                    1,
                    &raw const source,
                    &raw mut dst,
                )
            },
            0
        );

        let source_i = intensities(&geometry, 0x20);
        let mut dst_i = intensities(&geometry, 0xFF);
        assert_eq!(
            unsafe {
                autd3_pattern_group_null_intensity(
                    &raw const geometry,
                    indices.as_ptr(),
                    &raw mut dst_i,
                )
            },
            0
        );
        assert_eq!(
            unsafe {
                autd3_pattern_group_copy_intensity(
                    &raw const geometry,
                    indices.as_ptr(),
                    1,
                    &raw const source_i,
                    &raw mut dst_i,
                )
            },
            0
        );

        for dev in 0..2 {
            for tr in 0..Autd3::NUM_TRANSDUCERS {
                let (p, i) = match sides(dev, tr) {
                    0 => (Phase(0xFF), Intensity(0xFF)),
                    1 => (Phase(0x10), Intensity(0x20)),
                    _ => (Phase::ZERO, Intensity::MIN),
                };
                assert_eq!(dst.0[dev][tr], p, "dev {dev} tr {tr}");
                assert_eq!(dst_i.0[dev][tr], i, "dev {dev} tr {tr}");
            }
        }
    }

    #[test]
    fn group_null_and_copy_reject_invalid_arguments() {
        let geometry = geometry();
        let source = phases(&geometry, 0x10);
        let mut dst = phases(&geometry, 0xFF);
        let indices = keys(&geometry, |_, _| 0);
        let single_geometry = Geometry::new(vec![Autd3::default()]);
        let single = phases(&single_geometry, 0x10);
        let dst_ptr = &raw mut dst;

        let copy = |index: i32, source: *const PhaseBuffer| unsafe {
            autd3_pattern_group_copy_phase(
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
                autd3_pattern_group_copy_phase(
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
            unsafe { autd3_pattern_group_null_phase(std::ptr::null(), indices.as_ptr(), dst_ptr) },
            -1
        );
        assert_eq!(
            unsafe {
                autd3_pattern_group_null_phase(&raw const geometry, std::ptr::null(), dst_ptr)
            },
            -1
        );
        assert_eq!(
            unsafe {
                autd3_pattern_group_null_intensity(
                    &raw const geometry,
                    indices.as_ptr(),
                    std::ptr::null_mut(),
                )
            },
            -1
        );

        assert!(dst.0.iter().flatten().all(|&p| p == Phase(0xFF)));
    }

    #[test]
    fn laguerre_gaussian_geometry_level_matches_the_rust_api() {
        let geometry = geometry();
        let target = [86.0_f32, 66.0, 150.0];
        let axis = [0.0_f32, 0.0, 1.0];
        let option = autd3_rs_pattern::LaguerreGaussianOption {
            p: 1,
            l: 2,
            waist: Length::from_mm(10.0),
        };
        let lambda = Length::from_mm(8.5);
        let rust_target = Point3::new(86.0, 66.0, 150.0);

        let mut phase_buffer = phases(&geometry, 0);
        let mut intensity_buffer = intensities(&geometry, 0);
        let result = unsafe {
            autd3_pattern_laguerre_gaussian_phase(
                &raw const geometry,
                target.as_ptr(),
                axis.as_ptr(),
                1,
                2,
                10.0,
                8.5,
                &raw mut phase_buffer,
            )
        };
        assert_eq!(result, 0);
        let result = unsafe {
            autd3_pattern_laguerre_gaussian_intensity(
                &raw const geometry,
                target.as_ptr(),
                axis.as_ptr(),
                1,
                2,
                10.0,
                8.5,
                &raw mut intensity_buffer,
            )
        };
        assert_eq!(result, 0);

        let mut expected_p = geometry.phase_buffer();
        let mut expected_i = geometry.intensity_buffer();
        autd3_rs_pattern::laguerre_gaussian_phase(
            &geometry,
            rust_target,
            Vector3::z_axis(),
            option,
            lambda,
            &mut expected_p,
        );
        autd3_rs_pattern::laguerre_gaussian_intensity(
            &geometry,
            rust_target,
            Vector3::z_axis(),
            option,
            lambda,
            &mut expected_i,
        );
        assert_eq!(phase_buffer.0, expected_p);
        assert_eq!(intensity_buffer.0, expected_i);
    }

    #[test]
    fn laguerre_gaussian_device_level_matches_the_rust_api() {
        let geometry = geometry();
        let target = [86.0_f32, 66.0, 150.0];
        let axis = [0.0_f32, 0.0, 1.0];
        let option = autd3_rs_pattern::LaguerreGaussianOption {
            p: 1,
            l: 2,
            waist: Length::from_mm(10.0),
        };
        let lambda = Length::from_mm(8.5);
        let rust_target = Point3::new(86.0, 66.0, 150.0);

        let mut expected_p = geometry.phase_buffer();
        let mut expected_i = geometry.intensity_buffer();
        autd3_rs_pattern::laguerre_gaussian_phase(
            &geometry,
            rust_target,
            Vector3::z_axis(),
            option,
            lambda,
            &mut expected_p,
        );
        autd3_rs_pattern::laguerre_gaussian_intensity(
            &geometry,
            rust_target,
            Vector3::z_axis(),
            option,
            lambda,
            &mut expected_i,
        );

        let mut dst_p = vec![0u8; Autd3::NUM_TRANSDUCERS];
        let mut dst_i = vec![0u8; Autd3::NUM_TRANSDUCERS];
        let result = unsafe {
            autd3_pattern_laguerre_gaussian_phase_device(
                &raw const geometry,
                1,
                target.as_ptr(),
                axis.as_ptr(),
                1,
                2,
                10.0,
                8.5,
                dst_p.as_mut_ptr(),
            )
        };
        assert_eq!(result, 0);
        let result = unsafe {
            autd3_pattern_laguerre_gaussian_intensity_device(
                &raw const geometry,
                1,
                target.as_ptr(),
                axis.as_ptr(),
                1,
                2,
                10.0,
                8.5,
                dst_i.as_mut_ptr(),
            )
        };
        assert_eq!(result, 0);
        for (tr, (&p, &i)) in dst_p.iter().zip(&dst_i).enumerate() {
            assert_eq!(p, expected_p[1][tr].0);
            assert_eq!(i, expected_i[1][tr].0);
            let position = geometry[1].position(tr);
            let pos = [position.x, position.y, position.z];
            let mut phase = 0u8;
            let result = unsafe {
                autd3_pattern_laguerre_gaussian_phase_transducer(
                    pos.as_ptr(),
                    target.as_ptr(),
                    axis.as_ptr(),
                    1,
                    2,
                    10.0,
                    8.5,
                    &raw mut phase,
                )
            };
            assert_eq!(result, 0);
            assert_eq!(phase, p);
        }
    }

    #[test]
    fn hermite_gaussian_geometry_level_matches_the_rust_api() {
        let geometry = geometry();
        let target = [86.0_f32, 66.0, 150.0];
        let axis = [0.0_f32, 0.0, 1.0];
        let x_dir = [1.0_f32, 1.0, 0.0];
        let option = autd3_rs_pattern::HermiteGaussianOption {
            m: 1,
            n: 2,
            waist: Length::from_mm(10.0),
        };
        let lambda = Length::from_mm(8.5);
        let rust_target = Point3::new(86.0, 66.0, 150.0);
        let rust_x_dir = UnitVector3::new_normalize(Vector3::new(1.0, 1.0, 0.0));

        let mut phase_buffer = phases(&geometry, 0);
        let mut intensity_buffer = intensities(&geometry, 0);
        let result = unsafe {
            autd3_pattern_hermite_gaussian_phase(
                &raw const geometry,
                target.as_ptr(),
                axis.as_ptr(),
                x_dir.as_ptr(),
                1,
                2,
                10.0,
                8.5,
                &raw mut phase_buffer,
            )
        };
        assert_eq!(result, 0);
        let result = unsafe {
            autd3_pattern_hermite_gaussian_intensity(
                &raw const geometry,
                target.as_ptr(),
                axis.as_ptr(),
                x_dir.as_ptr(),
                1,
                2,
                10.0,
                8.5,
                &raw mut intensity_buffer,
            )
        };
        assert_eq!(result, 0);

        let mut expected_p = geometry.phase_buffer();
        let mut expected_i = geometry.intensity_buffer();
        autd3_rs_pattern::hermite_gaussian_phase(
            &geometry,
            rust_target,
            Vector3::z_axis(),
            rust_x_dir,
            option,
            lambda,
            &mut expected_p,
        );
        autd3_rs_pattern::hermite_gaussian_intensity(
            &geometry,
            rust_target,
            Vector3::z_axis(),
            rust_x_dir,
            option,
            lambda,
            &mut expected_i,
        );
        assert_eq!(phase_buffer.0, expected_p);
        assert_eq!(intensity_buffer.0, expected_i);
    }

    #[test]
    fn hermite_gaussian_device_level_matches_the_rust_api() {
        let geometry = geometry();
        let target = [86.0_f32, 66.0, 150.0];
        let axis = [0.0_f32, 0.0, 1.0];
        let x_dir = [1.0_f32, 1.0, 0.0];
        let option = autd3_rs_pattern::HermiteGaussianOption {
            m: 1,
            n: 2,
            waist: Length::from_mm(10.0),
        };
        let lambda = Length::from_mm(8.5);
        let rust_target = Point3::new(86.0, 66.0, 150.0);
        let rust_x_dir = UnitVector3::new_normalize(Vector3::new(1.0, 1.0, 0.0));

        let mut expected_p = geometry.phase_buffer();
        let mut expected_i = geometry.intensity_buffer();
        autd3_rs_pattern::hermite_gaussian_phase(
            &geometry,
            rust_target,
            Vector3::z_axis(),
            rust_x_dir,
            option,
            lambda,
            &mut expected_p,
        );
        autd3_rs_pattern::hermite_gaussian_intensity(
            &geometry,
            rust_target,
            Vector3::z_axis(),
            rust_x_dir,
            option,
            lambda,
            &mut expected_i,
        );

        let mut dst_p = vec![0u8; Autd3::NUM_TRANSDUCERS];
        let mut dst_i = vec![0u8; Autd3::NUM_TRANSDUCERS];
        let result = unsafe {
            autd3_pattern_hermite_gaussian_phase_device(
                &raw const geometry,
                1,
                target.as_ptr(),
                axis.as_ptr(),
                x_dir.as_ptr(),
                1,
                2,
                10.0,
                8.5,
                dst_p.as_mut_ptr(),
            )
        };
        assert_eq!(result, 0);
        let result = unsafe {
            autd3_pattern_hermite_gaussian_intensity_device(
                &raw const geometry,
                1,
                target.as_ptr(),
                axis.as_ptr(),
                x_dir.as_ptr(),
                1,
                2,
                10.0,
                8.5,
                dst_i.as_mut_ptr(),
            )
        };
        assert_eq!(result, 0);
        for (tr, (&p, &i)) in dst_p.iter().zip(&dst_i).enumerate() {
            assert_eq!(p, expected_p[1][tr].0);
            assert_eq!(i, expected_i[1][tr].0);
            let position = geometry[1].position(tr);
            let pos = [position.x, position.y, position.z];
            let mut phase = 0u8;
            let result = unsafe {
                autd3_pattern_hermite_gaussian_phase_transducer(
                    pos.as_ptr(),
                    target.as_ptr(),
                    axis.as_ptr(),
                    x_dir.as_ptr(),
                    1,
                    2,
                    10.0,
                    8.5,
                    &raw mut phase,
                )
            };
            assert_eq!(result, 0);
            assert_eq!(phase, p);
        }
    }

    #[test]
    fn gaussian_beams_reject_invalid_waist_without_writing() {
        let geometry = geometry();
        let target = [0.0_f32, 0.0, 150.0];
        let axis = [0.0_f32, 0.0, 1.0];
        let x_dir = [1.0_f32, 0.0, 0.0];
        for waist_mm in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
            let mut phase_buffer = phases(&geometry, 0x5A);
            let mut intensity_buffer = intensities(&geometry, 0x5A);
            let lg = unsafe {
                autd3_pattern_laguerre_gaussian_intensity(
                    &raw const geometry,
                    target.as_ptr(),
                    axis.as_ptr(),
                    0,
                    1,
                    waist_mm,
                    8.5,
                    &raw mut intensity_buffer,
                )
            };
            let hg = unsafe {
                autd3_pattern_hermite_gaussian_phase(
                    &raw const geometry,
                    target.as_ptr(),
                    axis.as_ptr(),
                    x_dir.as_ptr(),
                    1,
                    0,
                    waist_mm,
                    8.5,
                    &raw mut phase_buffer,
                )
            };
            assert_eq!((lg, hg), (-1, -1));
            assert!(phase_buffer.0.iter().flatten().all(|&p| p == Phase(0x5A)));
            assert!(
                intensity_buffer
                    .0
                    .iter()
                    .flatten()
                    .all(|&i| i == Intensity(0x5A))
            );

            let mut phase = 0u8;
            let result = unsafe {
                autd3_pattern_laguerre_gaussian_phase_transducer(
                    target.as_ptr(),
                    target.as_ptr(),
                    axis.as_ptr(),
                    0,
                    0,
                    waist_mm,
                    8.5,
                    &raw mut phase,
                )
            };
            assert_eq!(result, -1);
        }

        let mut short = Buffer(vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]]);
        let result = unsafe {
            autd3_pattern_laguerre_gaussian_phase(
                &raw const geometry,
                target.as_ptr(),
                axis.as_ptr(),
                0,
                1,
                10.0,
                8.5,
                &raw mut short,
            )
        };
        assert_eq!(result, -1);
    }
}
