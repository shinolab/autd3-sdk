use autd3_rs_core::value::{
    Emission as CoreEmission, Intensity as CoreIntensity, Phase as CorePhase,
    SamplingConfig as CoreSamplingConfig,
};
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

    fn divide(&self) -> PyResult<u16> {
        self.0.divide().map_err(to_pyerr)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}
