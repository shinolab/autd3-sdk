use autd3_rs_core::common::units::Hz;
use autd3_rs_core::common::{Angle as CoreAngle, Length as CoreLength, Velocity as CoreVelocity};
use autd3_rs_core::value::SamplingConfig as CoreSamplingConfig;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

fn number_f32(obj: &Bound<'_, PyAny>) -> PyResult<f32> {
    obj.extract::<f32>()
        .map_err(|_| PyValueError::new_err("expected a number"))
}

#[derive(Clone, Copy)]
enum FreqVal {
    Int(u32),
    Float(f32),
}

#[pyclass(name = "Freq", module = "autd3_core", from_py_object)]
#[derive(Clone, Copy)]
pub struct Freq(FreqVal);

#[pymethods]
impl Freq {
    #[getter]
    fn hz(&self) -> f32 {
        match self.0 {
            FreqVal::Int(v) => v as f32,
            FreqVal::Float(f) => f,
        }
    }

    #[getter]
    fn is_int(&self) -> bool {
        matches!(self.0, FreqVal::Int(_))
    }

    #[getter]
    fn hz_int(&self) -> Option<u32> {
        match self.0 {
            FreqVal::Int(v) => Some(v),
            FreqVal::Float(_) => None,
        }
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .extract::<Freq>()
            .is_ok_and(|o| self.hz().to_bits() == o.hz().to_bits())
    }

    fn __repr__(&self) -> String {
        match self.0 {
            FreqVal::Int(v) => format!("{v} Hz"),
            FreqVal::Float(f) => format!("{f} Hz"),
        }
    }
}

#[pyclass(name = "Velocity", module = "autd3_core", from_py_object)]
#[derive(Clone, Copy)]
pub struct Velocity(pub CoreVelocity);

#[pymethods]
impl Velocity {
    #[staticmethod]
    fn from_mm_s(mm_per_s: f32) -> Self {
        Self(CoreVelocity::from_mm_s(mm_per_s))
    }

    #[staticmethod]
    fn from_m_s(m_per_s: f32) -> Self {
        Self(CoreVelocity::from_m_s(m_per_s))
    }

    #[getter]
    fn mm_per_s(&self) -> f32 {
        self.0.mm_per_s()
    }

    #[getter]
    fn m_s(&self) -> f32 {
        self.0.m_s()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<Velocity>().is_ok_and(|o| self.0 == o.0)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "Angle", module = "autd3_core", from_py_object)]
#[derive(Clone, Copy)]
pub struct Angle(pub CoreAngle);

#[pymethods]
impl Angle {
    #[staticmethod]
    fn from_radian(radian: f32) -> Self {
        Self(CoreAngle::from_radian(radian))
    }

    #[staticmethod]
    fn from_degree(degree: f32) -> Self {
        Self(CoreAngle::from_degree(degree))
    }

    #[getter]
    fn radian(&self) -> f32 {
        self.0.radian()
    }

    #[getter]
    fn degree(&self) -> f32 {
        self.0.degree()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<Angle>().is_ok_and(|o| self.0 == o.0)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "Length", module = "autd3_core", from_py_object)]
#[derive(Clone, Copy)]
pub struct Length(pub CoreLength);

#[pymethods]
impl Length {
    #[getter]
    fn mm(&self) -> f32 {
        self.0.mm()
    }

    #[getter]
    fn m(&self) -> f32 {
        self.0.m()
    }

    fn __truediv__(&self, rhs: &Bound<'_, PyAny>) -> PyResult<Velocity> {
        if rhs.is_instance_of::<SecUnit>() {
            Ok(Velocity(CoreVelocity::from_mm_s(self.0.mm())))
        } else {
            Err(PyValueError::new_err(
                "a Length may only be divided by `s` to form a Velocity, e.g. 340 * m / s",
            ))
        }
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<Length>().is_ok_and(|o| self.0 == o.0)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "_FreqUnit", module = "autd3_core", skip_from_py_object)]
#[derive(Clone, Copy)]
pub struct FreqUnit {
    scale: u32,
}

impl FreqUnit {
    pub(crate) const HZ: Self = Self { scale: 1 };
    pub(crate) const KHZ: Self = Self { scale: 1000 };
}

#[pymethods]
impl FreqUnit {
    fn __rmul__(&self, lhs: &Bound<'_, PyAny>) -> PyResult<Freq> {
        if let Ok(i) = lhs.extract::<i64>() {
            let scaled = i
                .checked_mul(i64::from(self.scale))
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| {
                    PyValueError::new_err("integer frequency must be in 0..=4294967295 Hz")
                })?;
            Ok(Freq(FreqVal::Int(scaled)))
        } else {
            let f = number_f32(lhs)?;
            Ok(Freq(FreqVal::Float(f * self.scale as f32)))
        }
    }
}

#[pyclass(name = "_LengthUnit", module = "autd3_core", skip_from_py_object)]
#[derive(Clone, Copy)]
pub struct LengthUnit {
    scale_mm: f32,
}

impl LengthUnit {
    pub(crate) const M: Self = Self { scale_mm: 1000.0 };
    pub(crate) const MM: Self = Self { scale_mm: 1.0 };
}

#[pymethods]
impl LengthUnit {
    fn __rmul__(&self, lhs: &Bound<'_, PyAny>) -> PyResult<Length> {
        Ok(Length(CoreLength::millimeters(
            number_f32(lhs)? * self.scale_mm,
        )))
    }
}

#[pyclass(name = "_AngleUnit", module = "autd3_core", skip_from_py_object)]
#[derive(Clone, Copy)]
pub struct AngleUnit {
    degree: bool,
}

impl AngleUnit {
    pub(crate) const DEG: Self = Self { degree: true };
    pub(crate) const RAD: Self = Self { degree: false };
}

#[pymethods]
impl AngleUnit {
    fn __rmul__(&self, lhs: &Bound<'_, PyAny>) -> PyResult<Angle> {
        let v = number_f32(lhs)?;
        Ok(Angle(if self.degree {
            CoreAngle::from_degree(v)
        } else {
            CoreAngle::from_radian(v)
        }))
    }
}

#[pyclass(name = "_SecUnit", module = "autd3_core", skip_from_py_object)]
#[derive(Clone, Copy)]
pub struct SecUnit;

impl Freq {
    pub(crate) fn hz_f32(self) -> f32 {
        self.hz()
    }

    pub(crate) fn sampling_config(self) -> CoreSamplingConfig {
        CoreSamplingConfig::new(self.hz() * Hz)
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<Freq>()?;
    m.add_class::<Velocity>()?;
    m.add_class::<Angle>()?;
    m.add_class::<Length>()?;
    m.add_class::<FreqUnit>()?;
    m.add_class::<LengthUnit>()?;
    m.add_class::<AngleUnit>()?;
    m.add_class::<SecUnit>()?;
    m.add("Hz", Py::new(py, FreqUnit::HZ)?)?;
    m.add("kHz", Py::new(py, FreqUnit::KHZ)?)?;
    m.add("m", Py::new(py, LengthUnit::M)?)?;
    m.add("mm", Py::new(py, LengthUnit::MM)?)?;
    m.add("s", Py::new(py, SecUnit)?)?;
    m.add("deg", Py::new(py, AngleUnit::DEG)?)?;
    m.add("rad", Py::new(py, AngleUnit::RAD)?)?;
    Ok(())
}
