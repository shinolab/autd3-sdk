use core::hash::{Hash, Hasher};
use core::num::NonZeroU16;
use core::time::Duration as StdDuration;

use autd3_rs_core::nalgebra::Complex;
use autd3_rs_core::value::{
    Intensity as CoreIntensity, Phase as CorePhase, SamplingConfig as CoreSamplingConfig,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyComplex};

use crate::error::invalid_argument;
use crate::units::{Angle, Freq, hash_f32};

#[pyclass(
    name = "Intensity",
    module = "autd3_core",
    eq,
    ord,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Intensity(pub CoreIntensity);

#[pymethods]
impl Intensity {
    #[new]
    #[pyo3(signature = (value = CoreIntensity::default().0))]
    fn new(value: u8) -> Self {
        Self(CoreIntensity(value))
    }

    #[classattr]
    #[pyo3(name = "MAX")]
    fn py_max() -> Self {
        Self(CoreIntensity::MAX)
    }

    #[classattr]
    #[pyo3(name = "MIN")]
    fn py_min() -> Self {
        Self(CoreIntensity::MIN)
    }

    #[getter]
    fn value(&self) -> u8 {
        self.0.0
    }

    fn __int__(&self) -> u8 {
        self.0.0
    }

    fn __index__(&self) -> u8 {
        self.0.0
    }

    fn __repr__(&self) -> String {
        format!("Intensity(0x{:02X})", self.0.0)
    }
}

#[pyclass(
    name = "Phase",
    module = "autd3_core",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, PartialEq, Eq)]
pub struct Phase(pub CorePhase);

impl Hash for Phase {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.0.hash(state);
    }
}

fn extract_complex(value: &Bound<'_, PyAny>) -> PyResult<Option<Complex<f32>>> {
    let complex = if let Ok(complex) = value.cast::<PyComplex>() {
        complex.clone()
    } else if value.hasattr("__complex__")? {
        value
            .py()
            .get_type::<PyComplex>()
            .call1((value,))?
            .cast_into::<PyComplex>()?
    } else {
        return Ok(None);
    };
    #[allow(clippy::cast_possible_truncation)]
    Ok(Some(Complex::new(
        complex.real() as f32,
        complex.imag() as f32,
    )))
}

fn extract_phase(value: &Bound<'_, PyAny>) -> PyResult<CorePhase> {
    if let Ok(v) = value.extract::<u8>() {
        return Ok(CorePhase(v));
    }
    if let Ok(angle) = value.cast::<Angle>() {
        return Ok(CorePhase::from(angle.get().0));
    }
    if let Some(complex) = extract_complex(value)? {
        return Ok(CorePhase::from(complex));
    }
    Err(PyValueError::new_err(
        "Phase expects an int (0-255), an Angle (e.g. 0.5 * pi * rad) or a complex number",
    ))
}

#[pymethods]
impl Phase {
    #[new]
    #[pyo3(signature = (value = CorePhase::default()))]
    fn new(#[pyo3(from_py_with = extract_phase)] value: CorePhase) -> Self {
        Self(value)
    }

    #[classattr]
    #[pyo3(name = "ZERO")]
    fn py_zero() -> Self {
        Self(CorePhase::ZERO)
    }

    #[classattr]
    #[pyo3(name = "PI")]
    fn py_pi() -> Self {
        Self(CorePhase::PI)
    }

    #[getter]
    fn value(&self) -> u8 {
        self.0.0
    }

    fn rad(&self) -> f32 {
        self.0.rad()
    }

    fn __int__(&self) -> u8 {
        self.0.0
    }

    fn __index__(&self) -> u8 {
        self.0.0
    }

    fn __add__(&self, rhs: Phase) -> Self {
        Self(self.0 + rhs.0)
    }

    fn __sub__(&self, rhs: Phase) -> Self {
        Self(self.0 - rhs.0)
    }

    fn __repr__(&self) -> String {
        format!("Phase(0x{:02X})", self.0.0)
    }
}

#[pyclass(name = "SamplingConfig", module = "autd3_core", eq, hash, frozen)]
#[derive(Clone, PartialEq)]
pub struct SamplingConfig(pub CoreSamplingConfig);

impl Hash for SamplingConfig {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.divide().hash(state);
    }
}

#[pymethods]
impl SamplingConfig {
    #[classattr]
    #[pyo3(name = "FREQ_4K")]
    fn freq_4k() -> Self {
        Self(CoreSamplingConfig::FREQ_4K)
    }

    #[classattr]
    #[pyo3(name = "FREQ_40K")]
    fn freq_40k() -> Self {
        Self(CoreSamplingConfig::FREQ_40K)
    }

    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let py = value.py();
        if let Ok(freq) = value.extract::<Freq>() {
            return freq
                .sampling_config()
                .map(Self)
                .map_err(|e| invalid_argument(py, e));
        }
        if let Ok(period) = value.extract::<Duration>() {
            return CoreSamplingConfig::from_period(period.0)
                .map(Self)
                .map_err(|e| invalid_argument(py, e));
        }
        let divide: u16 = value.extract().map_err(|_| {
            PyValueError::new_err(
                "SamplingConfig expects an int divider, a frequency (e.g. 4000.0 * Hz), or a Duration",
            )
        })?;
        let divide = NonZeroU16::new(divide)
            .ok_or_else(|| invalid_argument(py, "divide must be non-zero"))?;
        Ok(Self(CoreSamplingConfig::new(divide)))
    }

    fn divide(&self) -> u16 {
        self.0.divide().get()
    }

    fn freq(&self) -> Freq {
        Freq(self.0.freq())
    }

    fn period(&self) -> Duration {
        Duration(self.0.period())
    }

    fn _sampling_config_capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        autd3_python_capsule::sampling_config_into_capsule(py, self.0)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "Nearest", module = "autd3_core", eq, hash, frozen)]
#[derive(PartialEq)]
pub struct PyNearest(f32);

impl Hash for PyNearest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_f32(self.0, state);
    }
}

#[pymethods]
impl PyNearest {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let freq = value
            .extract::<Freq>()
            .map_err(|_| PyValueError::new_err("Nearest expects a frequency (e.g. 4000.0 * Hz)"))?;
        Ok(Self(freq.0.hz()))
    }

    fn nearest_hz(&self) -> f32 {
        self.0
    }

    fn __repr__(&self) -> String {
        format!("Nearest({} Hz)", self.0)
    }
}

#[pyclass(
    name = "Duration",
    module = "autd3_core",
    eq,
    ord,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Hash)]
pub struct Duration(pub StdDuration);

#[pymethods]
impl Duration {
    #[staticmethod]
    fn from_nanos(nanos: u64) -> Self {
        Self(StdDuration::from_nanos(nanos))
    }

    #[staticmethod]
    fn from_micros(micros: u64) -> Self {
        Self(StdDuration::from_micros(micros))
    }

    #[staticmethod]
    fn from_millis(millis: u64) -> Self {
        Self(StdDuration::from_millis(millis))
    }

    #[staticmethod]
    fn from_secs(secs: u64) -> Self {
        Self(StdDuration::from_secs(secs))
    }

    #[staticmethod]
    fn from_secs_f64(secs: f64) -> PyResult<Self> {
        if !secs.is_finite() || secs < 0.0 {
            return Err(PyValueError::new_err(
                "secs must be finite and non-negative",
            ));
        }
        Ok(Self(StdDuration::from_secs_f64(secs)))
    }

    fn as_nanos(&self) -> u128 {
        self.0.as_nanos()
    }

    fn as_micros(&self) -> u128 {
        self.0.as_micros()
    }

    fn as_millis(&self) -> u128 {
        self.0.as_millis()
    }

    fn as_secs_f64(&self) -> f64 {
        self.0.as_secs_f64()
    }

    fn __repr__(&self) -> String {
        format!("Duration({:?})", self.0)
    }
}
