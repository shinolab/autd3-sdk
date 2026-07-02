use core::num::NonZeroU16;
use core::time::Duration as StdDuration;

use autd3_rs_core::units::Hz;
use autd3_rs_core::value::{
    Emission as CoreEmission, Intensity as CoreIntensity, Nearest, Phase as CorePhase,
    SamplingConfig as CoreSamplingConfig,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::error::to_pyerr;

#[pyclass(name = "Intensity", module = "autd3_core", from_py_object)]
#[derive(Clone)]
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

#[pyclass(name = "Phase", module = "autd3_core", from_py_object)]
#[derive(Clone)]
pub struct Phase(pub CorePhase);

#[pymethods]
impl Phase {
    #[new]
    fn new(value: u8) -> Self {
        Self(CorePhase(value))
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

    fn radian(&self) -> f32 {
        self.0.radian()
    }

    fn __int__(&self) -> u8 {
        self.0.0
    }

    fn __repr__(&self) -> String {
        format!("Phase(0x{:02X})", self.0.0)
    }
}

#[pyclass(name = "Emission", module = "autd3_core", skip_from_py_object)]
#[derive(Clone)]
pub struct Emission(pub CoreEmission);

#[pymethods]
impl Emission {
    #[new]
    fn new(phase: Phase, intensity: Intensity) -> Self {
        Self(CoreEmission {
            phase: phase.0,
            intensity: intensity.0,
        })
    }

    #[getter]
    fn phase(&self) -> Phase {
        Phase(self.0.phase)
    }

    #[getter]
    fn intensity(&self) -> Intensity {
        Intensity(self.0.intensity)
    }

    #[classattr]
    #[pyo3(name = "NULL")]
    fn py_null() -> Self {
        Self(CoreEmission::NULL)
    }

    fn __repr__(&self) -> String {
        format!(
            "Emission(phase=0x{:02X}, intensity=0x{:02X})",
            self.0.phase.0, self.0.intensity.0
        )
    }
}

#[pyclass(name = "SamplingConfig", module = "autd3_core", skip_from_py_object)]
#[derive(Clone)]
pub struct SamplingConfig(pub CoreSamplingConfig);

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
            return Ok(Self(match nearest.borrow().0 {
                NearestInner::Freq(hz) => CoreSamplingConfig::new(Nearest(hz * Hz)),
                NearestInner::Period(period) => CoreSamplingConfig::new(Nearest(period)),
            }));
        }
        if let Ok(freq) = value.extract::<crate::units::Freq>() {
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
        self.0.divide().map_err(to_pyerr)
    }

    fn freq(&self) -> PyResult<f32> {
        self.0.freq().map(|f| f.hz()).map_err(to_pyerr)
    }

    fn period(&self) -> PyResult<f32> {
        self.0
            .period()
            .map(|p| p.as_nanos() as f32 / 1000.0)
            .map_err(to_pyerr)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[derive(Clone, Copy)]
enum NearestInner {
    Freq(f32),
    Period(StdDuration),
}

#[pyclass(name = "Nearest", module = "autd3_core", skip_from_py_object)]
#[derive(Clone, Copy)]
pub struct PyNearest(NearestInner);

#[pymethods]
impl PyNearest {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(freq) = value.extract::<crate::units::Freq>() {
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

#[pyclass(name = "Duration", module = "autd3_core", from_py_object)]
#[derive(Clone, Copy)]
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
