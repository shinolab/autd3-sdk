use core::time::Duration;

use autd3_python_capsule::{DevicePattern, capsule_of, pattern_from_capsule};
use autd3_rs::commands::WriteFociBuffer as CoreWriteFociBuffer;
use autd3_rs::commands::{
    FociStm as CoreFociStm, FociStmOption as CoreFociStmOption,
    PatternStmMode as CorePatternStmMode, PatternStmOption as CorePatternStmOption,
    StmConfig as CoreStmConfig, circle as core_circle, line as core_line,
};
use autd3_rs::value::{
    ControlPoint as CoreControlPoint, ControlPoints as CoreControlPoints, Intensity, Nearest,
    PatternBank as CorePatternBank, Phase, SamplingConfig,
};
use autd3_rs::{Point3, Vector3, Velocity};
use autd3_rs_core::geometry::UnitVector3;
use autd3_rs_core::units::{Hz, mm};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::ops;

fn extract_point(obj: &Bound<'_, PyAny>) -> PyResult<Point3<f32>> {
    let [x, y, z] = obj.extract::<[f32; 3]>().map_err(|_| {
        PyValueError::new_err(
            "expected a length-3 array-like (numpy array, list, or tuple) of x, y, z in mm",
        )
    })?;
    Ok(Point3::new(x, y, z))
}

fn extract_direction(obj: &Bound<'_, PyAny>) -> PyResult<UnitVector3<f32>> {
    let [x, y, z] = obj
        .extract::<[f32; 3]>()
        .map_err(|_| PyValueError::new_err("expected a length-3 direction vector"))?;
    Ok(UnitVector3::new_normalize(Vector3::new(x, y, z)))
}

fn extract_u8(obj: &Bound<'_, PyAny>) -> PyResult<u8> {
    if let Ok(v) = obj.extract::<u8>() {
        return Ok(v);
    }
    obj.getattr("value")?.extract::<u8>()
}

fn extract_velocity(obj: &Bound<'_, PyAny>) -> PyResult<Velocity> {
    let mm_per_s: f32 = obj.getattr("mm_per_s").and_then(|v| v.extract()).map_err(|_| {
        PyValueError::new_err(
            "sound speed must be a Velocity, e.g. 340 * m / s (bare numbers are no longer accepted)",
        )
    })?;
    Ok(Velocity::from_mm_s(mm_per_s))
}

#[pyclass(name = "StmConfig", module = "autd3.commands", from_py_object)]
#[derive(Clone, Copy)]
pub struct StmConfig(pub(crate) CoreStmConfig);

pub(crate) fn extract_stm_config(value: &Bound<'_, PyAny>) -> PyResult<CoreStmConfig> {
    if let Ok(config) = value.cast::<StmConfig>() {
        return Ok(config.borrow().0);
    }
    if let Ok(hz) = value.call_method0("nearest_hz") {
        let nanos = value.call_method0("nearest_nanos")?;
        return match (
            hz.extract::<Option<f32>>()?,
            nanos.extract::<Option<u128>>()?,
        ) {
            (Some(hz), _) => Ok(CoreStmConfig::new(Nearest(hz * Hz))),
            (_, Some(nanos)) => Ok(CoreStmConfig::new(Nearest(nanos_to_duration(nanos)?))),
            _ => Err(PyValueError::new_err("invalid Nearest")),
        };
    }
    if let Ok(divide) = value.call_method0("divide") {
        let divide = core::num::NonZeroU16::new(divide.extract::<u16>()?)
            .ok_or_else(|| PyValueError::new_err("divide must be >= 1"))?;
        return Ok(CoreStmConfig::new(SamplingConfig::new(divide)));
    }
    if let Ok(hz) = value.getattr("hz") {
        return Ok(CoreStmConfig::new(hz.extract::<f32>()? * Hz));
    }
    if let Ok(nanos) = value.call_method0("as_nanos") {
        return Ok(CoreStmConfig::new(nanos_to_duration(
            nanos.extract::<u128>()?,
        )?));
    }
    Err(PyValueError::new_err(
        "StmConfig expects a frequency (e.g. 1.0 * Hz), a Duration, a SamplingConfig, or Nearest(...)",
    ))
}

#[pymethods]
impl StmConfig {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(extract_stm_config(value)?))
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_sampling_config<'py>(
        &self,
        py: Python<'py>,
        size: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let divide = self
            .0
            .into_sampling_config(size)
            .divide()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        py.import("autd3_core")?
            .getattr("SamplingConfig")?
            .call1((divide,))
    }
}

fn nanos_to_duration(nanos: u128) -> PyResult<Duration> {
    u64::try_from(nanos)
        .map(Duration::from_nanos)
        .map_err(|_| PyValueError::new_err("duration is out of range"))
}

#[pyclass(name = "ControlPoint", module = "autd3.value", skip_from_py_object)]
#[derive(Clone, Copy)]
pub struct ControlPoint {
    pub(crate) inner: CoreControlPoint,
}

#[pymethods]
impl ControlPoint {
    #[new]
    #[pyo3(signature = (point, phase_offset = None))]
    fn new(point: &Bound<'_, PyAny>, phase_offset: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let phase_offset = phase_offset
            .map(extract_u8)
            .transpose()?
            .map_or(Phase::ZERO, Phase);
        Ok(Self {
            inner: CoreControlPoint::new(extract_point(point)?, phase_offset),
        })
    }
}

#[pyclass(name = "ControlPoints", module = "autd3.value", skip_from_py_object)]
#[derive(Clone)]
pub struct ControlPoints {
    pub(crate) points: Vec<CoreControlPoint>,
    pub(crate) intensity: Intensity,
}

#[pymethods]
impl ControlPoints {
    #[new]
    #[pyo3(signature = (points, intensity = None))]
    fn new(
        points: Vec<PyRef<'_, ControlPoint>>,
        intensity: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let intensity = intensity
            .map(extract_u8)
            .transpose()?
            .map_or(Intensity::MAX, Intensity);
        Ok(Self {
            points: points.iter().map(|p| p.inner).collect(),
            intensity,
        })
    }
}

impl ControlPoints {
    fn from_core(cp: CoreControlPoints<1>) -> Self {
        Self {
            points: cp.points.to_vec(),
            intensity: cp.intensity,
        }
    }
}

#[pyclass(name = "FociStmOption", module = "autd3.commands", skip_from_py_object)]
pub struct FociStmOption {
    pub(crate) inner: CoreFociStmOption,
}

#[pymethods]
impl FociStmOption {
    #[new]
    #[pyo3(signature = (bank = None, sound_speed = None, loop_behavior = None, transition_mode = None))]
    fn new(
        bank: Option<ops::PatternBank>,
        sound_speed: Option<&Bound<'_, PyAny>>,
        loop_behavior: Option<ops::LoopBehavior>,
        transition_mode: Option<ops::TransitionMode>,
    ) -> PyResult<Self> {
        let mut inner = CoreFociStmOption::default();
        if let Some(sv) = sound_speed {
            inner.sound_speed = extract_velocity(sv)?;
        }
        if let Some(b) = bank {
            inner.bank = b.0;
        }
        if let Some(l) = loop_behavior {
            inner.loop_behavior = l.0;
        }
        if let Some(t) = transition_mode {
            inner.transition_mode = t.0;
        }
        Ok(Self { inner })
    }

    #[getter]
    fn bank(&self) -> ops::PatternBank {
        ops::PatternBank(self.inner.bank)
    }

    #[getter]
    fn sound_speed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("autd3_core")?
            .getattr("Velocity")?
            .call_method1("from_mm_s", (self.inner.sound_speed.mm_per_s(),))
    }

    #[getter]
    fn loop_behavior(&self) -> ops::LoopBehavior {
        ops::LoopBehavior(self.inner.loop_behavior)
    }

    #[getter]
    fn transition_mode(&self) -> ops::TransitionMode {
        ops::TransitionMode(self.inner.transition_mode)
    }
}

#[pyclass(name = "PatternStmMode", module = "autd3.commands", from_py_object)]
#[derive(Clone, Copy)]
pub struct PatternStmMode(pub(crate) CorePatternStmMode);

#[pymethods]
impl PatternStmMode {
    #[classattr]
    #[pyo3(name = "PhaseIntensityFull")]
    fn phase_intensity_full() -> Self {
        Self(CorePatternStmMode::PhaseIntensityFull)
    }

    #[classattr]
    #[pyo3(name = "PhaseFull")]
    fn phase_full() -> Self {
        Self(CorePatternStmMode::PhaseFull)
    }

    #[classattr]
    #[pyo3(name = "PhaseHalf")]
    fn phase_half() -> Self {
        Self(CorePatternStmMode::PhaseHalf)
    }
}

#[pyclass(
    name = "PatternStmOption",
    module = "autd3.commands",
    skip_from_py_object
)]
pub struct PatternStmOption {
    pub(crate) inner: CorePatternStmOption,
}

#[pymethods]
impl PatternStmOption {
    #[new]
    #[pyo3(signature = (bank = None, mode = None, loop_behavior = None, transition_mode = None))]
    fn new(
        bank: Option<ops::PatternBank>,
        mode: Option<PatternStmMode>,
        loop_behavior: Option<ops::LoopBehavior>,
        transition_mode: Option<ops::TransitionMode>,
    ) -> Self {
        let mut inner = CorePatternStmOption::default();
        if let Some(b) = bank {
            inner.bank = b.0;
        }
        if let Some(m) = mode {
            inner.mode = m.0;
        }
        if let Some(l) = loop_behavior {
            inner.loop_behavior = l.0;
        }
        if let Some(t) = transition_mode {
            inner.transition_mode = t.0;
        }
        Self { inner }
    }

    #[getter]
    fn bank(&self) -> ops::PatternBank {
        ops::PatternBank(self.inner.bank)
    }

    #[getter]
    fn mode(&self) -> PatternStmMode {
        PatternStmMode(self.inner.mode)
    }

    #[getter]
    fn loop_behavior(&self) -> ops::LoopBehavior {
        ops::LoopBehavior(self.inner.loop_behavior)
    }

    #[getter]
    fn transition_mode(&self) -> ops::TransitionMode {
        ops::TransitionMode(self.inner.transition_mode)
    }
}

macro_rules! foci_points {
    ($($n:literal => $variant:ident),* $(,)?) => {
        #[derive(Clone)]
        pub(crate) enum FociPoints {
            $($variant(Vec<CoreControlPoints<$n>>)),*
        }

        impl FociPoints {
            fn from_samples(samples: &[PyRef<'_, ControlPoints>]) -> PyResult<Self> {
                let num_foci = samples.first().map_or(0, |s| s.points.len());
                if num_foci == 0 {
                    return Err(PyValueError::new_err("FociStm needs at least one sample with at least one focus"));
                }
                if samples.iter().any(|s| s.points.len() != num_foci) {
                    return Err(PyValueError::new_err("every FociStm sample must have the same number of foci"));
                }
                match num_foci {
                    $($n => {
                        let v = samples
                            .iter()
                            .map(|s| {
                                let arr: [CoreControlPoint; $n] = core::array::from_fn(|k| s.points[k]);
                                CoreControlPoints::new(arr, s.intensity)
                            })
                            .collect();
                        Ok(FociPoints::$variant(v))
                    })*
                    other => Err(PyValueError::new_err(format!(
                        "number of foci per sample must be 1..=8, got {other}"
                    ))),
                }
            }

            pub(crate) fn push_into<'a>(
                &'a self,
                config: CoreStmConfig,
                option: CoreFociStmOption,
                builder: &mut autd3_rs::DatagramBuilder<'a>,
            ) {
                match self {
                    $(FociPoints::$variant(v) => {
                        builder.push(CoreFociStm::new(config, v.as_slice(), option));
                    })*
                }
            }

            pub(crate) fn push_write_foci<'a>(
                &'a self,
                bank: CorePatternBank,
                index_offset: usize,
                builder: &mut autd3_rs::DatagramBuilder<'a>,
            ) {
                match self {
                    $(FociPoints::$variant(v) => {
                        builder.push(CoreWriteFociBuffer {
                            bank,
                            index_offset,
                            points: v.as_slice(),
                        });
                    })*
                }
            }
        }
    };
}

foci_points!(1 => N1, 2 => N2, 3 => N3, 4 => N4, 5 => N5, 6 => N6, 7 => N7, 8 => N8);

#[pyclass(name = "FociStm", module = "autd3.commands")]
pub struct FociStm {
    pub(crate) config: CoreStmConfig,
    pub(crate) points: FociPoints,
    pub(crate) option: CoreFociStmOption,
}

#[pymethods]
impl FociStm {
    #[new]
    #[pyo3(signature = (config, samples, option = None))]
    fn new(
        config: &Bound<'_, PyAny>,
        samples: Vec<PyRef<'_, ControlPoints>>,
        option: Option<PyRef<'_, FociStmOption>>,
    ) -> PyResult<Self> {
        Ok(Self {
            config: extract_stm_config(config)?,
            points: FociPoints::from_samples(&samples)?,
            option: option.map_or_else(CoreFociStmOption::default, |o| o.inner),
        })
    }
}

#[pyclass(name = "WriteFociBuffer", module = "autd3.commands")]
pub struct WriteFociBuffer {
    pub(crate) bank: CorePatternBank,
    pub(crate) index_offset: usize,
    pub(crate) points: FociPoints,
}

#[pymethods]
impl WriteFociBuffer {
    #[new]
    fn new(
        bank: ops::PatternBank,
        index_offset: usize,
        points: Vec<PyRef<'_, ControlPoints>>,
    ) -> PyResult<Self> {
        Ok(Self {
            bank: bank.0,
            index_offset,
            points: FociPoints::from_samples(&points)?,
        })
    }
}

#[pyclass(name = "PatternStm", module = "autd3.commands")]
pub struct PatternStm {
    pub(crate) config: CoreStmConfig,
    pub(crate) patterns: Vec<Vec<DevicePattern>>,
    pub(crate) option: CorePatternStmOption,
}

#[pymethods]
impl PatternStm {
    #[new]
    #[pyo3(signature = (config, patterns, option = None))]
    fn new(
        config: &Bound<'_, PyAny>,
        patterns: Vec<Bound<'_, PyAny>>,
        option: Option<PyRef<'_, PatternStmOption>>,
    ) -> PyResult<Self> {
        let stm_config = extract_stm_config(config)?;
        let patterns = patterns
            .iter()
            .map(|buffer| {
                let capsule = capsule_of(buffer)?;
                Ok(pattern_from_capsule(&capsule)?.to_vec())
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            config: stm_config,
            patterns,
            option: option.map_or_else(CorePatternStmOption::default, |o| o.inner),
        })
    }
}

#[pyfunction]
#[pyo3(signature = (center, radius, num_points, normal, intensity, out))]
fn circle(
    center: &Bound<'_, PyAny>,
    radius: f32,
    num_points: usize,
    normal: &Bound<'_, PyAny>,
    intensity: &Bound<'_, PyAny>,
    out: &Bound<'_, PyList>,
) -> PyResult<()> {
    let intensity = Intensity(extract_u8(intensity)?);
    let mut pts = Vec::new();
    core_circle(
        extract_point(center)?,
        radius * mm,
        num_points,
        extract_direction(normal)?,
        intensity,
        &mut pts,
    );
    out.call_method0("clear")?;
    for cp in pts {
        out.append(ControlPoints::from_core(cp))?;
    }
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (start, end, num_points, intensity, out))]
fn line(
    start: &Bound<'_, PyAny>,
    end: &Bound<'_, PyAny>,
    num_points: usize,
    intensity: &Bound<'_, PyAny>,
    out: &Bound<'_, PyList>,
) -> PyResult<()> {
    let intensity = Intensity(extract_u8(intensity)?);
    let mut pts = Vec::new();
    core_line(
        extract_point(start)?,
        extract_point(end)?,
        num_points,
        intensity,
        &mut pts,
    );
    out.call_method0("clear")?;
    for cp in pts {
        out.append(ControlPoints::from_core(cp))?;
    }
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<StmConfig>()?;
    m.add_class::<ControlPoint>()?;
    m.add_class::<ControlPoints>()?;
    m.add_class::<FociStmOption>()?;
    m.add_class::<PatternStmMode>()?;
    m.add_class::<PatternStmOption>()?;
    m.add_class::<FociStm>()?;
    m.add_class::<WriteFociBuffer>()?;
    m.add_class::<PatternStm>()?;
    m.add_function(wrap_pyfunction!(circle, m)?)?;
    m.add_function(wrap_pyfunction!(line, m)?)?;
    Ok(())
}
