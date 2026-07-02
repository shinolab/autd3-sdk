use core::num::{NonZeroU8, NonZeroUsize};

use autd3_python_capsule::{capsule_of, geometry_from_capsule, pattern_from_capsule_mut};
use autd3_rs_core::Length;
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::geometry::Point3;
use autd3_rs_core::value::Intensity;
use autd3_rs_pattern_holo::{
    Amplitude as CoreAmplitude, ControlPoint as CoreControlPoint, Directivity as CoreDirectivity,
    EmissionConstraint as CoreEmissionConstraint, GreedyOption as CoreGreedyOption,
    GsOption as CoreGsOption, GspatOption as CoreGspatOption, NaiveOption as CoreNaiveOption, Pa,
    TransducerMask, dB, kPa,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

create_exception!(autd3_pattern_holo, HoloError, PyException);

fn holo_err(e: autd3_rs_pattern_holo::HoloError) -> PyErr {
    HoloError::new_err(e.to_string())
}

fn extract_point(obj: &Bound<'_, PyAny>) -> PyResult<Point3<f32>> {
    let [x, y, z] = obj.extract::<[f32; 3]>().map_err(|_| {
        PyValueError::new_err(
            "expected a length-3 array-like (numpy array, list, or tuple) of x, y, z in mm",
        )
    })?;
    Ok(Point3::new(x, y, z))
}

fn extract_u8(obj: &Bound<'_, PyAny>) -> PyResult<u8> {
    if let Ok(v) = obj.extract::<u8>() {
        return Ok(v);
    }
    obj.getattr("value")?.extract::<u8>()
}

fn number_f32(obj: &Bound<'_, PyAny>) -> PyResult<f32> {
    obj.extract::<f32>()
        .map_err(|_| PyValueError::new_err("expected a number"))
}

#[pyclass(name = "Amplitude", module = "autd3_pattern_holo", from_py_object)]
#[derive(Clone, Copy)]
pub struct Amplitude(pub(crate) CoreAmplitude);

#[pymethods]
impl Amplitude {
    #[getter]
    fn as_pascal(&self) -> f32 {
        self.0.pascal()
    }

    #[getter]
    fn as_spl(&self) -> f32 {
        self.0.spl()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<Amplitude>().is_ok_and(|o| self.0 == o.0)
    }

    fn __repr__(&self) -> String {
        format!("{} Pa", self.0.pascal())
    }
}

#[derive(Clone, Copy)]
enum AmpKind {
    Pa,
    KPa,
    Db,
}

#[pyclass(
    name = "_AmplitudeUnit",
    module = "autd3_pattern_holo",
    skip_from_py_object
)]
#[derive(Clone, Copy)]
pub struct AmplitudeUnit(AmpKind);

impl AmplitudeUnit {
    pub(crate) const PA: Self = Self(AmpKind::Pa);
    pub(crate) const KPA: Self = Self(AmpKind::KPa);
    pub(crate) const DB: Self = Self(AmpKind::Db);
}

#[pymethods]
impl AmplitudeUnit {
    fn __rmul__(&self, lhs: &Bound<'_, PyAny>) -> PyResult<Amplitude> {
        let v = number_f32(lhs)?;
        Ok(Amplitude(match self.0 {
            AmpKind::Pa => v * Pa,
            AmpKind::KPa => v * kPa,
            AmpKind::Db => v * dB,
        }))
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
    fn uniform(intensity: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(CoreEmissionConstraint::Uniform(Intensity(
            extract_u8(intensity)?,
        ))))
    }

    #[staticmethod]
    #[pyo3(name = "Clamp")]
    fn clamp(min: &Bound<'_, PyAny>, max: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(CoreEmissionConstraint::Clamp(
            Intensity(extract_u8(min)?),
            Intensity(extract_u8(max)?),
        )))
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

#[pyclass(name = "TransducerMask", module = "autd3_pattern_holo", from_py_object)]
#[derive(Clone)]
pub struct PyTransducerMask {
    pub(crate) mask: Option<Vec<Vec<bool>>>,
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
            if device.len() != Autd3::NUM_TRANSDUCERS {
                return Err(PyValueError::new_err(format!(
                    "each device mask needs {} entries, got {}",
                    Autd3::NUM_TRANSDUCERS,
                    device.len()
                )));
            }
            out.push(device);
        }
        Ok(Self { mask: Some(out) })
    }
}

#[pyclass(
    name = "NalgebraBackend",
    module = "autd3_pattern_holo",
    from_py_object
)]
#[derive(Clone)]
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
    inner: CoreNaiveOption<'static>,
    mask: Option<Vec<Vec<bool>>>,
}

#[pymethods]
impl NaiveOption {
    #[new]
    #[pyo3(signature = (
        constraint = EmissionConstraint(CoreEmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX)),
        directivity = Directivity(CoreDirectivity::Sphere),
        backend = PyNalgebraBackend,
        mask = PyTransducerMask::all_enabled(),
    ))]
    fn new(
        constraint: EmissionConstraint,
        directivity: Directivity,
        backend: PyNalgebraBackend,
        mask: PyTransducerMask,
    ) -> Self {
        let _ = backend;
        Self {
            inner: CoreNaiveOption {
                constraint: constraint.0,
                directivity: directivity.0,
                ..CoreNaiveOption::default()
            },
            mask: mask.mask,
        }
    }
}

#[pyclass(name = "GsOption", module = "autd3_pattern_holo", skip_from_py_object)]
pub struct GsOption {
    inner: CoreGsOption<'static>,
    mask: Option<Vec<Vec<bool>>>,
}

#[pymethods]
impl GsOption {
    #[new]
    #[pyo3(signature = (
        repeat = 100,
        constraint = EmissionConstraint(CoreEmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX)),
        directivity = Directivity(CoreDirectivity::Sphere),
        backend = PyNalgebraBackend,
        mask = PyTransducerMask::all_enabled(),
    ))]
    fn new(
        repeat: usize,
        constraint: EmissionConstraint,
        directivity: Directivity,
        backend: PyNalgebraBackend,
        mask: PyTransducerMask,
    ) -> PyResult<Self> {
        let _ = backend;
        Ok(Self {
            inner: CoreGsOption {
                repeat: NonZeroUsize::new(repeat)
                    .ok_or_else(|| PyValueError::new_err("repeat must be >= 1"))?,
                constraint: constraint.0,
                directivity: directivity.0,
                ..CoreGsOption::default()
            },
            mask: mask.mask,
        })
    }
}

#[pyclass(
    name = "GspatOption",
    module = "autd3_pattern_holo",
    skip_from_py_object
)]
pub struct GspatOption {
    inner: CoreGspatOption<'static>,
    mask: Option<Vec<Vec<bool>>>,
}

#[pymethods]
impl GspatOption {
    #[new]
    #[pyo3(signature = (
        repeat = 100,
        constraint = EmissionConstraint(CoreEmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX)),
        directivity = Directivity(CoreDirectivity::Sphere),
        backend = PyNalgebraBackend,
        mask = PyTransducerMask::all_enabled(),
    ))]
    fn new(
        repeat: usize,
        constraint: EmissionConstraint,
        directivity: Directivity,
        backend: PyNalgebraBackend,
        mask: PyTransducerMask,
    ) -> PyResult<Self> {
        let _ = backend;
        Ok(Self {
            inner: CoreGspatOption {
                repeat: NonZeroUsize::new(repeat)
                    .ok_or_else(|| PyValueError::new_err("repeat must be >= 1"))?,
                constraint: constraint.0,
                directivity: directivity.0,
                ..CoreGspatOption::default()
            },
            mask: mask.mask,
        })
    }
}

#[pyclass(
    name = "GreedyOption",
    module = "autd3_pattern_holo",
    skip_from_py_object
)]
pub struct GreedyOption {
    inner: CoreGreedyOption<'static>,
    mask: Option<Vec<Vec<bool>>>,
}

#[pymethods]
impl GreedyOption {
    #[new]
    #[pyo3(signature = (
        phase_quantization_levels = 16,
        constraint = EmissionConstraint(CoreEmissionConstraint::Uniform(Intensity::MAX)),
        directivity = Directivity(CoreDirectivity::Sphere),
        mask = PyTransducerMask::all_enabled(),
    ))]
    fn new(
        phase_quantization_levels: u8,
        constraint: EmissionConstraint,
        directivity: Directivity,
        mask: PyTransducerMask,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: CoreGreedyOption {
                phase_quantization_levels: NonZeroU8::new(phase_quantization_levels).ok_or_else(
                    || PyValueError::new_err("phase_quantization_levels must be >= 1"),
                )?,
                constraint: constraint.0,
                directivity: directivity.0,
                ..CoreGreedyOption::default()
            },
            mask: mask.mask,
        })
    }
}

fn collect_foci(foci: &[PyRef<'_, ControlPoint>]) -> Vec<CoreControlPoint> {
    foci.iter().map(|f| f.inner).collect()
}

fn mask_ref(mask: Option<&[Vec<bool>]>) -> TransducerMask<'_> {
    match mask {
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
#[pyo3(signature = (geometry, foci, wavelength, option, buffer))]
fn naive(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, ControlPoint>>,
    wavelength: f32,
    option: &NaiveOption,
    buffer: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let option = CoreNaiveOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_out_buffer(buffer, |out| {
        autd3_rs_pattern_holo::naive(
            geometry,
            &foci,
            Length::millimeters(wavelength),
            &option,
            out,
        )
        .map_err(holo_err)
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, buffer))]
fn gs(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, ControlPoint>>,
    wavelength: f32,
    option: &GsOption,
    buffer: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let option = CoreGsOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_out_buffer(buffer, |out| {
        autd3_rs_pattern_holo::gs(
            geometry,
            &foci,
            Length::millimeters(wavelength),
            &option,
            out,
        )
        .map_err(holo_err)
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, buffer))]
fn gspat(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, ControlPoint>>,
    wavelength: f32,
    option: &GspatOption,
    buffer: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let option = CoreGspatOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_out_buffer(buffer, |out| {
        autd3_rs_pattern_holo::gspat(
            geometry,
            &foci,
            Length::millimeters(wavelength),
            &option,
            out,
        )
        .map_err(holo_err)
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, buffer))]
fn greedy(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, ControlPoint>>,
    wavelength: f32,
    option: &GreedyOption,
    buffer: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let option = CoreGreedyOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_out_buffer(buffer, |out| {
        autd3_rs_pattern_holo::greedy(
            geometry,
            &foci,
            Length::millimeters(wavelength),
            &option,
            out,
        )
        .map_err(holo_err)
    })
}

#[pymodule]
fn autd3_pattern_holo(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Amplitude>()?;
    m.add_class::<AmplitudeUnit>()?;
    m.add_class::<ControlPoint>()?;
    m.add_class::<EmissionConstraint>()?;
    m.add_class::<Directivity>()?;
    m.add_class::<PyTransducerMask>()?;
    m.add_class::<PyNalgebraBackend>()?;
    m.add_class::<NaiveOption>()?;
    m.add_class::<GsOption>()?;
    m.add_class::<GspatOption>()?;
    m.add_class::<GreedyOption>()?;
    m.add("HoloError", m.py().get_type::<HoloError>())?;
    m.add("Pa", Py::new(m.py(), AmplitudeUnit::PA)?)?;
    m.add("kPa", Py::new(m.py(), AmplitudeUnit::KPA)?)?;
    m.add("dB", Py::new(m.py(), AmplitudeUnit::DB)?)?;
    m.add_function(wrap_pyfunction!(naive, m)?)?;
    m.add_function(wrap_pyfunction!(gs, m)?)?;
    m.add_function(wrap_pyfunction!(gspat, m)?)?;
    m.add_function(wrap_pyfunction!(greedy, m)?)?;
    Ok(())
}
