use autd3_rs_core::{
    Autd3 as CoreAutd3, Device as CoreDevice, Geometry as CoreGeometry, Point3, Quaternion,
    UnitQuaternion, UnitVector3, Vector3,
};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyIterator, PyList};

use crate::error::to_pyerr;
use crate::units::{Angle, Length};

fn np_vec3(py: Python<'_>, x: f32, y: f32, z: f32) -> PyResult<Bound<'_, PyAny>> {
    autd3_python_capsule::numpy::f32_vector(py, [x, y, z])
}

fn np_quat<'py>(py: Python<'py>, q: &UnitQuaternion<f32>) -> PyResult<Bound<'py, PyAny>> {
    autd3_python_capsule::numpy::f32_vector(py, [q.w, q.i, q.j, q.k])
}

#[pyclass(name = "EulerAngles", module = "autd3_core", from_py_object)]
#[derive(Clone, Copy)]
pub struct EulerAngles(UnitQuaternion<f32>);

impl EulerAngles {
    fn from_axes(
        a1: UnitVector3<f32>,
        first: Angle,
        a2: UnitVector3<f32>,
        second: Angle,
        a3: UnitVector3<f32>,
        third: Angle,
    ) -> Self {
        Self(
            UnitQuaternion::from_axis_angle(&a1, first.0.rad())
                * UnitQuaternion::from_axis_angle(&a2, second.0.rad())
                * UnitQuaternion::from_axis_angle(&a3, third.0.rad()),
        )
    }
}

macro_rules! euler_orders {
    ($(($name:literal, $method:ident, $a1:ident, $a2:ident, $a3:ident)),* $(,)?) => {
        #[pymethods]
        impl EulerAngles {
            $(
                #[staticmethod]
                #[pyo3(name = $name)]
                fn $method(first: Angle, second: Angle, third: Angle) -> Self {
                    Self::from_axes(
                        Vector3::$a1(),
                        first,
                        Vector3::$a2(),
                        second,
                        Vector3::$a3(),
                        third,
                    )
                }
            )*
        }
    };
}

euler_orders!(
    ("XYZ", xyz, x_axis, y_axis, z_axis),
    ("XZY", xzy, x_axis, z_axis, y_axis),
    ("YXZ", yxz, y_axis, x_axis, z_axis),
    ("YZX", yzx, y_axis, z_axis, x_axis),
    ("ZXY", zxy, z_axis, x_axis, y_axis),
    ("ZYX", zyx, z_axis, y_axis, x_axis),
    ("XYX", xyx, x_axis, y_axis, x_axis),
    ("XZX", xzx, x_axis, z_axis, x_axis),
    ("YXY", yxy, y_axis, x_axis, y_axis),
    ("YZY", yzy, y_axis, z_axis, y_axis),
    ("ZXZ", zxz, z_axis, x_axis, z_axis),
    ("ZYZ", zyz, z_axis, y_axis, z_axis),
);

fn scipy_rotation_to_quat(obj: &Bound<'_, PyAny>) -> PyResult<Option<[f32; 4]>> {
    let py = obj.py();
    let modules = py.import("sys")?.getattr("modules")?;
    let Some(m) = modules
        .call_method1("get", ("scipy.spatial.transform",))?
        .extract::<Option<Bound<'_, PyAny>>>()?
    else {
        return Ok(None);
    };
    let rot_cls = m.getattr("Rotation")?;
    if !obj.is_instance(&rot_cls)? {
        return Ok(None);
    }
    let [qx, qy, qz, qw]: [f32; 4] = obj.call_method0("as_quat")?.extract()?;
    Ok(Some([qw, qx, qy, qz]))
}

fn coerce_rotation(rotation: Option<&Bound<'_, PyAny>>) -> PyResult<UnitQuaternion<f32>> {
    let Some(rotation) = rotation.filter(|rotation| !rotation.is_none()) else {
        return Ok(UnitQuaternion::identity());
    };
    if let Ok(euler) = rotation.extract::<EulerAngles>() {
        return Ok(euler.0);
    }
    let quat = if let Some(q) = scipy_rotation_to_quat(rotation)? {
        q
    } else if let Ok(q) = rotation.extract::<[f32; 4]>() {
        q
    } else {
        return Err(PyValueError::new_err(
            "rotation must be a scalar-first quaternion [w, x, y, z], an EulerAngles, or a scipy.spatial.transform.Rotation",
        ));
    };
    let [w, qx, qy, qz] = quat;
    let quat = Quaternion::new(w, qx, qy, qz);
    let norm = quat.norm();
    if norm.is_nan() || (norm - 1.0).abs() > CoreAutd3::ROTATION_NORM_TOLERANCE {
        return Err(PyValueError::new_err(format!(
            "`rotation` must be a unit quaternion [w, x, y, z], but its norm is {norm}"
        )));
    }
    Ok(UnitQuaternion::from_quaternion(quat))
}

#[pyclass(name = "Autd3", module = "autd3_core", eq, frozen, from_py_object)]
#[derive(Clone, PartialEq)]
pub struct Autd3 {
    origin: Point3<f32>,
    rotation: UnitQuaternion<f32>,
}

#[pymethods]
impl Autd3 {
    #[new]
    #[pyo3(signature = (origin, rotation = None))]
    fn new(origin: [f32; 3], rotation: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let [x, y, z] = origin;
        Ok(Self {
            origin: Point3::new(x, y, z),
            rotation: coerce_rotation(rotation)?,
        })
    }

    #[classattr]
    const NUM_TRANSDUCERS: usize = CoreAutd3::NUM_TRANSDUCERS;

    #[classattr]
    const GRID_X: u32 = CoreAutd3::GRID_X;

    #[classattr]
    const GRID_Y: u32 = CoreAutd3::GRID_Y;

    #[classattr]
    const PITCH_MM: f32 = CoreAutd3::PITCH_MM;

    #[classattr]
    const DEVICE_WIDTH: f32 = CoreAutd3::DEVICE_WIDTH;

    #[classattr]
    const DEVICE_HEIGHT: f32 = CoreAutd3::DEVICE_HEIGHT;

    #[getter]
    fn origin<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        np_vec3(py, self.origin.x, self.origin.y, self.origin.z)
    }

    #[getter]
    fn rotation<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        np_quat(py, &self.rotation)
    }

    fn __repr__(&self) -> String {
        format!(
            "Autd3(origin=[{}, {}, {}], rotation=[{}, {}, {}, {}])",
            self.origin.x,
            self.origin.y,
            self.origin.z,
            self.rotation.w,
            self.rotation.i,
            self.rotation.j,
            self.rotation.k
        )
    }
}

#[pyfunction]
pub(crate) fn point(py: Python<'_>, x: Length, y: Length, z: Length) -> PyResult<Bound<'_, PyAny>> {
    let p = autd3_rs_core::point(x.0, y.0, z.0);
    np_vec3(py, p.x, p.y, p.z)
}

#[pyfunction]
pub(crate) fn offset(
    py: Python<'_>,
    x: Length,
    y: Length,
    z: Length,
) -> PyResult<Bound<'_, PyAny>> {
    let v = autd3_rs_core::offset(x.0, y.0, z.0);
    np_vec3(py, v.x, v.y, v.z)
}

fn same_device(lhs: &CoreDevice, rhs: &CoreDevice) -> bool {
    lhs.idx() == rhs.idx()
        && lhs.rotation() == rhs.rotation()
        && lhs.positions() == rhs.positions()
        && lhs.directions() == rhs.directions()
}

#[pyclass(name = "Geometry", module = "autd3_core")]
pub struct Geometry(CoreGeometry);

#[pymethods]
impl Geometry {
    #[new]
    fn new(devices: Vec<Autd3>) -> Self {
        let devices = devices
            .into_iter()
            .map(|d| CoreAutd3::new(d.origin, d.rotation))
            .collect();
        Self(CoreGeometry::new(devices))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        CoreGeometry::from_json(json).map(Self).map_err(to_pyerr)
    }

    fn to_json(&self) -> PyResult<String> {
        self.0.to_json().map_err(to_pyerr)
    }

    fn center<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let c = self.0.center();
        np_vec3(py, c.x, c.y, c.z)
    }

    fn num_devices(&self) -> usize {
        self.0.num_devices()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn num_transducers(&self) -> usize {
        self.0.num_transducers()
    }

    fn phase_buffer<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("autd3_pattern")?
            .getattr("PhaseBuffer")?
            .call1((self.0.num_devices(),))
    }

    fn intensity_buffer<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("autd3_pattern")?
            .getattr("IntensityBuffer")?
            .call1((self.0.num_devices(),))
    }

    fn device(&self, index: usize) -> PyResult<Device> {
        if index >= self.0.num_devices() {
            return Err(PyIndexError::new_err("device index out of range"));
        }
        Ok(Device(self.0[index].clone()))
    }

    fn __getitem__(&self, index: usize) -> PyResult<Device> {
        self.device(index)
    }

    fn __len__(&self) -> usize {
        self.0.num_devices()
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        let devices = self.0.iter().cloned().map(Device).collect::<Vec<_>>();
        PyList::new(py, devices)?.try_iter()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.cast::<Self>().is_ok_and(|other| {
            let other = other.borrow();
            self.0.num_devices() == other.0.num_devices()
                && self
                    .0
                    .iter()
                    .zip(other.0.iter())
                    .all(|(lhs, rhs)| same_device(lhs, rhs))
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Geometry(num_devices={}, num_transducers={})",
            self.0.num_devices(),
            self.0.num_transducers()
        )
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        autd3_python_capsule::geometry_into_capsule(py, self.0.clone())
    }

    #[staticmethod]
    fn _from_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<Self> {
        Ok(Self(
            autd3_python_capsule::geometry_from_capsule(capsule)?.clone(),
        ))
    }
}

#[pyclass(name = "Device", module = "autd3_core")]
pub struct Device(CoreDevice);

#[pymethods]
impl Device {
    fn idx(&self) -> usize {
        self.0.idx()
    }

    fn num_transducers(&self) -> usize {
        self.0.num_transducers()
    }

    fn __len__(&self) -> usize {
        self.0.num_transducers()
    }

    fn __getitem__<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyAny>> {
        self.position(py, index)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .cast::<Self>()
            .is_ok_and(|other| same_device(&self.0, &other.borrow().0))
    }

    fn __repr__(&self) -> String {
        format!(
            "Device(idx={}, num_transducers={})",
            self.0.idx(),
            self.0.num_transducers()
        )
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        autd3_python_capsule::device_into_capsule(py, self.0.clone())
    }

    fn to_local<'py>(&self, py: Python<'py>, point: [f32; 3]) -> PyResult<Bound<'py, PyAny>> {
        let [x, y, z] = point;
        let p = self.0.to_local(Point3::new(x, y, z));
        np_vec3(py, p.x, p.y, p.z)
    }

    fn center<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let c = self.0.center();
        np_vec3(py, c.x, c.y, c.z)
    }

    fn positions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        autd3_python_capsule::numpy::f32_vec3_rows(
            py,
            self.0.positions().iter().map(|p| [p.x, p.y, p.z]),
        )
    }

    fn directions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        autd3_python_capsule::numpy::f32_vec3_rows(
            py,
            self.0.directions().iter().map(|d| [d.x, d.y, d.z]),
        )
    }

    fn position<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyAny>> {
        if index >= self.0.num_transducers() {
            return Err(PyIndexError::new_err("transducer index out of range"));
        }
        let p = self.0.position(index);
        np_vec3(py, p.x, p.y, p.z)
    }

    fn direction<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyAny>> {
        if index >= self.0.num_transducers() {
            return Err(PyIndexError::new_err("transducer index out of range"));
        }
        let d = self.0.direction(index).into_inner();
        np_vec3(py, d.x, d.y, d.z)
    }

    fn rotation<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        np_quat(py, &self.0.rotation())
    }

    fn x_direction<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = self.0.x_direction().into_inner();
        np_vec3(py, d.x, d.y, d.z)
    }

    fn y_direction<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = self.0.y_direction().into_inner();
        np_vec3(py, d.x, d.y, d.z)
    }

    fn axial_direction<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = self.0.axial_direction().into_inner();
        np_vec3(py, d.x, d.y, d.z)
    }
}
