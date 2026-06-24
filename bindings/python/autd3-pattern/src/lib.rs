use autd3_python_capsule::{
    DevicePattern, capsule_of, geometry_from_capsule, pattern_from_capsule, pattern_into_capsule,
};
use autd3_rs_core::common::Angle;
use autd3_rs_core::geometry::{UnitVector3, Vector3};
use autd3_rs_core::params::NUM_TRANSDUCERS;
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

#[pyclass(name = "FocusOption", module = "autd3_pattern", from_py_object)]
#[derive(Clone, Copy)]
pub struct FocusOption(pub(crate) CoreFocusOption);

#[pymethods]
impl FocusOption {
    #[new]
    #[pyo3(signature = (intensity = 0xFF, phase_offset = 0))]
    fn new(intensity: u8, phase_offset: u8) -> Self {
        Self(CoreFocusOption {
            intensity: Intensity(intensity),
            phase_offset: Phase(phase_offset),
        })
    }
}

#[pyclass(name = "PlaneOption", module = "autd3_pattern", from_py_object)]
#[derive(Clone, Copy)]
pub struct PlaneOption(pub(crate) CorePlaneOption);

#[pymethods]
impl PlaneOption {
    #[new]
    #[pyo3(signature = (intensity = 0xFF, phase_offset = 0))]
    fn new(intensity: u8, phase_offset: u8) -> Self {
        Self(CorePlaneOption {
            intensity: Intensity(intensity),
            phase_offset: Phase(phase_offset),
        })
    }
}

#[pyclass(name = "BesselOption", module = "autd3_pattern", from_py_object)]
#[derive(Clone, Copy)]
pub struct BesselOption(pub(crate) CoreBesselOption);

#[pymethods]
impl BesselOption {
    #[new]
    #[pyo3(signature = (intensity = 0xFF, phase_offset = 0))]
    fn new(intensity: u8, phase_offset: u8) -> Self {
        Self(CoreBesselOption {
            intensity: Intensity(intensity),
            phase_offset: Phase(phase_offset),
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
            inner: vec![[Emission::default(); NUM_TRANSDUCERS]; num_devices],
        }
    }

    #[staticmethod]
    fn from_array(emissions: Vec<Vec<(u8, u8)>>) -> PyResult<PatternBuffer> {
        let mut inner = Vec::with_capacity(emissions.len());
        for device in emissions {
            if device.len() != NUM_TRANSDUCERS {
                return Err(PyValueError::new_err(format!(
                    "each device needs {NUM_TRANSDUCERS} emissions, got {}",
                    device.len()
                )));
            }
            let mut slot = [Emission::default(); NUM_TRANSDUCERS];
            for (e, (phase, intensity)) in slot.iter_mut().zip(device) {
                *e = Emission {
                    phase: Phase(phase),
                    intensity: Intensity(intensity),
                };
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
fn wavelength(sound_speed_mm_per_s: f32) -> f32 {
    autd3_rs_pattern::wavelength(Velocity::from_mm_s(sound_speed_mm_per_s)).mm()
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
#[pyo3(signature = (geometry, apex, direction, theta_rad, wavelength, option, buffer))]
fn bessel(
    geometry: &Bound<'_, PyAny>,
    apex: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    theta_rad: f32,
    wavelength: f32,
    option: BesselOption,
    mut buffer: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let apex = extract_point(apex)?;
    let direction = extract_direction(direction)?;
    autd3_rs_pattern::bessel(
        geometry,
        apex,
        direction,
        Angle::from_radian(theta_rad),
        Length::millimeters(wavelength),
        &option.0,
        &mut buffer.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (intensity, phase, buffer))]
fn uniform(intensity: u8, phase: u8, mut buffer: PyRefMut<'_, PatternBuffer>) {
    autd3_rs_pattern::uniform(
        Emission {
            phase: Phase(phase),
            intensity: Intensity(intensity),
        },
        &mut buffer.inner,
    );
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
