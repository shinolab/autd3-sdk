use core::num::{NonZeroU8, NonZeroUsize};

use autd3_python_capsule::error_code;
use autd3_python_capsule::extract::{extract_length, extract_point, extract_u8, number_f32};
use autd3_python_capsule::numpy::f32_vec3;
use autd3_python_capsule::{
    capsule_of, geometry_from_capsule, intensities_from_capsule, intensities_from_capsule_mut,
    intensity_buffer_addr, phase_buffer_addr, phases_from_capsule_mut,
};
use autd3_rs_core::geometry::TransducerMask;
use autd3_rs_core::value::{Intensity, PatternIntensity, Phase};
use autd3_rs_pattern_holo::{
    Amplitude as CoreAmplitude, AmplitudeTarget as CoreAmplitudeTarget,
    Directivity as CoreDirectivity, GreedyOption as CoreGreedyOption, GsOption as CoreGsOption,
    GspatOption as CoreGspatOption, IntensityConstraint as CoreIntensityConstraint,
    NaiveOption as CoreNaiveOption, Pa, dB, kPa,
};
use pyo3::exceptions::{PyTypeError, PyValueError, PyZeroDivisionError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyTuple, PyType};

static HOLO_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

fn holo_error_type(py: Python<'_>) -> PyResult<&Bound<'_, PyType>> {
    HOLO_ERROR
        .get_or_try_init(py, || {
            let base = py.import("autd3_core")?.getattr("Autd3Error")?;
            let namespace = PyDict::new(py);
            namespace.set_item("__module__", "autd3_pattern_holo")?;
            namespace.set_item("code", error_code::INVALID_ARGUMENT)?;
            Ok::<_, PyErr>(
                py.get_type::<PyType>()
                    .call1(("HoloError", PyTuple::new(py, [base])?, namespace))?
                    .cast_into::<PyType>()?
                    .unbind(),
            )
        })
        .map(|ty| ty.bind(py))
}

fn holo_err(e: autd3_rs_pattern_holo::HoloError) -> PyErr {
    Python::attach(|py| {
        match holo_error_type(py).and_then(|ty| {
            let err = ty.call1((e.to_string(),))?;
            err.setattr("code", error_code::INVALID_ARGUMENT)?;
            Ok(err)
        }) {
            Ok(err) => PyErr::from_value(err),
            Err(err) => err,
        }
    })
}

fn hash_f32<H: core::hash::Hasher>(value: f32, state: &mut H) {
    core::hash::Hash::hash(&(value + 0.0).to_bits(), state);
}

#[pyclass(
    name = "Amplitude",
    module = "autd3_pattern_holo",
    eq,
    ord,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Amplitude(pub(crate) CoreAmplitude);

impl core::hash::Hash for Amplitude {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        hash_f32(self.0.pascal(), state);
    }
}

#[pymethods]
impl Amplitude {
    #[getter]
    fn pascal(&self) -> f32 {
        self.0.pascal()
    }

    #[getter]
    fn spl(&self) -> f32 {
        self.0.spl()
    }

    fn __mul__(&self, rhs: f32) -> Self {
        Self(self.0 * rhs)
    }

    fn __rmul__(&self, lhs: f32) -> Self {
        Self(lhs * self.0)
    }

    fn __truediv__(&self, rhs: f32) -> PyResult<Self> {
        if rhs == 0.0 {
            return Err(PyZeroDivisionError::new_err("division by zero"));
        }
        Ok(Self(self.0 / rhs))
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

#[pyclass(name = "_AmplitudeUnit", module = "autd3_pattern_holo")]
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

#[pyclass(name = "AmplitudeTarget", module = "autd3_pattern_holo", eq, frozen)]
#[derive(PartialEq)]
pub struct AmplitudeTarget(pub(crate) CoreAmplitudeTarget);

#[pymethods]
impl AmplitudeTarget {
    #[new]
    fn new(point: &Bound<'_, PyAny>, amplitude: Amplitude) -> PyResult<Self> {
        Ok(Self(CoreAmplitudeTarget {
            point: extract_point(point)?,
            amplitude: amplitude.0,
        }))
    }

    #[getter]
    fn point<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let p = self.0.point;
        f32_vec3(py, [p.x, p.y, p.z])
    }

    #[getter]
    fn amplitude(&self) -> Amplitude {
        Amplitude(self.0.amplitude)
    }

    fn __repr__(&self) -> String {
        let p = self.0.point;
        format!(
            "AmplitudeTarget(point=[{}, {}, {}], amplitude={} Pa)",
            p.x,
            p.y,
            p.z,
            self.0.amplitude.pascal()
        )
    }
}

#[pyclass(
    name = "IntensityConstraint",
    module = "autd3_pattern_holo",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq)]
pub struct IntensityConstraint(pub(crate) CoreIntensityConstraint);

impl core::hash::Hash for IntensityConstraint {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(&self.0).hash(state);
        match self.0 {
            CoreIntensityConstraint::Multiply(value) => hash_f32(value, state),
            CoreIntensityConstraint::Uniform(intensity) => intensity.hash(state),
            CoreIntensityConstraint::Clamp(min, max) => (min, max).hash(state),
            CoreIntensityConstraint::Normalize => {}
            _ => unreachable!(),
        }
    }
}

#[pymethods]
impl IntensityConstraint {
    #[classattr]
    #[pyo3(name = "Normalize")]
    fn normalize() -> Self {
        Self(CoreIntensityConstraint::Normalize)
    }

    #[staticmethod]
    #[pyo3(name = "Multiply")]
    fn multiply(value: f32) -> Self {
        Self(CoreIntensityConstraint::Multiply(value))
    }

    #[staticmethod]
    #[pyo3(name = "Uniform")]
    fn uniform(intensity: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(CoreIntensityConstraint::Uniform(Intensity(
            extract_u8(intensity)?,
        ))))
    }

    #[staticmethod]
    #[pyo3(name = "Clamp")]
    fn clamp(min: &Bound<'_, PyAny>, max: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(CoreIntensityConstraint::Clamp(
            Intensity(extract_u8(min)?),
            Intensity(extract_u8(max)?),
        )))
    }

    fn __repr__(&self) -> String {
        format!("IntensityConstraint.{:?}", self.0)
    }
}

#[pyclass(
    name = "Directivity",
    module = "autd3_pattern_holo",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Directivity(pub(crate) CoreDirectivity);

impl core::hash::Hash for Directivity {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(&self.0).hash(state);
    }
}

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

    fn __repr__(&self) -> String {
        format!("Directivity.{:?}", self.0)
    }
}

type Mask = Option<Vec<Vec<bool>>>;

fn all_enabled() -> Mask {
    None
}

fn extract_mask(mask: &Bound<'_, PyAny>) -> PyResult<Mask> {
    if !mask.hasattr("_mask")? {
        return Err(PyTypeError::new_err(
            "mask must be an autd3_pattern.TransducerMask, e.g. TransducerMask.AllEnabled",
        ));
    }
    mask.call_method0("_mask")?.extract()
}

fn mask_to_py<'py>(py: Python<'py>, mask: &Mask) -> PyResult<Bound<'py, PyAny>> {
    let cls = py.import("autd3_pattern")?.getattr("TransducerMask")?;
    match mask {
        Some(rows) => cls.call_method1("masked", (rows.clone(),)),
        None => cls.getattr("AllEnabled"),
    }
}

fn mask_repr(mask: &Mask) -> String {
    match mask {
        Some(rows) => format!("TransducerMask.masked(<{} devices>)", rows.len()),
        None => "TransducerMask.AllEnabled".to_owned(),
    }
}

#[pyclass(name = "NaiveOption", module = "autd3_pattern_holo", eq, frozen)]
#[derive(PartialEq)]
pub struct NaiveOption {
    inner: CoreNaiveOption<'static>,
    mask: Mask,
}

#[pymethods]
impl NaiveOption {
    #[new]
    #[pyo3(signature = (
        constraint = IntensityConstraint(CoreNaiveOption::default().constraint),
        directivity = Directivity(CoreNaiveOption::default().directivity),
        mask = all_enabled(),
    ))]
    fn new(
        constraint: IntensityConstraint,
        directivity: Directivity,
        #[pyo3(from_py_with = extract_mask)] mask: Mask,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: CoreNaiveOption {
                constraint: constraint.0,
                directivity: directivity.0,
                ..CoreNaiveOption::default()
            },
            mask,
        })
    }

    #[getter]
    fn constraint(&self) -> IntensityConstraint {
        IntensityConstraint(self.inner.constraint)
    }

    #[getter]
    fn directivity(&self) -> Directivity {
        Directivity(self.inner.directivity)
    }

    #[getter]
    fn mask<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        mask_to_py(py, &self.mask)
    }

    fn __repr__(&self) -> String {
        format!(
            "NaiveOption(constraint={:?}, directivity={:?}, mask={})",
            self.inner.constraint,
            self.inner.directivity,
            mask_repr(&self.mask)
        )
    }
}

#[pyclass(name = "GsOption", module = "autd3_pattern_holo", eq, frozen)]
#[derive(PartialEq)]
pub struct GsOption {
    inner: CoreGsOption<'static>,
    mask: Mask,
}

#[pymethods]
impl GsOption {
    #[new]
    #[pyo3(signature = (
        repeat = CoreGsOption::default().repeat.get(),
        constraint = IntensityConstraint(CoreGsOption::default().constraint),
        directivity = Directivity(CoreGsOption::default().directivity),
        mask = all_enabled(),
    ))]
    fn new(
        repeat: usize,
        constraint: IntensityConstraint,
        directivity: Directivity,
        #[pyo3(from_py_with = extract_mask)] mask: Mask,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: CoreGsOption {
                repeat: NonZeroUsize::new(repeat)
                    .ok_or_else(|| PyValueError::new_err("repeat must be >= 1"))?,
                constraint: constraint.0,
                directivity: directivity.0,
                ..CoreGsOption::default()
            },
            mask,
        })
    }

    #[getter]
    fn repeat(&self) -> usize {
        self.inner.repeat.get()
    }

    #[getter]
    fn constraint(&self) -> IntensityConstraint {
        IntensityConstraint(self.inner.constraint)
    }

    #[getter]
    fn directivity(&self) -> Directivity {
        Directivity(self.inner.directivity)
    }

    #[getter]
    fn mask<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        mask_to_py(py, &self.mask)
    }

    fn __repr__(&self) -> String {
        format!(
            "GsOption(repeat={}, constraint={:?}, directivity={:?}, mask={})",
            self.inner.repeat,
            self.inner.constraint,
            self.inner.directivity,
            mask_repr(&self.mask)
        )
    }
}

#[pyclass(name = "GspatOption", module = "autd3_pattern_holo", eq, frozen)]
#[derive(PartialEq)]
pub struct GspatOption {
    inner: CoreGspatOption<'static>,
    mask: Mask,
}

#[pymethods]
impl GspatOption {
    #[new]
    #[pyo3(signature = (
        repeat = CoreGspatOption::default().repeat.get(),
        constraint = IntensityConstraint(CoreGspatOption::default().constraint),
        directivity = Directivity(CoreGspatOption::default().directivity),
        mask = all_enabled(),
    ))]
    fn new(
        repeat: usize,
        constraint: IntensityConstraint,
        directivity: Directivity,
        #[pyo3(from_py_with = extract_mask)] mask: Mask,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: CoreGspatOption {
                repeat: NonZeroUsize::new(repeat)
                    .ok_or_else(|| PyValueError::new_err("repeat must be >= 1"))?,
                constraint: constraint.0,
                directivity: directivity.0,
                ..CoreGspatOption::default()
            },
            mask,
        })
    }

    #[getter]
    fn repeat(&self) -> usize {
        self.inner.repeat.get()
    }

    #[getter]
    fn constraint(&self) -> IntensityConstraint {
        IntensityConstraint(self.inner.constraint)
    }

    #[getter]
    fn directivity(&self) -> Directivity {
        Directivity(self.inner.directivity)
    }

    #[getter]
    fn mask<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        mask_to_py(py, &self.mask)
    }

    fn __repr__(&self) -> String {
        format!(
            "GspatOption(repeat={}, constraint={:?}, directivity={:?}, mask={})",
            self.inner.repeat,
            self.inner.constraint,
            self.inner.directivity,
            mask_repr(&self.mask)
        )
    }
}

#[pyclass(name = "GreedyOption", module = "autd3_pattern_holo", frozen)]
pub struct GreedyOption {
    inner: CoreGreedyOption<'static>,
    mask: Mask,
}

#[pymethods]
impl GreedyOption {
    #[new]
    #[pyo3(signature = (
        phase_quantization_levels = CoreGreedyOption::default().phase_quantization_levels.get(),
        directivity = Directivity(CoreGreedyOption::default().directivity),
        mask = all_enabled(),
    ))]
    fn new(
        phase_quantization_levels: u8,
        directivity: Directivity,
        #[pyo3(from_py_with = extract_mask)] mask: Mask,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: CoreGreedyOption {
                phase_quantization_levels: NonZeroU8::new(phase_quantization_levels).ok_or_else(
                    || PyValueError::new_err("phase_quantization_levels must be >= 1"),
                )?,
                directivity: directivity.0,
                ..CoreGreedyOption::default()
            },
            mask,
        })
    }

    #[getter]
    fn phase_quantization_levels(&self) -> u8 {
        self.inner.phase_quantization_levels.get()
    }

    #[getter]
    fn directivity(&self) -> Directivity {
        Directivity(self.inner.directivity)
    }

    #[getter]
    fn mask<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        mask_to_py(py, &self.mask)
    }

    fn __repr__(&self) -> String {
        format!(
            "GreedyOption(phase_quantization_levels={}, directivity={:?}, mask={})",
            self.inner.phase_quantization_levels,
            self.inner.directivity,
            mask_repr(&self.mask)
        )
    }
}

fn collect_foci(foci: &[PyRef<'_, AmplitudeTarget>]) -> Vec<CoreAmplitudeTarget> {
    foci.iter().map(|f| f.0).collect()
}

fn mask_ref(mask: Option<&[Vec<bool>]>) -> TransducerMask<'_> {
    match mask {
        Some(m) => TransducerMask::Masked(m),
        None => TransducerMask::AllEnabled,
    }
}

fn with_dst_buffers<F>(
    phases: &Bound<'_, PyAny>,
    intensities: &Bound<'_, PyAny>,
    f: F,
) -> PyResult<()>
where
    F: FnOnce(&mut [Vec<Phase>], &mut [Vec<Intensity>]) -> PyResult<()>,
{
    let phase_capsule = capsule_of(phases)?;
    let intensity_capsule = capsule_of(intensities)?;
    let mut phases = phases_from_capsule_mut(&phase_capsule)?;
    let mut intensities = intensities_from_capsule_mut(&intensity_capsule)?;
    f(phases.as_mut_slice(), intensities.as_mut_slice())
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, phases, intensities))]
fn naive(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, AmplitudeTarget>>,
    wavelength: &Bound<'_, PyAny>,
    option: &NaiveOption,
    phases: &Bound<'_, PyAny>,
    intensities: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let wavelength = extract_length(wavelength)?;
    let option = CoreNaiveOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_dst_buffers(phases, intensities, |phases, intensities| {
        autd3_rs_pattern_holo::naive(
            &autd3_rs_pattern_holo::NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &option,
            phases,
            intensities,
        )
        .map_err(holo_err)
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, phases, intensities))]
fn gs(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, AmplitudeTarget>>,
    wavelength: &Bound<'_, PyAny>,
    option: &GsOption,
    phases: &Bound<'_, PyAny>,
    intensities: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let wavelength = extract_length(wavelength)?;
    let option = CoreGsOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_dst_buffers(phases, intensities, |phases, intensities| {
        autd3_rs_pattern_holo::gs(
            &autd3_rs_pattern_holo::NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &option,
            phases,
            intensities,
        )
        .map_err(holo_err)
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, phases, intensities))]
fn gspat(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, AmplitudeTarget>>,
    wavelength: &Bound<'_, PyAny>,
    option: &GspatOption,
    phases: &Bound<'_, PyAny>,
    intensities: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let wavelength = extract_length(wavelength)?;
    let option = CoreGspatOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_dst_buffers(phases, intensities, |phases, intensities| {
        autd3_rs_pattern_holo::gspat(
            &autd3_rs_pattern_holo::NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &option,
            phases,
            intensities,
        )
        .map_err(holo_err)
    })
}

fn ensure_distinct(addrs: &[usize]) -> PyResult<()> {
    for (i, lhs) in addrs.iter().enumerate() {
        if addrs[..i].contains(lhs) {
            return Err(PyValueError::new_err(
                "the same buffer must not be passed twice",
            ));
        }
    }
    Ok(())
}

fn with_dst_batches<F>(
    phases: &[Bound<'_, PyAny>],
    intensities: &[Bound<'_, PyAny>],
    f: F,
) -> PyResult<()>
where
    F: FnOnce(&mut [Vec<Vec<Phase>>], &mut [Vec<Vec<Intensity>>]) -> PyResult<()>,
{
    let phase_capsules = phases
        .iter()
        .map(capsule_of)
        .collect::<PyResult<Vec<_>>>()?;
    let intensity_capsules = intensities
        .iter()
        .map(capsule_of)
        .collect::<PyResult<Vec<_>>>()?;
    ensure_distinct(
        &phase_capsules
            .iter()
            .map(phase_buffer_addr)
            .collect::<PyResult<Vec<_>>>()?,
    )?;
    ensure_distinct(
        &intensity_capsules
            .iter()
            .map(intensity_buffer_addr)
            .collect::<PyResult<Vec<_>>>()?,
    )?;
    let mut phase_slots = phase_capsules
        .iter()
        .map(phases_from_capsule_mut)
        .collect::<PyResult<Vec<_>>>()?;
    let mut intensity_slots = intensity_capsules
        .iter()
        .map(intensities_from_capsule_mut)
        .collect::<PyResult<Vec<_>>>()?;
    let mut phase_problems = phase_slots
        .iter_mut()
        .map(|slot| core::mem::take(&mut **slot))
        .collect::<Vec<_>>();
    let mut intensity_problems = intensity_slots
        .iter_mut()
        .map(|slot| core::mem::take(&mut **slot))
        .collect::<Vec<_>>();
    let result = f(&mut phase_problems, &mut intensity_problems);
    for (slot, problem) in phase_slots.iter_mut().zip(phase_problems) {
        **slot = problem;
    }
    for (slot, problem) in intensity_slots.iter_mut().zip(intensity_problems) {
        **slot = problem;
    }
    result
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, phases, intensities))]
fn naive_batch(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, AmplitudeTarget>>,
    wavelength: &Bound<'_, PyAny>,
    option: &NaiveOption,
    phases: Vec<Bound<'_, PyAny>>,
    intensities: Vec<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let wavelength = extract_length(wavelength)?;
    let option = CoreNaiveOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_dst_batches(&phases, &intensities, |phases, intensities| {
        autd3_rs_pattern_holo::naive_batch(
            &autd3_rs_pattern_holo::NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &option,
            phases,
            intensities,
        )
        .map_err(holo_err)
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, phases, intensities))]
fn gs_batch(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, AmplitudeTarget>>,
    wavelength: &Bound<'_, PyAny>,
    option: &GsOption,
    phases: Vec<Bound<'_, PyAny>>,
    intensities: Vec<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let wavelength = extract_length(wavelength)?;
    let option = CoreGsOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_dst_batches(&phases, &intensities, |phases, intensities| {
        autd3_rs_pattern_holo::gs_batch(
            &autd3_rs_pattern_holo::NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &option,
            phases,
            intensities,
        )
        .map_err(holo_err)
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, option, phases, intensities))]
fn gspat_batch(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, AmplitudeTarget>>,
    wavelength: &Bound<'_, PyAny>,
    option: &GspatOption,
    phases: Vec<Bound<'_, PyAny>>,
    intensities: Vec<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let wavelength = extract_length(wavelength)?;
    let option = CoreGspatOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    with_dst_batches(&phases, &intensities, |phases, intensities| {
        autd3_rs_pattern_holo::gspat_batch(
            &autd3_rs_pattern_holo::NalgebraBackend,
            geometry,
            &foci,
            wavelength,
            &option,
            phases,
            intensities,
        )
        .map_err(holo_err)
    })
}

#[pyfunction]
#[pyo3(signature = (geometry, foci, wavelength, intensities, order, option, dst))]
fn greedy(
    geometry: &Bound<'_, PyAny>,
    foci: Vec<PyRef<'_, AmplitudeTarget>>,
    wavelength: &Bound<'_, PyAny>,
    intensities: &Bound<'_, PyAny>,
    order: Vec<usize>,
    option: &GreedyOption,
    dst: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let geo_capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&geo_capsule)?;
    let foci = collect_foci(&foci);
    let wavelength = extract_length(wavelength)?;
    let option = CoreGreedyOption {
        mask: mask_ref(option.mask.as_deref()),
        ..option.inner
    };
    let intensity_capsule = if intensities.hasattr("_capsule")? {
        Some(capsule_of(intensities)?)
    } else {
        None
    };
    let intensity_guard = intensity_capsule
        .as_ref()
        .map(intensities_from_capsule)
        .transpose()?;
    let intensities = match &intensity_guard {
        Some(guard) => PatternIntensity::PerDevice(guard),
        None => PatternIntensity::Uniform(Intensity(extract_u8(intensities)?)),
    };
    let dst_capsule = capsule_of(dst)?;
    let mut dst = phases_from_capsule_mut(&dst_capsule)?;
    autd3_rs_pattern_holo::greedy(
        geometry,
        &foci,
        wavelength,
        intensities,
        &order,
        &option,
        dst.as_mut_slice(),
    )
    .map_err(holo_err)
}

#[pymodule(gil_used = false)]
fn autd3_pattern_holo(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Amplitude>()?;
    m.add_class::<AmplitudeUnit>()?;
    m.add_class::<AmplitudeTarget>()?;
    m.add_class::<IntensityConstraint>()?;
    m.add_class::<Directivity>()?;
    m.add_class::<NaiveOption>()?;
    m.add_class::<GsOption>()?;
    m.add_class::<GspatOption>()?;
    m.add_class::<GreedyOption>()?;
    m.add("HoloError", holo_error_type(m.py())?)?;
    m.add("Pa", Py::new(m.py(), AmplitudeUnit::PA)?)?;
    m.add("kPa", Py::new(m.py(), AmplitudeUnit::KPA)?)?;
    m.add("dB", Py::new(m.py(), AmplitudeUnit::DB)?)?;
    m.add_function(wrap_pyfunction!(naive, m)?)?;
    m.add_function(wrap_pyfunction!(gs, m)?)?;
    m.add_function(wrap_pyfunction!(gspat, m)?)?;
    m.add_function(wrap_pyfunction!(greedy, m)?)?;
    m.add_function(wrap_pyfunction!(naive_batch, m)?)?;
    m.add_function(wrap_pyfunction!(gs_batch, m)?)?;
    m.add_function(wrap_pyfunction!(gspat_batch, m)?)?;
    Ok(())
}
