use autd3_python_capsule::{
    DevicePattern, capsule_of, geometry_from_capsule, pattern_from_capsule, pattern_into_capsule,
};
use autd3_rs_core::common::Angle;
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::geometry::{UnitVector3, Vector3};
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_core::{Length, Point3, Velocity};
use autd3_rs_pattern::{
    BesselOption as CoreBesselOption, FocusOption as CoreFocusOption,
    PlaneOption as CorePlaneOption,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

fn extract_point(obj: &Bound<'_, PyAny>) -> PyResult<Point3<f32>> {
    let [x, y, z] = obj.extract::<[f32; 3]>().map_err(|_| {
        PyValueError::new_err(
            "expected a length-3 array-like (numpy array, list, or tuple) of x, y, z in mm",
        )
    })?;
    Ok(Point3::new(x, y, z))
}

fn extract_direction(obj: &Bound<'_, PyAny>) -> PyResult<UnitVector3<f32>> {
    let [x, y, z] = obj.extract::<[f32; 3]>().map_err(|_| {
        PyValueError::new_err(
            "expected a length-3 array-like (numpy array, list, or tuple) of a direction vector",
        )
    })?;
    Ok(UnitVector3::new_normalize(Vector3::new(x, y, z)))
}

fn extract_u8(obj: &Bound<'_, PyAny>) -> PyResult<u8> {
    if let Ok(v) = obj.extract::<u8>() {
        return Ok(v);
    }
    obj.getattr("value")?.extract::<u8>()
}

fn extract_intensity(obj: &Bound<'_, PyAny>) -> PyResult<Intensity> {
    Ok(Intensity(extract_u8(obj)?))
}

fn extract_phase(obj: &Bound<'_, PyAny>) -> PyResult<Phase> {
    Ok(Phase(extract_u8(obj)?))
}

fn extract_velocity(obj: &Bound<'_, PyAny>) -> PyResult<Velocity> {
    let mm_per_s: f32 = obj.getattr("mm_per_s").and_then(|v| v.extract()).map_err(|_| {
        PyValueError::new_err(
            "sound speed must be a Velocity, e.g. 340 * m / s (bare numbers are no longer accepted)",
        )
    })?;
    Ok(Velocity::from_mm_s(mm_per_s))
}

fn extract_angle(obj: &Bound<'_, PyAny>) -> PyResult<Angle> {
    let radian: f32 = obj
        .getattr("radian")
        .and_then(|v| v.extract())
        .map_err(|_| PyValueError::new_err("theta must be an Angle, e.g. 18 * deg"))?;
    Ok(Angle::from_radian(radian))
}

fn extract_emission(obj: &Bound<'_, PyAny>) -> PyResult<Emission> {
    if let (Ok(phase), Ok(intensity)) = (obj.getattr("phase"), obj.getattr("intensity")) {
        return Ok(Emission {
            phase: Phase(extract_u8(&phase)?),
            intensity: Intensity(extract_u8(&intensity)?),
        });
    }
    let (phase, intensity): (u8, u8) = obj
        .extract()
        .map_err(|_| PyValueError::new_err("expected an Emission or a (phase, intensity) tuple"))?;
    Ok(Emission {
        phase: Phase(phase),
        intensity: Intensity(intensity),
    })
}

#[pyclass(name = "FocusOption", module = "autd3_pattern", from_py_object)]
#[derive(Clone, Copy)]
pub struct FocusOption(pub(crate) CoreFocusOption);

#[pymethods]
impl FocusOption {
    #[new]
    #[pyo3(signature = (intensity = Intensity::MAX, phase_offset = Phase::ZERO))]
    fn new(
        #[pyo3(from_py_with = extract_intensity)] intensity: Intensity,
        #[pyo3(from_py_with = extract_phase)] phase_offset: Phase,
    ) -> Self {
        Self(CoreFocusOption {
            intensity,
            phase_offset,
        })
    }
}

#[pyclass(name = "PlaneOption", module = "autd3_pattern", from_py_object)]
#[derive(Clone, Copy)]
pub struct PlaneOption(pub(crate) CorePlaneOption);

#[pymethods]
impl PlaneOption {
    #[new]
    #[pyo3(signature = (intensity = Intensity::MAX, phase_offset = Phase::ZERO))]
    fn new(
        #[pyo3(from_py_with = extract_intensity)] intensity: Intensity,
        #[pyo3(from_py_with = extract_phase)] phase_offset: Phase,
    ) -> Self {
        Self(CorePlaneOption {
            intensity,
            phase_offset,
        })
    }
}

#[pyclass(name = "BesselOption", module = "autd3_pattern", from_py_object)]
#[derive(Clone, Copy)]
pub struct BesselOption(pub(crate) CoreBesselOption);

#[pymethods]
impl BesselOption {
    #[new]
    #[pyo3(signature = (intensity = Intensity::MAX, phase_offset = Phase::ZERO))]
    fn new(
        #[pyo3(from_py_with = extract_intensity)] intensity: Intensity,
        #[pyo3(from_py_with = extract_phase)] phase_offset: Phase,
    ) -> Self {
        Self(CoreBesselOption {
            intensity,
            phase_offset,
        })
    }
}

#[pyclass(name = "PatternBuffer", module = "autd3_pattern")]
pub struct PatternBuffer {
    inner: Vec<DevicePattern>,
}

#[pymethods]
impl PatternBuffer {
    #[new]
    fn new(num_devices: usize) -> Self {
        Self {
            inner: vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; num_devices],
        }
    }

    #[staticmethod]
    fn from_array(emissions: Vec<Vec<Bound<'_, PyAny>>>) -> PyResult<PatternBuffer> {
        let mut inner = Vec::with_capacity(emissions.len());
        for device in emissions {
            if device.len() != Autd3::NUM_TRANSDUCERS {
                return Err(PyValueError::new_err(format!(
                    "each device needs {} emissions, got {}",
                    Autd3::NUM_TRANSDUCERS,
                    device.len()
                )));
            }
            let mut slot = vec![Emission::default(); Autd3::NUM_TRANSDUCERS];
            for (e, obj) in slot.iter_mut().zip(device) {
                *e = extract_emission(&obj)?;
            }
            inner.push(slot);
        }
        Ok(PatternBuffer { inner })
    }

    fn num_devices(&self) -> usize {
        self.inner.len()
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        pattern_into_capsule(py, self.inner.clone())
    }

    fn _capsule_mut<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let ptr = core::ptr::NonNull::from(&mut self.inner);
        // SAFETY: `self.inner` lives as long as this `PatternBuffer`, which the caller
        // keeps alive while the borrowed capsule is in use; no destructor frees it.
        unsafe { autd3_python_capsule::pattern_capsule_mut(py, ptr) }
    }
}

#[pyfunction]
fn wavelength(sound_speed: &Bound<'_, PyAny>) -> PyResult<f32> {
    Ok(autd3_rs_pattern::wavelength(extract_velocity(sound_speed)?).mm())
}

#[pyfunction]
#[pyo3(signature = (geometry, target, wavelength, option, buffer))]
fn focus(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    wavelength: f32,
    option: FocusOption,
    mut buffer: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let target = extract_point(target)?;
    autd3_rs_pattern::focus(
        geometry,
        target,
        Length::millimeters(wavelength),
        &option.0,
        &mut buffer.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, direction, wavelength, option, buffer))]
fn plane(
    geometry: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    wavelength: f32,
    option: PlaneOption,
    mut buffer: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let direction = extract_direction(direction)?;
    autd3_rs_pattern::plane(
        geometry,
        direction,
        Length::millimeters(wavelength),
        &option.0,
        &mut buffer.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, apex, direction, theta, wavelength, option, buffer))]
fn bessel(
    geometry: &Bound<'_, PyAny>,
    apex: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    theta: &Bound<'_, PyAny>,
    wavelength: f32,
    option: BesselOption,
    mut buffer: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let apex = extract_point(apex)?;
    let direction = extract_direction(direction)?;
    let theta = extract_angle(theta)?;
    autd3_rs_pattern::bessel(
        geometry,
        apex,
        direction,
        theta,
        Length::millimeters(wavelength),
        &option.0,
        &mut buffer.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (emission, buffer))]
fn uniform(emission: &Bound<'_, PyAny>, mut buffer: PyRefMut<'_, PatternBuffer>) -> PyResult<()> {
    autd3_rs_pattern::uniform(extract_emission(emission)?, &mut buffer.inner);
    Ok(())
}

#[pyfunction]
fn null(mut buffer: PyRefMut<'_, PatternBuffer>) {
    autd3_rs_pattern::null(&mut buffer.inner);
}

#[pyfunction]
fn _read_pattern_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(pattern_from_capsule(capsule)?.len())
}

#[pymodule]
fn autd3_pattern(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PatternBuffer>()?;
    m.add_class::<FocusOption>()?;
    m.add_class::<PlaneOption>()?;
    m.add_class::<BesselOption>()?;
    m.add_function(wrap_pyfunction!(wavelength, m)?)?;
    m.add_function(wrap_pyfunction!(focus, m)?)?;
    m.add_function(wrap_pyfunction!(plane, m)?)?;
    m.add_function(wrap_pyfunction!(bessel, m)?)?;
    m.add_function(wrap_pyfunction!(uniform, m)?)?;
    m.add_function(wrap_pyfunction!(null, m)?)?;
    m.add_function(wrap_pyfunction!(_read_pattern_capsule, m)?)?;
    Ok(())
}
