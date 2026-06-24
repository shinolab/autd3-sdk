use core::time::Duration;

use autd3_python_capsule::{DevicePattern, capsule_of, pattern_from_capsule};
use autd3_rs::stm::{
    ControlPoint as CoreControlPoint, ControlPoints as CoreControlPoints, FociStm as CoreFociStm,
    FociStmOption as CoreFociStmOption, PatternStmMode as CorePatternStmMode,
    PatternStmOption as CorePatternStmOption, StmConfig as CoreStmConfig, circle as core_circle,
    line as core_line,
};
use autd3_rs::value::{Intensity, Phase, SamplingConfig};
use autd3_rs::{Point3, Vector3, Velocity};
use autd3_rs_core::geometry::UnitVector3;
use autd3_rs_core::units::{Hz, mm};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

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

#[pyclass(name = "StmConfig", module = "autd3", from_py_object)]
#[derive(Clone, Copy)]
pub struct StmConfig(pub(crate) CoreStmConfig);

#[pymethods]
impl StmConfig {
    #[staticmethod]
    #[pyo3(name = "Freq")]
    fn freq(hz: f32) -> Self {
        Self(CoreStmConfig::Freq(hz * Hz))
    }

    #[staticmethod]
    #[pyo3(name = "FreqNearest")]
    fn freq_nearest(hz: f32) -> Self {
        Self(CoreStmConfig::FreqNearest(hz * Hz))
    }

    #[staticmethod]
    #[pyo3(name = "Period")]
    fn period(secs: f32) -> Self {
        Self(CoreStmConfig::Period(Duration::from_secs_f32(secs)))
    }

    #[staticmethod]
    #[pyo3(name = "PeriodNearest")]
    fn period_nearest(secs: f32) -> Self {
        Self(CoreStmConfig::PeriodNearest(Duration::from_secs_f32(secs)))
    }

    #[staticmethod]
    #[pyo3(name = "Sampling")]
    fn sampling(divide: u16) -> PyResult<Self> {
        let divide = core::num::NonZeroU16::new(divide)
            .ok_or_else(|| PyValueError::new_err("divide must be >= 1"))?;
        Ok(Self(CoreStmConfig::Sampling(SamplingConfig::Divide(
            divide,
        ))))
    }
}

#[pyclass(name = "ControlPoint", module = "autd3", skip_from_py_object)]
#[derive(Clone, Copy)]
pub struct ControlPoint {
    pub(crate) inner: CoreControlPoint,
}

#[pymethods]
impl ControlPoint {
    #[new]
    #[pyo3(signature = (point, phase_offset = 0))]
    fn new(point: &Bound<'_, PyAny>, phase_offset: u8) -> PyResult<Self> {
        Ok(Self {
            inner: CoreControlPoint::new(extract_point(point)?, Phase(phase_offset)),
        })
    }
}

#[pyclass(name = "ControlPoints", module = "autd3", skip_from_py_object)]
#[derive(Clone)]
pub struct ControlPoints {
    pub(crate) points: Vec<CoreControlPoint>,
    pub(crate) intensity: Intensity,
}

#[pymethods]
impl ControlPoints {
    #[new]
    #[pyo3(signature = (points, intensity = 0xFF))]
    fn new(points: Vec<PyRef<'_, ControlPoint>>, intensity: u8) -> Self {
        Self {
            points: points.iter().map(|p| p.inner).collect(),
            intensity: Intensity(intensity),
        }
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

#[pyclass(name = "FociStmOption", module = "autd3", skip_from_py_object)]
pub struct FociStmOption {
    pub(crate) inner: CoreFociStmOption,
}

#[pymethods]
impl FociStmOption {
    #[new]
    #[pyo3(signature = (bank = None, sound_speed_m_s = 340.0, loop_behavior = None, transition_mode = None))]
    fn new(
        bank: Option<ops::PatternBank>,
        sound_speed_m_s: f32,
        loop_behavior: Option<ops::LoopBehavior>,
        transition_mode: Option<ops::TransitionMode>,
    ) -> Self {
        let mut inner = CoreFociStmOption {
            sound_speed: Velocity::from_m_s(sound_speed_m_s),
            ..CoreFociStmOption::default()
        };
        if let Some(b) = bank {
            inner.bank = b.0;
        }
        if let Some(l) = loop_behavior {
            inner.loop_behavior = l.0;
        }
        if let Some(t) = transition_mode {
            inner.transition_mode = t.0;
        }
        Self { inner }
    }
}

#[pyclass(name = "PatternStmMode", module = "autd3", from_py_object)]
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

#[pyclass(name = "PatternStmOption", module = "autd3", skip_from_py_object)]
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
        }
    };
}

foci_points!(1 => N1, 2 => N2, 3 => N3, 4 => N4, 5 => N5, 6 => N6, 7 => N7, 8 => N8);

#[pyclass(name = "FociStm", module = "autd3")]
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
        config: StmConfig,
        samples: Vec<PyRef<'_, ControlPoints>>,
        option: Option<PyRef<'_, FociStmOption>>,
    ) -> PyResult<Self> {
        Ok(Self {
            config: config.0,
            points: FociPoints::from_samples(&samples)?,
            option: option.map_or_else(CoreFociStmOption::default, |o| o.inner),
        })
    }
}

#[pyclass(name = "PatternStm", module = "autd3")]
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
        config: StmConfig,
        patterns: Vec<Bound<'_, PyAny>>,
        option: Option<PyRef<'_, PatternStmOption>>,
    ) -> PyResult<Self> {
        let patterns = patterns
            .iter()
            .map(|buffer| {
                let capsule = capsule_of(buffer)?;
                Ok(pattern_from_capsule(&capsule)?.to_vec())
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            config: config.0,
            patterns,
            option: option.map_or_else(CorePatternStmOption::default, |o| o.inner),
        })
    }
}

#[pyfunction]
#[pyo3(signature = (center, radius_mm, num_points, normal, intensity = 0xFF))]
fn circle(
    center: &Bound<'_, PyAny>,
    radius_mm: f32,
    num_points: usize,
    normal: &Bound<'_, PyAny>,
    intensity: u8,
) -> PyResult<Vec<ControlPoints>> {
    let pts = core_circle(
        extract_point(center)?,
        radius_mm * mm,
        num_points,
        extract_direction(normal)?,
        Intensity(intensity),
    );
    Ok(pts.into_iter().map(ControlPoints::from_core).collect())
}

#[pyfunction]
#[pyo3(signature = (start, end, num_points, intensity = 0xFF))]
fn line(
    start: &Bound<'_, PyAny>,
    end: &Bound<'_, PyAny>,
    num_points: usize,
    intensity: u8,
) -> PyResult<Vec<ControlPoints>> {
    let pts = core_line(
        extract_point(start)?,
        extract_point(end)?,
        num_points,
        Intensity(intensity),
    );
    Ok(pts.into_iter().map(ControlPoints::from_core).collect())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<StmConfig>()?;
    m.add_class::<ControlPoint>()?;
    m.add_class::<ControlPoints>()?;
    m.add_class::<FociStmOption>()?;
    m.add_class::<PatternStmMode>()?;
    m.add_class::<PatternStmOption>()?;
    m.add_class::<FociStm>()?;
    m.add_class::<PatternStm>()?;
    m.add_function(wrap_pyfunction!(circle, m)?)?;
    m.add_function(wrap_pyfunction!(line, m)?)?;
    Ok(())
}
