use autd3_python_capsule::cell::{BufferCell, ExclusiveGuard, SharedGuard};
use autd3_python_capsule::extract::{
    extract_angle, extract_direction, extract_length, extract_point, extract_u8, extract_velocity,
    length_to_py,
};
use autd3_python_capsule::numpy;
use autd3_python_capsule::{capsule_of, device_from_capsule, geometry_from_capsule};
use autd3_rs_core::Length;
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::geometry::TransducerMask as CoreTransducerMask;
use autd3_rs_core::value::{Intensity, Phase};
use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use zerocopy::{FromBytes, IntoBytes};

fn phase_to_py(py: Python<'_>, phase: Phase) -> PyResult<Py<PyAny>> {
    Ok(autd3_python_capsule::extract::phase_to_py(py, phase)?.unbind())
}

fn intensity_to_py(py: Python<'_>, intensity: Intensity) -> PyResult<Py<PyAny>> {
    Ok(autd3_python_capsule::extract::intensity_to_py(py, intensity)?.unbind())
}

fn extract_intensity(obj: &Bound<'_, PyAny>) -> PyResult<Intensity> {
    Ok(Intensity(extract_u8(obj)?))
}

fn extract_phase(obj: &Bound<'_, PyAny>) -> PyResult<Phase> {
    Ok(Phase(extract_u8(obj)?))
}

macro_rules! buffer_class {
    (
        $buffer:ident,
        $view:ident,
        $name:literal,
        $view_name:literal,
        $ty:ty,
        $init:expr,
        $extract:ident,
        $to_py:ident,
        $capsule:path
    ) => {
        #[pyclass(name = $name, module = "autd3_pattern", frozen)]
        pub struct $buffer(BufferCell<$ty>);

        impl $buffer {
            fn from_vec(inner: Vec<Vec<$ty>>) -> Self {
                Self(BufferCell::new(inner))
            }

            fn read(&self) -> PyResult<SharedGuard<'_, $ty>> {
                Ok(self.0.try_read()?)
            }

            fn write(&self) -> PyResult<ExclusiveGuard<'_, $ty>> {
                Ok(self.0.try_write()?)
            }
        }

        #[pymethods]
        impl $buffer {
            #[new]
            fn new(num_devices: usize) -> Self {
                Self::from_vec(vec![vec![$init; Autd3::NUM_TRANSDUCERS]; num_devices])
            }

            #[staticmethod]
            fn from_array(values: &Bound<'_, PyAny>) -> PyResult<$buffer> {
                if numpy::is_ndarray(values)? {
                    let bytes = numpy::u8_matrix_bytes(values, Autd3::NUM_TRANSDUCERS, None)?;
                    let inner = <[$ty]>::ref_from_bytes(bytes.as_bytes())
                        .map_err(|e| PyValueError::new_err(e.to_string()))?
                        .chunks_exact(Autd3::NUM_TRANSDUCERS)
                        .map(<[$ty]>::to_vec)
                        .collect::<Vec<Vec<$ty>>>();
                    return Ok($buffer::from_vec(inner));
                }
                let values: Vec<Vec<Bound<'_, PyAny>>> = values.extract().map_err(|e| {
                    PyTypeError::new_err(format!(
                        "values must be a uint8 numpy.ndarray or a list of per-device lists: {e}"
                    ))
                })?;
                let mut inner = Vec::with_capacity(values.len());
                for device in values {
                    if device.len() != Autd3::NUM_TRANSDUCERS {
                        return Err(PyValueError::new_err(format!(
                            "each device needs {} values, got {}",
                            Autd3::NUM_TRANSDUCERS,
                            device.len()
                        )));
                    }
                    inner.push(device.iter().map($extract).collect::<PyResult<Vec<_>>>()?);
                }
                Ok($buffer::from_vec(inner))
            }

            fn copy_from(&self, values: &Bound<'_, PyAny>) -> PyResult<()> {
                let num_devices = self.read()?.len();
                let bytes =
                    numpy::u8_matrix_bytes(values, Autd3::NUM_TRANSDUCERS, Some(num_devices))?;
                let mut this = self.write()?;
                for (dst, src) in this
                    .iter_mut()
                    .zip(bytes.as_bytes().chunks_exact(Autd3::NUM_TRANSDUCERS))
                {
                    dst.as_mut_bytes().copy_from_slice(src);
                }
                Ok(())
            }

            fn to_numpy<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                let this = self.read()?;
                numpy::u8_matrix(py, this.len(), Autd3::NUM_TRANSDUCERS, |dst| {
                    for (dst, src) in dst
                        .chunks_exact_mut(Autd3::NUM_TRANSDUCERS)
                        .zip(this.iter())
                    {
                        dst.copy_from_slice(src.as_bytes());
                    }
                })
            }

            fn num_devices(&self) -> PyResult<usize> {
                Ok(self.read()?.len())
            }

            fn __len__(&self) -> PyResult<usize> {
                Ok(self.read()?.len())
            }

            fn __getitem__(slf: &Bound<'_, Self>, index: usize) -> PyResult<$view> {
                if index >= slf.get().read()?.len() {
                    return Err(PyIndexError::new_err("device index out of range"));
                }
                Ok($view {
                    buffer: slf.clone().unbind(),
                    device: index,
                })
            }

            fn _capsule<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyCapsule>> {
                let py = slf.py();
                let cell = core::ptr::NonNull::from(&slf.get().0);
                unsafe { $capsule(py, cell, slf.clone().into_any().unbind()) }
            }
        }

        #[pyclass(name = $view_name, module = "autd3_pattern", frozen)]
        pub struct $view {
            buffer: Py<$buffer>,
            device: usize,
        }

        #[pymethods]
        impl $view {
            fn __len__(&self) -> PyResult<usize> {
                Ok(self.buffer.get().read()?[self.device].len())
            }

            fn __getitem__(&self, py: Python<'_>, index: usize) -> PyResult<Py<PyAny>> {
                let value = *self.buffer.get().read()?[self.device]
                    .get(index)
                    .ok_or_else(|| PyIndexError::new_err("transducer index out of range"))?;
                $to_py(py, value)
            }

            fn __setitem__(&self, index: usize, value: &Bound<'_, PyAny>) -> PyResult<()> {
                let value = $extract(value)?;
                *self.buffer.get().write()?[self.device]
                    .get_mut(index)
                    .ok_or_else(|| PyIndexError::new_err("transducer index out of range"))? = value;
                Ok(())
            }
        }
    };
}

buffer_class!(
    PhaseBuffer,
    DevicePhaseView,
    "PhaseBuffer",
    "DevicePhaseView",
    Phase,
    Phase::ZERO,
    extract_phase,
    phase_to_py,
    autd3_python_capsule::phase_capsule
);

buffer_class!(
    IntensityBuffer,
    DeviceIntensityView,
    "IntensityBuffer",
    "DeviceIntensityView",
    Intensity,
    Intensity::MAX,
    extract_intensity,
    intensity_to_py,
    autd3_python_capsule::intensity_capsule
);

#[pyfunction]
fn wavelength<'py>(sound_speed: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    length_to_py(
        sound_speed.py(),
        autd3_rs_pattern::wavelength(extract_velocity(sound_speed)?),
    )
}

#[pyfunction]
#[pyo3(signature = (geometry, target, wavelength, dst))]
fn focus(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    wavelength: &Bound<'_, PyAny>,
    dst: PyRef<'_, PhaseBuffer>,
) -> PyResult<()> {
    let mut dst = dst.write()?;
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    check_slots(geometry, &dst)?;
    let target = extract_point(target)?;
    autd3_rs_pattern::focus(geometry, target, extract_length(wavelength)?, &mut dst);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, direction, wavelength, dst))]
fn plane(
    geometry: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    wavelength: &Bound<'_, PyAny>,
    dst: PyRef<'_, PhaseBuffer>,
) -> PyResult<()> {
    let mut dst = dst.write()?;
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    check_slots(geometry, &dst)?;
    let direction = extract_direction(direction)?;
    autd3_rs_pattern::plane(geometry, direction, extract_length(wavelength)?, &mut dst);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, apex, direction, theta, wavelength, dst))]
fn bessel(
    geometry: &Bound<'_, PyAny>,
    apex: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    theta: &Bound<'_, PyAny>,
    wavelength: &Bound<'_, PyAny>,
    dst: PyRef<'_, PhaseBuffer>,
) -> PyResult<()> {
    let mut dst = dst.write()?;
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    check_slots(geometry, &dst)?;
    let apex = extract_point(apex)?;
    let direction = extract_direction(direction)?;
    let theta = extract_angle(theta)?;
    autd3_rs_pattern::bessel(
        geometry,
        apex,
        direction,
        theta,
        extract_length(wavelength)?,
        &mut dst,
    );
    Ok(())
}

fn check_waist(waist: Length) -> PyResult<()> {
    if waist.mm().is_finite() && waist.mm() > 0.0 {
        Ok(())
    } else {
        Err(PyValueError::new_err(
            "waist must be a positive finite length",
        ))
    }
}

fn check_slots<T>(geometry: &autd3_rs_core::Geometry, dst: &[Vec<T>]) -> PyResult<()> {
    if dst.len() == geometry.num_devices() {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "dst must have one slot per device: the geometry has {} devices, but dst has {}",
            geometry.num_devices(),
            dst.len()
        )))
    }
}

#[pyclass(name = "LaguerreGaussianOption", module = "autd3_pattern", eq, frozen)]
#[derive(PartialEq)]
pub struct LaguerreGaussianOption(autd3_rs_pattern::LaguerreGaussianOption);

impl LaguerreGaussianOption {
    fn checked(&self) -> PyResult<autd3_rs_pattern::LaguerreGaussianOption> {
        check_waist(self.0.waist)?;
        Ok(self.0)
    }
}

#[pymethods]
impl LaguerreGaussianOption {
    #[new]
    #[pyo3(signature = (p, l, waist))]
    fn new(p: u32, l: i32, #[pyo3(from_py_with = extract_length)] waist: Length) -> Self {
        Self(autd3_rs_pattern::LaguerreGaussianOption { p, l, waist })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    #[getter]
    fn p(&self) -> u32 {
        self.0.p
    }

    #[getter]
    fn l(&self) -> i32 {
        self.0.l
    }

    #[getter]
    fn waist<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        length_to_py(py, self.0.waist)
    }
}

#[pyclass(name = "HermiteGaussianOption", module = "autd3_pattern", eq, frozen)]
#[derive(PartialEq)]
pub struct HermiteGaussianOption(autd3_rs_pattern::HermiteGaussianOption);

impl HermiteGaussianOption {
    fn checked(&self) -> PyResult<autd3_rs_pattern::HermiteGaussianOption> {
        check_waist(self.0.waist)?;
        Ok(self.0)
    }
}

#[pymethods]
impl HermiteGaussianOption {
    #[new]
    #[pyo3(signature = (m, n, waist))]
    fn new(m: u32, n: u32, #[pyo3(from_py_with = extract_length)] waist: Length) -> Self {
        Self(autd3_rs_pattern::HermiteGaussianOption { m, n, waist })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    #[getter]
    fn m(&self) -> u32 {
        self.0.m
    }

    #[getter]
    fn n(&self) -> u32 {
        self.0.n
    }

    #[getter]
    fn waist<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        length_to_py(py, self.0.waist)
    }
}

#[pyfunction]
#[pyo3(signature = (geometry, target, axis, option, wavelength, dst))]
fn laguerre_gaussian_phase(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    axis: &Bound<'_, PyAny>,
    option: &LaguerreGaussianOption,
    wavelength: &Bound<'_, PyAny>,
    dst: PyRef<'_, PhaseBuffer>,
) -> PyResult<()> {
    let mut dst = dst.write()?;
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    check_slots(geometry, &dst)?;
    autd3_rs_pattern::laguerre_gaussian_phase(
        geometry,
        extract_point(target)?,
        extract_direction(axis)?,
        option.checked()?,
        extract_length(wavelength)?,
        &mut dst,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, target, axis, option, wavelength, dst))]
fn laguerre_gaussian_intensity(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    axis: &Bound<'_, PyAny>,
    option: &LaguerreGaussianOption,
    wavelength: &Bound<'_, PyAny>,
    dst: PyRef<'_, IntensityBuffer>,
) -> PyResult<()> {
    let mut dst = dst.write()?;
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    check_slots(geometry, &dst)?;
    autd3_rs_pattern::laguerre_gaussian_intensity(
        geometry,
        extract_point(target)?,
        extract_direction(axis)?,
        option.checked()?,
        extract_length(wavelength)?,
        &mut dst,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, target, axis, x_dir, option, wavelength, dst))]
fn hermite_gaussian_phase(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    axis: &Bound<'_, PyAny>,
    x_dir: &Bound<'_, PyAny>,
    option: &HermiteGaussianOption,
    wavelength: &Bound<'_, PyAny>,
    dst: PyRef<'_, PhaseBuffer>,
) -> PyResult<()> {
    let mut dst = dst.write()?;
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    check_slots(geometry, &dst)?;
    autd3_rs_pattern::hermite_gaussian_phase(
        geometry,
        extract_point(target)?,
        extract_direction(axis)?,
        extract_direction(x_dir)?,
        option.checked()?,
        extract_length(wavelength)?,
        &mut dst,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, target, axis, x_dir, option, wavelength, dst))]
fn hermite_gaussian_intensity(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    axis: &Bound<'_, PyAny>,
    x_dir: &Bound<'_, PyAny>,
    option: &HermiteGaussianOption,
    wavelength: &Bound<'_, PyAny>,
    dst: PyRef<'_, IntensityBuffer>,
) -> PyResult<()> {
    let mut dst = dst.write()?;
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    check_slots(geometry, &dst)?;
    autd3_rs_pattern::hermite_gaussian_intensity(
        geometry,
        extract_point(target)?,
        extract_direction(axis)?,
        extract_direction(x_dir)?,
        option.checked()?,
        extract_length(wavelength)?,
        &mut dst,
    );
    Ok(())
}

macro_rules! device_fn {
    ($name:ident, $core:path, $ty:ty, $init:expr, ($($arg:ident: $extract:expr),*)) => {
        #[pyfunction]
        #[pyo3(signature = (device, $($arg,)* dst))]
        fn $name(
            device: &Bound<'_, PyAny>,
            $($arg: &Bound<'_, PyAny>,)*
            dst: &Bound<'_, PyAny>,
        ) -> PyResult<()> {
            let capsule = capsule_of(device)?;
            let device = device_from_capsule(&capsule)?;
            let dst = numpy::u8_vector_dst(dst, device.num_transducers())?;
            let mut values: Vec<$ty> = vec![$init; device.num_transducers()];
            $core(device, $($extract($arg)?,)* &mut values);
            numpy::u8_vector_write(dst, values.as_bytes())
        }
    };
}

macro_rules! transducer_fn {
    ($name:ident, $core:path, ($($arg:ident: $extract:expr),*)) => {
        #[pyfunction]
        #[pyo3(signature = (position, $($arg),*))]
        fn $name(
            position: &Bound<'_, PyAny>,
            $($arg: &Bound<'_, PyAny>,)*
        ) -> PyResult<Py<PyAny>> {
            phase_to_py(
                position.py(),
                $core(extract_point(position)?, $($extract($arg)?),*),
            )
        }
    };
}

fn extract_lg_option(
    option: &Bound<'_, PyAny>,
) -> PyResult<autd3_rs_pattern::LaguerreGaussianOption> {
    option.cast::<LaguerreGaussianOption>()?.get().checked()
}

fn extract_hg_option(
    option: &Bound<'_, PyAny>,
) -> PyResult<autd3_rs_pattern::HermiteGaussianOption> {
    option.cast::<HermiteGaussianOption>()?.get().checked()
}

device_fn!(
    focus_device,
    autd3_rs_pattern::focus_device,
    Phase,
    Phase::ZERO,
    (target: extract_point, wavelength: extract_length)
);
transducer_fn!(
    focus_transducer,
    autd3_rs_pattern::focus_transducer,
    (target: extract_point, wavelength: extract_length)
);
device_fn!(
    plane_device,
    autd3_rs_pattern::plane_device,
    Phase,
    Phase::ZERO,
    (direction: extract_direction, wavelength: extract_length)
);
transducer_fn!(
    plane_transducer,
    autd3_rs_pattern::plane_transducer,
    (direction: extract_direction, wavelength: extract_length)
);
device_fn!(
    bessel_device,
    autd3_rs_pattern::bessel_device,
    Phase,
    Phase::ZERO,
    (
        apex: extract_point,
        direction: extract_direction,
        theta: extract_angle,
        wavelength: extract_length
    )
);
transducer_fn!(
    bessel_transducer,
    autd3_rs_pattern::bessel_transducer,
    (
        apex: extract_point,
        direction: extract_direction,
        theta: extract_angle,
        wavelength: extract_length
    )
);
device_fn!(
    laguerre_gaussian_phase_device,
    autd3_rs_pattern::laguerre_gaussian_phase_device,
    Phase,
    Phase::ZERO,
    (
        target: extract_point,
        axis: extract_direction,
        option: extract_lg_option,
        wavelength: extract_length
    )
);
transducer_fn!(
    laguerre_gaussian_phase_transducer,
    autd3_rs_pattern::laguerre_gaussian_phase_transducer,
    (
        target: extract_point,
        axis: extract_direction,
        option: extract_lg_option,
        wavelength: extract_length
    )
);
device_fn!(
    laguerre_gaussian_intensity_device,
    autd3_rs_pattern::laguerre_gaussian_intensity_device,
    Intensity,
    Intensity::MAX,
    (
        target: extract_point,
        axis: extract_direction,
        option: extract_lg_option,
        wavelength: extract_length
    )
);
device_fn!(
    hermite_gaussian_phase_device,
    autd3_rs_pattern::hermite_gaussian_phase_device,
    Phase,
    Phase::ZERO,
    (
        target: extract_point,
        axis: extract_direction,
        x_dir: extract_direction,
        option: extract_hg_option,
        wavelength: extract_length
    )
);
transducer_fn!(
    hermite_gaussian_phase_transducer,
    autd3_rs_pattern::hermite_gaussian_phase_transducer,
    (
        target: extract_point,
        axis: extract_direction,
        x_dir: extract_direction,
        option: extract_hg_option,
        wavelength: extract_length
    )
);
device_fn!(
    hermite_gaussian_intensity_device,
    autd3_rs_pattern::hermite_gaussian_intensity_device,
    Intensity,
    Intensity::MAX,
    (
        target: extract_point,
        axis: extract_direction,
        x_dir: extract_direction,
        option: extract_hg_option,
        wavelength: extract_length
    )
);

#[pyfunction]
#[pyo3(signature = (intensity, dst))]
fn set_intensity(intensity: &Bound<'_, PyAny>, dst: PyRef<'_, IntensityBuffer>) -> PyResult<()> {
    let mut dst = dst.write()?;
    autd3_rs_pattern::set_intensity(extract_intensity(intensity)?, &mut dst);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (phase, dst))]
fn set_phase(phase: &Bound<'_, PyAny>, dst: PyRef<'_, PhaseBuffer>) -> PyResult<()> {
    let mut dst = dst.write()?;
    autd3_rs_pattern::set_phase(extract_phase(phase)?, &mut dst);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (phase, dst))]
fn add_phase(phase: &Bound<'_, PyAny>, dst: PyRef<'_, PhaseBuffer>) -> PyResult<()> {
    let mut dst = dst.write()?;
    autd3_rs_pattern::add_phase(extract_phase(phase)?, &mut dst);
    Ok(())
}

#[pyclass(name = "TransducerMask", module = "autd3_pattern", from_py_object)]
#[derive(Clone)]
pub struct TransducerMask {
    mask: Option<Vec<Vec<bool>>>,
}

#[pymethods]
impl TransducerMask {
    #[classattr]
    #[pyo3(name = "AllEnabled")]
    fn all_enabled() -> Self {
        Self { mask: None }
    }

    #[staticmethod]
    fn masked(mask: Vec<Vec<bool>>) -> Self {
        Self { mask: Some(mask) }
    }

    fn validate(&self, geometry: &Bound<'_, PyAny>) -> PyResult<()> {
        let capsule = capsule_of(geometry)?;
        let geometry = geometry_from_capsule(&capsule)?;
        self.as_core()
            .validate(geometry)
            .map_err(|e| autd3_python_capsule::invalid_argument(capsule.py(), e))
    }

    fn is_enabled(&self, device: usize, transducer: usize) -> PyResult<bool> {
        if let Some(mask) = &self.mask
            && mask.get(device).and_then(|d| d.get(transducer)).is_none()
        {
            return Err(PyIndexError::new_err("transducer index out of range"));
        }
        Ok(self.as_core().is_enabled(device, transducer))
    }

    fn num_enabled(&self, geometry: &Bound<'_, PyAny>) -> PyResult<usize> {
        let capsule = capsule_of(geometry)?;
        Ok(self.as_core().num_enabled(geometry_from_capsule(&capsule)?))
    }

    fn _mask(&self) -> Option<Vec<Vec<bool>>> {
        self.mask.clone()
    }
}

impl TransducerMask {
    fn as_core(&self) -> CoreTransducerMask<'_> {
        match &self.mask {
            Some(mask) => CoreTransducerMask::Masked(mask),
            None => CoreTransducerMask::AllEnabled,
        }
    }
}

#[pymodule(gil_used = false)]
fn autd3_pattern(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PhaseBuffer>()?;
    m.add_class::<DevicePhaseView>()?;
    m.add_class::<IntensityBuffer>()?;
    m.add_class::<DeviceIntensityView>()?;
    m.add_class::<TransducerMask>()?;
    m.add_class::<LaguerreGaussianOption>()?;
    m.add_class::<HermiteGaussianOption>()?;
    m.add_function(wrap_pyfunction!(wavelength, m)?)?;
    m.add_function(wrap_pyfunction!(focus, m)?)?;
    m.add_function(wrap_pyfunction!(plane, m)?)?;
    m.add_function(wrap_pyfunction!(bessel, m)?)?;
    m.add_function(wrap_pyfunction!(laguerre_gaussian_phase, m)?)?;
    m.add_function(wrap_pyfunction!(laguerre_gaussian_intensity, m)?)?;
    m.add_function(wrap_pyfunction!(hermite_gaussian_phase, m)?)?;
    m.add_function(wrap_pyfunction!(hermite_gaussian_intensity, m)?)?;
    m.add_function(wrap_pyfunction!(focus_device, m)?)?;
    m.add_function(wrap_pyfunction!(focus_transducer, m)?)?;
    m.add_function(wrap_pyfunction!(plane_device, m)?)?;
    m.add_function(wrap_pyfunction!(plane_transducer, m)?)?;
    m.add_function(wrap_pyfunction!(bessel_device, m)?)?;
    m.add_function(wrap_pyfunction!(bessel_transducer, m)?)?;
    m.add_function(wrap_pyfunction!(laguerre_gaussian_phase_device, m)?)?;
    m.add_function(wrap_pyfunction!(laguerre_gaussian_phase_transducer, m)?)?;
    m.add_function(wrap_pyfunction!(laguerre_gaussian_intensity_device, m)?)?;
    m.add_function(wrap_pyfunction!(hermite_gaussian_phase_device, m)?)?;
    m.add_function(wrap_pyfunction!(hermite_gaussian_phase_transducer, m)?)?;
    m.add_function(wrap_pyfunction!(hermite_gaussian_intensity_device, m)?)?;
    m.add_function(wrap_pyfunction!(set_intensity, m)?)?;
    m.add_function(wrap_pyfunction!(set_phase, m)?)?;
    m.add_function(wrap_pyfunction!(add_phase, m)?)?;
    Ok(())
}
