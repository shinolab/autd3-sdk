use core::num::NonZeroU16;

use autd3_python_capsule::{modulation_from_capsule, modulation_into_capsule, to_pyerr};
use autd3_rs_core::units::{Hz, rad};
use autd3_rs_core::value::{Intensity, SamplingConfig};
use autd3_rs_modulation::SineOption as CoreSineOption;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

#[pyclass(name = "SineOption", module = "autd3_modulation", skip_from_py_object)]
pub struct SineOption {
    inner: CoreSineOption,
}

#[pymethods]
impl SineOption {
    #[new]
    #[pyo3(signature = (intensity = 0xFF, offset = 0x80, phase = 0.0, clamp = false, sampling_divide = None))]
    fn new(
        intensity: u8,
        offset: u8,
        phase: f32,
        clamp: bool,
        sampling_divide: Option<u16>,
    ) -> PyResult<Self> {
        let sampling_config = match sampling_divide {
            Some(d) => SamplingConfig::Divide(
                NonZeroU16::new(d)
                    .ok_or_else(|| PyValueError::new_err("sampling_divide must be >= 1"))?,
            ),
            None => SamplingConfig::FREQ_4K,
        };
        Ok(Self {
            inner: CoreSineOption {
                intensity: Intensity(intensity),
                offset,
                phase: phase * rad,
                clamp,
                sampling_config,
            },
        })
    }
}

#[pyclass(name = "ModulationBuffer", module = "autd3_modulation")]
pub struct ModulationBuffer {
    data: Vec<u8>,
}

#[pymethods]
impl ModulationBuffer {
    #[new]
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    #[staticmethod]
    fn from_bytes(data: Vec<u8>) -> Self {
        Self { data }
    }

    fn __len__(&self) -> usize {
        self.data.len()
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        modulation_into_capsule(py, self.data.clone())
    }
}

#[pyfunction]
fn sine(
    py: Python<'_>,
    freq: f32,
    option: &SineOption,
    mut buffer: PyRefMut<'_, ModulationBuffer>,
) -> PyResult<()> {
    autd3_rs_modulation::sine(freq * Hz, &option.inner, &mut buffer.data)
        .map_err(|e| to_pyerr(py, e))?;
    Ok(())
}

#[pyfunction]
fn _read_modulation_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(modulation_from_capsule(capsule)?.len())
}

#[pymodule]
fn autd3_modulation(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SineOption>()?;
    m.add_class::<ModulationBuffer>()?;
    m.add_function(wrap_pyfunction!(sine, m)?)?;
    m.add_function(wrap_pyfunction!(_read_modulation_capsule, m)?)?;
    Ok(())
}
