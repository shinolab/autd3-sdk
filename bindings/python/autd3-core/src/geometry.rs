use autd3_rs_core::{
    Autd3 as CoreAutd3, Device as CoreDevice, Geometry as CoreGeometry, Point3, Quaternion,
    UnitQuaternion, UnitVector3, Vector3,
};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyCapsule;

use crate::units::Angle;

fn np_vec3(py: Python<'_>, x: f32, y: f32, z: f32) -> PyResult<Bound<'_, PyAny>> {
    py.import("numpy")?.call_method1("array", ((x, y, z),))
}

fn np_vec4(py: Python<'_>, x: f32, y: f32, z: f32, w: f32) -> PyResult<Bound<'_, PyAny>> {
    py.import("numpy")?.call_method1("array", ((x, y, z, w),))
}

fn np_rows(py: Python<'_>, rows: Vec<(f32, f32, f32)>) -> PyResult<Bound<'_, PyAny>> {
    py.import("numpy")?.call_method1("array", (rows,))
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
            UnitQuaternion::from_axis_angle(&a1, first.0.radian())
                * UnitQuaternion::from_axis_angle(&a2, second.0.radian())
                * UnitQuaternion::from_axis_angle(&a3, third.0.radian()),
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

fn coerce_rotation(rotation: &Bound<'_, PyAny>) -> PyResult<UnitQuaternion<f32>> {
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
    Ok(UnitQuaternion::from_quaternion(Quaternion::new(
        w, qx, qy, qz,
    )))
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
    fn new(origin: [f32; 3], rotation: &Bound<'_, PyAny>) -> PyResult<Self> {
        let [x, y, z] = origin;
        Ok(Self {
            origin: Point3::new(x, y, z),
            rotation: coerce_rotation(rotation)?,
        })
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
