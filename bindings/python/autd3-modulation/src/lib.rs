use core::num::NonZeroU16;

use autd3_python_capsule::{modulation_from_capsule, modulation_into_capsule, to_pyerr};
use autd3_rs_core::common::Freq;
use autd3_rs_core::params::MOD_BUFFER_SAMPLES;
use autd3_rs_core::units::{Hz, rad};
use autd3_rs_core::value::SamplingConfig;
use autd3_rs_modulation::{
    FourierOption as CoreFourierOption, SineComponent as CoreSineComponent,
    SineOption as CoreSineOption, SquareOption as CoreSquareOption,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

#[pyclass(name = "SineOption", module = "autd3_modulation", skip_from_py_object)]
pub struct SineOption {
    pub(crate) inner: CoreSineOption,
}

#[pymethods]
impl SineOption {
    #[new]
    #[pyo3(signature = (amplitude = 0xFF, offset = 0x80, phase = 0.0, clamp = false, sampling_divide = None))]
    fn new(
        amplitude: u8,
        offset: u8,
        phase: f32,
        clamp: bool,
        sampling_divide: Option<u16>,
    ) -> PyResult<Self> {
        let sampling_config = match sampling_divide {
            Some(d) => SamplingConfig::new(
                NonZeroU16::new(d)
                    .ok_or_else(|| PyValueError::new_err("sampling_divide must be >= 1"))?,
            ),
            None => SamplingConfig::FREQ_4K,
        };
        Ok(Self {
            inner: CoreSineOption {
                amplitude,
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
fn modulation_buffer() -> ModulationBuffer {
    ModulationBuffer {
        data: Vec::with_capacity(MOD_BUFFER_SAMPLES),
    }
}

#[pyclass(
    name = "SquareOption",
    module = "autd3_modulation",
    skip_from_py_object
)]
pub struct SquareOption {
    inner: CoreSquareOption,
}

#[pymethods]
impl SquareOption {
    #[new]
    #[pyo3(signature = (low = 0x00, high = 0xFF, duty = 0.5, sampling_divide = None))]
    fn new(low: u8, high: u8, duty: f32, sampling_divide: Option<u16>) -> PyResult<Self> {
        let sampling_config = match sampling_divide {
            Some(d) => SamplingConfig::new(
                NonZeroU16::new(d)
                    .ok_or_else(|| PyValueError::new_err("sampling_divide must be >= 1"))?,
            ),
            None => SamplingConfig::FREQ_4K,
        };
        Ok(Self {
            inner: CoreSquareOption {
                low,
                high,
                duty,
                sampling_config,
            },
        })
    }
}

#[pyclass(
    name = "FourierOption",
    module = "autd3_modulation",
    skip_from_py_object
)]
pub struct FourierOption {
    inner: CoreFourierOption,
}

#[pymethods]
impl FourierOption {
    #[new]
    #[pyo3(signature = (scale_factor = None, clamp = false, offset = 0x00))]
    fn new(scale_factor: Option<f32>, clamp: bool, offset: u8) -> Self {
        Self {
            inner: CoreFourierOption {
                scale_factor,
                clamp,
                offset,
            },
        }
    }
}

#[pyclass(
    name = "SineComponent",
    module = "autd3_modulation",
    skip_from_py_object
)]
pub struct SineComponent {
    inner: CoreSineComponent<Freq<f32>>,
}

#[pymethods]
impl SineComponent {
    #[new]
    fn new(freq: f32, option: &SineOption) -> Self {
        Self {
            inner: CoreSineComponent {
                freq: freq * Hz,
                option: option.inner,
            },
        }
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
fn square(
    py: Python<'_>,
    freq: f32,
    option: &SquareOption,
    mut buffer: PyRefMut<'_, ModulationBuffer>,
) -> PyResult<()> {
    autd3_rs_modulation::square(freq * Hz, &option.inner, &mut buffer.data)
        .map_err(|e| to_pyerr(py, e))?;
    Ok(())
}

#[pyfunction]
fn fourier(
    py: Python<'_>,
    components: Vec<PyRef<'_, SineComponent>>,
    option: &FourierOption,
    mut buffer: PyRefMut<'_, ModulationBuffer>,
) -> PyResult<()> {
    let components = components.iter().map(|c| c.inner).collect::<Vec<_>>();
    autd3_rs_modulation::fourier(&components, &option.inner, &mut buffer.data)
        .map_err(|e| to_pyerr(py, e))?;
    Ok(())
}

#[pyfunction]
fn radiation_pressure(src: PyRef<'_, ModulationBuffer>, mut out: PyRefMut<'_, ModulationBuffer>) {
    autd3_rs_modulation::radiation_pressure(&src.data, &mut out.data);
}

#[pyfunction]
fn radiation_pressure_inplace(mut buffer: PyRefMut<'_, ModulationBuffer>) {
    autd3_rs_modulation::radiation_pressure_inplace(&mut buffer.data);
}

#[pyfunction]
fn _read_modulation_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(modulation_from_capsule(capsule)?.len())
}

#[pymodule]
fn autd3_modulation(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SineOption>()?;
    m.add_class::<SquareOption>()?;
    m.add_class::<FourierOption>()?;
    m.add_class::<SineComponent>()?;
    m.add_class::<ModulationBuffer>()?;
    m.add_function(wrap_pyfunction!(modulation_buffer, m)?)?;
    m.add_function(wrap_pyfunction!(sine, m)?)?;
    m.add_function(wrap_pyfunction!(square, m)?)?;
    m.add_function(wrap_pyfunction!(fourier, m)?)?;
    m.add_function(wrap_pyfunction!(radiation_pressure, m)?)?;
    m.add_function(wrap_pyfunction!(radiation_pressure_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(_read_modulation_capsule, m)?)?;
    Ok(())
}
