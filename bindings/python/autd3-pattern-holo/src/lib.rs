use core::num::{NonZeroU8, NonZeroUsize};

use autd3_python_capsule::{capsule_of, geometry_from_capsule, pattern_from_capsule_mut, to_pyerr};
use autd3_rs_core::Length;
use autd3_rs_core::geometry::Point3;
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::Intensity;
use autd3_rs_pattern_holo::{
    Amplitude as CoreAmplitude, ControlPoint as CoreControlPoint, Directivity as CoreDirectivity,
    EmissionConstraint as CoreEmissionConstraint, GreedyOption as CoreGreedyOption,
    GsOption as CoreGsOption, GspatOption as CoreGspatOption, NaiveOption as CoreNaiveOption,
    NalgebraBackend, Pa, TransducerMask, dB, kPa,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

fn extract_point(obj: &Bound<'_, PyAny>) -> PyResult<Point3<f32>> {
    let [x, y, z] = obj.extract::<[f32; 3]>().map_err(|_| {
        PyValueError::new_err(
            "expected a length-3 array-like (numpy array, list, or tuple) of x, y, z in mm",
        )
    })?;
    Ok(Point3::new(x, y, z))
}

#[pyclass(name = "Amplitude", module = "autd3_pattern_holo", from_py_object)]
#[derive(Clone, Copy)]
pub struct Amplitude(pub(crate) CoreAmplitude);

#[pymethods]
impl Amplitude {
    #[staticmethod]
    fn pascal(value: f32) -> Self {
        Self(value * Pa)
    }

    #[staticmethod]
    fn kilo_pascal(value: f32) -> Self {
        Self(value * kPa)
    }

    #[staticmethod]
    fn spl(value: f32) -> Self {
        Self(value * dB)
    }

    #[pyo3(name = "as_pascal")]
    fn get_pascal(&self) -> f32 {
        self.0.pascal()
    }

    #[pyo3(name = "as_spl")]
    fn get_spl(&self) -> f32 {
        self.0.spl()
    }
}

#[pyclass(name = "ControlPoint", module = "autd3_pattern_holo")]
pub struct ControlPoint {
    pub(crate) inner: CoreControlPoint,
}

#[pymethods]
impl ControlPoint {
    #[new]
    fn new(point: &Bound<'_, PyAny>, amplitude: Amplitude) -> PyResult<Self> {
        Ok(Self {
            inner: CoreControlPoint {
                point: extract_point(point)?,
                amplitude: amplitude.0,
            },
        })
    }
}

#[pyclass(
    name = "EmissionConstraint",
    module = "autd3_pattern_holo",
    from_py_object
)]
#[derive(Clone, Copy)]
pub struct EmissionConstraint(pub(crate) CoreEmissionConstraint);

#[pymethods]
impl EmissionConstraint {
    #[classattr]
    #[pyo3(name = "Normalize")]
    fn normalize() -> Self {
        Self(CoreEmissionConstraint::Normalize)
    }

    #[staticmethod]
    #[pyo3(name = "Multiply")]
    fn multiply(value: f32) -> Self {
        Self(CoreEmissionConstraint::Multiply(value))
    }

    #[staticmethod]
    #[pyo3(name = "Uniform")]
    fn uniform(intensity: u8) -> Self {
        Self(CoreEmissionConstraint::Uniform(Intensity(intensity)))
    }

    #[staticmethod]
    #[pyo3(name = "Clamp")]
    fn clamp(min: u8, max: u8) -> Self {
        Self(CoreEmissionConstraint::Clamp(
            Intensity(min),
            Intensity(max),
        ))
    }
}

#[pyclass(name = "Directivity", module = "autd3_pattern_holo", from_py_object)]
#[derive(Clone, Copy)]
pub struct Directivity(pub(crate) CoreDirectivity);

#[pymethods]
impl Directivity {
    #[classattr]
    #[pyo3(name = "Sphere")]
    fn sphere() -> Self {
        Self(CoreDirectivity::Sphere)
    }

    #[classattr]
    #[pyo3(name = "T4010A1")]
    fn t4010a1() -> Self {
        Self(CoreDirectivity::T4010A1)
    }
}

#[pyclass(name = "TransducerMask", module = "autd3_pattern_holo")]
pub struct PyTransducerMask {
    pub(crate) mask: Option<Vec<[bool; NUM_TRANSDUCERS]>>,
}

#[pymethods]
impl PyTransducerMask {
    #[classattr]
    #[pyo3(name = "AllEnabled")]
    fn all_enabled() -> Self {
        Self { mask: None }
    }

    #[staticmethod]
    fn masked(mask: Vec<Vec<bool>>) -> PyResult<Self> {
        let mut out = Vec::with_capacity(mask.len());
        for device in mask {
            let slot: [bool; NUM_TRANSDUCERS] = device.try_into().map_err(|v: Vec<bool>| {
                PyValueError::new_err(format!(
                    "each device mask needs {NUM_TRANSDUCERS} entries, got {}",
                    v.len()
                ))
            })?;
            out.push(slot);
        }
        Ok(Self { mask: Some(out) })
    }
}

#[pyclass(name = "NalgebraBackend", module = "autd3_pattern_holo")]
pub struct PyNalgebraBackend;

#[pymethods]
impl PyNalgebraBackend {
    #[new]
    fn new() -> Self {
        Self
    }
}

#[pyclass(
    name = "NaiveOption",
    module = "autd3_pattern_holo",
    skip_from_py_object
)]
pub struct NaiveOption {
    inner: CoreNaiveOption,
}

#[pymethods]
impl NaiveOption {
    #[new]
    #[pyo3(signature = (constraint = None, directivity = None))]
    fn new(constraint: Option<EmissionConstraint>, directivity: Option<Directivity>) -> Self {
        let mut inner = CoreNaiveOption::default();
        if let Some(c) = constraint {
            inner.constraint = c.0;
        }
        if let Some(d) = directivity {
            inner.directivity = d.0;
        }
        Self { inner }
    }
}

#[pyclass(name = "GsOption", module = "autd3_pattern_holo", skip_from_py_object)]
pub struct GsOption {
    inner: CoreGsOption,
}

#[pymethods]
impl GsOption {
    #[new]
    #[pyo3(signature = (repeat = 100, constraint = None, directivity = None))]
    fn new(
        repeat: usize,
        constraint: Option<EmissionConstraint>,
        directivity: Option<Directivity>,
    ) -> PyResult<Self> {
        let mut inner = CoreGsOption {
            repeat: NonZeroUsize::new(repeat)
                .ok_or_else(|| PyValueError::new_err("repeat must be >= 1"))?,
            ..CoreGsOption::default()
        };
        if let Some(c) = constraint {
            inner.constraint = c.0;
        }
        if let Some(d) = directivity {
            inner.directivity = d.0;
        }
        Ok(Self { inner })
    }
}

#[pyclass(
    name = "GspatOption",
    module = "autd3_pattern_holo",
    skip_from_py_object
)]
pub struct GspatOption {
    inner: CoreGspatOption,
}

#[pymethods]
impl GspatOption {
    #[new]
    #[pyo3(signature = (repeat = 100, constraint = None, directivity = None))]
    fn new(
        repeat: usize,
        constraint: Option<EmissionConstraint>,
        directivity: Option<Directivity>,
    ) -> PyResult<Self> {
        let mut inner = CoreGspatOption {
            repeat: NonZeroUsize::new(repeat)
                .ok_or_else(|| PyValueError::new_err("repeat must be >= 1"))?,
            ..CoreGspatOption::default()
        };
        if let Some(c) = constraint {
            inner.constraint = c.0;
        }
        if let Some(d) = directivity {
            inner.directivity = d.0;
        }
        Ok(Self { inner })
    }
}

#[pyclass(
    name = "GreedyOption",
    module = "autd3_pattern_holo",
    skip_from_py_object
)]
pub struct GreedyOption {
    inner: CoreGreedyOption,
}

#[pymethods]
impl GreedyOption {
    #[new]
    #[pyo3(signature = (phase_quantization_levels = 16, constraint = None, directivity = None))]
    fn new(
        phase_quantization_levels: u8,
        constraint: Option<EmissionConstraint>,
        directivity: Option<Directivity>,
    ) -> PyResult<Self> {
        let mut inner = CoreGreedyOption {
            phase_quantization_levels: NonZeroU8::new(phase_quantization_levels)
                .ok_or_else(|| PyValueError::new_err("phase_quantization_levels must be >= 1"))?,
            ..CoreGreedyOption::default()
        };
        if let Some(c) = constraint {
            inner.constraint = c.0;
        }
        if let Some(d) = directivity {
            inner.directivity = d.0;
        }
        Ok(Self { inner })
    }
}

fn collect_foci(foci: &[PyRef<'_, ControlPoint>]) -> Vec<CoreControlPoint> {
    foci.iter().map(|f| f.inner).collect()
}

fn mask_ref(mask: Option<&PyTransducerMask>) -> TransducerMask<'_> {
    match mask.and_then(|m| m.mask.as_deref()) {
        Some(m) => TransducerMask::Masked(m),
        None => TransducerMask::AllEnabled,
    }
}

fn with_out_buffer<F>(buffer: &Bound<'_, PyAny>, f: F) -> PyResult<()>
where
    F: FnOnce(&mut [autd3_python_capsule::DevicePattern]) -> PyResult<()>,
{
    let capsule = buffer
        .call_method0("_capsule_mut")?
        .cast_into::<PyCapsule>()?;
    let out = pattern_from_capsule_mut(&capsule)?;
    f(out.as_mut_slice())
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, buffer, mask = None))]
fn naive(
    py: Python<'_>,
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, ControlPoint>>,
    wavelength: f32,
    option: &NaiveOption,
    buffer: &Bound<'_, PyAny>,
    mask: Option<PyRef<'_, PyTransducerMask>>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let mask = mask_ref(mask.as_deref());
    with_out_buffer(buffer, |out| {
        autd3_rs_pattern_holo::naive(
            geometry,
            &foci,
            Length::millimeters(wavelength),
            &option.inner,
            &NalgebraBackend,
            mask,
            out,
        )
        .map_err(|e| to_pyerr(py, e))
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, buffer, mask = None))]
fn gs(
    py: Python<'_>,
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, ControlPoint>>,
    wavelength: f32,
    option: &GsOption,
    buffer: &Bound<'_, PyAny>,
    mask: Option<PyRef<'_, PyTransducerMask>>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let mask = mask_ref(mask.as_deref());
    with_out_buffer(buffer, |out| {
        autd3_rs_pattern_holo::gs(
            geometry,
            &foci,
            Length::millimeters(wavelength),
            &option.inner,
            &NalgebraBackend,
            mask,
            out,
        )
        .map_err(|e| to_pyerr(py, e))
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, buffer, mask = None))]
fn gspat(
    py: Python<'_>,
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, ControlPoint>>,
    wavelength: f32,
    option: &GspatOption,
    buffer: &Bound<'_, PyAny>,
    mask: Option<PyRef<'_, PyTransducerMask>>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let mask = mask_ref(mask.as_deref());
    with_out_buffer(buffer, |out| {
        autd3_rs_pattern_holo::gspat(
            geometry,
            &foci,
            Length::millimeters(wavelength),
            &option.inner,
            &NalgebraBackend,
            mask,
            out,
        )
        .map_err(|e| to_pyerr(py, e))
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, buffer, mask = None))]
fn greedy(
    py: Python<'_>,
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, ControlPoint>>,
    wavelength: f32,
    option: &GreedyOption,
    buffer: &Bound<'_, PyAny>,
    mask: Option<PyRef<'_, PyTransducerMask>>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let mask = mask_ref(mask.as_deref());
    with_out_buffer(buffer, |out| {
        autd3_rs_pattern_holo::greedy(
            geometry,
            &foci,
            Length::millimeters(wavelength),
            &option.inner,
            mask,
            out,
        )
        .map_err(|e| to_pyerr(py, e))
    })
}

#[pymodule]
fn autd3_pattern_holo(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Amplitude>()?;
    m.add_class::<ControlPoint>()?;
    m.add_class::<EmissionConstraint>()?;
    m.add_class::<Directivity>()?;
    m.add_class::<PyTransducerMask>()?;
    m.add_class::<PyNalgebraBackend>()?;
    m.add_class::<NaiveOption>()?;
    m.add_class::<GsOption>()?;
    m.add_class::<GspatOption>()?;
    m.add_class::<GreedyOption>()?;
    m.add_function(wrap_pyfunction!(naive, m)?)?;
    m.add_function(wrap_pyfunction!(gs, m)?)?;
    m.add_function(wrap_pyfunction!(gspat, m)?)?;
    m.add_function(wrap_pyfunction!(greedy, m)?)?;
    Ok(())
}
