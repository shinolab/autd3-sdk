use autd3_python_capsule::{
    DevicePattern, capsule_of, geometry_from_capsule, pattern_from_capsule, pattern_into_capsule,
};
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_core::{Length, Point3, Velocity};
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
}

#[pyfunction]
fn wavelength(sound_speed_mm_per_s: f32) -> f32 {
    autd3_rs_pattern::wavelength(Velocity::from_mm_s(sound_speed_mm_per_s)).mm()
}

#[pyfunction]
fn focus(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    wavelength: f32,
    intensity: u8,
    mut buffer: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let target = extract_point(target)?;
    autd3_rs_pattern::focus(
        geometry,
        target,
        Length::millimeters(wavelength),
        Intensity(intensity),
        &mut buffer.inner,
    );
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
    m.add_function(wrap_pyfunction!(wavelength, m)?)?;
    m.add_function(wrap_pyfunction!(focus, m)?)?;
    m.add_function(wrap_pyfunction!(null, m)?)?;
    m.add_function(wrap_pyfunction!(_read_pattern_capsule, m)?)?;
    Ok(())
}
