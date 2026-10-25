use core::hash::{Hash, Hasher};

use autd3_python_capsule::extract::number_f32;
use autd3_rs_core::common::{
    Angle as CoreAngle, Freq as CoreFreq, Length as CoreLength, Velocity as CoreVelocity,
};
use autd3_rs_core::value::{SamplingConfig as CoreSamplingConfig, SamplingConfigError};
use pyo3::exceptions::PyZeroDivisionError;
use pyo3::prelude::*;

pub(crate) fn hash_f32<H: Hasher>(value: f32, state: &mut H) {
    (value + 0.0).to_bits().hash(state);
}

macro_rules! float_unit_eq_hash {
    ($ty:ident, $value:ident) => {
        impl PartialEq for $ty {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }

        impl Hash for $ty {
            fn hash<H: Hasher>(&self, state: &mut H) {
                hash_f32(self.0.$value(), state);
            }
        }
    };
}

#[pyclass(
    name = "Freq",
    module = "autd3_core",
    eq,
    ord,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy)]
pub struct Freq(pub CoreFreq);

float_unit_eq_hash!(Freq, hz);

impl PartialOrd for Freq {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[pymethods]
impl Freq {
    #[staticmethod]
    fn from_hz(freq: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(CoreFreq::from_hz(number_f32(freq)?)))
    }

    #[getter]
    fn hz(&self) -> f32 {
        self.0.hz()
    }

    fn __add__(&self, rhs: Freq) -> Freq {
        Freq(self.0 + rhs.0)
    }

    fn __sub__(&self, rhs: Freq) -> Freq {
        Freq(self.0 - rhs.0)
    }

    fn __mul__(&self, rhs: f32) -> Freq {
        Freq(self.0 * rhs)
    }

    fn __rmul__(&self, lhs: f32) -> Freq {
        Freq(self.0 * lhs)
    }

    fn __truediv__(&self, rhs: f32) -> PyResult<Freq> {
        if rhs == 0.0 {
            return Err(PyZeroDivisionError::new_err("division by zero"));
        }
        Ok(Freq(self.0 / rhs))
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(
    name = "Velocity",
    module = "autd3_core",
    eq,
    ord,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy)]
pub struct Velocity(pub CoreVelocity);

float_unit_eq_hash!(Velocity, mm_s);

impl PartialOrd for Velocity {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

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
    fn mm_s(&self) -> f32 {
        self.0.mm_s()
    }

    #[getter]
    fn m_s(&self) -> f32 {
        self.0.m_s()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(
    name = "Angle",
    module = "autd3_core",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy)]
pub struct Angle(pub CoreAngle);

float_unit_eq_hash!(Angle, rad);

#[pymethods]
impl Angle {
    #[classattr]
    #[pyo3(name = "ZERO")]
    fn zero() -> Self {
        Self(CoreAngle::ZERO)
    }

    #[staticmethod]
    fn from_rad(radian: f32) -> Self {
        Self(CoreAngle::from_rad(radian))
    }

    #[staticmethod]
    fn from_deg(degree: f32) -> Self {
        Self(CoreAngle::from_deg(degree))
    }

    #[getter]
    fn rad(&self) -> f32 {
        self.0.rad()
    }

    #[getter]
    fn deg(&self) -> f32 {
        self.0.deg()
    }

    fn __neg__(&self) -> Self {
        Self(-self.0)
    }

    fn __add__(&self, rhs: Angle) -> Self {
        Self(self.0 + rhs.0)
    }

    fn __sub__(&self, rhs: Angle) -> Self {
        Self(self.0 - rhs.0)
    }

    fn __mul__(&self, rhs: f32) -> Self {
        Self(self.0 * rhs)
    }

    fn __rmul__(&self, lhs: f32) -> Self {
        Self(lhs * self.0)
    }

    fn __truediv__(&self, rhs: f32) -> PyResult<Self> {
        non_zero_divisor(rhs)?;
        Ok(Self(self.0 / rhs))
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

fn non_zero_divisor(rhs: f32) -> PyResult<()> {
    if rhs == 0.0 {
        return Err(PyZeroDivisionError::new_err("division by zero"));
    }
    Ok(())
}

#[pyclass(
    name = "Length",
    module = "autd3_core",
    eq,
    ord,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy)]
pub struct Length(pub CoreLength);

float_unit_eq_hash!(Length, mm);

impl PartialOrd for Length {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[pymethods]
impl Length {
    #[staticmethod]
    fn from_mm(millimetres: f32) -> Self {
        Self(CoreLength::from_mm(millimetres))
    }

    #[staticmethod]
    fn from_m(metres: f32) -> Self {
        Self(CoreLength::from_m(metres))
    }

    #[getter]
    fn mm(&self) -> f32 {
        self.0.mm()
    }

    #[getter]
    fn m(&self) -> f32 {
        self.0.m()
    }

    fn __neg__(&self) -> Self {
        Self(-self.0)
    }

    fn __add__(&self, rhs: Length) -> Self {
        Self(self.0 + rhs.0)
    }

    fn __sub__(&self, rhs: Length) -> Self {
        Self(self.0 - rhs.0)
    }

    fn __mul__(&self, rhs: f32) -> Self {
        Self(self.0 * rhs)
    }

    fn __rmul__(&self, lhs: f32) -> Self {
        Self(lhs * self.0)
    }

    fn __truediv__<'py>(&self, rhs: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let py = rhs.py();
        if rhs.is_instance_of::<SecUnit>() {
            return Ok(Bound::new(py, Velocity(CoreVelocity::from_mm_s(self.0.mm())))?.into_any());
        }
        let Ok(divisor) = rhs.extract::<f32>() else {
            return Ok(py.NotImplemented().into_bound(py));
        };
        non_zero_divisor(divisor)?;
        Ok(Bound::new(py, Self(self.0 / divisor))?.into_any())
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "_FreqUnit", module = "autd3_core")]
pub struct FreqUnit {
    scale: f32,
}

impl FreqUnit {
    pub(crate) const HZ: Self = Self { scale: 1.0 };
    pub(crate) const KHZ: Self = Self { scale: 1000.0 };
}

#[pymethods]
impl FreqUnit {
    fn __rmul__(&self, lhs: &Bound<'_, PyAny>) -> PyResult<Freq> {
        Ok(Freq(CoreFreq::from_hz(number_f32(lhs)? * self.scale)))
    }
}

#[pyclass(name = "_LengthUnit", module = "autd3_core")]
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
        Ok(Length(CoreLength::from_mm(
            number_f32(lhs)? * self.scale_mm,
        )))
    }
}

#[pyclass(name = "_AngleUnit", module = "autd3_core")]
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
            CoreAngle::from_deg(v)
        } else {
            CoreAngle::from_rad(v)
        }))
    }
}

#[pyclass(name = "_SecUnit", module = "autd3_core")]
pub(crate) struct SecUnit;

impl Freq {
    pub(crate) fn sampling_config(self) -> Result<CoreSamplingConfig, SamplingConfigError> {
        CoreSamplingConfig::from_freq(self.0)
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
