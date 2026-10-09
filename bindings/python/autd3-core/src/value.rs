use core::hash::{Hash, Hasher};
use core::num::NonZeroU16;
use core::time::Duration as StdDuration;

use autd3_rs_core::nalgebra::Complex;
use autd3_rs_core::units::Hz;
use autd3_rs_core::value::{
    Intensity as CoreIntensity, Nearest, Phase as CorePhase, SamplingConfig as CoreSamplingConfig,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyComplex;

use crate::error::to_pyerr;
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

#[pymethods]
impl Phase {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(v) = value.extract::<u8>() {
            return Ok(Self(CorePhase(v)));
        }
        if let Ok(angle) = value.cast::<Angle>() {
            return Ok(Self(CorePhase::from(angle.get().0)));
        }
        if let Some(complex) = extract_complex(value)? {
            return Ok(Self(CorePhase::from(complex)));
        }
        Err(PyValueError::new_err(
            "Phase expects an int (0-255), an Angle (e.g. 0.5 * pi * rad) or a complex number",
        ))
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

    fn __repr__(&self) -> String {
        format!("Phase(0x{:02X})", self.0.0)
    }
}

#[pyclass(
    name = "SamplingConfig",
    module = "autd3_core",
    eq,
    hash,
    frozen,
    skip_from_py_object
)]
#[derive(Clone, PartialEq)]
pub struct SamplingConfig(pub CoreSamplingConfig);

impl Hash for SamplingConfig {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.divide().ok().hash(state);
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
        if let Ok(nearest) = value.cast::<PyNearest>() {
            return Ok(Self(match nearest.get().0 {
                NearestInner::Freq(hz) => CoreSamplingConfig::new(Nearest(hz * Hz)),
                NearestInner::Period(period) => CoreSamplingConfig::new(Nearest(period)),
            }));
        }
        if let Ok(freq) = value.extract::<Freq>() {
            return Ok(Self(freq.sampling_config()));
        }
        if let Ok(period) = value.extract::<Duration>() {
            return Ok(Self(CoreSamplingConfig::new(period.0)));
        }
        let divide: u16 = value.extract().map_err(|_| {
            PyValueError::new_err(
                "SamplingConfig expects an int divider, a frequency (e.g. 4000.0 * Hz), a Duration, or Nearest(...)",
            )
        })?;
        let divide = NonZeroU16::new(divide)
            .ok_or_else(|| PyValueError::new_err("divide must be non-zero"))?;
        Ok(Self(CoreSamplingConfig::new(divide)))
    }

    fn divide(&self) -> PyResult<u16> {
        self.0.divide().map(NonZeroU16::get).map_err(to_pyerr)
    }

    fn freq(&self) -> PyResult<Freq> {
        self.0
            .freq()
            .map(|f| Freq::from_hz_f32(f.hz()))
            .map_err(to_pyerr)
    }

    fn period(&self) -> PyResult<Duration> {
        self.0.period().map(Duration).map_err(to_pyerr)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum NearestInner {
    Freq(f32),
    Period(StdDuration),
}

#[pyclass(
    name = "Nearest",
    module = "autd3_core",
    eq,
    hash,
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Copy, PartialEq)]
pub struct PyNearest(NearestInner);

impl Hash for PyNearest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.0 {
            NearestInner::Freq(hz) => {
                0u8.hash(state);
                hash_f32(hz, state);
            }
            NearestInner::Period(period) => {
                1u8.hash(state);
                period.hash(state);
            }
        }
    }
}

#[pymethods]
impl PyNearest {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(freq) = value.extract::<Freq>() {
            return Ok(Self(NearestInner::Freq(freq.hz_f32())));
        }
        if let Ok(period) = value.extract::<Duration>() {
            return Ok(Self(NearestInner::Period(period.0)));
        }
        Err(PyValueError::new_err(
            "Nearest expects a frequency (e.g. 4000.0 * Hz) or a Duration",
        ))
    }

    fn nearest_hz(&self) -> Option<f32> {
        match self.0 {
            NearestInner::Freq(hz) => Some(hz),
            NearestInner::Period(_) => None,
        }
    }

    fn nearest_nanos(&self) -> Option<u128> {
        match self.0 {
            NearestInner::Freq(_) => None,
            NearestInner::Period(period) => Some(period.as_nanos()),
        }
    }

    fn __repr__(&self) -> String {
        match self.0 {
            NearestInner::Freq(hz) => format!("Nearest({hz} Hz)"),
            NearestInner::Period(period) => format!("Nearest({period:?})"),
        }
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
