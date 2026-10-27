use std::sync::Arc;

use autd3_python_capsule::error::invalid_argument;
use autd3_python_capsule::extract::{
    extract_point, extract_sampling_config, extract_u8, extract_velocity, intensity_to_py,
    phase_to_py, velocity_to_py,
};
use autd3_python_capsule::numpy::{f32_vec3, f32_vec3_rows_from_ndarray, is_ndarray};
use autd3_rs::commands::WriteFociBuffer as CoreWriteFociBuffer;
use autd3_rs::commands::{Expansion, Operation};
use autd3_rs::commands::{
    FociStm as CoreFociStm, FociStmOption as CoreFociStmOption, PatternStm as CorePatternStm,
    PatternStmOption as CorePatternStmOption, StmIntensity as CoreStmIntensity,
};
use autd3_rs::params::NUM_FOCI_MAX;
use autd3_rs::value::{
    ControlPoint as CoreControlPoint, ControlPoints as CoreControlPoints, Intensity,
    PatternBank as CorePatternBank, Phase, SamplingConfig,
};
use autd3_rs::{Point3, Velocity};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

use crate::datagram::{PushCommand, PushOperation};
use crate::ops;

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
pub struct ControlPoints(pub(crate) CoreControlPoints);

#[pymethods]
impl ControlPoints {
    #[new]
    #[pyo3(signature = (points, intensity = Intensity::MAX.0))]
    fn new(
        points: &Bound<'_, PyAny>,
        #[pyo3(from_py_with = extract_u8)] intensity: u8,
    ) -> PyResult<Self> {
        let points = if is_ndarray(points)? {
            f32_vec3_rows_from_ndarray(points)?
                .into_iter()
                .map(|[x, y, z]| CoreControlPoint::new(Point3::new(x, y, z), Phase::ZERO))
                .collect::<Vec<_>>()
        } else if let Ok(point) = points.cast::<ControlPoint>() {
            vec![point.get().0]
        } else {
            points
                .extract::<Vec<PyRef<'_, ControlPoint>>>()
                .map_err(|_| {
                    PyTypeError::new_err(
                        "expected a ControlPoint, a sequence of ControlPoint, or a numpy.ndarray of shape (3,) or (n, 3)",
                    )
                })?
                .iter()
                .map(|p| p.0)
                .collect()
        };
        Ok(Self(CoreControlPoints::new(points, Intensity(intensity))))
    }

    #[getter]
    fn points(&self) -> Vec<ControlPoint> {
        self.0.points.iter().copied().map(ControlPoint).collect()
    }

    #[getter]
    fn intensity<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        intensity_to_py(py, self.0.intensity)
    }

    fn __repr__(&self) -> String {
        format!(
            "ControlPoints {{ points: {:?}, intensity: {:?} }}",
            self.0.points, self.0.intensity
        )
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

pub(crate) struct FociStmData {
    config: SamplingConfig,
    points: Vec<CoreControlPoints>,
    option: CoreFociStmOption,
}

impl PushCommand for FociStmData {
    fn push_into(&self, expansion: &mut Expansion<'_>) -> Result<(), autd3_rs::Error> {
        expansion.push(CoreFociStm::new(self.config, &self.points, self.option))?;
        Ok(())
    }
}

fn core_points(points: &[PyRef<'_, ControlPoints>]) -> Vec<CoreControlPoints> {
    points.iter().map(|p| p.0.clone()).collect()
}

#[pyclass(name = "FociStm", module = "autd3.commands", frozen)]
pub struct FociStm(pub(crate) Arc<FociStmData>);

#[pymethods]
impl FociStm {
    #[new]
    #[pyo3(signature = (config, points, option))]
    fn new(
        #[pyo3(from_py_with = extract_sampling_config)] config: SamplingConfig,
        points: Vec<PyRef<'_, ControlPoints>>,
        option: PyRef<'_, FociStmOption>,
    ) -> Self {
        Self(Arc::new(FociStmData {
            config,
            points: core_points(&points),
            option: option.0,
        }))
    }
}

pub(crate) struct WriteFociBufferData {
    bank: CorePatternBank,
    start_index: usize,
    points: Vec<CoreControlPoints>,
}

impl PushOperation for WriteFociBufferData {
    fn operation(&self) -> Box<dyn Operation + '_> {
        Box::new(CoreWriteFociBuffer {
            bank: self.bank,
            start_index: self.start_index,
            points: &self.points,
        })
    }
}

#[pyclass(name = "WriteFociBuffer", module = "autd3.commands", frozen)]
pub struct WriteFociBuffer(pub(crate) Arc<WriteFociBufferData>);

#[pymethods]
impl WriteFociBuffer {
    #[staticmethod]
    fn max_len(num_foci: usize) -> PyResult<usize> {
        u8::try_from(num_foci)
            .ok()
            .map(CoreWriteFociBuffer::max_len)
            .filter(|&len| len > 0)
            .ok_or_else(|| {
                invalid_argument(format!(
                    "number of foci per sample must be 1..={NUM_FOCI_MAX}, got {num_foci}"
                ))
            })
    }

    #[new]
    fn new(
        bank: ops::PatternBank,
        start_index: usize,
        points: Vec<PyRef<'_, ControlPoints>>,
    ) -> Self {
        Self(Arc::new(WriteFociBufferData {
            bank: bank.0,
            start_index,
            points: core_points(&points),
        }))
    }
}

pub(crate) enum OwnedStmIntensity {
    Uniform(Intensity),
    SameForAllIndices(Vec<Vec<Intensity>>),
    PerIndex(Vec<Vec<Vec<Intensity>>>),
}

impl OwnedStmIntensity {
    pub(crate) fn as_ref(&self) -> CoreStmIntensity<'_> {
        match self {
            OwnedStmIntensity::Uniform(intensity) => CoreStmIntensity::Uniform(*intensity),
            OwnedStmIntensity::SameForAllIndices(intensities) => {
                CoreStmIntensity::SameForAllIndices(intensities)
            }
            OwnedStmIntensity::PerIndex(intensities) => CoreStmIntensity::PerIndex(intensities),
        }
    }
}

pub(crate) fn extract_stm_intensity(obj: &Bound<'_, PyAny>) -> PyResult<OwnedStmIntensity> {
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
    config: SamplingConfig,
    phases: Vec<Vec<Vec<Phase>>>,
    intensities: OwnedStmIntensity,
    option: CorePatternStmOption,
}

impl PushCommand for PatternStmData {
    fn push_into(&self, expansion: &mut Expansion<'_>) -> Result<(), autd3_rs::Error> {
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
        #[pyo3(from_py_with = extract_sampling_config)] config: SamplingConfig,
        phases: Vec<Bound<'_, PyAny>>,
        intensities: &Bound<'_, PyAny>,
        option: PyRef<'_, PatternStmOption>,
    ) -> PyResult<Self> {
        let phases = phases
            .iter()
            .map(crate::datagram::extract_phases)
            .collect::<PyResult<Vec<_>>>()?;
        let intensities = extract_stm_intensity(intensities)?;
        Ok(Self(Arc::new(PatternStmData {
            config,
            phases,
            intensities,
            option: option.0,
        })))
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ControlPoint>()?;
    m.add_class::<ControlPoints>()?;
    m.add_class::<FociStmOption>()?;
    m.add_class::<PatternStmOption>()?;
    m.add_class::<FociStm>()?;
    m.add_class::<WriteFociBuffer>()?;
    m.add_class::<PatternStm>()?;
    Ok(())
}
