use autd3_rs_core::{
    Autd3 as CoreAutd3, Device as CoreDevice, Geometry as CoreGeometry, Point3, Quaternion,
    UnitQuaternion,
};
use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

fn np_vec3(py: Python<'_>, x: f32, y: f32, z: f32) -> PyResult<Bound<'_, PyAny>> {
    py.import("numpy")?.call_method1("array", ((x, y, z),))
}

fn np_vec4(py: Python<'_>, x: f32, y: f32, z: f32, w: f32) -> PyResult<Bound<'_, PyAny>> {
    py.import("numpy")?.call_method1("array", ((x, y, z, w),))
}

fn np_rows(py: Python<'_>, rows: Vec<(f32, f32, f32)>) -> PyResult<Bound<'_, PyAny>> {
    py.import("numpy")?.call_method1("array", (rows,))
}

#[pyclass(name = "Autd3", module = "autd3_core", from_py_object)]
#[derive(Clone)]
pub struct Autd3 {
    origin: Point3<f32>,
    rotation: UnitQuaternion<f32>,
}

#[pymethods]
impl Autd3 {
    #[new]
    fn new(origin: [f32; 3], rotation: [f32; 4]) -> Self {
        let [x, y, z] = origin;
        let [w, qx, qy, qz] = rotation;
        Self {
            origin: Point3::new(x, y, z),
            rotation: UnitQuaternion::from_quaternion(Quaternion::new(w, qx, qy, qz)),
        }
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

    fn center<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let c = self.inner.center();
        np_vec3(py, c.x, c.y, c.z)
    }

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn num_transducers(&self) -> usize {
        self.inner.num_transducers()
    }

    fn pattern_buffer<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("autd3_pattern")?
            .getattr("PatternBuffer")?
            .call1((self.inner.num_devices(),))
    }

    fn device(&self, index: usize) -> PyResult<Device> {
        if index >= self.inner.num_devices() {
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
        self.inner.num_devices()
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
        self.inner.num_transducers()
    }

    fn __len__(&self) -> usize {
        self.inner.num_transducers()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn center<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let c = self.inner.center();
        np_vec3(py, c.x, c.y, c.z)
    }

    fn positions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rows = self
            .inner
            .positions()
            .iter()
            .map(|p| (p.x, p.y, p.z))
            .collect();
        np_rows(py, rows)
    }

    fn directions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rows = self
            .inner
            .directions()
            .iter()
            .map(|d| {
                let d = d.into_inner();
                (d.x, d.y, d.z)
            })
            .collect();
        np_rows(py, rows)
    }

    fn position<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyAny>> {
        if index >= self.inner.num_transducers() {
            return Err(PyIndexError::new_err("transducer index out of range"));
        }
        let p = self.inner.position(index);
        np_vec3(py, p.x, p.y, p.z)
    }

    fn direction<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyAny>> {
        if index >= self.inner.num_transducers() {
            return Err(PyIndexError::new_err("transducer index out of range"));
        }
        let d = self.inner.direction(index).into_inner();
        np_vec3(py, d.x, d.y, d.z)
    }

    fn rotation<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let q = self.inner.rotation();
        np_vec4(py, q.w, q.i, q.j, q.k)
    }

    fn x_direction<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = self.inner.x_direction().into_inner();
        np_vec3(py, d.x, d.y, d.z)
    }

    fn y_direction<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = self.inner.y_direction().into_inner();
        np_vec3(py, d.x, d.y, d.z)
    }

    fn axial_direction<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = self.inner.axial_direction().into_inner();
        np_vec3(py, d.x, d.y, d.z)
    }
}

#[pyfunction]
pub fn _read_geometry_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(autd3_python_capsule::geometry_from_capsule(capsule)?.num_devices())
}
