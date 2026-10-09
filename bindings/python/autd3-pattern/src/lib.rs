use autd3_python_capsule::extract::{
    extract_angle, extract_direction, extract_length, extract_point, extract_u8, extract_velocity,
    length_to_py,
};
use autd3_python_capsule::numpy;
use autd3_python_capsule::{capsule_of, device_from_capsule, geometry_from_capsule};
use autd3_rs_core::Length;
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::geometry::{
    TransducerGroups as CoreTransducerGroups, TransducerMask as CoreTransducerMask,
};
use autd3_rs_core::value::{Intensity, Phase};
use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict};

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
        $ctor:path,
        $init:expr,
        $extract:ident,
        $to_py:ident,
        $capsule:path
    ) => {
        #[pyclass(name = $name, module = "autd3_pattern")]
        pub struct $buffer(Vec<Vec<$ty>>);

        #[pymethods]
        impl $buffer {
            #[new]
            fn new(num_devices: usize) -> Self {
                Self(vec![vec![$init; Autd3::NUM_TRANSDUCERS]; num_devices])
            }

            #[staticmethod]
            fn from_array(values: &Bound<'_, PyAny>) -> PyResult<$buffer> {
                if numpy::is_ndarray(values)? {
                    let bytes = numpy::u8_matrix_bytes(values, Autd3::NUM_TRANSDUCERS, None)?;
                    let inner = bytes
                        .as_bytes()
                        .chunks_exact(Autd3::NUM_TRANSDUCERS)
                        .map(|row| row.iter().map(|&v| $ctor(v)).collect())
                        .collect::<Vec<Vec<$ty>>>();
                    return Ok($buffer(inner));
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
                Ok($buffer(inner))
            }

            fn copy_from(slf: &Bound<'_, Self>, values: &Bound<'_, PyAny>) -> PyResult<()> {
                let num_devices = slf.borrow().0.len();
                let bytes =
                    numpy::u8_matrix_bytes(values, Autd3::NUM_TRANSDUCERS, Some(num_devices))?;
                let mut this = slf.borrow_mut();
                for (dst, src) in this
                    .0
                    .iter_mut()
                    .zip(bytes.as_bytes().chunks_exact(Autd3::NUM_TRANSDUCERS))
                {
                    for (d, &s) in dst.iter_mut().zip(src) {
                        *d = $ctor(s);
                    }
                }
                Ok(())
            }

            fn to_numpy<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                numpy::u8_matrix(py, self.0.len(), Autd3::NUM_TRANSDUCERS, |dst| {
                    for (dst, src) in dst.chunks_exact_mut(Autd3::NUM_TRANSDUCERS).zip(&self.0) {
                        for (d, s) in dst.iter_mut().zip(src) {
                            *d = s.0;
                        }
                    }
                })
            }

            fn num_devices(&self) -> usize {
                self.0.len()
            }

            fn __len__(&self) -> usize {
                self.0.len()
            }

            fn __getitem__(slf: &Bound<'_, Self>, index: usize) -> PyResult<$view> {
                if index >= slf.borrow().0.len() {
                    return Err(PyIndexError::new_err("device index out of range"));
                }
                Ok($view {
                    buffer: slf.clone().unbind(),
                    device: index,
                })
            }

            fn _capsule<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyCapsule>> {
                let py = slf.py();
                let ptr = core::ptr::NonNull::from(&mut slf.borrow_mut().0);
                unsafe { $capsule(py, ptr, slf.clone().into_any().unbind()) }
            }
        }

        #[pyclass(name = $view_name, module = "autd3_pattern")]
        pub struct $view {
            buffer: Py<$buffer>,
            device: usize,
        }

        #[pymethods]
        impl $view {
            fn __len__(&self, py: Python<'_>) -> usize {
                self.buffer.borrow(py).0[self.device].len()
            }

            fn __getitem__(&self, py: Python<'_>, index: usize) -> PyResult<Py<PyAny>> {
                let buf = self.buffer.borrow(py);
                let value = *buf.0[self.device]
                    .get(index)
                    .ok_or_else(|| PyIndexError::new_err("transducer index out of range"))?;
                $to_py(py, value)
            }

            fn __setitem__(
                &self,
                py: Python<'_>,
                index: usize,
                value: &Bound<'_, PyAny>,
            ) -> PyResult<()> {
                let value = $extract(value)?;
                let mut buf = self.buffer.borrow_mut(py);
                *buf.0[self.device]
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
    mut dst: PyRefMut<'_, PhaseBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let target = extract_point(target)?;
    autd3_rs_pattern::focus(geometry, target, extract_length(wavelength)?, &mut dst.0);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, direction, wavelength, dst))]
fn plane(
    geometry: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    wavelength: &Bound<'_, PyAny>,
    mut dst: PyRefMut<'_, PhaseBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let direction = extract_direction(direction)?;
    autd3_rs_pattern::plane(geometry, direction, extract_length(wavelength)?, &mut dst.0);
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
    mut dst: PyRefMut<'_, PhaseBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let apex = extract_point(apex)?;
    let direction = extract_direction(direction)?;
    let theta = extract_angle(theta)?;
    autd3_rs_pattern::bessel(
        geometry,
        apex,
        direction,
        theta,
        extract_length(wavelength)?,
        &mut dst.0,
    );
    Ok(())
}

fn extract_waist(waist: &Bound<'_, PyAny>) -> PyResult<Length> {
    let waist = extract_length(waist)?;
    if waist.mm().is_finite() && waist.mm() > 0.0 {
        Ok(waist)
    } else {
        Err(PyValueError::new_err(
            "waist must be a positive finite length",
        ))
    }
}

#[pyclass(
    name = "LaguerreGaussianOption",
    module = "autd3_pattern",
    skip_from_py_object
)]
pub struct LaguerreGaussianOption(autd3_rs_pattern::LaguerreGaussianOption);

#[pymethods]
impl LaguerreGaussianOption {
    #[new]
    #[pyo3(signature = (p, l, waist))]
    fn new(p: u32, l: i32, waist: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(autd3_rs_pattern::LaguerreGaussianOption {
            p,
            l,
            waist: extract_waist(waist)?,
        }))
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

#[pyclass(
    name = "HermiteGaussianOption",
    module = "autd3_pattern",
    skip_from_py_object
)]
pub struct HermiteGaussianOption(autd3_rs_pattern::HermiteGaussianOption);

#[pymethods]
impl HermiteGaussianOption {
    #[new]
    #[pyo3(signature = (m, n, waist))]
    fn new(m: u32, n: u32, waist: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(autd3_rs_pattern::HermiteGaussianOption {
            m,
            n,
            waist: extract_waist(waist)?,
        }))
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
    mut dst: PyRefMut<'_, PhaseBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    autd3_rs_pattern::laguerre_gaussian_phase(
        geometry,
        extract_point(target)?,
        extract_direction(axis)?,
        option.0,
        extract_length(wavelength)?,
        &mut dst.0,
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
    mut dst: PyRefMut<'_, IntensityBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    autd3_rs_pattern::laguerre_gaussian_intensity(
        geometry,
        extract_point(target)?,
        extract_direction(axis)?,
        option.0,
        extract_length(wavelength)?,
        &mut dst.0,
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
    mut dst: PyRefMut<'_, PhaseBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    autd3_rs_pattern::hermite_gaussian_phase(
        geometry,
        extract_point(target)?,
        extract_direction(axis)?,
        extract_direction(x_dir)?,
        option.0,
        extract_length(wavelength)?,
        &mut dst.0,
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
    mut dst: PyRefMut<'_, IntensityBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    autd3_rs_pattern::hermite_gaussian_intensity(
        geometry,
        extract_point(target)?,
        extract_direction(axis)?,
        extract_direction(x_dir)?,
        option.0,
        extract_length(wavelength)?,
        &mut dst.0,
    );
    Ok(())
}

fn phase_bytes(phases: &[Phase]) -> Vec<u8> {
    phases.iter().map(|phase| phase.0).collect()
}

fn intensity_bytes(intensities: &[Intensity]) -> Vec<u8> {
    intensities.iter().map(|intensity| intensity.0).collect()
}

macro_rules! device_fn {
    ($name:ident, $core:path, $ty:ty, $init:expr, $bytes:ident, ($($arg:ident: $extract:expr),*)) => {
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
            numpy::u8_vector_write(dst, &$bytes(&values))
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
    Ok(option.cast::<LaguerreGaussianOption>()?.borrow().0)
}

fn extract_hg_option(
    option: &Bound<'_, PyAny>,
) -> PyResult<autd3_rs_pattern::HermiteGaussianOption> {
    Ok(option.cast::<HermiteGaussianOption>()?.borrow().0)
}

device_fn!(
    focus_device,
    autd3_rs_pattern::focus_device,
    Phase,
    Phase::ZERO,
    phase_bytes,
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
    phase_bytes,
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
    phase_bytes,
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
    phase_bytes,
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
    intensity_bytes,
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
    phase_bytes,
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
    intensity_bytes,
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
fn set_intensity(
    intensity: &Bound<'_, PyAny>,
    mut dst: PyRefMut<'_, IntensityBuffer>,
) -> PyResult<()> {
    autd3_rs_pattern::set_intensity(extract_intensity(intensity)?, &mut dst.0);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (phase, dst))]
fn set_phase(phase: &Bound<'_, PyAny>, mut dst: PyRefMut<'_, PhaseBuffer>) -> PyResult<()> {
    autd3_rs_pattern::set_phase(extract_phase(phase)?, &mut dst.0);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (phase, dst))]
fn add_phase(phase: &Bound<'_, PyAny>, mut dst: PyRefMut<'_, PhaseBuffer>) -> PyResult<()> {
    autd3_rs_pattern::add_phase(extract_phase(phase)?, &mut dst.0);
    Ok(())
}

fn matches_geometry<T>(geometry: &autd3_rs_core::Geometry, buffer: &[Vec<T>]) -> bool {
    buffer.len() == geometry.num_devices()
        && geometry
            .iter()
            .zip(buffer)
            .all(|(device, slot)| slot.len() == device.num_transducers())
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
            .map_err(|e| autd3_python_capsule::to_pyerr(capsule.py(), e))
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

#[pyclass(name = "TransducerGroups", module = "autd3_pattern")]
pub struct TransducerGroups {
    inner: CoreTransducerGroups<usize>,
    keys: Vec<Py<PyAny>>,
    lookup: Py<PyDict>,
}

#[pymethods]
impl TransducerGroups {
    #[new]
    fn new(geometry: &Bound<'_, PyAny>, key: &Bound<'_, PyAny>) -> PyResult<Self> {
        let py = geometry.py();
        let capsule = capsule_of(geometry)?;
        let core_geometry = geometry_from_capsule(&capsule)?;
        let devices = (0..core_geometry.num_devices())
            .map(|dev| geometry.get_item(dev))
            .collect::<PyResult<Vec<_>>>()?;
        let lookup = PyDict::new(py);
        let mut keys = Vec::new();
        let mut error = None;
        let inner = CoreTransducerGroups::new(core_geometry, |device, tr| {
            if error.is_some() {
                return 0;
            }
            key.call1((&devices[device.idx()], tr))
                .and_then(|k| {
                    if k.is_none() {
                        return Err(PyValueError::new_err(
                            "every transducer must be assigned a key (the key function returned None)",
                        ));
                    }
                    if let Some(index) = lookup.get_item(&k)? {
                        return index.extract::<usize>();
                    }
                    let index = keys.len();
                    lookup.set_item(&k, index)?;
                    keys.push(k.unbind());
                    Ok(index)
                })
                .unwrap_or_else(|e| {
                    error = Some(e);
                    0
                })
        });
        if let Some(e) = error {
            return Err(e);
        }
        Ok(Self {
            inner,
            keys,
            lookup: lookup.unbind(),
        })
    }

    fn keys(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        self.keys.iter().map(|key| key.clone_ref(py)).collect()
    }

    fn key(&self, py: Python<'_>, device: usize, transducer: usize) -> PyResult<Py<PyAny>> {
        self.check_transducer(device, transducer)?;
        Ok(self.keys[self.inner.index(device, transducer)].clone_ref(py))
    }

    fn index(&self, device: usize, transducer: usize) -> PyResult<usize> {
        self.check_transducer(device, transducer)?;
        Ok(self.inner.index(device, transducer))
    }

    fn indices(&self, device: usize) -> PyResult<Vec<usize>> {
        self.check_device(device)?;
        Ok(self.inner.indices(device).to_vec())
    }

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn num_transducers(&self, device: usize) -> PyResult<usize> {
        self.check_device(device)?;
        Ok(self.inner.num_transducers(device))
    }

    fn num_transducers_in(&self, key: &Bound<'_, PyAny>) -> PyResult<usize> {
        Ok(match self.lookup.bind(key.py()).get_item(key)? {
            Some(index) => self.inner.num_transducers_in(index.extract()?),
            None => 0,
        })
    }

    fn masks(&self, py: Python<'_>) -> Vec<(Py<PyAny>, TransducerMask)> {
        self.keys
            .iter()
            .enumerate()
            .map(|(index, key)| (key.clone_ref(py), self.mask_at(index)))
            .collect()
    }

    fn mask(&self, key: &Bound<'_, PyAny>) -> PyResult<TransducerMask> {
        match self.lookup.bind(key.py()).get_item(key)? {
            Some(index) => Ok(self.mask_at(index.extract()?)),
            None => Err(PyKeyError::new_err(key.clone().unbind())),
        }
    }
}

impl TransducerGroups {
    fn check_device(&self, device: usize) -> PyResult<()> {
        if device >= self.inner.num_devices() {
            return Err(PyIndexError::new_err("device index out of range"));
        }
        Ok(())
    }

    fn check_transducer(&self, device: usize, transducer: usize) -> PyResult<()> {
        self.check_device(device)?;
        if transducer >= self.inner.num_transducers(device) {
            return Err(PyIndexError::new_err("transducer index out of range"));
        }
        Ok(())
    }

    fn mask_at(&self, index: usize) -> TransducerMask {
        TransducerMask {
            mask: Some(
                (0..self.inner.num_devices())
                    .map(|dev| {
                        (0..self.inner.num_transducers(dev))
                            .map(|tr| self.inner.key(dev, tr) == index)
                            .collect()
                    })
                    .collect(),
            ),
        }
    }
}

fn groups_match(geometry: &autd3_rs_core::Geometry, groups: &CoreTransducerGroups<usize>) -> bool {
    groups.num_devices() == geometry.num_devices()
        && geometry
            .iter()
            .enumerate()
            .all(|(dev, device)| groups.num_transducers(dev) == device.num_transducers())
}

macro_rules! group_into {
    ($py:expr, $geometry:expr, $groups:expr, $sources:expr, $dst:expr, $buffer:ty) => {{
        let buffers = $groups
            .keys
            .iter()
            .map(|key| {
                let source = $sources
                    .get_item(key.bind($py))?
                    .cast_into::<$buffer>()
                    .map_err(|_| {
                        PyTypeError::new_err("every source must have the same buffer type as dst")
                    })?;
                if source.is($dst) {
                    return Err(PyValueError::new_err("dst must not be one of the sources"));
                }
                Ok(source)
            })
            .collect::<PyResult<Vec<_>>>()?;
        let borrowed = buffers
            .iter()
            .map(Bound::try_borrow)
            .collect::<Result<Vec<_>, _>>()?;
        let mut dst = $dst.try_borrow_mut()?;
        if !groups_match($geometry, &$groups.inner)
            || !matches_geometry($geometry, &dst.0)
            || !borrowed
                .iter()
                .all(|source| matches_geometry($geometry, &source.0))
        {
            return Err(PyValueError::new_err(
                "the groups and every buffer must match the geometry",
            ));
        }
        autd3_rs_pattern::group(
            $geometry,
            &$groups.inner,
            |index| borrowed[index].0.as_slice(),
            &mut dst.0,
        );
        Ok(())
    }};
}

#[pyfunction]
#[pyo3(signature = (geometry, groups, sources, dst))]
fn group(
    geometry: &Bound<'_, PyAny>,
    groups: PyRef<'_, TransducerGroups>,
    sources: &Bound<'_, PyAny>,
    dst: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let py = geometry.py();
    let capsule = capsule_of(geometry)?;
    let core_geometry = geometry_from_capsule(&capsule)?;
    if let Ok(dst) = dst.cast::<PhaseBuffer>() {
        return group_into!(py, core_geometry, groups, sources, dst, PhaseBuffer);
    }
    if let Ok(dst) = dst.cast::<IntensityBuffer>() {
        return group_into!(py, core_geometry, groups, sources, dst, IntensityBuffer);
    }
    Err(PyTypeError::new_err(
        "dst must be a PhaseBuffer or an IntensityBuffer",
    ))
}

macro_rules! group_device_into {
    ($py:expr, $device:expr, $groups:expr, $sources:expr, $dst:expr, $buffer:ty, $bytes:ident) => {{
        let buffers = $groups
            .keys
            .iter()
            .map(|key| {
                $sources
                    .get_item(key.bind($py))?
                    .cast_into::<$buffer>()
                    .map_err(|_| {
                        PyTypeError::new_err("every source must have the same buffer type")
                    })
            })
            .collect::<PyResult<Vec<_>>>()?;
        let borrowed = buffers
            .iter()
            .map(Bound::try_borrow)
            .collect::<Result<Vec<_>, _>>()?;
        let dev = $device.idx();
        let len = $device.num_transducers();
        if dev >= $groups.inner.num_devices()
            || $groups.inner.num_transducers(dev) != len
            || !borrowed
                .iter()
                .all(|source| source.0.get(dev).is_some_and(|slot| slot.len() == len))
        {
            return Err(PyValueError::new_err(
                "the groups and every source must cover the device",
            ));
        }
        let mut values = borrowed[0].0[dev].clone();
        autd3_rs_pattern::group_device(
            $device,
            &$groups.inner,
            |index| borrowed[index].0.as_slice(),
            &mut values,
        );
        numpy::u8_vector_write($dst, &$bytes(&values))
    }};
}

#[pyfunction]
#[pyo3(signature = (device, groups, sources, dst))]
fn group_device(
    device: &Bound<'_, PyAny>,
    groups: PyRef<'_, TransducerGroups>,
    sources: &Bound<'_, PyAny>,
    dst: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let py = device.py();
    let capsule = capsule_of(device)?;
    let core_device = device_from_capsule(&capsule)?;
    let dst = numpy::u8_vector_dst(dst, core_device.num_transducers())?;
    let Some(first) = groups.keys.first() else {
        return Ok(());
    };
    if sources
        .get_item(first.bind(py))?
        .cast::<IntensityBuffer>()
        .is_ok()
    {
        return group_device_into!(
            py,
            core_device,
            groups,
            sources,
            dst,
            IntensityBuffer,
            intensity_bytes
        );
    }
    group_device_into!(
        py,
        core_device,
        groups,
        sources,
        dst,
        PhaseBuffer,
        phase_bytes
    )
}

fn copy_group<T: Copy>(
    groups: &CoreTransducerGroups<usize>,
    index: usize,
    source: &[Vec<T>],
    dst: &mut [Vec<T>],
) {
    for (dev, (slot, src)) in dst.iter_mut().zip(source).enumerate() {
        for (tr, (o, &v)) in slot.iter_mut().zip(src).enumerate() {
            if groups.key(dev, tr) == index {
                *o = v;
            }
        }
    }
}

#[pyfunction]
#[pyo3(signature = (geometry, groups, compute, phases, intensities))]
fn group_compute(
    geometry: &Bound<'_, PyAny>,
    groups: PyRef<'_, TransducerGroups>,
    compute: &Bound<'_, PyAny>,
    phases: &Bound<'_, PhaseBuffer>,
    intensities: &Bound<'_, IntensityBuffer>,
) -> PyResult<()> {
    let py = geometry.py();
    let capsule = capsule_of(geometry)?;
    let core_geometry = geometry_from_capsule(&capsule)?;
    if !compute.is_callable() {
        return Err(PyTypeError::new_err("compute must be callable"));
    }
    if !groups_match(core_geometry, &groups.inner)
        || !matches_geometry(core_geometry, &phases.try_borrow()?.0)
        || !matches_geometry(core_geometry, &intensities.try_borrow()?.0)
    {
        return Err(PyValueError::new_err(
            "the groups, phases and intensities must match the geometry",
        ));
    }

    let scratch_phases = Bound::new(py, PhaseBuffer(core_geometry.phase_buffer()))?;
    let scratch_intensities = Bound::new(py, IntensityBuffer(core_geometry.intensity_buffer()))?;
    for (index, key) in groups.keys.iter().enumerate() {
        autd3_rs_pattern::set_phase(Phase::ZERO, &mut scratch_phases.try_borrow_mut()?.0);
        autd3_rs_pattern::set_intensity(
            Intensity::MAX,
            &mut scratch_intensities.try_borrow_mut()?.0,
        );
        compute.call1((
            key.bind(py),
            groups.mask_at(index),
            &scratch_phases,
            &scratch_intensities,
        ))?;
        copy_group(
            &groups.inner,
            index,
            &scratch_phases.try_borrow()?.0,
            &mut phases.try_borrow_mut()?.0,
        );
        copy_group(
            &groups.inner,
            index,
            &scratch_intensities.try_borrow()?.0,
            &mut intensities.try_borrow_mut()?.0,
        );
    }
    Ok(())
}

#[pymodule]
fn autd3_pattern(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PhaseBuffer>()?;
    m.add_class::<DevicePhaseView>()?;
    m.add_class::<IntensityBuffer>()?;
    m.add_class::<DeviceIntensityView>()?;
    m.add_class::<TransducerMask>()?;
    m.add_class::<TransducerGroups>()?;
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
    m.add_function(wrap_pyfunction!(group_device, m)?)?;
    m.add_function(wrap_pyfunction!(set_intensity, m)?)?;
    m.add_function(wrap_pyfunction!(set_phase, m)?)?;
    m.add_function(wrap_pyfunction!(add_phase, m)?)?;
    m.add_function(wrap_pyfunction!(group, m)?)?;
    m.add_function(wrap_pyfunction!(group_compute, m)?)?;
    Ok(())
}
