use core::time::Duration;

use autd3_rs_core::common::Angle;
use autd3_rs_core::geometry::{UnitVector3, Vector3};
use autd3_rs_core::value::{Intensity, Phase, SamplingConfig};
use autd3_rs_core::{Geometry, Length, Point3, Velocity};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyCapsule;

static DURATION: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

pub fn extract_u8(obj: &Bound<'_, PyAny>) -> PyResult<u8> {
    if let Ok(v) = obj.extract::<u8>() {
        return Ok(v);
    }
    obj.getattr("value")?.extract::<u8>()
}

pub fn number_f32(obj: &Bound<'_, PyAny>) -> PyResult<f32> {
    obj.extract::<f32>()
        .map_err(|_| PyValueError::new_err("expected a number"))
}

pub fn nanos_to_duration(nanos: u128) -> PyResult<Duration> {
    u64::try_from(nanos)
        .map(Duration::from_nanos)
        .map_err(|_| PyValueError::new_err("duration is out of range"))
}

pub fn is_duration(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(DURATION.import(obj.py(), "autd3_core", "Duration")?)
}

pub fn extract_duration(obj: &Bound<'_, PyAny>) -> PyResult<Duration> {
    if !is_duration(obj)? {
        return Err(PyTypeError::new_err(format!(
            "expected a Duration, e.g. Duration.from_millis(10), got {}",
            obj.get_type().name()?
        )));
    }
    nanos_to_duration(obj.call_method0("as_nanos")?.extract::<u128>()?)
}

pub fn extract_velocity(obj: &Bound<'_, PyAny>) -> PyResult<Velocity> {
    let mm_per_s: f32 = obj.getattr("mm_s").and_then(|v| v.extract()).map_err(|_| {
        PyTypeError::new_err(
            "sound speed must be a Velocity, e.g. 340 * m / s (bare numbers are no longer accepted)",
        )
    })?;
    Ok(Velocity::from_mm_s(mm_per_s))
}

pub fn extract_length(obj: &Bound<'_, PyAny>) -> PyResult<Length> {
    let millimetres: f32 = obj.getattr("mm").and_then(|v| v.extract()).map_err(|_| {
        PyTypeError::new_err(
            "a length must be a Length, e.g. 10 * mm (bare numbers are no longer accepted)",
        )
    })?;
    Ok(Length::from_mm(millimetres))
}

pub fn extract_angle(obj: &Bound<'_, PyAny>) -> PyResult<Angle> {
    let radian: f32 = obj
        .getattr("rad")
        .and_then(|v| v.extract())
        .map_err(|_| PyTypeError::new_err("expected an Angle, e.g. 90 * deg"))?;
    Ok(Angle::from_rad(radian))
}

pub fn extract_point(obj: &Bound<'_, PyAny>) -> PyResult<Point3<f32>> {
    let [x, y, z] = obj.extract::<[f32; 3]>().map_err(|_| {
        PyValueError::new_err(
            "expected a length-3 array-like (numpy array, list, or tuple) of x, y, z in mm",
        )
    })?;
    Ok(Point3::new(x, y, z))
}

pub fn extract_direction(obj: &Bound<'_, PyAny>) -> PyResult<UnitVector3<f32>> {
    let [x, y, z] = obj.extract::<[f32; 3]>().map_err(|_| {
        PyValueError::new_err(
            "expected a length-3 array-like (numpy array, list, or tuple) of a direction vector",
        )
    })?;
    Ok(UnitVector3::new_normalize(Vector3::new(x, y, z)))
}

pub fn extract_sampling_config(obj: &Bound<'_, PyAny>) -> PyResult<SamplingConfig> {
    let not_a_config = || -> PyResult<PyErr> {
        Ok(PyTypeError::new_err(format!(
            "expected a SamplingConfig, e.g. SamplingConfig(4000 * Hz), got {}",
            obj.get_type().name()?
        )))
    };
    if !obj.hasattr("_sampling_config_capsule")? {
        return Err(not_a_config()?);
    }
    let Ok(capsule) = obj
        .call_method0("_sampling_config_capsule")?
        .cast_into::<PyCapsule>()
    else {
        return Err(not_a_config()?);
    };
    crate::sampling_config_from_capsule(&capsule)
}

pub fn velocity_to_py(py: Python<'_>, v: Velocity) -> PyResult<Bound<'_, PyAny>> {
    py.import("autd3_core")?
        .getattr("Velocity")?
        .call_method1("from_mm_s", (v.mm_s(),))
}

pub fn length_to_py(py: Python<'_>, v: Length) -> PyResult<Bound<'_, PyAny>> {
    py.import("autd3_core")?
        .getattr("Length")?
        .call_method1("from_mm", (v.mm(),))
}

pub fn angle_to_py(py: Python<'_>, v: Angle) -> PyResult<Bound<'_, PyAny>> {
    py.import("autd3_core")?
        .getattr("Angle")?
        .call_method1("from_rad", (v.rad(),))
}

pub fn sampling_config_to_py(py: Python<'_>, config: SamplingConfig) -> PyResult<Bound<'_, PyAny>> {
    let capsule = crate::sampling_config_into_capsule(py, config)?;
    py.import("autd3_core")?
        .getattr("SamplingConfig")?
        .call_method1("_from_sampling_config_capsule", (capsule,))
}

pub fn duration_to_py(py: Python<'_>, d: Duration) -> PyResult<Bound<'_, PyAny>> {
    py.import("autd3_core")?.getattr("Duration")?.call_method1(
        "from_nanos",
        (u64::try_from(d.as_nanos()).unwrap_or(u64::MAX),),
    )
}

pub fn geometry_to_py<'py>(py: Python<'py>, geometry: &Geometry) -> PyResult<Bound<'py, PyAny>> {
    let capsule = crate::geometry_into_capsule(py, geometry.clone())?;
    py.import("autd3_core")?
        .getattr("Geometry")?
        .call_method1("_from_capsule", (capsule,))
}

pub fn phase_to_py(py: Python<'_>, phase: Phase) -> PyResult<Bound<'_, PyAny>> {
    py.import("autd3_core")?.getattr("Phase")?.call1((phase.0,))
}

pub fn intensity_to_py(py: Python<'_>, intensity: Intensity) -> PyResult<Bound<'_, PyAny>> {
    py.import("autd3_core")?
        .getattr("Intensity")?
        .call1((intensity.0,))
}
