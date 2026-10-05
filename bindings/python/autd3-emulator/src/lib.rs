use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use autd3_python_capsule::{capsule_of, frame_from_capsule, geometry_from_capsule, to_pyerr_gil};
use autd3_rs_core::common::Velocity;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_emulator::{
    AxisOrder, ClientApi, Emulator as CoreEmulator, Grid as CoreGrid, Instant as CoreInstant,
    InstantRecordOption as CoreInstantOption, RawColumn, RawFrame, Record as CoreRecord,
    Recorder as CoreRecorder, Rms as CoreRms, RmsRecordOption as CoreRmsOption,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

fn extract_duration(obj: &Bound<'_, PyAny>) -> PyResult<Duration> {
    let nanos = obj.call_method0("as_nanos")?.extract::<u128>()?;
    u64::try_from(nanos)
        .map(Duration::from_nanos)
        .map_err(|_| PyValueError::new_err("duration is out of range"))
}

fn extract_velocity(obj: &Bound<'_, PyAny>) -> PyResult<Velocity> {
    let mm_per_s: f32 = obj.getattr("mm_s").and_then(|v| v.extract()).map_err(|_| {
        PyValueError::new_err(
            "sound speed must be a Velocity, e.g. 340 * m / s (bare numbers are no longer accepted)",
        )
    })?;
    Ok(Velocity::from_mm_s(mm_per_s))
}

fn velocity_to_py(py: Python<'_>, v: Velocity) -> PyResult<Bound<'_, PyAny>> {
    py.import("autd3_core")?
        .getattr("Velocity")?
        .call_method1("from_mm_s", (v.mm_s(),))
}

fn raw_to_polars(py: Python<'_>, frame: RawFrame) -> PyResult<Bound<'_, PyAny>> {
    let frombuffer = py.import("numpy")?.getattr("frombuffer")?;
    let data = PyDict::new(py);
    for (name, col) in frame.columns {
        let (bytes, dtype) = match col {
            RawColumn::U8(v) => (PyBytes::new(py, &v), "uint8"),
            RawColumn::U16(v) => (PyBytes::new(py, bytemuck::cast_slice(&v)), "uint16"),
            RawColumn::F32(v) => (PyBytes::new(py, bytemuck::cast_slice(&v)), "float32"),
        };
        data.set_item(name, frombuffer.call1((bytes, dtype))?)?;
    }
    py.import("polars")?.getattr("DataFrame")?.call1((data,))
}

fn extract_axis(obj: &Bound<'_, PyAny>) -> PyResult<RangeInclusive<f32>> {
    if let Ok(v) = obj.extract::<f32>() {
        return Ok(v..=v);
    }
    let (start, end): (f32, f32) = obj
        .extract()
        .map_err(|_| PyValueError::new_err("an axis must be a float or a (start, end) tuple"))?;
    Ok(start..=end)
}

fn extract_order(order: &str) -> PyResult<AxisOrder> {
    match order {
        "xyz" => Ok(AxisOrder::XYZ),
        "xzy" => Ok(AxisOrder::XZY),
        "yxz" => Ok(AxisOrder::YXZ),
        "yzx" => Ok(AxisOrder::YZX),
        "zxy" => Ok(AxisOrder::ZXY),
        "zyx" => Ok(AxisOrder::ZYX),
        _ => Err(PyValueError::new_err(
            "order must be one of 'xyz', 'xzy', 'yxz', 'yzx', 'zxy', 'zyx'",
        )),
    }
}

#[pyclass(name = "Grid", module = "autd3_emulator")]
pub struct Grid {
    inner: CoreGrid,
}

#[pymethods]
impl Grid {
    #[new]
    #[pyo3(signature = (x, y, z, resolution, order = "xyz"))]
    fn new(
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
        resolution: f32,
        order: &str,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: CoreGrid {
                x: extract_axis(x)?,
                y: extract_axis(y)?,
                z: extract_axis(z)?,
                resolution,
                order: extract_order(order)?,
            },
        })
    }
}

#[pyclass(name = "RmsRecordOption", module = "autd3_emulator")]
pub struct RmsRecordOption {
    sound_speed: Velocity,
}

#[pymethods]
impl RmsRecordOption {
    #[new]
    #[pyo3(signature = (sound_speed = None))]
    fn new(sound_speed: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            sound_speed: match sound_speed {
                Some(v) => extract_velocity(v)?,
                None => CoreRmsOption::default().sound_speed,
            },
        })
    }

    #[getter]
    fn sound_speed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        velocity_to_py(py, self.sound_speed)
    }

    #[setter]
    fn set_sound_speed(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.sound_speed = extract_velocity(value)?;
        Ok(())
    }
}

#[pyclass(name = "InstantRecordOption", module = "autd3_emulator")]
pub struct InstantRecordOption {
    sound_speed: Velocity,
    time_step_ns: u64,
    #[pyo3(get, set)]
    memory_limits_hint_mb: usize,
}

#[pymethods]
impl InstantRecordOption {
    #[new]
    #[pyo3(signature = (sound_speed = None, time_step = None, memory_limits_hint_mb = 128))]
    fn new(
        sound_speed: Option<&Bound<'_, PyAny>>,
        time_step: Option<&Bound<'_, PyAny>>,
        memory_limits_hint_mb: usize,
    ) -> PyResult<Self> {
        let time_step_ns = match time_step {
            Some(d) => u64::try_from(extract_duration(d)?.as_nanos())
                .map_err(|_| PyValueError::new_err("time_step is out of range"))?,
            None => 1_000,
        };
        Ok(Self {
            sound_speed: match sound_speed {
                Some(v) => extract_velocity(v)?,
                None => CoreInstantOption::default().sound_speed,
            },
            time_step_ns,
            memory_limits_hint_mb,
        })
    }

    #[getter]
    fn sound_speed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        velocity_to_py(py, self.sound_speed)
    }

    #[setter]
    fn set_sound_speed(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.sound_speed = extract_velocity(value)?;
        Ok(())
    }
}

#[pyclass(name = "Rms", module = "autd3_emulator")]
pub struct Rms {
    inner: CoreRms,
}

#[pymethods]
impl Rms {
    fn observe_points<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        raw_to_polars(py, self.inner.observe_points_raw())
    }

    fn next<'py>(
        &mut self,
        py: Python<'py>,
        duration: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let d = extract_duration(duration)?;
        let frame = self.inner.next_raw(d).map_err(to_pyerr_gil)?;
        raw_to_polars(py, frame)
    }

    fn skip(&mut self, duration: &Bound<'_, PyAny>) -> PyResult<()> {
        let d = extract_duration(duration)?;
        self.inner.skip(d).map_err(to_pyerr_gil)?;
        Ok(())
    }
}

#[pyclass(name = "Instant", module = "autd3_emulator")]
pub struct Instant {
    _record: Arc<CoreRecord>,
    inner: CoreInstant<'static>,
}

#[pymethods]
impl Instant {
    fn observe_points<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        raw_to_polars(py, self.inner.observe_points_raw())
    }

    fn next<'py>(
        &mut self,
        py: Python<'py>,
        duration: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let d = extract_duration(duration)?;
        let frame = self.inner.next_raw(d).map_err(to_pyerr_gil)?;
        raw_to_polars(py, frame)
    }

    fn skip(&mut self, duration: &Bound<'_, PyAny>) -> PyResult<()> {
        let d = extract_duration(duration)?;
        self.inner.skip(d).map_err(to_pyerr_gil)?;
        Ok(())
    }
}

#[pyclass(name = "Record", module = "autd3_emulator")]
pub struct Record {
    inner: Arc<CoreRecord>,
}

#[pymethods]
impl Record {
    fn num_transducers(&self) -> usize {
        self.inner.num_transducers()
    }

    fn num_samples(&self) -> usize {
        self.inner.num_samples()
    }

    fn phase<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        raw_to_polars(py, self.inner.phase_raw())
    }

    fn pulse_width<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        raw_to_polars(py, self.inner.pulse_width_raw())
    }

    fn output_voltage<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        raw_to_polars(py, self.inner.output_voltage_raw())
    }

    fn output_ultrasound<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        raw_to_polars(py, self.inner.output_ultrasound_raw())
    }

    fn sound_field(
        &self,
        py: Python<'_>,
        range: &Grid,
        option: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if let Ok(opt) = option.cast::<RmsRecordOption>() {
            let opt = opt.borrow();
            let rms = self
                .inner
                .sound_field(
                    range.inner.clone(),
                    CoreRmsOption {
                        sound_speed: opt.sound_speed,
                    },
                )
                .map_err(to_pyerr_gil)?;
            return Ok(Py::new(py, Rms { inner: rms })?.into_any());
        }
        if let Ok(opt) = option.cast::<InstantRecordOption>() {
            let opt = opt.borrow();
            let instant = self
                .inner
                .sound_field(
                    range.inner.clone(),
                    CoreInstantOption {
                        sound_speed: opt.sound_speed,
                        time_step: Duration::from_nanos(opt.time_step_ns),
                        memory_limits_hint_mb: opt.memory_limits_hint_mb,
                    },
                )
                .map_err(to_pyerr_gil)?;
            let instant: CoreInstant<'static> = unsafe { std::mem::transmute(instant) };
            return Ok(Py::new(
                py,
                Instant {
                    _record: Arc::clone(&self.inner),
                    inner: instant,
                },
            )?
            .into_any());
        }
        Err(PyValueError::new_err(
            "option must be RmsRecordOption or InstantRecordOption",
        ))
    }
}

#[pyclass(name = "Recorder", module = "autd3_emulator", unsendable)]
pub struct Recorder {
    inner: Option<CoreRecorder>,
    geometry: Py<PyAny>,
    num_devices: usize,
}

#[pymethods]
impl Recorder {
    #[new]
    #[pyo3(signature = (geometry, start_ns = 0))]
    fn new(geometry: &Bound<'_, PyAny>, start_ns: u64) -> PyResult<Self> {
        let capsule = capsule_of(geometry)?;
        let inner = geometry_from_capsule(&capsule)?;
        Ok(Self {
            num_devices: inner.num_devices(),
            inner: Some(CoreRecorder::new(inner, start_ns)),
            geometry: geometry.clone().unbind(),
        })
    }

    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn datagram_builder<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("autd3")?
            .getattr("DatagramBuilder")?
            .call1((self.geometry.bind(py),))
    }

    fn send_checked(&mut self, frame: &Bound<'_, PyAny>) -> PyResult<()> {
        let recorder = self
            .inner
            .as_mut()
            .ok_or_else(|| PyValueError::new_err("recorder has already been consumed"))?;
        let capsule = capsule_of(frame)?;
        let (frames, index) = frame_from_capsule(&capsule)?;
        let f = frames
            .frame(index)
            .ok_or_else(|| PyValueError::new_err("frame index out of range"))?;
        pollster::block_on(recorder.send_checked(f)).map_err(to_pyerr_gil)?;
        Ok(())
    }

    fn tick(&mut self, duration: &Bound<'_, PyAny>) -> PyResult<()> {
        let recorder = self
            .inner
            .as_mut()
            .ok_or_else(|| PyValueError::new_err("recorder has already been consumed"))?;
        let d = extract_duration(duration)?;
        recorder.tick(d).map_err(to_pyerr_gil)?;
        Ok(())
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_record(&mut self) -> PyResult<Record> {
        let recorder = self
            .inner
            .take()
            .ok_or_else(|| PyValueError::new_err("recorder has already been consumed"))?;
        Ok(Record {
            inner: Arc::new(recorder.into_record()),
        })
    }
}

#[pyclass(name = "Emulator", module = "autd3_emulator")]
pub struct Emulator {
    geometry: Geometry,
    geometry_py: Py<PyAny>,
}

#[pymethods]
impl Emulator {
    #[new]
    fn new(geometry: &Bound<'_, PyAny>) -> PyResult<Self> {
        let capsule = capsule_of(geometry)?;
        Ok(Self {
            geometry: geometry_from_capsule(&capsule)?.clone(),
            geometry_py: geometry.clone().unbind(),
        })
    }

    fn transducer_table<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        raw_to_polars(
            py,
            CoreEmulator::new(self.geometry.clone()).transducer_table_raw(),
        )
    }

    fn record(&self, py: Python<'_>, callback: &Bound<'_, PyAny>) -> PyResult<Record> {
        let recorder = CoreRecorder::new(&self.geometry, 0);
        let py_recorder = Py::new(
            py,
            Recorder {
                inner: Some(recorder),
                geometry: self.geometry_py.clone_ref(py),
                num_devices: self.geometry.num_devices(),
            },
        )?;
        callback.call1((py_recorder.clone_ref(py),))?;
        let inner = py_recorder
            .borrow_mut(py)
            .inner
            .take()
            .ok_or_else(|| PyValueError::new_err("recorder has already been consumed"))?;
        Ok(Record {
            inner: Arc::new(inner.into_record()),
        })
    }
}

#[pymodule]
fn autd3_emulator(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Emulator>()?;
    m.add_class::<Recorder>()?;
    m.add_class::<Record>()?;
    m.add_class::<Rms>()?;
    m.add_class::<Instant>()?;
    m.add_class::<RmsRecordOption>()?;
    m.add_class::<InstantRecordOption>()?;
    m.add_class::<Grid>()?;
    Ok(())
}
