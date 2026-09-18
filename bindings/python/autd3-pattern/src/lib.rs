use autd3_python_capsule::{
    DevicePattern, capsule_of, geometry_from_capsule, pattern_from_capsule, pattern_into_capsule,
};
use autd3_rs_core::common::Angle;
use autd3_rs_core::geometry::Autd3;
use autd3_rs_core::geometry::TransducerGroups as CoreTransducerGroups;
use autd3_rs_core::geometry::{UnitVector3, Vector3};
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_core::{Length, Point3, Velocity};
use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict};

fn emission_to_py(py: Python<'_>, emission: Emission) -> PyResult<Py<PyAny>> {
    let core = py.import("autd3_core")?;
    let phase = core.getattr("Phase")?.call1((emission.phase.0,))?;
    let intensity = core.getattr("Intensity")?.call1((emission.intensity.0,))?;
    Ok(core
        .getattr("Emission")?
        .call1((phase, intensity))?
        .unbind())
}

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
    let mm_per_s: f32 = obj.getattr("mm_s").and_then(|v| v.extract()).map_err(|_| {
        PyValueError::new_err(
            "sound speed must be a Velocity, e.g. 340 * m / s (bare numbers are no longer accepted)",
        )
    })?;
    Ok(Velocity::from_mm_s(mm_per_s))
}

fn extract_angle(obj: &Bound<'_, PyAny>) -> PyResult<Angle> {
    let radian: f32 = obj
        .getattr("rad")
        .and_then(|v| v.extract())
        .map_err(|_| PyValueError::new_err("theta must be an Angle, e.g. 18 * deg"))?;
    Ok(Angle::from_rad(radian))
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

    fn __getitem__(slf: &Bound<'_, Self>, index: usize) -> PyResult<DevicePatternView> {
        if index >= slf.borrow().inner.len() {
            return Err(PyIndexError::new_err("device index out of range"));
        }
        Ok(DevicePatternView {
            buffer: slf.clone().unbind(),
            device: index,
        })
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        pattern_into_capsule(py, self.inner.clone())
    }

    fn _capsule_mut<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyCapsule>> {
        let py = slf.py();
        let ptr = core::ptr::NonNull::from(&mut slf.borrow_mut().inner);
        unsafe {
            autd3_python_capsule::pattern_capsule_mut(py, ptr, slf.clone().into_any().unbind())
        }
    }
}

#[pyclass(name = "DevicePatternView", module = "autd3_pattern")]
pub struct DevicePatternView {
    buffer: Py<PatternBuffer>,
    device: usize,
}

#[pymethods]
impl DevicePatternView {
    fn __len__(&self, py: Python<'_>) -> usize {
        self.buffer.borrow(py).inner[self.device].len()
    }

    fn __getitem__(&self, py: Python<'_>, index: usize) -> PyResult<Py<PyAny>> {
        let buf = self.buffer.borrow(py);
        let slot = &buf.inner[self.device];
        let emission = *slot
            .get(index)
            .ok_or_else(|| PyIndexError::new_err("transducer index out of range"))?;
        emission_to_py(py, emission)
    }

    fn __setitem__(&self, py: Python<'_>, index: usize, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let emission = extract_emission(value)?;
        let mut buf = self.buffer.borrow_mut(py);
        let slot = &mut buf.inner[self.device];
        *slot
            .get_mut(index)
            .ok_or_else(|| PyIndexError::new_err("transducer index out of range"))? = emission;
        Ok(())
    }
}

#[pyfunction]
fn wavelength(sound_speed: &Bound<'_, PyAny>) -> PyResult<f32> {
    Ok(autd3_rs_pattern::wavelength(extract_velocity(sound_speed)?).mm())
}

#[pyfunction]
#[pyo3(signature = (geometry, target, wavelength, dst))]
fn focus(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    wavelength: f32,
    mut dst: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let target = extract_point(target)?;
    autd3_rs_pattern::focus(
        geometry,
        target,
        Length::from_mm(wavelength),
        &mut dst.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, direction, wavelength, dst))]
fn plane(
    geometry: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    wavelength: f32,
    mut dst: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let direction = extract_direction(direction)?;
    autd3_rs_pattern::plane(
        geometry,
        direction,
        Length::from_mm(wavelength),
        &mut dst.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, apex, direction, theta, wavelength, dst))]
fn bessel(
    geometry: &Bound<'_, PyAny>,
    apex: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    theta: &Bound<'_, PyAny>,
    wavelength: f32,
    mut dst: PyRefMut<'_, PatternBuffer>,
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
        Length::from_mm(wavelength),
        &mut dst.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, target, normal, wavelength, dst))]
fn twin_trap(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    normal: &Bound<'_, PyAny>,
    wavelength: f32,
    mut dst: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let target = extract_point(target)?;
    let normal = extract_direction(normal)?;
    autd3_rs_pattern::twin_trap(
        geometry,
        target,
        normal,
        Length::from_mm(wavelength),
        &mut dst.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, target, axis, order, wavelength, dst))]
fn vortex(
    geometry: &Bound<'_, PyAny>,
    target: &Bound<'_, PyAny>,
    axis: &Bound<'_, PyAny>,
    order: i32,
    wavelength: f32,
    mut dst: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    let capsule = capsule_of(geometry)?;
    let geometry = geometry_from_capsule(&capsule)?;
    let target = extract_point(target)?;
    let axis = extract_direction(axis)?;
    autd3_rs_pattern::vortex(
        geometry,
        target,
        axis,
        order,
        Length::from_mm(wavelength),
        &mut dst.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (intensity, dst))]
fn set_intensity(
    intensity: &Bound<'_, PyAny>,
    mut dst: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    autd3_rs_pattern::set_intensity(extract_intensity(intensity)?, &mut dst.inner);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (phase, dst))]
fn set_phase(phase: &Bound<'_, PyAny>, mut dst: PyRefMut<'_, PatternBuffer>) -> PyResult<()> {
    autd3_rs_pattern::set_phase(extract_phase(phase)?, &mut dst.inner);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (phase, intensity, dst))]
fn set_phase_and_intensity(
    phase: &Bound<'_, PyAny>,
    intensity: &Bound<'_, PyAny>,
    mut dst: PyRefMut<'_, PatternBuffer>,
) -> PyResult<()> {
    autd3_rs_pattern::set_phase_and_intensity(
        extract_phase(phase)?,
        extract_intensity(intensity)?,
        &mut dst.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (phase, dst))]
fn add_phase(phase: &Bound<'_, PyAny>, mut dst: PyRefMut<'_, PatternBuffer>) -> PyResult<()> {
    autd3_rs_pattern::add_phase(extract_phase(phase)?, &mut dst.inner);
    Ok(())
}

fn matches_geometry(geometry: &autd3_rs_core::Geometry, buffer: &[DevicePattern]) -> bool {
    buffer.len() == geometry.num_devices()
        && geometry
            .iter()
            .zip(buffer)
            .all(|(device, slot)| slot.len() == device.num_transducers())
}

#[pyclass(name = "TransducerMask", module = "autd3_pattern", from_py_object)]
#[derive(Clone)]
pub struct TransducerMask {
    mask: Option<Vec<Vec<bool>>>,
}

#[pymethods]
impl TransducerMask {
    #[classattr]
    #[pyo3(name = "AllEnabled")]
    fn all_enabled() -> Self {
        Self { mask: None }
    }

    #[staticmethod]
    fn masked(mask: Vec<Vec<bool>>) -> PyResult<Self> {
        if let Some(device) = mask
            .iter()
            .find(|device| device.len() != Autd3::NUM_TRANSDUCERS)
        {
            return Err(PyValueError::new_err(format!(
                "each device mask needs {} entries, got {}",
                Autd3::NUM_TRANSDUCERS,
                device.len()
            )));
        }
        Ok(Self { mask: Some(mask) })
    }

    fn _mask(&self) -> Option<Vec<Vec<bool>>> {
        self.mask.clone()
    }
}

#[pyclass(name = "TransducerGroups", module = "autd3_pattern")]
pub struct TransducerGroups {
    inner: CoreTransducerGroups<usize>,
    keys: Vec<Py<PyAny>>,
    lookup: Py<PyDict>,
}

#[pymethods]
impl TransducerGroups {
    #[new]
    fn new(geometry: &Bound<'_, PyAny>, key: &Bound<'_, PyAny>) -> PyResult<Self> {
        let py = geometry.py();
        let capsule = capsule_of(geometry)?;
        let core_geometry = geometry_from_capsule(&capsule)?;
        let devices = (0..core_geometry.num_devices())
            .map(|dev| geometry.get_item(dev))
            .collect::<PyResult<Vec<_>>>()?;
        let lookup = PyDict::new(py);
        let mut keys = Vec::new();
        let mut error = None;
        let inner = CoreTransducerGroups::new(core_geometry, |device, tr| {
            if error.is_some() {
                return None;
            }
            key.call1((&devices[device.idx()], tr))
                .and_then(|k| {
                    if k.is_none() {
                        return Ok(None);
                    }
                    if let Some(index) = lookup.get_item(&k)? {
                        return index.extract::<usize>().map(Some);
                    }
                    let index = keys.len();
                    lookup.set_item(&k, index)?;
                    keys.push(k.unbind());
                    Ok(Some(index))
                })
                .unwrap_or_else(|e| {
                    error = Some(e);
                    None
                })
        });
        if let Some(e) = error {
            return Err(e);
        }
        Ok(Self {
            inner,
            keys,
            lookup: lookup.unbind(),
        })
    }

    fn keys(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        self.keys.iter().map(|key| key.clone_ref(py)).collect()
    }

    fn key(&self, py: Python<'_>, device: usize, transducer: usize) -> PyResult<Option<Py<PyAny>>> {
        if device >= self.inner.num_devices() || transducer >= self.inner.num_transducers(device) {
            return Err(PyIndexError::new_err("transducer index out of range"));
        }
        Ok(self
            .inner
            .index(device, transducer)
            .map(|index| self.keys[index].clone_ref(py)))
    }

    fn mask(&self, key: &Bound<'_, PyAny>) -> PyResult<TransducerMask> {
        let index: usize = self
            .lookup
            .bind(key.py())
            .get_item(key)?
            .ok_or_else(|| PyKeyError::new_err(key.clone().unbind()))?
            .extract()?;
        Ok(self.mask_at(index))
    }
}

impl TransducerGroups {
    fn mask_at(&self, index: usize) -> TransducerMask {
        TransducerMask {
            mask: Some(
                (0..self.inner.num_devices())
                    .map(|dev| {
                        (0..self.inner.num_transducers(dev))
                            .map(|tr| self.inner.key(dev, tr) == Some(index))
                            .collect()
                    })
                    .collect(),
            ),
        }
    }
}

fn groups_match(geometry: &autd3_rs_core::Geometry, groups: &CoreTransducerGroups<usize>) -> bool {
    groups.num_devices() == geometry.num_devices()
        && geometry
            .iter()
            .enumerate()
            .all(|(dev, device)| groups.num_transducers(dev) == device.num_transducers())
}

#[pyfunction]
#[pyo3(signature = (geometry, groups, sources, dst))]
fn group(
    geometry: &Bound<'_, PyAny>,
    groups: PyRef<'_, TransducerGroups>,
    sources: &Bound<'_, PyAny>,
    dst: &Bound<'_, PatternBuffer>,
) -> PyResult<()> {
    let py = geometry.py();
    let capsule = capsule_of(geometry)?;
    let core_geometry = geometry_from_capsule(&capsule)?;
    let buffers = groups
        .keys
        .iter()
        .map(|key| {
            let source = sources
                .get_item(key.bind(py))?
                .cast_into::<PatternBuffer>()
                .map_err(|_| PyTypeError::new_err("every source must be a PatternBuffer"))?;
            if source.is(dst) {
                return Err(PyValueError::new_err("dst must not be one of the sources"));
            }
            Ok(source)
        })
        .collect::<PyResult<Vec<_>>>()?;
    let borrowed = buffers
        .iter()
        .map(Bound::try_borrow)
        .collect::<Result<Vec<_>, _>>()?;
    let mut dst = dst.try_borrow_mut()?;
    if !groups_match(core_geometry, &groups.inner)
        || !matches_geometry(core_geometry, &dst.inner)
        || !borrowed
            .iter()
            .all(|source| matches_geometry(core_geometry, &source.inner))
    {
        return Err(PyValueError::new_err(
            "the groups and every pattern buffer must match the geometry",
        ));
    }

    autd3_rs_pattern::group(
        core_geometry,
        &groups.inner,
        |index| borrowed[index].inner.as_slice(),
        &mut dst.inner,
    );
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (geometry, groups, compute, dst))]
fn group_compute(
    geometry: &Bound<'_, PyAny>,
    groups: PyRef<'_, TransducerGroups>,
    compute: &Bound<'_, PyAny>,
    dst: &Bound<'_, PatternBuffer>,
) -> PyResult<()> {
    let py = geometry.py();
    let capsule = capsule_of(geometry)?;
    let core_geometry = geometry_from_capsule(&capsule)?;
    if !compute.is_callable() {
        return Err(PyTypeError::new_err("compute must be callable"));
    }
    if !groups_match(core_geometry, &groups.inner)
        || !matches_geometry(core_geometry, &dst.try_borrow()?.inner)
    {
        return Err(PyValueError::new_err(
            "the groups and dst must match the geometry",
        ));
    }

    for (dev, slot) in dst.try_borrow_mut()?.inner.iter_mut().enumerate() {
        for (tr, out) in slot.iter_mut().enumerate() {
            if groups.inner.key(dev, tr).is_none() {
                *out = Emission::NULL;
            }
        }
    }

    let scratch = Bound::new(
        py,
        PatternBuffer {
            inner: core_geometry.pattern_buffer(),
        },
    )?;
    for (index, key) in groups.keys.iter().enumerate() {
        autd3_rs_pattern::set_phase_and_intensity(
            Phase::ZERO,
            Intensity::MAX,
            &mut scratch.try_borrow_mut()?.inner,
        );
        compute.call1((key.bind(py), groups.mask_at(index), &scratch))?;
        let source = scratch.try_borrow()?;
        let mut out = dst.try_borrow_mut()?;
        for (dev, (slot, src)) in out.inner.iter_mut().zip(&source.inner).enumerate() {
            for (tr, (o, &e)) in slot.iter_mut().zip(src).enumerate() {
                if groups.inner.key(dev, tr) == Some(index) {
                    *o = e;
                }
            }
        }
    }
    Ok(())
}

#[pyfunction]
fn _read_pattern_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(pattern_from_capsule(capsule)?.len())
}

#[pymodule]
fn autd3_pattern(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PatternBuffer>()?;
    m.add_class::<DevicePatternView>()?;
    m.add_class::<TransducerMask>()?;
    m.add_class::<TransducerGroups>()?;
    m.add_function(wrap_pyfunction!(wavelength, m)?)?;
    m.add_function(wrap_pyfunction!(focus, m)?)?;
    m.add_function(wrap_pyfunction!(plane, m)?)?;
    m.add_function(wrap_pyfunction!(bessel, m)?)?;
    m.add_function(wrap_pyfunction!(twin_trap, m)?)?;
    m.add_function(wrap_pyfunction!(vortex, m)?)?;
    m.add_function(wrap_pyfunction!(set_intensity, m)?)?;
    m.add_function(wrap_pyfunction!(set_phase, m)?)?;
    m.add_function(wrap_pyfunction!(set_phase_and_intensity, m)?)?;
    m.add_function(wrap_pyfunction!(add_phase, m)?)?;
    m.add_function(wrap_pyfunction!(group, m)?)?;
    m.add_function(wrap_pyfunction!(group_compute, m)?)?;
    m.add_function(wrap_pyfunction!(_read_pattern_capsule, m)?)?;
    Ok(())
}
