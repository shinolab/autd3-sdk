use core::num::NonZeroU32;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use autd3_python_capsule::{
    LegacyClientBackend, capsule_of, geometry_from_capsule, legacy_capsule_of,
    legacy_frame_into_capsule, take_legacy_client_opener, to_pyerr, to_pyerr_gil,
};
use autd3_rs::Geometry;
use autd3_rs::commands::{
    ChangeModulationBank as CoreChangeModulationBank, Modulation as CoreModulation,
    Pattern as CorePattern, PatternStm as CorePatternStm,
};
use autd3_rs::legacy::{
    LegacyChangePatternBank as CoreLegacyChangePatternBank,
    LegacyClientConfig as CoreLegacyClientConfig, LegacyCommand,
    LegacyDatagramBuilder as CoreLegacyBuilder, LegacyFrames as CoreLegacyFrames,
};
use autd3_rs::value::{SamplingConfig, TransitionMode as CoreTransitionMode};
use autd3_rs::{CoreId, ThreadPriority, ThreadPriorityValue};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;

use crate::client::{Checker, CheckerSource, FpgaState};
use crate::config::RtSchedulePolicy;
use crate::datagram::{DatagramBuilder, Pending, validate_pending};
use crate::future::future_into_py;
use crate::ops::{PatternBank, TransitionMode};

#[pyclass(name = "LegacyClientConfig", module = "autd3", skip_from_py_object)]
#[derive(Clone)]
pub struct LegacyClientConfig {
    pub(crate) inner: CoreLegacyClientConfig,
}

#[pymethods]
impl LegacyClientConfig {
    #[new]
    #[pyo3(signature = (
        timeout_cycles = None,
        rt_priority = None,
        disable_rt_priority = false,
        rt_policy = None,
        rt_affinity = None,
    ))]
    fn new(
        timeout_cycles: Option<u32>,
        rt_priority: Option<u8>,
        disable_rt_priority: bool,
        rt_policy: Option<RtSchedulePolicy>,
        rt_affinity: Option<usize>,
    ) -> PyResult<Self> {
        let mut inner = CoreLegacyClientConfig::default();
        if let Some(v) = timeout_cycles {
            inner.timeout_cycles = NonZeroU32::new(v)
                .ok_or_else(|| PyValueError::new_err("timeout_cycles must be >= 1"))?;
        }
        if rt_priority.is_some() && disable_rt_priority {
            return Err(PyValueError::new_err(
                "rt_priority and disable_rt_priority are mutually exclusive",
            ));
        }
        if disable_rt_priority {
            inner.rt_priority = None;
        }
        if let Some(v) = rt_priority {
            let value = ThreadPriorityValue::try_from(v)
                .map_err(|e| PyValueError::new_err(format!("invalid rt_priority: {e}")))?;
            inner.rt_priority = Some(ThreadPriority::Crossplatform(value));
        }
        if let Some(v) = rt_policy {
            inner.rt_policy = v.0;
        }
        if let Some(v) = rt_affinity {
            inner.rt_affinity = Some(CoreId { id: v });
        }
        Ok(Self { inner })
    }
}

fn unsupported(name: &str) -> PyErr {
    PyValueError::new_err(format!(
        "legacy firmware does not support the `{name}` command"
    ))
}

type ErrSlot = Rc<RefCell<Option<PyErr>>>;

struct PendingCommand<'a> {
    pending: &'a Pending,
    err: ErrSlot,
}

impl<'a> LegacyCommand<'a> for PendingCommand<'a> {
    fn expand(self, builder: &mut CoreLegacyBuilder<'a>) {
        if let Err(e) = push_pending(self.pending, builder) {
            let mut slot = self.err.borrow_mut();
            if slot.is_none() {
                *slot = Some(e);
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn push_pending<'a>(pending: &'a Pending, builder: &mut CoreLegacyBuilder<'a>) -> PyResult<()> {
    match pending {
        Pending::Pattern {
            bank,
            emissions,
            transition_mode,
        } => {
            builder.push(CorePattern {
                transition_mode: *transition_mode,
                ..CorePattern::with_bank(*bank, emissions)
            });
        }
        Pending::Modulation {
            bank,
            divider,
            data,
            loop_behavior,
            transition_mode,
        } => {
            let divider = core::num::NonZeroU16::new(*divider)
                .ok_or_else(|| PyValueError::new_err("divider must be >= 1"))?;
            let mut cmd = CoreModulation::with_bank(*bank, SamplingConfig::new(divider), data);
            cmd.loop_behavior = *loop_behavior;
            cmd.transition_mode = *transition_mode;
            builder.push(cmd);
        }
        Pending::ChangeModulationBank {
            bank,
            transition_mode,
        } => {
            builder.push(CoreChangeModulationBank {
                bank: *bank,
                transition_mode: *transition_mode,
            });
        }
        Pending::FociStm {
            config,
            points,
            option,
        } => {
            points.push_legacy_into(*config, *option, builder);
        }
        Pending::PatternStm {
            config,
            patterns,
            option,
        } => {
            builder.push(CorePatternStm::new(*config, patterns.as_slice(), *option));
        }
        Pending::Command(command) => {
            command.push_legacy_into(builder);
        }
        Pending::WritePatternBuffer { .. } => return Err(unsupported("WritePatternBuffer")),
        Pending::WriteFociBuffer { .. } => return Err(unsupported("WriteFociBuffer")),
        Pending::WritePatternCompressed { .. } => {
            return Err(unsupported("WritePatternCompressed"));
        }
        Pending::ConfigPattern { .. } => return Err(unsupported("ConfigPattern")),
        Pending::ConfigFociStm { .. } => return Err(unsupported("ConfigFociStm")),
        Pending::ChangePatternBank { .. } => return Err(unsupported("ChangePatternBank")),
        Pending::WriteModulationBuffer { .. } => return Err(unsupported("WriteModulationBuffer")),
        Pending::ConfigModulation { .. } => return Err(unsupported("ConfigModulation")),
        Pending::Each { devices } => {
            let err: ErrSlot = Rc::default();
            builder.push_each(|device| {
                devices
                    .get(device.idx())
                    .and_then(Option::as_ref)
                    .map(|pending| PendingCommand {
                        pending,
                        err: Rc::clone(&err),
                    })
            });
            if let Some(e) = err.borrow_mut().take() {
                return Err(e);
            }
        }
    }
    Ok(())
}

#[pyclass(
    name = "LegacyChangePatternBank",
    module = "autd3",
    skip_from_py_object
)]
#[derive(Clone, Copy)]
pub struct LegacyChangePatternBank {
    inner: CoreLegacyChangePatternBank,
}

#[pymethods]
impl LegacyChangePatternBank {
    #[staticmethod]
    fn pattern(bank: PatternBank) -> Self {
        Self {
            inner: CoreLegacyChangePatternBank::pattern(bank.0),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (bank, transition_mode = None))]
    fn foci_stm(bank: PatternBank, transition_mode: Option<TransitionMode>) -> Self {
        Self {
            inner: CoreLegacyChangePatternBank::foci_stm(
                bank.0,
                unwrap_transition_mode(transition_mode),
            ),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (bank, transition_mode = None))]
    fn pattern_stm(bank: PatternBank, transition_mode: Option<TransitionMode>) -> Self {
        Self {
            inner: CoreLegacyChangePatternBank::pattern_stm(
                bank.0,
                unwrap_transition_mode(transition_mode),
            ),
        }
    }
}

fn unwrap_transition_mode(mode: Option<TransitionMode>) -> CoreTransitionMode {
    mode.map_or(CoreTransitionMode::Immediate, |t| t.0)
}

enum LegacyItem {
    Current(Pending),
    LegacyChangePatternBank(CoreLegacyChangePatternBank),
}

#[pyclass(name = "LegacyDatagramBuilder", module = "autd3")]
pub struct LegacyDatagramBuilder {
    geometry: Arc<Geometry>,
    inner: DatagramBuilder,
    pending: Vec<LegacyItem>,
    backend: Option<Arc<dyn LegacyClientBackend>>,
}

impl LegacyDatagramBuilder {
    pub(crate) fn with_geometry(geometry: Arc<Geometry>) -> Self {
        Self::with_backend(geometry, None)
    }

    pub(crate) fn with_backend(
        geometry: Arc<Geometry>,
        backend: Option<Arc<dyn LegacyClientBackend>>,
    ) -> Self {
        Self {
            inner: DatagramBuilder::with_geometry(Arc::clone(&geometry)),
            geometry,
            pending: Vec::new(),
            backend,
        }
    }

    fn dc_offset_ns(&self) -> i64 {
        self.backend.as_ref().map_or(0, |b| b.dc_offset_ns())
    }
}

#[pymethods]
impl LegacyDatagramBuilder {
    #[new]
    fn new(geometry: &Bound<'_, PyAny>) -> PyResult<Self> {
        let geometry = geometry_from_capsule(&capsule_of(geometry)?)?.clone();
        Ok(Self::with_geometry(Arc::new(geometry)))
    }

    fn push(&mut self, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(op) = obj.cast::<LegacyChangePatternBank>() {
            let op = *op.borrow();
            self.pending
                .push(LegacyItem::LegacyChangePatternBank(op.inner));
            return Ok(());
        }
        let pending = self.inner.pop_pushed(obj)?;
        self.pending.push(LegacyItem::Current(pending));
        Ok(())
    }

    fn push_each(&mut self, py: Python<'_>, assign: &Bound<'_, PyAny>) -> PyResult<()> {
        let pending = self.inner.pop_pushed_each(py, assign)?;
        self.pending.push(LegacyItem::Current(pending));
        Ok(())
    }

    fn build(&self, py: Python<'_>) -> PyResult<LegacyFrames> {
        let mut builder =
            CoreLegacyBuilder::with_dc_offset(Arc::clone(&self.geometry), self.dc_offset_ns());
        for item in &self.pending {
            match item {
                LegacyItem::Current(pending) => {
                    validate_pending(pending)?;
                    push_pending(pending, &mut builder)?;
                }
                LegacyItem::LegacyChangePatternBank(cmd) => {
                    builder.push(*cmd);
                }
            }
        }
        let frames = builder.build().map_err(|e| to_pyerr(py, e))?;
        Ok(LegacyFrames {
            inner: Arc::new(frames),
        })
    }
}

#[pyclass(name = "LegacyFrame", module = "autd3")]
pub struct LegacyFrame {
    pub(crate) frames: Arc<CoreLegacyFrames>,
    pub(crate) index: usize,
}

#[pymethods]
impl LegacyFrame {
    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyCapsule>> {
        legacy_frame_into_capsule(py, Arc::clone(&self.frames), self.index)
    }
}

#[pyclass(name = "LegacyFrames", module = "autd3")]
pub struct LegacyFrames {
    pub(crate) inner: Arc<CoreLegacyFrames>,
}

#[pymethods]
impl LegacyFrames {
    fn num_frames(&self) -> usize {
        self.inner.len()
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __getitem__(&self, index: usize) -> PyResult<LegacyFrame> {
        if index >= self.inner.len() {
            return Err(PyIndexError::new_err("frame index out of range"));
        }
        Ok(LegacyFrame {
            frames: Arc::clone(&self.inner),
            index,
        })
    }
}

#[pyclass(name = "LegacyClient", module = "autd3")]
pub struct LegacyClient {
    backend: Arc<dyn LegacyClientBackend>,
    geometry: Arc<Geometry>,
}

#[pymethods]
impl LegacyClient {
    #[staticmethod]
    fn open<'py>(
        py: Python<'py>,
        geometry: &Bound<'py, PyAny>,
        link: &Bound<'py, PyAny>,
        config: &LegacyClientConfig,
    ) -> PyResult<Bound<'py, PyAny>> {
        let geometry = geometry_from_capsule(&capsule_of(geometry)?)?.clone();
        let opener = take_legacy_client_opener(&legacy_capsule_of(link)?)?;
        let config = config.inner;
        future_into_py(py, async move {
            let geometry_for_client = Arc::new(geometry.clone());
            let backend = opener(geometry, config).await.map_err(to_pyerr_gil)?;
            Ok(LegacyClient {
                backend: Arc::from(backend),
                geometry: geometry_for_client,
            })
        })
    }

    #[staticmethod]
    fn open_with_checker<'py>(
        py: Python<'py>,
        geometry: &Bound<'py, PyAny>,
        link: &Bound<'py, PyAny>,
        config: &LegacyClientConfig,
    ) -> PyResult<Bound<'py, PyAny>> {
        let geometry = geometry_from_capsule(&capsule_of(geometry)?)?.clone();
        let opener = take_legacy_client_opener(&legacy_capsule_of(link)?)?;
        let config = config.inner;
        future_into_py(py, async move {
            let geometry_for_client = Arc::new(geometry.clone());
            let backend: Arc<dyn LegacyClientBackend> =
                Arc::from(opener(geometry, config).await.map_err(to_pyerr_gil)?);
            Ok((
                LegacyClient {
                    backend: Arc::clone(&backend),
                    geometry: geometry_for_client,
                },
                Checker {
                    source: CheckerSource::Legacy(backend),
                },
            ))
        })
    }

    fn num_devices(&self) -> usize {
        self.backend.num_devices()
    }

    fn datagram_builder(&self) -> LegacyDatagramBuilder {
        LegacyDatagramBuilder::with_backend(
            Arc::clone(&self.geometry),
            Some(Arc::clone(&self.backend)),
        )
    }

    fn read_firmware_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            backend.read_firmware_version().await.map_err(to_pyerr_gil)
        })
    }

    fn read_fpga_state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            let states = backend.read_fpga_state().await.map_err(to_pyerr_gil)?;
            Ok(states
                .into_iter()
                .map(|s| FpgaState(autd3_rs::FpgaState(s)))
                .collect::<Vec<_>>())
        })
    }

    fn send<'py>(
        &self,
        py: Python<'py>,
        frame: PyRef<'_, LegacyFrame>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        let frames = Arc::clone(&frame.frames);
        let index = frame.index;
        future_into_py(py, async move {
            backend.send(frames, index).await.map_err(to_pyerr_gil)
        })
    }

    fn send_checked<'py>(
        &self,
        py: Python<'py>,
        frame: PyRef<'_, LegacyFrame>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        let frames = Arc::clone(&frame.frames);
        let index = frame.index;
        future_into_py(py, async move {
            backend
                .send_checked(frames, Some(index))
                .await
                .map_err(to_pyerr_gil)
        })
    }

    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(
            py,
            async move { backend.stop().await.map_err(to_pyerr_gil) },
        )
    }

    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(
            py,
            async move { backend.close().await.map_err(to_pyerr_gil) },
        )
    }
}
