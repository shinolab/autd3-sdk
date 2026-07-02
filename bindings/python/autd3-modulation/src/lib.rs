use core::num::NonZeroU16;

use autd3_python_capsule::{modulation_from_capsule, modulation_into_capsule, to_pyerr};
use autd3_rs_core::common::Angle;
use autd3_rs_core::params::MOD_BUFFER_SAMPLES;
use autd3_rs_core::units::Hz;
use autd3_rs_core::value::SamplingConfig;
use autd3_rs_modulation::{
    FourierOption as CoreFourierOption, SamplingMode, SineComponent as CoreSineComponent,
    SineOption as CoreSineOption, SquareOption as CoreSquareOption,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

fn extract_sampling_config(obj: &Bound<'_, PyAny>) -> PyResult<SamplingConfig> {
    let divide: u16 = obj.call_method0("divide")?.extract()?;
    let divide = NonZeroU16::new(divide)
        .ok_or_else(|| PyValueError::new_err("sampling_config divide must be >= 1"))?;
    Ok(SamplingConfig::new(divide))
}

fn freq_mode(freq: &Bound<'_, PyAny>) -> PyResult<SamplingMode> {
    if let Ok(hz) = freq.call_method0("nearest_hz") {
        return match hz.extract::<Option<f32>>()? {
            Some(hz) => Ok(SamplingMode::NearestFreq(hz * Hz)),
            None => Err(PyValueError::new_err(
                "modulation sampling frequency does not accept a period-based Nearest; use Nearest(freq)",
            )),
        };
    }
    let is_int: bool = freq
        .getattr("is_int")
        .and_then(|v| v.extract())
        .map_err(|_| {
            PyValueError::new_err(
                "frequency must carry a unit, e.g. 200 * Hz (bare numbers are no longer accepted)",
            )
        })?;
    Ok(if is_int {
        let hz: u32 = freq.getattr("hz_int")?.extract()?;
        SamplingMode::ExactFreq(hz * Hz)
    } else {
        let hz: f32 = freq.getattr("hz")?.extract()?;
        SamplingMode::ExactFreqFloat(hz * Hz)
    })
}

fn extract_angle(obj: &Bound<'_, PyAny>) -> PyResult<Angle> {
    let radian: f32 = obj
        .getattr("radian")
        .and_then(|v| v.extract())
        .map_err(|_| PyValueError::new_err("phase must be an Angle, e.g. 90 * deg"))?;
    Ok(Angle::from_radian(radian))
}

#[pyclass(name = "SineOption", module = "autd3_modulation", skip_from_py_object)]
pub struct SineOption {
    pub(crate) inner: CoreSineOption,
}

#[pymethods]
impl SineOption {
    #[new]
    #[pyo3(signature = (amplitude = 0xFF, offset = 0x80, phase = Angle::ZERO, clamp = false, sampling_config = SamplingConfig::FREQ_4K))]
    fn new(
        amplitude: u8,
        offset: u8,
        #[pyo3(from_py_with = extract_angle)] phase: Angle,
        clamp: bool,
        #[pyo3(from_py_with = extract_sampling_config)] sampling_config: SamplingConfig,
    ) -> Self {
        Self {
            inner: CoreSineOption {
                amplitude,
                offset,
                phase,
                clamp,
                sampling_config,
            },
        }
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
    #[pyo3(signature = (low = 0x00, high = 0xFF, duty = 0.5, sampling_config = SamplingConfig::FREQ_4K))]
    fn new(
        low: u8,
        high: u8,
        duty: f32,
        #[pyo3(from_py_with = extract_sampling_config)] sampling_config: SamplingConfig,
    ) -> Self {
        Self {
            inner: CoreSquareOption {
                low,
                high,
                duty,
                sampling_config,
            },
        }
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
    inner: CoreSineComponent<SamplingMode>,
}

#[pymethods]
impl SineComponent {
    #[new]
    #[pyo3(signature = (freq, option))]
    fn new(freq: &Bound<'_, PyAny>, option: &SineOption) -> PyResult<Self> {
        Ok(Self {
            inner: CoreSineComponent {
                freq: freq_mode(freq)?,
                option: option.inner,
            },
        })
    }
}

#[pyfunction]
#[pyo3(signature = (freq, option, buffer))]
fn sine(
    py: Python<'_>,
    freq: &Bound<'_, PyAny>,
    option: &SineOption,
    mut buffer: PyRefMut<'_, ModulationBuffer>,
) -> PyResult<()> {
    autd3_rs_modulation::sine(freq_mode(freq)?, &option.inner, &mut buffer.data)
        .map_err(|e| to_pyerr(py, e))?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (freq, option, buffer))]
fn square(
    py: Python<'_>,
    freq: &Bound<'_, PyAny>,
    option: &SquareOption,
    mut buffer: PyRefMut<'_, ModulationBuffer>,
) -> PyResult<()> {
    autd3_rs_modulation::square(freq_mode(freq)?, &option.inner, &mut buffer.data)
        .map_err(|e| to_pyerr(py, e))?;
    Ok(())
}

#[pyfunction]
fn constant(intensity: u8, mut buffer: PyRefMut<'_, ModulationBuffer>) {
    autd3_rs_modulation::constant(intensity, &mut buffer.data);
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
fn samples_per_period(divider: u16, freq_hz: u32) -> Option<u32> {
    autd3_rs_modulation::samples_per_period(divider, freq_hz)
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
    m.add_function(wrap_pyfunction!(constant, m)?)?;
    m.add_function(wrap_pyfunction!(fourier, m)?)?;
    m.add_function(wrap_pyfunction!(radiation_pressure, m)?)?;
    m.add_function(wrap_pyfunction!(radiation_pressure_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(samples_per_period, m)?)?;
    m.add_function(wrap_pyfunction!(_read_modulation_capsule, m)?)?;
    Ok(())
}
