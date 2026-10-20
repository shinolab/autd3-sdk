use std::sync::Arc;

use autd3_python_capsule::extract::{
    extract_direction, extract_duration, extract_length, extract_point, extract_sampling_config,
    extract_u8, extract_velocity, intensity_to_py, is_duration, nanos_to_duration, phase_to_py,
    sampling_config_to_py, velocity_to_py,
};
use autd3_python_capsule::numpy::f32_vec3;
use autd3_rs::Velocity;
use autd3_rs::commands::Expansion;
use autd3_rs::commands::WriteFociBuffer as CoreWriteFociBuffer;
use autd3_rs::commands::{
    FociStm as CoreFociStm, FociStmOption as CoreFociStmOption, PatternStm as CorePatternStm,
    PatternStmOption as CorePatternStmOption, StmConfig as CoreStmConfig,
    StmIntensity as CoreStmIntensity, circle as core_circle, line as core_line,
};
use autd3_rs::value::{
    ControlPoint as CoreControlPoint, ControlPoints as CoreControlPoints, Intensity, Nearest,
    PatternBank as CorePatternBank, Phase,
};
use autd3_rs_core::units::Hz;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::datagram::PushCommand;
use crate::ops;

#[pyclass(
    name = "StmConfig",
    module = "autd3.commands",
    eq,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq)]
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
    if value.hasattr("_sampling_config_capsule")? {
        return Ok(CoreStmConfig::new(extract_sampling_config(value)?));
    }
    if let Ok(hz) = value.getattr("hz") {
        return Ok(CoreStmConfig::new(hz.extract::<f32>()? * Hz));
    }
    if is_duration(value)? {
        return Ok(CoreStmConfig::new(extract_duration(value)?));
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
        sampling_config_to_py(py, self.0.into_sampling_config(size))
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "ControlPoint", module = "autd3.value", eq, frozen)]
#[derive(Clone, Copy, PartialEq)]
pub struct ControlPoint(pub(crate) CoreControlPoint);

#[pymethods]
impl ControlPoint {
    #[new]
    #[pyo3(signature = (point, phase_offset))]
    fn new(point: &Bound<'_, PyAny>, phase_offset: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(CoreControlPoint::new(
            extract_point(point)?,
            Phase(extract_u8(phase_offset)?),
        )))
    }

    #[getter]
    fn point<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let p = self.0.point;
        f32_vec3(py, [p.x, p.y, p.z])
    }

    #[getter]
    fn phase_offset<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        phase_to_py(py, self.0.phase_offset)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "ControlPoints", module = "autd3.value", eq, frozen)]
#[derive(Clone, PartialEq)]
pub struct ControlPoints {
    pub(crate) points: Vec<CoreControlPoint>,
    pub(crate) intensity: Intensity,
}

#[pymethods]
impl ControlPoints {
    #[new]
    #[pyo3(signature = (points, intensity))]
    fn new(points: Vec<PyRef<'_, ControlPoint>>, intensity: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            points: points.iter().map(|p| p.0).collect(),
            intensity: Intensity(extract_u8(intensity)?),
        })
    }

    #[getter]
    fn points(&self) -> Vec<ControlPoint> {
        self.points.iter().copied().map(ControlPoint).collect()
    }

    #[getter]
    fn intensity<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        intensity_to_py(py, self.intensity)
    }

    fn __repr__(&self) -> String {
        format!(
            "ControlPoints {{ points: {:?}, intensity: {:?} }}",
            self.points, self.intensity
        )
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

#[pyclass(name = "FociStmOption", module = "autd3.commands", eq, frozen)]
#[derive(PartialEq)]
pub struct FociStmOption(pub(crate) CoreFociStmOption);

#[pymethods]
impl FociStmOption {
    #[new]
    #[pyo3(signature = (
        bank = ops::PatternBank(CoreFociStmOption::default().bank),
        sound_speed = CoreFociStmOption::default().sound_speed,
        loop_behavior = ops::LoopBehavior(CoreFociStmOption::default().loop_behavior),
        transition_mode = ops::TransitionMode(CoreFociStmOption::default().transition_mode),
    ))]
    fn new(
        bank: ops::PatternBank,
        #[pyo3(from_py_with = extract_velocity)] sound_speed: Velocity,
        loop_behavior: ops::LoopBehavior,
        transition_mode: ops::TransitionMode,
    ) -> Self {
        Self(CoreFociStmOption {
            bank: bank.0,
            sound_speed,
            loop_behavior: loop_behavior.0,
            transition_mode: transition_mode.0,
        })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    #[getter]
    fn bank(&self) -> ops::PatternBank {
        ops::PatternBank(self.0.bank)
    }

    #[getter]
    fn sound_speed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        velocity_to_py(py, self.0.sound_speed)
    }

    #[getter]
    fn loop_behavior(&self) -> ops::LoopBehavior {
        ops::LoopBehavior(self.0.loop_behavior)
    }

    #[getter]
    fn transition_mode(&self) -> ops::TransitionMode {
        ops::TransitionMode(self.0.transition_mode)
    }
}

#[pyclass(name = "PatternStmOption", module = "autd3.commands", eq, frozen)]
#[derive(PartialEq)]
pub struct PatternStmOption(pub(crate) CorePatternStmOption);

#[pymethods]
impl PatternStmOption {
    #[new]
    #[pyo3(signature = (
        bank = ops::PatternBank(CorePatternStmOption::default().bank),
        phase_depth = ops::PhaseDepth(CorePatternStmOption::default().phase_depth),
        loop_behavior = ops::LoopBehavior(CorePatternStmOption::default().loop_behavior),
        transition_mode = ops::TransitionMode(CorePatternStmOption::default().transition_mode),
    ))]
    fn new(
        bank: ops::PatternBank,
        phase_depth: ops::PhaseDepth,
        loop_behavior: ops::LoopBehavior,
        transition_mode: ops::TransitionMode,
    ) -> Self {
        Self(CorePatternStmOption {
            bank: bank.0,
            phase_depth: phase_depth.0,
            loop_behavior: loop_behavior.0,
            transition_mode: transition_mode.0,
        })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    #[getter]
    fn bank(&self) -> ops::PatternBank {
        ops::PatternBank(self.0.bank)
    }

    #[getter]
    fn phase_depth(&self) -> ops::PhaseDepth {
        ops::PhaseDepth(self.0.phase_depth)
    }

    #[getter]
    fn loop_behavior(&self) -> ops::LoopBehavior {
        ops::LoopBehavior(self.0.loop_behavior)
    }

    #[getter]
    fn transition_mode(&self) -> ops::TransitionMode {
        ops::TransitionMode(self.0.transition_mode)
    }
}

macro_rules! foci_points {
    ($($n:literal => $variant:ident),* $(,)?) => {
        pub(crate) enum FociPoints {
            $($variant(Vec<CoreControlPoints<$n>>)),*
        }

        impl FociPoints {
            fn from_samples(samples: &[PyRef<'_, ControlPoints>]) -> PyResult<Self> {
                let Some(num_foci) = samples.first().map(|s| s.points.len()) else {
                    return Ok(FociPoints::N1(Vec::new()));
                };
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

            fn push_into<'a>(
                &'a self,
                config: CoreStmConfig,
                option: CoreFociStmOption,
                expansion: &mut Expansion<'_, 'a>,
            ) -> Result<(), autd3_rs::Error> {
                match self {
                    $(FociPoints::$variant(v) => {
                        expansion.push(CoreFociStm::new(config, v.as_slice(), option))?;
                    })*
                }
                Ok(())
            }

            fn push_write_foci<'a>(
                &'a self,
                bank: CorePatternBank,
                start_index: usize,
                expansion: &mut Expansion<'_, 'a>,
            ) -> Result<(), autd3_rs::Error> {
                match self {
                    $(FociPoints::$variant(v) => {
                        expansion.push(CoreWriteFociBuffer {
                            bank,
                            start_index,
                            points: v.as_slice(),
                        })?;
                    })*
                }
                Ok(())
            }
        }
    };
}

foci_points!(1 => N1, 2 => N2, 3 => N3, 4 => N4, 5 => N5, 6 => N6, 7 => N7, 8 => N8);

pub(crate) struct FociStmData {
    config: CoreStmConfig,
    points: FociPoints,
    option: CoreFociStmOption,
}

impl PushCommand for FociStmData {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        self.points.push_into(self.config, self.option, expansion)
    }
}

#[pyclass(name = "FociStm", module = "autd3.commands", frozen)]
pub struct FociStm(pub(crate) Arc<FociStmData>);

#[pymethods]
impl FociStm {
    #[new]
    #[pyo3(signature = (config, points, option))]
    fn new(
        config: &Bound<'_, PyAny>,
        points: Vec<PyRef<'_, ControlPoints>>,
        option: PyRef<'_, FociStmOption>,
    ) -> PyResult<Self> {
        Ok(Self(Arc::new(FociStmData {
            config: extract_stm_config(config)?,
            points: FociPoints::from_samples(&points)?,
            option: option.0,
        })))
    }
}

pub(crate) struct WriteFociBufferData {
    bank: CorePatternBank,
    start_index: usize,
    points: FociPoints,
}

impl PushCommand for WriteFociBufferData {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        self.points
            .push_write_foci(self.bank, self.start_index, expansion)
    }
}

#[pyclass(name = "WriteFociBuffer", module = "autd3.commands", frozen)]
pub struct WriteFociBuffer(pub(crate) Arc<WriteFociBufferData>);

#[pymethods]
impl WriteFociBuffer {
    #[new]
    fn new(
        bank: ops::PatternBank,
        start_index: usize,
        points: Vec<PyRef<'_, ControlPoints>>,
    ) -> PyResult<Self> {
        Ok(Self(Arc::new(WriteFociBufferData {
            bank: bank.0,
            start_index,
            points: FociPoints::from_samples(&points)?,
        })))
    }
}

pub(crate) enum OwnedStmIntensity {
    Uniform(Intensity),
    SameForAllIndices(Vec<Vec<Intensity>>),
    PerIndex(Vec<Vec<Vec<Intensity>>>),
}

impl OwnedStmIntensity {
    fn as_ref(&self) -> CoreStmIntensity<'_> {
        match self {
            OwnedStmIntensity::Uniform(intensity) => CoreStmIntensity::Uniform(*intensity),
            OwnedStmIntensity::SameForAllIndices(intensities) => {
                CoreStmIntensity::SameForAllIndices(intensities)
            }
            OwnedStmIntensity::PerIndex(intensities) => CoreStmIntensity::PerIndex(intensities),
        }
    }
}

fn extract_stm_intensity(obj: &Bound<'_, PyAny>) -> PyResult<OwnedStmIntensity> {
    if let Some(intensity) = crate::datagram::extract_uniform_intensity(obj) {
        return Ok(OwnedStmIntensity::Uniform(intensity));
    }
    if obj.hasattr("_capsule")? {
        return Ok(OwnedStmIntensity::SameForAllIndices(
            crate::datagram::extract_intensities(obj)?,
        ));
    }
    if let Ok(buffers) = obj.extract::<Vec<Bound<'_, PyAny>>>() {
        return buffers
            .iter()
            .map(crate::datagram::extract_intensities)
            .collect::<PyResult<Vec<_>>>()
            .map(OwnedStmIntensity::PerIndex);
    }
    Ok(OwnedStmIntensity::SameForAllIndices(
        crate::datagram::extract_intensities(obj)?,
    ))
}

pub(crate) struct PatternStmData {
    config: CoreStmConfig,
    phases: Vec<Vec<Vec<Phase>>>,
    intensities: OwnedStmIntensity,
    option: CorePatternStmOption,
}

impl PushCommand for PatternStmData {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(CorePatternStm::new(
            self.config,
            self.phases.as_slice(),
            self.intensities.as_ref(),
            self.option,
        ))?;
        Ok(())
    }
}

#[pyclass(name = "PatternStm", module = "autd3.commands", frozen)]
pub struct PatternStm(pub(crate) Arc<PatternStmData>);

#[pymethods]
impl PatternStm {
    #[new]
    #[pyo3(signature = (config, phases, intensities, option))]
    fn new(
        config: &Bound<'_, PyAny>,
        phases: Vec<Bound<'_, PyAny>>,
        intensities: &Bound<'_, PyAny>,
        option: PyRef<'_, PatternStmOption>,
    ) -> PyResult<Self> {
        let stm_config = extract_stm_config(config)?;
        let phases = phases
            .iter()
            .map(crate::datagram::extract_phases)
            .collect::<PyResult<Vec<_>>>()?;
        let intensities = extract_stm_intensity(intensities)?;
        Ok(Self(Arc::new(PatternStmData {
            config: stm_config,
            phases,
            intensities,
            option: option.0,
        })))
    }
}

#[pyfunction]
#[pyo3(signature = (center, radius, num_points, normal, intensity, dst))]
fn circle(
    center: &Bound<'_, PyAny>,
    radius: &Bound<'_, PyAny>,
    num_points: usize,
    normal: &Bound<'_, PyAny>,
    intensity: &Bound<'_, PyAny>,
    dst: &Bound<'_, PyList>,
) -> PyResult<()> {
    let intensity = Intensity(extract_u8(intensity)?);
    let mut pts = Vec::new();
    core_circle(
        extract_point(center)?,
        extract_length(radius)?,
        num_points,
        extract_direction(normal)?,
        intensity,
        &mut pts,
    );
    dst.call_method0("clear")?;
    for cp in pts {
        dst.append(ControlPoints::from_core(cp))?;
    }
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (start, end, num_points, intensity, dst))]
fn line(
    start: &Bound<'_, PyAny>,
    end: &Bound<'_, PyAny>,
    num_points: usize,
    intensity: &Bound<'_, PyAny>,
    dst: &Bound<'_, PyList>,
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
    dst.call_method0("clear")?;
    for cp in pts {
        dst.append(ControlPoints::from_core(cp))?;
    }
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<StmConfig>()?;
    m.add_class::<ControlPoint>()?;
    m.add_class::<ControlPoints>()?;
    m.add_class::<FociStmOption>()?;
    m.add_class::<PatternStmOption>()?;
    m.add_class::<FociStm>()?;
    m.add_class::<WriteFociBuffer>()?;
    m.add_class::<PatternStm>()?;
    m.add_function(wrap_pyfunction!(circle, m)?)?;
    m.add_function(wrap_pyfunction!(line, m)?)?;
    Ok(())
}
