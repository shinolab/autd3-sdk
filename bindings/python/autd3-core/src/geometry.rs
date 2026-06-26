use autd3_rs_core::{
    Autd3 as CoreAutd3, Device as CoreDevice, Geometry as CoreGeometry, Point3, Quaternion,
    UnitQuaternion,
};
use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

#[pyclass(name = "Autd3", module = "autd3_core", from_py_object)]
#[derive(Clone)]
pub struct Autd3 {
    origin: Point3<f32>,
    rotation: UnitQuaternion<f32>,
}

#[pymethods]
impl Autd3 {
    #[new]
    #[pyo3(signature = (origin = None, rotation = None))]
    fn new(origin: Option<[f32; 3]>, rotation: Option<[f32; 4]>) -> Self {
        let origin = origin.map_or_else(Point3::origin, |[x, y, z]| Point3::new(x, y, z));
        let rotation = rotation.map_or_else(UnitQuaternion::identity, |[w, x, y, z]| {
            UnitQuaternion::from_quaternion(Quaternion::new(w, x, y, z))
        });
        Self { origin, rotation }
    }

    #[classattr]
    const DEVICE_WIDTH: f32 = 192.0;

    #[classattr]
    const DEVICE_HEIGHT: f32 = 151.4;
}

#[pyclass(name = "Geometry", module = "autd3_core")]
pub struct Geometry {
    inner: CoreGeometry,
}

#[pymethods]
impl Geometry {
    #[new]
    fn new(devices: Vec<Autd3>) -> Self {
        let devices = devices
            .into_iter()
            .map(|d| CoreAutd3::new(d.origin, d.rotation))
            .collect();
        Self {
            inner: CoreGeometry::new(devices),
        }
    }

    fn center(&self) -> (f32, f32, f32) {
        let c = self.inner.center();
        (c.x, c.y, c.z)
    }

    fn num_devices(&self) -> usize {
        self.inner.len()
    }

    fn num_transducers(&self) -> usize {
        self.inner.num_transducers()
    }

    fn device(&self, index: usize) -> PyResult<Device> {
        if index >= self.inner.len() {
            return Err(PyIndexError::new_err("device index out of range"));
        }
        Ok(Device {
            inner: self.inner[index].clone(),
        })
    }

    fn __getitem__(&self, index: usize) -> PyResult<Device> {
        self.device(index)
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        autd3_python_capsule::geometry_into_capsule(py, self.inner.clone())
    }
}

#[pyclass(name = "Device", module = "autd3_core")]
pub struct Device {
    inner: CoreDevice,
}

#[pymethods]
impl Device {
    fn idx(&self) -> usize {
        self.inner.idx()
    }

    fn num_transducers(&self) -> usize {
        self.inner.len()
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn center(&self) -> (f32, f32, f32) {
        let c = self.inner.center();
        (c.x, c.y, c.z)
    }

    fn rotation(&self) -> (f32, f32, f32, f32) {
        let q = self.inner.rotation();
        (q.w, q.i, q.j, q.k)
    }

    fn x_direction(&self) -> (f32, f32, f32) {
        let d = self.inner.x_direction().into_inner();
        (d.x, d.y, d.z)
    }

    fn y_direction(&self) -> (f32, f32, f32) {
        let d = self.inner.y_direction().into_inner();
        (d.x, d.y, d.z)
    }

    fn axial_direction(&self) -> (f32, f32, f32) {
        let d = self.inner.axial_direction().into_inner();
        (d.x, d.y, d.z)
    }
}

#[pyfunction]
pub fn _read_geometry_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(autd3_python_capsule::geometry_from_capsule(capsule)?.len())
}
