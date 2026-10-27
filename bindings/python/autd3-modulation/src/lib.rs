use autd3_python_capsule::extract::{angle_to_py, extract_angle, extract_sampling_config};
use autd3_python_capsule::numpy;
use autd3_python_capsule::{modulation_into_capsule, to_pyerr};
use autd3_rs_core::common::Angle;
use autd3_rs_core::params::MOD_BUFFER_SAMPLES;
use autd3_rs_core::units::Hz;
use autd3_rs_core::value::SamplingConfig;
use autd3_rs_modulation::{
    FourierOption as CoreFourierOption, SineComponent as CoreSineComponent,
    SineOption as CoreSineOption, SquareOption as CoreSquareOption,
};
use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

fn freq_hz(freq: &Bound<'_, PyAny>) -> PyResult<f32> {
    freq.getattr("hz").and_then(|v| v.extract()).map_err(|_| {
        PyValueError::new_err(
            "frequency must carry a unit, e.g. 200 * Hz (bare numbers are no longer accepted)",
        )
    })
}

#[pyclass(name = "SineOption", module = "autd3_modulation", eq, frozen)]
#[derive(Clone, Copy, PartialEq)]
pub struct SineOption(pub(crate) CoreSineOption);

#[pymethods]
impl SineOption {
    #[new]
    #[pyo3(signature = (
        amplitude = CoreSineOption::default().amplitude,
        offset = CoreSineOption::default().offset,
        phase = CoreSineOption::default().phase,
        clamp = CoreSineOption::default().clamp,
    ))]
    fn new(
        amplitude: u8,
        offset: u8,
        #[pyo3(from_py_with = extract_angle)] phase: Angle,
        clamp: bool,
    ) -> Self {
        Self(CoreSineOption {
            amplitude,
            offset,
            phase,
            clamp,
        })
    }

    #[getter]
    fn amplitude(&self) -> u8 {
        self.0.amplitude
    }

    #[getter]
    fn offset(&self) -> u8 {
        self.0.offset
    }

    #[getter]
    fn phase<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        angle_to_py(py, self.0.phase)
    }

    #[getter]
    fn clamp(&self) -> bool {
        self.0.clamp
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "ModulationBuffer", module = "autd3_modulation")]
pub struct ModulationBuffer {
    data: Vec<u8>,
}

#[pymethods]
impl ModulationBuffer {
    #[new]
    fn new(length: usize) -> Self {
        Self {
            data: vec![0u8; length],
        }
    }

    #[staticmethod]
    fn from_array(values: &Bound<'_, PyAny>) -> PyResult<Self> {
        if numpy::is_ndarray(values)? {
            return Ok(Self {
                data: numpy::u8_vector_bytes(values)?.as_bytes().to_vec(),
            });
        }
        let data: Vec<u8> = values.extract().map_err(|e| {
            PyTypeError::new_err(format!(
                "values must be a uint8 numpy.ndarray or a sequence of ints in 0..=255: {e}"
            ))
        })?;
        Ok(Self { data })
    }

    fn copy_from(&mut self, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let bytes = numpy::u8_vector_bytes(values)?;
        self.data.clear();
        self.data.extend_from_slice(bytes.as_bytes());
        Ok(())
    }

    fn to_numpy<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        numpy::u8_vector(py, &self.data)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .cast::<Self>()
            .is_ok_and(|other| self.data == other.borrow().data)
    }

    fn __repr__(&self) -> String {
        format!("ModulationBuffer(len={})", self.data.len())
    }

    fn __len__(&self) -> usize {
        self.data.len()
    }

    fn __getitem__(&self, index: usize) -> PyResult<u8> {
        self.data
            .get(index)
            .copied()
            .ok_or_else(|| PyIndexError::new_err("modulation index out of range"))
    }

    fn __setitem__(&mut self, index: usize, value: u8) -> PyResult<()> {
        *self
            .data
            .get_mut(index)
            .ok_or_else(|| PyIndexError::new_err("modulation index out of range"))? = value;
        Ok(())
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

#[pyclass(name = "SquareOption", module = "autd3_modulation", eq, frozen)]
#[derive(PartialEq)]
pub struct SquareOption(CoreSquareOption);

#[pymethods]
impl SquareOption {
    #[new]
    #[pyo3(signature = (
        low = CoreSquareOption::default().low,
        high = CoreSquareOption::default().high,
        duty = CoreSquareOption::default().duty,
    ))]
    fn new(low: u8, high: u8, duty: f32) -> Self {
        Self(CoreSquareOption { low, high, duty })
    }

    #[getter]
    fn low(&self) -> u8 {
        self.0.low
    }

    #[getter]
    fn high(&self) -> u8 {
        self.0.high
    }

    #[getter]
    fn duty(&self) -> f32 {
        self.0.duty
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "FourierOption", module = "autd3_modulation", eq, frozen)]
#[derive(PartialEq)]
pub struct FourierOption(CoreFourierOption);

#[pymethods]
impl FourierOption {
    #[new]
    #[pyo3(signature = (
        scale_factor = CoreFourierOption::default().scale_factor,
        clamp = CoreFourierOption::default().clamp,
        offset = CoreFourierOption::default().offset,
    ))]
    fn new(scale_factor: Option<f32>, clamp: bool, offset: u8) -> Self {
        Self(CoreFourierOption {
            scale_factor,
            clamp,
            offset,
        })
    }

    #[getter]
    fn scale_factor(&self) -> Option<f32> {
        self.0.scale_factor
    }

    #[getter]
    fn clamp(&self) -> bool {
        self.0.clamp
    }

    #[getter]
    fn offset(&self) -> u8 {
        self.0.offset
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "SineComponent", module = "autd3_modulation", eq, frozen)]
#[derive(Clone, Copy, PartialEq)]
pub struct SineComponent(CoreSineComponent);

#[pymethods]
impl SineComponent {
    #[new]
    #[pyo3(signature = (freq, option))]
    fn new(freq: &Bound<'_, PyAny>, option: &SineOption) -> PyResult<Self> {
        Ok(Self(CoreSineComponent {
            freq: freq_hz(freq)? * Hz,
            option: option.0,
        }))
    }

    #[getter]
    fn freq<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("autd3_core")?
            .getattr("Freq")?
            .call_method1("from_hz", (self.0.freq.hz(),))
    }

    #[getter]
    fn option(&self) -> SineOption {
        SineOption(self.0.option)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyfunction]
#[pyo3(signature = (freq, option, config, dst))]
fn sine(
    py: Python<'_>,
    freq: &Bound<'_, PyAny>,
    option: &SineOption,
    #[pyo3(from_py_with = extract_sampling_config)] config: SamplingConfig,
    mut dst: PyRefMut<'_, ModulationBuffer>,
) -> PyResult<()> {
    autd3_rs_modulation::sine(freq_hz(freq)? * Hz, &option.0, config, &mut dst.data)
        .map_err(|e| to_pyerr(py, e))?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (freq, option, config, dst))]
fn square(
    py: Python<'_>,
    freq: &Bound<'_, PyAny>,
    option: &SquareOption,
    #[pyo3(from_py_with = extract_sampling_config)] config: SamplingConfig,
    mut dst: PyRefMut<'_, ModulationBuffer>,
) -> PyResult<()> {
    autd3_rs_modulation::square(freq_hz(freq)? * Hz, &option.0, config, &mut dst.data)
        .map_err(|e| to_pyerr(py, e))?;
    Ok(())
}

#[pyfunction]
fn constant(amplitude: u8, mut dst: PyRefMut<'_, ModulationBuffer>) {
    autd3_rs_modulation::constant(amplitude, &mut dst.data);
}

#[pyfunction]
fn fourier(
    py: Python<'_>,
    components: Vec<PyRef<'_, SineComponent>>,
    option: &FourierOption,
    #[pyo3(from_py_with = extract_sampling_config)] config: SamplingConfig,
    mut dst: PyRefMut<'_, ModulationBuffer>,
) -> PyResult<()> {
    let components = components.iter().map(|c| c.0).collect::<Vec<_>>();
    autd3_rs_modulation::fourier(&components, &option.0, config, &mut dst.data)
        .map_err(|e| to_pyerr(py, e))?;
    Ok(())
}

#[pyfunction]
fn radiation_pressure_inplace(mut buffer: PyRefMut<'_, ModulationBuffer>) {
    autd3_rs_modulation::radiation_pressure_inplace(&mut buffer.data);
}

#[pyfunction]
fn samples_per_period(divider: u16, freq: &Bound<'_, PyAny>) -> PyResult<Option<u32>> {
    let freq_hz = freq_hz(freq)?;
    Ok(core::num::NonZeroU16::new(divider).and_then(|divider| {
        autd3_rs_modulation::samples_per_period(divider, autd3_rs_core::Freq::from_hz(freq_hz))
    }))
}

#[pymodule(gil_used = false)]
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
    m.add_function(wrap_pyfunction!(radiation_pressure_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(samples_per_period, m)?)?;
    Ok(())
}
